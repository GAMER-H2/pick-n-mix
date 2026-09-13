//! Stereo peak metering for the processed master bus.
//!
//! The worker feeds this immediately after the limiter and before the player
//! volume/fade in the output callback. Processing is allocation-free; callers
//! publish the small copied frame only when [`OutputMeter::push`] or
//! [`OutputMeter::decay`] says the display interval has elapsed.

use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use serde::Serialize;

pub const FLOOR_DB: f32 = -60.0;
const RELEASE_SECS: f32 = 0.30;
const PEAK_HOLD_SECS: f32 = 1.25;
const PEAK_FALL_DB_PER_SEC: f32 = 24.0;
const PUBLISH_HZ: f32 = 50.0;

/// One ready-to-draw frame of the post-limiter stereo bus.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLevelFrame {
    /// Fast-attack, smoothly released peak level for left and right, in dBFS.
    pub levels_db: [f32; 2],
    /// Held peak marker for left and right, in dBFS.
    pub peaks_db: [f32; 2],
    pub floor_db: f32,
}

impl OutputLevelFrame {
    pub fn silence() -> Self {
        Self {
            levels_db: [FLOOR_DB; 2],
            peaks_db: [FLOOR_DB; 2],
            floor_db: FLOOR_DB,
        }
    }
}

/// Ballistics for a conventional stereo peak meter.
pub struct OutputMeter {
    frame: OutputLevelFrame,
    peak_hold_remaining: [f32; 2],
    publish_elapsed: f32,
}

impl OutputMeter {
    pub fn new() -> Self {
        Self {
            frame: OutputLevelFrame::silence(),
            peak_hold_remaining: [0.0; 2],
            publish_elapsed: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.frame = OutputLevelFrame::silence();
        self.peak_hold_remaining = [0.0; 2];
        self.publish_elapsed = 0.0;
    }

    /// Process one planar stereo block. Returns true at the publication rate.
    pub fn push(&mut self, mix: &[Vec<f32>], frames: usize, sample_rate: f32) -> bool {
        if frames == 0 || sample_rate <= 0.0 {
            return false;
        }

        let mut block_peaks = [FLOOR_DB; 2];
        for (channel, peak_db) in block_peaks.iter_mut().enumerate() {
            let amplitude = mix
                .get(channel)
                .map(|samples| {
                    samples
                        .iter()
                        .take(frames)
                        .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
                })
                .unwrap_or(0.0);
            *peak_db = amplitude_to_db(amplitude);
        }

        self.advance(block_peaks, frames as f32 / sample_rate)
    }

    /// Let levels and held peaks fall while no blocks are being rendered.
    pub fn decay(&mut self, elapsed: Duration) -> bool {
        self.advance([FLOOR_DB; 2], elapsed.as_secs_f32())
    }

    pub fn frame(&self) -> OutputLevelFrame {
        self.frame
    }

    pub fn is_silent(&self) -> bool {
        self.frame
            .levels_db
            .iter()
            .chain(self.frame.peaks_db.iter())
            .all(|level| *level <= FLOOR_DB + 0.05)
    }

    fn advance(&mut self, input_db: [f32; 2], elapsed: f32) -> bool {
        if elapsed <= 0.0 {
            return false;
        }

        // Instant attack preserves transients; the exponential release is
        // time-based so it behaves identically for active blocks and idle ticks.
        let release = (-elapsed / RELEASE_SECS).exp();
        for channel in 0..2 {
            let input = input_db[channel].max(FLOOR_DB);
            if input >= self.frame.levels_db[channel] {
                self.frame.levels_db[channel] = input;
            } else {
                self.frame.levels_db[channel] =
                    input + (self.frame.levels_db[channel] - input) * release;
            }

            if input >= self.frame.peaks_db[channel] {
                self.frame.peaks_db[channel] = input;
                self.peak_hold_remaining[channel] = PEAK_HOLD_SECS;
            } else {
                let held_for = self.peak_hold_remaining[channel].min(elapsed);
                self.peak_hold_remaining[channel] -= held_for;
                let falling_for = elapsed - held_for;
                if falling_for > 0.0 {
                    self.frame.peaks_db[channel] = (self.frame.peaks_db[channel]
                        - PEAK_FALL_DB_PER_SEC * falling_for)
                        .max(self.frame.levels_db[channel])
                        .max(FLOOR_DB);
                }
            }
        }

        self.publish_elapsed += elapsed;
        if self.publish_elapsed >= 1.0 / PUBLISH_HZ {
            self.publish_elapsed %= 1.0 / PUBLISH_HZ;
            true
        } else {
            false
        }
    }
}

/// Every meter in one block's effect rack: what went into the chain, then the
/// output of each stage in turn. `stages` is therefore one longer than the
/// number of effects, which is what puts a meter between each pair of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainLevelFrame {
    /// The block these readings belong to, so a frame that arrives after the
    /// selection has moved on can be ignored rather than drawn against the
    /// wrong rack.
    pub block_id: String,
    pub stages: Vec<OutputLevelFrame>,
}

/// The meters for one chosen block, and the way the editor asks for them.
///
/// The master mixer picks a block; the timeline meters that block's chain as
/// it renders it and publishes here. Nothing is measured while no block is
/// chosen, so a rack that is closed costs nothing.
pub struct ChainMeterBus {
    block: ArcSwap<Option<String>>,
    frame: ArcSwap<ChainLevelFrame>,
}

impl ChainMeterBus {
    pub fn new() -> Self {
        Self {
            block: ArcSwap::from_pointee(None),
            frame: ArcSwap::from_pointee(ChainLevelFrame::default()),
        }
    }

    /// Choose the block to meter, or `None` to stop metering.
    pub fn select(&self, block_id: Option<String>) {
        let changed = self.block.load().as_ref().as_deref() != block_id.as_deref();
        self.block.store(Arc::new(block_id.clone()));
        if changed {
            // Nothing has been measured for the new block yet, and the old
            // block's levels are not its levels.
            self.frame.store(Arc::new(ChainLevelFrame {
                block_id: block_id.unwrap_or_default(),
                stages: Vec::new(),
            }));
        }
    }

    pub fn selected(&self) -> Option<String> {
        self.block.load().as_ref().clone()
    }

    /// Whether this block is the one being metered.
    pub fn wants(&self, block_id: &str) -> bool {
        self.block
            .load()
            .as_ref()
            .as_deref()
            .is_some_and(|chosen| chosen == block_id)
    }

    pub fn publish(&self, frame: ChainLevelFrame) {
        self.frame.store(Arc::new(frame));
    }

    pub fn frame(&self) -> ChainLevelFrame {
        self.frame.load().as_ref().clone()
    }
}

impl Default for ChainMeterBus {
    fn default() -> Self {
        Self::new()
    }
}

/// One [`OutputMeter`] per tap point in a chain, with the same ballistics as
/// the master meter so a level read here means what it means over there.
pub struct ChainMeters {
    stages: Vec<OutputMeter>,
}

impl ChainMeters {
    pub fn new() -> Self {
        Self { stages: Vec::new() }
    }

    pub fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.reset();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// Feed the signal at tap `index`. Returns true when the whole set is due
    /// to be published, which the first tap decides for all of them so the
    /// stages of one frame are always from the same block of audio.
    pub fn push(
        &mut self,
        index: usize,
        mix: &[Vec<f32>],
        frames: usize,
        sample_rate: f32,
    ) -> bool {
        while self.stages.len() <= index {
            self.stages.push(OutputMeter::new());
        }
        let due = self.stages[index].push(mix, frames, sample_rate);
        due && index == 0
    }

    /// Let every stage fall while the block is not sounding.
    pub fn decay(&mut self, elapsed: Duration) -> bool {
        let mut due = false;
        for (index, stage) in self.stages.iter_mut().enumerate() {
            let published = stage.decay(elapsed);
            if index == 0 {
                due = published;
            }
        }
        due
    }

    pub fn is_silent(&self) -> bool {
        self.stages.iter().all(|stage| stage.is_silent())
    }

    pub fn frames(&self) -> Vec<OutputLevelFrame> {
        self.stages.iter().map(|stage| stage.frame()).collect()
    }
}

impl Default for ChainMeters {
    fn default() -> Self {
        Self::new()
    }
}

fn amplitude_to_db(amplitude: f32) -> f32 {
    if amplitude <= 0.0 {
        FLOOR_DB
    } else {
        (20.0 * amplitude.log10()).max(FLOOR_DB)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: f32 = 48_000.0;

    fn block(left: f32, right: f32, frames: usize) -> Vec<Vec<f32>> {
        vec![vec![left; frames], vec![right; frames]]
    }

    #[test]
    fn channels_are_measured_independently_in_dbfs() {
        let mut meter = OutputMeter::new();
        meter.push(&block(1.0, 0.5, 1024), 1024, RATE);
        let frame = meter.frame();

        assert!(frame.levels_db[0].abs() < 0.001);
        assert!((frame.levels_db[1] + 6.0206).abs() < 0.01);
        assert_eq!(frame.peaks_db, frame.levels_db);
    }

    #[test]
    fn a_signal_in_one_channel_does_not_light_the_other() {
        let mut meter = OutputMeter::new();
        meter.push(&block(0.25, 0.0, 1024), 1024, RATE);

        assert!((meter.frame().levels_db[0] + 12.0412).abs() < 0.01);
        assert_eq!(meter.frame().levels_db[1], FLOOR_DB);
    }

    #[test]
    fn levels_release_smoothly_to_the_floor() {
        let mut meter = OutputMeter::new();
        meter.push(&block(1.0, 1.0, 1024), 1024, RATE);
        meter.decay(Duration::from_millis(300));
        let after_one_release = meter.frame().levels_db[0];

        assert!(after_one_release < -30.0 && after_one_release > FLOOR_DB);
        meter.decay(Duration::from_secs(3));
        assert!(meter.frame().levels_db[0] < FLOOR_DB + 0.01);
    }

    #[test]
    fn peak_is_held_then_falls_without_crossing_the_live_level() {
        let mut meter = OutputMeter::new();
        meter.push(&block(1.0, 1.0, 1024), 1024, RATE);
        meter.decay(Duration::from_secs(1));
        assert!(meter.frame().peaks_db[0].abs() < 0.001);

        meter.decay(Duration::from_millis(300));
        let falling = meter.frame();
        assert!(falling.peaks_db[0] < 0.0);
        assert!(falling.peaks_db[0] >= falling.levels_db[0]);
    }

    #[test]
    fn reset_clears_levels_peaks_and_publication_clock() {
        let mut meter = OutputMeter::new();
        assert!(meter.push(&block(1.0, 1.0, 1024), 1024, RATE));
        meter.reset();

        assert_eq!(meter.frame(), OutputLevelFrame::silence());
        assert!(meter.is_silent());
        assert!(!meter.push(&block(0.0, 0.0, 512), 512, RATE));
    }

    #[test]
    fn chain_meters_grow_to_fit_the_taps_they_are_given() {
        let mut meters = ChainMeters::new();
        assert!(meters.is_empty());
        for index in 0..4 {
            meters.push(index, &block(0.5, 0.5, 512), 512, RATE);
        }
        assert_eq!(meters.frames().len(), 4);
    }

    /// Every stage of one published frame has to come from the same block of
    /// audio, or the meters either side of an effect would be comparing
    /// different moments. The first tap decides for all of them.
    #[test]
    fn only_the_first_tap_calls_a_frame_due() {
        let mut meters = ChainMeters::new();
        let signal = block(0.5, 0.5, 512);
        // The first push of each stage is short of the publication interval.
        assert!(!meters.push(0, &signal, 512, RATE));
        assert!(!meters.push(1, &signal, 512, RATE));
        assert!(meters.push(0, &signal, 512, RATE));
        assert!(!meters.push(1, &signal, 512, RATE));
    }

    #[test]
    fn chain_meters_fall_to_the_floor_when_the_block_stops_sounding() {
        let mut meters = ChainMeters::new();
        meters.push(0, &block(1.0, 1.0, 512), 512, RATE);
        assert!(!meters.is_silent());

        meters.decay(Duration::from_secs(4));
        assert!(meters.is_silent());
    }

    /// A frame left over from the block that was selected a moment ago would
    /// be drawn against the new block's devices, which is worse than no frame.
    #[test]
    fn choosing_another_block_drops_the_last_one_s_levels() {
        let bus = ChainMeterBus::new();
        bus.select(Some("blk-1".into()));
        bus.publish(ChainLevelFrame {
            block_id: "blk-1".into(),
            stages: vec![OutputLevelFrame::silence(); 3],
        });
        assert_eq!(bus.frame().stages.len(), 3);

        // Re-selecting the same block leaves the readings in place.
        bus.select(Some("blk-1".into()));
        assert_eq!(bus.frame().stages.len(), 3);

        bus.select(Some("blk-2".into()));
        assert!(bus.frame().stages.is_empty());
        assert_eq!(bus.frame().block_id, "blk-2");
    }

    #[test]
    fn only_the_chosen_block_is_metered() {
        let bus = ChainMeterBus::new();
        assert!(!bus.wants("blk-1"));

        bus.select(Some("blk-1".into()));
        assert!(bus.wants("blk-1"));
        assert!(!bus.wants("blk-2"));

        bus.select(None);
        assert!(!bus.wants("blk-1"));
        assert_eq!(bus.selected(), None);
    }

    #[test]
    fn publication_is_throttled_to_about_fifty_hertz() {
        let mut meter = OutputMeter::new();
        assert!(!meter.push(&block(0.5, 0.5, 512), 512, RATE));
        assert!(meter.push(&block(0.5, 0.5, 512), 512, RATE));
    }
}
