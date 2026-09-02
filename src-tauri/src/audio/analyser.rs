//! Spectrum analysis of the processed output, for the EQ graph.
//!
//! The tap is on the master bus *after* the effect chain and limiter, so what
//! the graph draws is what is actually leaving the app — move an EQ band and
//! the spectrum moves with it, which is the whole point of drawing it behind
//! the EQ curve.
//!
//! The FFT is written out here rather than pulled in as a dependency: only one
//! fixed power-of-two size is ever needed, and a radix-2 Cooley-Tukey is short
//! enough to test directly. The reduction to log-spaced bins, the dB
//! conversion and the ballistics all happen on this side too, so the UI
//! receives a few dozen ready-to-draw numbers instead of a raw spectrum.

use std::f32::consts::PI;

/// Window length. At 48 kHz this is a ~43 ms window and ~23 Hz per bin, which
/// is the coarsest resolution that still separates anything useful in the
/// bottom two octaves.
pub const FFT_SIZE: usize = 2048;

/// How many new samples between recomputes: ~21 ms, or about 47 frames a
/// second, which is finer than the UI can draw anyway.
const HOP: usize = FFT_SIZE / 2;

/// Display bins handed to the UI. Log-spaced, so this is a resolution choice
/// about the *drawing*, not about the FFT.
pub const BINS: usize = 96;

pub const MIN_HZ: f32 = 20.0;
pub const MAX_HZ: f32 = 20_000.0;
/// Anything quieter than this reads as silence and is clamped, which also
/// stops `log10(0)` from producing `-inf`.
pub const FLOOR_DB: f32 = -90.0;

/// Fraction of the way to a *louder* reading each frame. Fast, so transients
/// are not swallowed.
const ATTACK: f32 = 0.55;
/// Fraction of the way to a *quieter* reading each frame. Slow, so the display
/// falls away smoothly rather than flickering between blocks.
const RELEASE: f32 = 0.12;

/// In-place iterative radix-2 FFT with precomputed twiddles and bit-reversal.
struct Fft {
    size: usize,
    cos: Vec<f32>,
    sin: Vec<f32>,
    rev: Vec<u32>,
}

impl Fft {
    fn new(size: usize) -> Self {
        assert!(size.is_power_of_two(), "radix-2 needs a power of two");
        let half = size / 2;
        let mut cos = Vec::with_capacity(half);
        let mut sin = Vec::with_capacity(half);
        for i in 0..half {
            let angle = -2.0 * PI * i as f32 / size as f32;
            cos.push(angle.cos());
            sin.push(angle.sin());
        }

        let bits = size.trailing_zeros();
        let rev = (0..size)
            .map(|i| (i as u32).reverse_bits() >> (32 - bits))
            .collect();

        Fft {
            size,
            cos,
            sin,
            rev,
        }
    }

    /// Forward transform of `re`/`im`, both of length `size`.
    fn forward(&self, re: &mut [f32], im: &mut [f32]) {
        debug_assert_eq!(re.len(), self.size);
        debug_assert_eq!(im.len(), self.size);

        for i in 0..self.size {
            let j = self.rev[i] as usize;
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }

        let mut len = 2;
        while len <= self.size {
            let step = self.size / len;
            let half = len / 2;
            for start in (0..self.size).step_by(len) {
                for k in 0..half {
                    let tw = k * step;
                    let (wr, wi) = (self.cos[tw], self.sin[tw]);
                    let i = start + k;
                    let j = i + half;
                    let tr = re[j] * wr - im[j] * wi;
                    let ti = re[j] * wi + im[j] * wr;
                    re[j] = re[i] - tr;
                    im[j] = im[i] - ti;
                    re[i] += tr;
                    im[i] += ti;
                }
            }
            len <<= 1;
        }
    }
}

pub struct Analyser {
    fft: Fft,
    /// Hann, to stop a tone that does not land exactly on a bin from smearing
    /// across the whole spectrum.
    window: Vec<f32>,
    /// The last `FFT_SIZE` mono samples, as a ring.
    history: Vec<f32>,
    write: usize,
    filled: usize,
    since_last: usize,

    re: Vec<f32>,
    im: Vec<f32>,

    /// First and last FFT bin feeding each display bin.
    ranges: Vec<(usize, usize)>,
    /// Smoothed output in dBFS, what the UI is handed.
    bins: Vec<f32>,
    /// Reciprocal of the normalisation that puts a full-scale sine at 0 dBFS.
    scale: f32,
}

impl Analyser {
    pub fn new(sample_rate: f32) -> Self {
        let fft = Fft::new(FFT_SIZE);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f32 / FFT_SIZE as f32).cos())
            .collect();

        // A Hann window has a coherent gain of 0.5, and a real FFT splits a
        // sine's energy across two mirrored bins, so a full-scale sine peaks
        // at `FFT_SIZE / 4`. Dividing that out puts it at exactly 0 dBFS.
        let scale = 4.0 / FFT_SIZE as f32;

        let nyquist_bin = FFT_SIZE / 2;
        let ranges = (0..BINS)
            .map(|i| {
                // Edges of this bin in log-frequency space.
                let lo_hz = log_edge(i as f32 / BINS as f32);
                let hi_hz = log_edge((i + 1) as f32 / BINS as f32);
                let to_bin =
                    |hz: f32| ((hz / sample_rate) * FFT_SIZE as f32).round().max(1.0) as usize;
                let lo = to_bin(lo_hz).min(nyquist_bin - 1);
                // Always at least one FFT bin wide: at the bottom of the range
                // several display bins fall inside a single FFT bin, and an
                // empty range would read as silence.
                let hi = to_bin(hi_hz).max(lo + 1).min(nyquist_bin);
                (lo, hi)
            })
            .collect();

        Analyser {
            fft,
            window,
            history: vec![0.0; FFT_SIZE],
            write: 0,
            filled: 0,
            since_last: 0,
            re: vec![0.0; FFT_SIZE],
            im: vec![0.0; FFT_SIZE],
            ranges,
            bins: vec![FLOOR_DB; BINS],
            scale,
        }
    }

    /// Feed one processed block. Returns true when the spectrum was recomputed.
    pub fn push(&mut self, mix: &[Vec<f32>], frames: usize) -> bool {
        let channels = mix.len().max(1);
        for f in 0..frames {
            let mut sum = 0.0;
            for ch in mix.iter() {
                sum += ch[f];
            }
            self.history[self.write] = sum / channels as f32;
            self.write = (self.write + 1) % FFT_SIZE;
            self.filled = (self.filled + 1).min(FFT_SIZE);
            self.since_last += 1;
        }

        if self.since_last < HOP || self.filled < FFT_SIZE {
            return false;
        }
        self.since_last = 0;
        self.compute();
        true
    }

    fn compute(&mut self) {
        // Unwrap the ring oldest-first as the window is applied.
        for i in 0..FFT_SIZE {
            let sample = self.history[(self.write + i) % FFT_SIZE];
            self.re[i] = sample * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.forward(&mut self.re, &mut self.im);

        for (i, &(lo, hi)) in self.ranges.iter().enumerate() {
            // Peak rather than mean across the range: a narrow tone should
            // stay at its true height instead of being averaged down by the
            // quiet bins beside it.
            let mut peak = 0.0f32;
            for b in lo..hi {
                let mag = (self.re[b] * self.re[b] + self.im[b] * self.im[b]).sqrt();
                peak = peak.max(mag);
            }
            let db = amplitude_to_db(peak * self.scale);
            let coeff = if db > self.bins[i] { ATTACK } else { RELEASE };
            self.bins[i] += (db - self.bins[i]) * coeff;
        }
    }

    /// Let the display fall away while nothing is playing, instead of leaving
    /// the last spectrum frozen on screen.
    pub fn decay(&mut self) {
        for bin in self.bins.iter_mut() {
            *bin += (FLOOR_DB - *bin) * RELEASE;
        }
    }

    pub fn bins(&self) -> &[f32] {
        &self.bins
    }

    pub fn is_silent(&self) -> bool {
        self.bins.iter().all(|&db| db <= FLOOR_DB + 0.5)
    }
}

/// Frequency at `t` along the display's log axis, `t` in 0..=1.
fn log_edge(t: f32) -> f32 {
    let lo = MIN_HZ.log10();
    let hi = MAX_HZ.log10();
    10f32.powf(lo + (hi - lo) * t)
}

fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        return FLOOR_DB;
    }
    (20.0 * amplitude.log10()).max(FLOOR_DB)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fill the analyser with a steady tone and settle the ballistics.
    fn feed_tone(analyser: &mut Analyser, freq: f32, rate: f32, amplitude: f32) {
        let block = 512;
        // Enough blocks for the ring to fill and the smoothing to converge.
        let mut phase = 0.0f32;
        for _ in 0..80 {
            let mut mix = vec![vec![0.0f32; block]; 2];
            for f in 0..block {
                let s = amplitude * (2.0 * PI * phase).sin();
                phase = (phase + freq / rate).fract();
                mix[0][f] = s;
                mix[1][f] = s;
            }
            analyser.push(&mix, block);
        }
    }

    fn bin_for(freq: f32) -> usize {
        let lo = MIN_HZ.log10();
        let hi = MAX_HZ.log10();
        (((freq.log10() - lo) / (hi - lo)) * BINS as f32) as usize
    }

    #[test]
    fn a_dc_signal_transforms_to_a_single_spike() {
        let fft = Fft::new(8);
        let mut re = vec![1.0f32; 8];
        let mut im = vec![0.0f32; 8];
        fft.forward(&mut re, &mut im);

        assert!((re[0] - 8.0).abs() < 1e-4, "DC bin was {}", re[0]);
        for b in 1..8 {
            let mag = (re[b] * re[b] + im[b] * im[b]).sqrt();
            assert!(mag < 1e-4, "bin {b} should be empty, was {mag}");
        }
    }

    #[test]
    fn a_sine_lands_in_its_own_bin() {
        let size = 16;
        let fft = Fft::new(size);
        // Exactly two cycles across the window, so it falls on bin 2.
        let mut re: Vec<f32> = (0..size)
            .map(|i| (2.0 * PI * 2.0 * i as f32 / size as f32).sin())
            .collect();
        let mut im = vec![0.0f32; size];
        fft.forward(&mut re, &mut im);

        let mag = |b: usize| (re[b] * re[b] + im[b] * im[b]).sqrt();
        assert!(mag(2) > 7.0, "bin 2 was {}", mag(2));
        for b in [0usize, 1, 3, 4, 5, 6, 7] {
            assert!(mag(b) < 1e-3, "bin {b} leaked {}", mag(b));
        }
    }

    #[test]
    fn a_full_scale_tone_reads_near_zero_dbfs_in_its_own_band() {
        let rate = 48_000.0;
        let mut analyser = Analyser::new(rate);
        feed_tone(&mut analyser, 1000.0, rate, 1.0);

        let bin = bin_for(1000.0);
        let db = analyser.bins()[bin];
        assert!(
            db > -3.0 && db < 1.0,
            "1 kHz at full scale read {db} dBFS in bin {bin}",
        );
    }

    #[test]
    fn a_quieter_tone_reads_proportionally_lower() {
        let rate = 48_000.0;
        let mut loud = Analyser::new(rate);
        feed_tone(&mut loud, 1000.0, rate, 1.0);
        let mut quiet = Analyser::new(rate);
        // -20 dB.
        feed_tone(&mut quiet, 1000.0, rate, 0.1);

        let bin = bin_for(1000.0);
        let difference = loud.bins()[bin] - quiet.bins()[bin];
        assert!(
            (difference - 20.0).abs() < 2.0,
            "expected ~20 dB between them, got {difference}",
        );
    }

    #[test]
    fn a_tone_does_not_light_up_a_distant_band() {
        let rate = 48_000.0;
        let mut analyser = Analyser::new(rate);
        feed_tone(&mut analyser, 1000.0, rate, 1.0);

        let near = analyser.bins()[bin_for(1000.0)];
        let far = analyser.bins()[bin_for(8000.0)];
        assert!(far < near - 40.0, "1 kHz tone put {far} dBFS at 8 kHz");
    }

    #[test]
    fn silence_decays_to_the_floor() {
        let rate = 48_000.0;
        let mut analyser = Analyser::new(rate);
        feed_tone(&mut analyser, 1000.0, rate, 1.0);
        assert!(!analyser.is_silent());

        for _ in 0..400 {
            analyser.decay();
        }
        assert!(analyser.is_silent(), "spectrum never fell to the floor");
    }

    #[test]
    fn every_display_bin_covers_at_least_one_fft_bin() {
        let analyser = Analyser::new(48_000.0);
        for (i, &(lo, hi)) in analyser.ranges.iter().enumerate() {
            assert!(hi > lo, "display bin {i} is empty: {lo}..{hi}");
            assert!(hi <= FFT_SIZE / 2, "display bin {i} runs past Nyquist");
        }
    }

    /// The bottom of the range is where a log axis asks for more resolution
    /// than the FFT has, so it is worth pinning that the ranges stay ordered.
    #[test]
    fn bin_ranges_increase_across_the_spectrum() {
        let analyser = Analyser::new(48_000.0);
        let mut previous = 0;
        for &(lo, _) in analyser.ranges.iter() {
            assert!(lo >= previous, "ranges went backwards at {lo}");
            previous = lo;
        }
    }
}
