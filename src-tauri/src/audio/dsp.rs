//! Real-time DSP nodes.
//!
//! Everything here runs on the DSP worker thread and is allocation-free once
//! `prepare` has run. Buffers are planar stereo: `[[f32; frames]; 2]`.

use crate::audio::params::{BandKind, EqBand, Resolved};

pub const CHANNELS: usize = 2;

/// Per-block parameter smoothing, so dragging a slider never clicks.
#[derive(Debug, Clone, Copy)]
pub struct Smoothed {
    current: f32,
    target: f32,
    coeff: f32,
}

impl Smoothed {
    pub fn new(value: f32) -> Self {
        Smoothed {
            current: value,
            target: value,
            coeff: 0.0,
        }
    }

    /// `time_ms` is the time constant of the one-pole glide.
    pub fn prepare(&mut self, sample_rate: f32, time_ms: f32) {
        let tau = (time_ms / 1000.0).max(1e-4) * sample_rate;
        self.coeff = (-1.0 / tau).exp();
    }

    pub fn set_target(&mut self, target: f32) {
        self.target = target;
    }

    pub fn jump_to(&mut self, value: f32) {
        self.current = value;
        self.target = value;
    }

    #[inline]
    pub fn next(&mut self) -> f32 {
        self.current = self.target + (self.current - self.target) * self.coeff;
        self.current
    }

    #[inline]
    pub fn is_settled(&self) -> bool {
        (self.current - self.target).abs() < 1e-6
    }

    /// True once the value has faded far enough down to be inaudible.
    ///
    /// The glide is exponential, so it approaches zero without ever reaching
    /// it; testing for exactly zero would keep a switched-off node processing
    /// for the rest of the session.
    #[inline]
    pub fn is_silent(&self) -> bool {
        self.current.abs() < 1e-4
    }
}

#[inline]
pub fn db_to_gain(db: f32) -> f32 {
    10.0f32.powf(db / 20.0)
}

// ---------------------------------------------------------------------------
// Biquad
// ---------------------------------------------------------------------------

/// Direct Form 1 biquad. DF1 is used deliberately: it behaves far better than
/// DF2 when coefficients are retuned while audio is flowing, which happens
/// every time an EQ slider moves.
#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    pub fn bypass() -> Self {
        Biquad {
            b0: 1.0,
            ..Default::default()
        }
    }

    pub fn set(&mut self, kind: BandKind, sample_rate: f32, freq: f32, gain_db: f32, q: f32) {
        // Keep the frequency below Nyquist or the bilinear transform blows up.
        let freq = freq.clamp(10.0, sample_rate * 0.49);
        let q = q.max(0.05);
        let a = db_to_gain(gain_db / 2.0);
        let w0 = 2.0 * std::f32::consts::PI * freq / sample_rate;
        let (sin_w0, cos_w0) = w0.sin_cos();
        let alpha = sin_w0 / (2.0 * q);

        let (b0, b1, b2, a0, a1, a2) = match kind {
            BandKind::Peak => (
                1.0 + alpha * a,
                -2.0 * cos_w0,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cos_w0,
                1.0 - alpha / a,
            ),
            BandKind::LowShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                    a * ((a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                    (a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BandKind::HighShelf => {
                let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;
                (
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 + two_sqrt_a_alpha),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                    a * ((a + 1.0) + (a - 1.0) * cos_w0 - two_sqrt_a_alpha),
                    (a + 1.0) - (a - 1.0) * cos_w0 + two_sqrt_a_alpha,
                    2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                    (a + 1.0) - (a - 1.0) * cos_w0 - two_sqrt_a_alpha,
                )
            }
            BandKind::LowPass => (
                (1.0 - cos_w0) / 2.0,
                1.0 - cos_w0,
                (1.0 - cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
            BandKind::HighPass => (
                (1.0 + cos_w0) / 2.0,
                -(1.0 + cos_w0),
                (1.0 + cos_w0) / 2.0,
                1.0 + alpha,
                -2.0 * cos_w0,
                1.0 - alpha,
            ),
        };

        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    #[inline]
    pub fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

// ---------------------------------------------------------------------------
// EQ
// ---------------------------------------------------------------------------

const MAX_BANDS: usize = 12;

pub struct EqChain {
    filters: [[Biquad; MAX_BANDS]; CHANNELS],
    active: usize,
    sample_rate: f32,
    preamp: Smoothed,
    /// Retained so we only recompute coefficients when a band actually changes.
    current: Vec<EqBand>,
    enabled: bool,
}

impl EqChain {
    pub fn new() -> Self {
        EqChain {
            filters: [[Biquad::bypass(); MAX_BANDS]; CHANNELS],
            active: 0,
            sample_rate: 48000.0,
            preamp: Smoothed::new(1.0),
            current: Vec::new(),
            enabled: true,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.preamp.prepare(sample_rate, 20.0);
        self.current.clear();
        for ch in self.filters.iter_mut() {
            for f in ch.iter_mut() {
                f.reset();
            }
        }
    }

    pub fn update(&mut self, eq: &crate::audio::params::Eq) {
        self.enabled = eq.enabled;
        self.preamp.set_target(db_to_gain(eq.preamp_db));

        let bands: Vec<&EqBand> = eq
            .bands
            .iter()
            .filter(|b| b.enabled)
            .take(MAX_BANDS)
            .collect();
        let changed = bands.len() != self.current.len()
            || bands.iter().zip(self.current.iter()).any(|(a, b)| *a != b);
        if !changed {
            return;
        }

        self.current = bands.iter().map(|b| (*b).clone()).collect();
        self.active = bands.len();
        for ch in 0..CHANNELS {
            for (i, band) in bands.iter().enumerate() {
                self.filters[ch][i].set(
                    band.kind,
                    self.sample_rate,
                    band.freq,
                    band.gain_db,
                    band.q,
                );
            }
        }
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if !self.enabled {
            return;
        }
        for i in 0..frames {
            let preamp = self.preamp.next();
            for ch in 0..CHANNELS {
                let mut s = buf[ch][i];
                for f in self.filters[ch][..self.active].iter_mut() {
                    s = f.process(s);
                }
                buf[ch][i] = s * preamp;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Delay
// ---------------------------------------------------------------------------

pub struct DelayFx {
    lines: [Vec<f32>; CHANNELS],
    write: [usize; CHANNELS],
    tone: [Biquad; CHANNELS],
    sample_rate: f32,
    time: [Smoothed; CHANNELS],
    feedback: Smoothed,
    mix: Smoothed,
    enabled: bool,
}

impl DelayFx {
    /// Longest delay the line can hold. Fixed so the buffer is allocated once.
    const MAX_SECONDS: f32 = 4.0;

    pub fn new() -> Self {
        DelayFx {
            lines: [Vec::new(), Vec::new()],
            write: [0; CHANNELS],
            tone: [Biquad::bypass(); CHANNELS],
            sample_rate: 48000.0,
            time: [Smoothed::new(0.0), Smoothed::new(0.0)],
            feedback: Smoothed::new(0.0),
            mix: Smoothed::new(0.0),
            enabled: false,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        let len = (sample_rate * Self::MAX_SECONDS) as usize + 4;
        for ch in 0..CHANNELS {
            self.lines[ch] = vec![0.0; len];
            self.write[ch] = 0;
            self.tone[ch].reset();
            // Glide delay time slowly: fast changes are a tape-warble artefact.
            self.time[ch].prepare(sample_rate, 120.0);
        }
        self.feedback.prepare(sample_rate, 20.0);
        self.mix.prepare(sample_rate, 20.0);
    }

    pub fn update(&mut self, d: &crate::audio::params::Delay) {
        self.enabled = d.enabled;
        let base = (d.time_ms / 1000.0 * self.sample_rate)
            .clamp(1.0, self.sample_rate * Self::MAX_SECONDS - 2.0);
        // Right channel runs longer for a ping-pong feel as spread increases.
        let right = (base * (1.0 + d.spread.clamp(0.0, 1.0)))
            .min(self.sample_rate * Self::MAX_SECONDS - 2.0);
        self.time[0].set_target(base);
        self.time[1].set_target(right);
        self.feedback.set_target(d.feedback.clamp(0.0, 0.95));
        self.mix.set_target(d.mix.clamp(0.0, 1.0));
        for ch in 0..CHANNELS {
            self.tone[ch].set(BandKind::LowPass, self.sample_rate, d.tone_hz, 0.0, 0.707);
        }
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if !self.enabled && self.mix.is_silent() {
            return;
        }
        // When switching off, ride the mix down to zero rather than cutting.
        if !self.enabled {
            self.mix.set_target(0.0);
        }

        for i in 0..frames {
            let fb = self.feedback.next();
            let mix = self.mix.next();
            for ch in 0..CHANNELS {
                let delay = self.time[ch].next();
                let line_len = self.lines[ch].len();

                // Fractional read for smooth time changes.
                let read_pos = self.write[ch] as f32 - delay;
                let read_pos = if read_pos < 0.0 {
                    read_pos + line_len as f32
                } else {
                    read_pos
                };
                let i0 = read_pos.floor() as usize % line_len;
                let i1 = (i0 + 1) % line_len;
                let frac = read_pos - read_pos.floor();
                let delayed = self.lines[ch][i0] * (1.0 - frac) + self.lines[ch][i1] * frac;

                let dry = buf[ch][i];
                let darkened = self.tone[ch].process(delayed);
                self.lines[ch][self.write[ch]] = dry + darkened * fb;
                self.write[ch] = (self.write[ch] + 1) % line_len;

                buf[ch][i] = dry * (1.0 - mix) + delayed * mix;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reverb (Freeverb-style: 8 combs + 4 allpasses per channel)
// ---------------------------------------------------------------------------

const COMB_TUNING: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const ALLPASS_TUNING: [usize; 4] = [556, 441, 341, 225];
/// Offset applied to the right channel's delay lengths to decorrelate the tail.
const STEREO_SPREAD: usize = 23;

struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(len: usize) -> Self {
        Comb {
            buf: vec![0.0; len.max(1)],
            pos: 0,
            store: 0.0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32, feedback: f32, damp: f32) -> f32 {
        let out = self.buf[self.pos];
        self.store = out * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = input + self.store * feedback;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(len: usize) -> Self {
        Allpass {
            buf: vec![0.0; len.max(1)],
            pos: 0,
        }
    }

    #[inline]
    fn process(&mut self, input: f32) -> f32 {
        const FEEDBACK: f32 = 0.5;
        let buffered = self.buf[self.pos];
        let out = -input + buffered;
        self.buf[self.pos] = input + buffered * FEEDBACK;
        self.pos = (self.pos + 1) % self.buf.len();
        out
    }
}

pub struct ReverbFx {
    combs: Vec<Vec<Comb>>,
    allpasses: Vec<Vec<Allpass>>,
    predelay: [Vec<f32>; CHANNELS],
    predelay_pos: usize,
    predelay_len: usize,
    feedback: Smoothed,
    damping: Smoothed,
    mix: Smoothed,
    width: Smoothed,
    enabled: bool,
    sample_rate: f32,
}

impl ReverbFx {
    const MAX_PREDELAY_MS: f32 = 250.0;

    pub fn new() -> Self {
        ReverbFx {
            combs: Vec::new(),
            allpasses: Vec::new(),
            predelay: [Vec::new(), Vec::new()],
            predelay_pos: 0,
            predelay_len: 0,
            feedback: Smoothed::new(0.0),
            damping: Smoothed::new(0.5),
            mix: Smoothed::new(0.0),
            width: Smoothed::new(1.0),
            enabled: false,
            sample_rate: 48000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        // Freeverb's tunings are quoted at 44.1 kHz; scale to the real rate.
        let scale = sample_rate / 44100.0;
        self.combs = (0..CHANNELS)
            .map(|ch| {
                COMB_TUNING
                    .iter()
                    .map(|&t| Comb::new(((t + ch * STEREO_SPREAD) as f32 * scale) as usize))
                    .collect()
            })
            .collect();
        self.allpasses = (0..CHANNELS)
            .map(|ch| {
                ALLPASS_TUNING
                    .iter()
                    .map(|&t| Allpass::new(((t + ch * STEREO_SPREAD) as f32 * scale) as usize))
                    .collect()
            })
            .collect();

        let pre = (sample_rate * Self::MAX_PREDELAY_MS / 1000.0) as usize + 2;
        for ch in 0..CHANNELS {
            self.predelay[ch] = vec![0.0; pre];
        }
        self.predelay_pos = 0;
        self.predelay_len = 1;

        self.feedback.prepare(sample_rate, 40.0);
        self.damping.prepare(sample_rate, 40.0);
        self.mix.prepare(sample_rate, 30.0);
        self.width.prepare(sample_rate, 30.0);
    }

    pub fn update(&mut self, r: &crate::audio::params::Reverb) {
        self.enabled = r.enabled;
        // Map 0..1 size onto Freeverb's usable room-size feedback range.
        self.feedback
            .set_target(0.7 + r.size.clamp(0.0, 1.0) * 0.28);
        self.damping.set_target(r.damping.clamp(0.0, 1.0) * 0.4);
        self.mix.set_target(r.mix.clamp(0.0, 1.0));
        self.width.set_target(r.width.clamp(0.0, 1.0));
        let pre =
            (r.predelay_ms.clamp(0.0, Self::MAX_PREDELAY_MS) / 1000.0 * self.sample_rate) as usize;
        self.predelay_len = pre
            .max(1)
            .min(self.predelay[0].len().saturating_sub(1).max(1));
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if !self.enabled && self.mix.is_silent() {
            return;
        }
        if !self.enabled {
            self.mix.set_target(0.0);
        }

        for i in 0..frames {
            let fb = self.feedback.next();
            let damp = self.damping.next();
            let mix = self.mix.next();
            let width = self.width.next();

            let dry_l = buf[0][i];
            let dry_r = buf[1][i];

            // Pre-delay, then feed the tank with the mono sum.
            let plen = self.predelay[0].len();
            let read = (self.predelay_pos + plen - self.predelay_len) % plen;
            let pre_l = self.predelay[0][read];
            let pre_r = self.predelay[1][read];
            self.predelay[0][self.predelay_pos] = dry_l;
            self.predelay[1][self.predelay_pos] = dry_r;
            self.predelay_pos = (self.predelay_pos + 1) % plen;

            let input = (pre_l + pre_r) * 0.015;

            let mut wet = [0.0f32; CHANNELS];
            for ch in 0..CHANNELS {
                let mut acc = 0.0;
                for comb in self.combs[ch].iter_mut() {
                    acc += comb.process(input, fb, damp);
                }
                for ap in self.allpasses[ch].iter_mut() {
                    acc = ap.process(acc);
                }
                wet[ch] = acc;
            }

            // Collapse the tail toward mono as width drops.
            let mid = (wet[0] + wet[1]) * 0.5;
            let wet_l = mid + (wet[0] - mid) * width;
            let wet_r = mid + (wet[1] - mid) * width;

            buf[0][i] = dry_l * (1.0 - mix) + wet_l * mix;
            buf[1][i] = dry_r * (1.0 - mix) + wet_r * mix;
        }
    }
}

// ---------------------------------------------------------------------------
// Lo-fi decimator / bit crusher
// ---------------------------------------------------------------------------

pub struct LofiFx {
    phase: f32,
    held: [f32; CHANNELS],
    step: f32,
    levels: f32,
    mix: Smoothed,
    enabled: bool,
    sample_rate: f32,
}

impl LofiFx {
    pub fn new() -> Self {
        LofiFx {
            phase: 0.0,
            held: [0.0; CHANNELS],
            step: 1.0,
            levels: 65536.0,
            mix: Smoothed::new(0.0),
            enabled: false,
            sample_rate: 48000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.mix.prepare(sample_rate, 20.0);
        self.phase = 0.0;
    }

    pub fn update(&mut self, l: &crate::audio::params::Lofi) {
        self.enabled = l.enabled;
        let target = l.sample_rate_hz.clamp(500.0, self.sample_rate);
        // How many held samples per output sample.
        self.step = target / self.sample_rate;
        let bits = l.bit_depth.clamp(1.0, 24.0);
        self.levels = 2.0f32.powf(bits) - 1.0;
        self.mix.set_target(l.mix.clamp(0.0, 1.0));
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if !self.enabled && self.mix.is_silent() {
            return;
        }
        if !self.enabled {
            self.mix.set_target(0.0);
        }

        for i in 0..frames {
            let mix = self.mix.next();
            self.phase += self.step;
            let resample = self.phase >= 1.0;
            if resample {
                self.phase -= self.phase.floor();
            }
            for ch in 0..CHANNELS {
                let dry = buf[ch][i];
                if resample {
                    // Sample-and-hold, then quantise to the chosen bit depth.
                    self.held[ch] = (dry.clamp(-1.0, 1.0) * self.levels).round() / self.levels;
                }
                buf[ch][i] = dry * (1.0 - mix) + self.held[ch] * mix;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Look-ahead limiter
// ---------------------------------------------------------------------------

pub struct Limiter {
    delay: [Vec<f32>; CHANNELS],
    pos: usize,
    lookahead: usize,
    envelope: f32,
    release_coeff: f32,
    ceiling: f32,
    enabled: bool,
    /// Peak gain reduction seen since the last poll, for the UI meter.
    reduction_db: f32,
}

impl Limiter {
    const LOOKAHEAD_MS: f32 = 5.0;

    pub fn new() -> Self {
        Limiter {
            delay: [Vec::new(), Vec::new()],
            pos: 0,
            lookahead: 0,
            envelope: 1.0,
            release_coeff: 0.999,
            ceiling: 1.0,
            enabled: true,
            reduction_db: 0.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.lookahead = ((Self::LOOKAHEAD_MS / 1000.0) * sample_rate) as usize;
        let len = self.lookahead.max(1) + 1;
        for ch in 0..CHANNELS {
            self.delay[ch] = vec![0.0; len];
        }
        self.pos = 0;
        self.envelope = 1.0;
        self.set_release(120.0, sample_rate);
    }

    fn set_release(&mut self, release_ms: f32, sample_rate: f32) {
        let tau = (release_ms / 1000.0).max(1e-4) * sample_rate;
        self.release_coeff = (-1.0 / tau).exp();
    }

    pub fn update(&mut self, n: &crate::audio::params::Normalisation, sample_rate: f32) {
        self.enabled = n.limiter_enabled;
        self.ceiling = db_to_gain(n.limiter_ceiling_db.clamp(-24.0, 0.0));
        self.set_release(n.limiter_release_ms.clamp(5.0, 2000.0), sample_rate);
    }

    /// Peak gain reduction in dB since the last call, then resets.
    pub fn take_reduction_db(&mut self) -> f32 {
        std::mem::replace(&mut self.reduction_db, 0.0)
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        let len = self.delay[0].len();
        for i in 0..frames {
            let peak = buf[0][i].abs().max(buf[1][i].abs());

            // Instant attack on a new peak, exponential release afterwards.
            let needed = if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            };
            if needed < self.envelope {
                self.envelope = needed;
            } else {
                self.envelope = needed + (self.envelope - needed) * self.release_coeff;
            }

            // Delay the audio by the look-ahead so the gain curve arrives first.
            let read = (self.pos + len - self.lookahead.min(len - 1)) % len;
            for ch in 0..CHANNELS {
                let delayed = self.delay[ch][read];
                self.delay[ch][self.pos] = buf[ch][i];
                buf[ch][i] = if self.enabled {
                    (delayed * self.envelope).clamp(-1.0, 1.0)
                } else {
                    delayed
                };
            }
            self.pos = (self.pos + 1) % len;

            if self.enabled {
                let red = -20.0 * self.envelope.max(1e-6).log10();
                if red > self.reduction_db {
                    self.reduction_db = red;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Chain
// ---------------------------------------------------------------------------

/// The ordered effect chain applied to one voice's music signal.
///
/// The limiter is deliberately not a member of `Chain`: during a crossfade two
/// `Chain`s run at once, and a limiter on each side would let the sum of two
/// already-ceilinged signals clip. There is exactly one limiter, owned by the
/// engine as the last stage of the master bus, after voices are summed.
pub struct Chain {
    pub eq: EqChain,
    pub delay: DelayFx,
    pub reverb: ReverbFx,
    pub lofi: LofiFx,
    gain: Smoothed,
    bypassed: bool,
}

impl Chain {
    pub fn new() -> Self {
        Chain {
            eq: EqChain::new(),
            delay: DelayFx::new(),
            reverb: ReverbFx::new(),
            lofi: LofiFx::new(),
            gain: Smoothed::new(1.0),
            bypassed: false,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.eq.prepare(sample_rate);
        self.delay.prepare(sample_rate);
        self.reverb.prepare(sample_rate);
        self.lofi.prepare(sample_rate);
        self.gain.prepare(sample_rate, 25.0);
    }

    /// `track_gain_db` is the per-track normalisation gain worked out from
    /// tags or analysis; it is folded into the master gain here.
    pub fn update(&mut self, settings: &Resolved, track_gain_db: f32) {
        self.bypassed = !settings.enabled;
        if self.bypassed {
            self.gain.set_target(1.0);
            return;
        }

        self.eq.update(&settings.eq);
        self.delay.update(&settings.delay);
        self.reverb.update(&settings.reverb);
        self.lofi.update(&settings.lofi);

        let norm_db = if settings.normalisation.enabled {
            track_gain_db + settings.normalisation.gain_db
        } else {
            settings.normalisation.gain_db
        };
        self.gain.set_target(db_to_gain(norm_db.clamp(-24.0, 24.0)));
    }

    /// Music effects. The ambience bed is mixed in by the caller between this
    /// and [`Chain::apply_gain`], so beds are not coloured by the track's EQ.
    pub fn process_music(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        if self.bypassed {
            return;
        }
        self.eq.process(buf, frames);
        self.delay.process(buf, frames);
        self.reverb.process(buf, frames);
        self.lofi.process(buf, frames);
    }

    /// This voice's own gain ramp (normalisation plus any manual trim).
    ///
    /// Not the limiter: that runs once, on the master bus, after every voice
    /// has been summed. A caller mixing a single voice with nothing else on
    /// the bus still needs to run the master limiter afterwards.
    pub fn apply_gain(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        for i in 0..frames {
            let g = self.gain.next();
            for ch in 0..CHANNELS {
                buf[ch][i] *= g;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::params::{Eq, Lofi, Normalisation};

    fn silence(frames: usize) -> Vec<Vec<f32>> {
        vec![vec![0.0; frames]; CHANNELS]
    }

    #[test]
    fn flat_eq_passes_signal_through() {
        let mut eq = EqChain::new();
        eq.prepare(48000.0);
        eq.update(&Eq::default());
        let mut buf = silence(256);
        for i in 0..256 {
            buf[0][i] = 0.5;
            buf[1][i] = 0.5;
        }
        eq.process(&mut buf, 256);
        // Allow for the preamp glide over the first samples.
        assert!((buf[0][255] - 0.5).abs() < 0.01);
    }

    #[test]
    fn limiter_holds_below_the_ceiling() {
        let mut lim = Limiter::new();
        lim.prepare(48000.0);
        lim.update(
            &Normalisation {
                limiter_enabled: true,
                limiter_ceiling_db: -6.0,
                ..Default::default()
            },
            48000.0,
        );
        let mut buf = vec![vec![4.0f32; 4096]; CHANNELS];
        lim.process(&mut buf, 4096);
        let ceiling = db_to_gain(-6.0);
        // Skip the look-ahead priming region.
        let worst = buf[0][1000..].iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(
            worst <= ceiling + 1e-3,
            "peak {worst} exceeded ceiling {ceiling}"
        );
    }

    #[test]
    fn disabled_lofi_is_transparent() {
        let mut lofi = LofiFx::new();
        lofi.prepare(48000.0);
        lofi.update(&Lofi {
            enabled: false,
            ..Default::default()
        });
        let mut buf = vec![vec![0.3f32; 128]; CHANNELS];
        lofi.process(&mut buf, 128);
        assert!((buf[0][127] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn smoothed_reaches_its_target() {
        let mut s = Smoothed::new(0.0);
        s.prepare(48000.0, 5.0);
        s.set_target(1.0);
        for _ in 0..48000 {
            s.next();
        }
        assert!((s.next() - 1.0).abs() < 1e-3);
    }
}

#[cfg(test)]
mod fade_tests {
    use super::*;
    use crate::audio::params::{Delay, Reverb};

    /// Switching an effect off must eventually let its `process` short-circuit.
    /// The glide is exponential, so this only holds if the "is it done" test
    /// uses a threshold rather than comparing against exactly zero.
    #[test]
    fn a_disabled_delay_stops_processing_once_it_has_faded() {
        let mut delay = DelayFx::new();
        delay.prepare(48000.0);
        delay.update(&Delay {
            enabled: true,
            mix: 0.5,
            ..Default::default()
        });

        let mut buf = vec![vec![0.2f32; 512]; CHANNELS];
        delay.process(&mut buf, 512);

        delay.update(&Delay {
            enabled: false,
            ..Default::default()
        });
        // A second of audio is far longer than the 20 ms mix glide.
        for _ in 0..100 {
            delay.process(&mut buf, 512);
        }
        assert!(
            delay.mix.is_silent(),
            "mix never reached the silent threshold"
        );

        // Once silent it must pass audio through untouched.
        let mut probe = vec![vec![0.7f32; 64]; CHANNELS];
        delay.process(&mut probe, 64);
        assert!(
            (probe[0][63] - 0.7).abs() < 1e-6,
            "a faded-out delay still coloured the signal"
        );
    }

    #[test]
    fn a_disabled_reverb_stops_processing_once_it_has_faded() {
        let mut reverb = ReverbFx::new();
        reverb.prepare(48000.0);
        reverb.update(&Reverb {
            enabled: true,
            mix: 0.6,
            ..Default::default()
        });

        let mut buf = vec![vec![0.2f32; 512]; CHANNELS];
        reverb.process(&mut buf, 512);

        reverb.update(&Reverb {
            enabled: false,
            ..Default::default()
        });
        for _ in 0..100 {
            reverb.process(&mut buf, 512);
        }
        assert!(reverb.mix.is_silent());

        let mut probe = vec![vec![0.7f32; 64]; CHANNELS];
        reverb.process(&mut probe, 64);
        assert!(
            (probe[0][63] - 0.7).abs() < 1e-6,
            "a faded-out reverb still coloured the signal"
        );
    }

    #[test]
    fn a_reverb_updated_before_prepare_does_not_panic() {
        // Guards against the predelay buffer being empty.
        let mut reverb = ReverbFx::new();
        reverb.update(&Reverb::default());
    }
}
