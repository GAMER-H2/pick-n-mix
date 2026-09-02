//! Playing a master mix.
//!
//! A [`TimelineSource`] looks to the engine exactly like a decoder: it is
//! asked for interleaved stereo at the device rate and hands it back. What it
//! does underneath is mix an arbitrary number of overlapping blocks, each with
//! its own file, its own position in the source, its own effect chain and its
//! own volume envelope.
//!
//! Two things make this safe to run on the DSP worker:
//!
//! * **Files are opened on another thread.** `TrackDecoder::open` can take
//!   hundreds of milliseconds on a cold disk, which the engine's short ring
//!   cannot absorb. A block is therefore requested [`LOOKAHEAD_SECS`] before it
//!   is due and arrives over a channel; if it is somehow late, it fades in from
//!   wherever the timeline has got to rather than dragging the mix backwards.
//! * **Everything is bounded.** At most [`MAX_VOICES`] blocks sound at once and
//!   the effect chains they use come from a pool, so a mix with two hundred
//!   blocks costs no more than a mix with eight.
//!
//! The same type is what a bounce will render through, driven as fast as the
//! CPU allows instead of in real time — which is the point of putting the
//! mixing here rather than in the worker loop: what you hear while editing and
//! what lands in the exported file come from one piece of code.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use crossbeam_channel::{unbounded, Receiver, Sender};

use crate::audio::decode::{StreamInfo, TrackDecoder};
use crate::audio::dsp::{Chain, CHANNELS};
use crate::audio::params::Resolved;
use crate::master_mix::Block;

/// Blocks that may sound simultaneously. Beyond this the quietest thing to do
/// is not start more: eight overlapping songs is already far past anything a
/// crossfade needs.
pub const MAX_VOICES: usize = 8;
/// How far ahead of a block's start its file is opened.
const LOOKAHEAD_SECS: f64 = 4.0;
/// A decoder that arrives late is nudged forward by reading and discarding,
/// up to this much; past it a seek is cheaper.
const MAX_CATCH_UP_SECS: f64 = 1.0;

/// One block, resolved to a file on this machine and a settled mixer cascade.
#[derive(Debug, Clone)]
pub struct PlanBlock {
    pub path: PathBuf,
    /// The block itself, kept whole so its fades and automation can be
    /// evaluated per frame.
    pub block: Block,
    /// The lane's gain, folded in as a constant. Mute and solo are resolved
    /// when the plan is built; silent lanes stay in the plan to preserve its
    /// duration, but their files are never opened.
    pub lane_gain: f32,
    pub settings: Arc<Resolved>,
    /// Replay-gain normalisation for the underlying file.
    pub track_gain_db: f32,
}

/// A whole master mix, ready to play.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    /// Sorted by start time, which is what makes the "what is due soon" scan
    /// in [`TimelineSource::request_openings`] a short walk rather than a
    /// sweep of every block.
    pub blocks: Vec<PlanBlock>,
    pub duration_secs: f64,
}

impl Plan {
    pub fn new(mut blocks: Vec<PlanBlock>) -> Self {
        blocks.sort_by(|a, b| a.block.start_secs.total_cmp(&b.block.start_secs));
        let duration_secs = blocks
            .iter()
            .map(|b| b.block.end_secs())
            .fold(0.0, f64::max);
        Plan {
            blocks,
            duration_secs,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }
}

struct OpenRequest {
    generation: u64,
    block_ix: usize,
    path: PathBuf,
    offset_secs: f64,
    into_block_secs: f64,
    rate: u32,
    /// The timeline frame this decoder's first sample belongs to.
    enter_frame: u64,
}

struct Opened {
    generation: u64,
    block_ix: usize,
    enter_frame: u64,
    decoder: Option<Box<TrackDecoder>>,
}

/// A block currently sounding.
struct Active {
    block_ix: usize,
    decoder: Box<TrackDecoder>,
    chain_ix: usize,
    /// Timeline frame the decoder's next sample belongs to.
    next_frame: u64,
}

pub struct TimelineSource {
    plan: Arc<Plan>,
    rate: u32,
    total_frames: u64,
    /// Next timeline frame to render.
    cursor: u64,
    active: Vec<Active>,
    /// Blocks whose files have been asked for but have not arrived.
    pending: HashSet<usize>,
    /// Blocks already dealt with at the current cursor, so a block is never
    /// reopened after it has played out. Cleared by a seek.
    finished: HashSet<usize>,
    /// Incremented on every seek so replies for the old position are dropped
    /// instead of being mixed in at the wrong place.
    generation: u64,
    open_tx: Option<Sender<OpenRequest>>,
    ready_rx: Receiver<Opened>,
    /// Effect chains, lent to active blocks and reset on return. A `Chain`
    /// owns delay lines and reverb combs, so they are worth reusing.
    chains: Vec<Chain>,
    free_chains: Vec<usize>,
    interleaved: Vec<f32>,
    planar: Vec<Vec<f32>>,
    mix: Vec<Vec<f32>>,
    info: StreamInfo,
}

impl TimelineSource {
    pub fn new(plan: Plan, rate: u32) -> Self {
        let plan = Arc::new(plan);
        let (open_tx, open_rx) = unbounded::<OpenRequest>();
        let (ready_tx, ready_rx) = unbounded::<Opened>();

        // Dies when the source is dropped and `open_tx` with it.
        std::thread::Builder::new()
            .name("pnm-timeline-open".into())
            .spawn(move || opener(open_rx, ready_tx))
            .ok();

        let total_frames = (plan.duration_secs * rate as f64).round() as u64;
        let info = StreamInfo {
            sample_rate: rate,
            channels: CHANNELS as u16,
            duration_secs: plan.duration_secs,
            codec: "master mix".into(),
            bits_per_sample: Some(32),
            bitrate_kbps: None,
        };

        TimelineSource {
            plan,
            rate,
            total_frames,
            cursor: 0,
            active: Vec::new(),
            pending: HashSet::new(),
            finished: HashSet::new(),
            generation: 0,
            open_tx: Some(open_tx),
            ready_rx,
            chains: Vec::new(),
            free_chains: Vec::new(),
            interleaved: Vec::new(),
            planar: vec![Vec::new(); CHANNELS],
            mix: vec![Vec::new(); CHANNELS],
            info,
        }
    }

    pub fn info(&self) -> &StreamInfo {
        &self.info
    }

    pub fn decoded_secs(&self) -> f64 {
        self.cursor as f64 / self.rate as f64
    }

    pub fn is_eof(&self) -> bool {
        self.cursor >= self.total_frames
    }

    /// Jump to a point on the timeline. Everything sounding is torn down: a
    /// block that spans the destination is reopened at the right offset by the
    /// normal lookahead path on the next read.
    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let secs = secs.clamp(0.0, self.plan.duration_secs);
        self.cursor = (secs * self.rate as f64) as u64;
        self.generation = self.generation.wrapping_add(1);
        self.pending.clear();
        self.finished.clear();
        while let Some(voice) = self.active.pop() {
            self.release_chain(voice.chain_ix);
        }
        Ok(())
    }

    /// Varispeed does not apply to a mix as a whole: the arrangement is laid
    /// out in seconds, and stretching it would move every block. Accepted and
    /// ignored so the engine can treat both kinds of source alike.
    pub fn set_speed(&mut self, _speed: f64) -> Result<()> {
        Ok(())
    }

    /// Render the next stretch of the timeline as interleaved stereo.
    ///
    /// Returns 0 only at the end of the mix. A stretch with nothing arranged
    /// in it is silence, not an ending — a gap between blocks is part of the
    /// arrangement.
    pub fn read(&mut self, out: &mut [f32]) -> Result<usize> {
        if self.cursor >= self.total_frames {
            return Ok(0);
        }
        let wanted = out.len() / CHANNELS;
        let frames = wanted.min((self.total_frames - self.cursor) as usize);
        if frames == 0 {
            return Ok(0);
        }
        self.ensure_capacity(frames);

        for channel in self.mix.iter_mut() {
            for sample in channel[..frames].iter_mut() {
                *sample = 0.0;
            }
        }

        self.collect_ready();
        self.request_openings();
        self.render_active(frames)?;

        for f in 0..frames {
            for ch in 0..CHANNELS {
                out[f * CHANNELS + ch] = self.mix[ch][f];
            }
        }
        self.cursor += frames as u64;
        Ok(frames * CHANNELS)
    }

    /// Like [`read`], but waits for any file that should already be open.
    ///
    /// Real-time playback can afford to miss a late open and fade the block in;
    /// a bounce cannot, or the exported file would drop the start of every song.
    pub fn read_offline(&mut self, out: &mut [f32]) -> Result<usize> {
        self.request_openings();
        self.wait_pending()?;
        self.read(out)
    }

    fn ensure_capacity(&mut self, frames: usize) {
        if self.interleaved.len() < frames * CHANNELS {
            self.interleaved.resize(frames * CHANNELS, 0.0);
        }
        for buffer in self.planar.iter_mut().chain(self.mix.iter_mut()) {
            if buffer.len() < frames {
                buffer.resize(frames, 0.0);
            }
        }
    }

    /// Take delivery of any files the opener has finished with.
    fn collect_ready(&mut self) {
        while let Ok(ready) = self.ready_rx.try_recv() {
            self.apply_ready(ready);
        }
    }

    fn wait_pending(&mut self) -> Result<()> {
        while !self.pending.is_empty() {
            let ready = self
                .ready_rx
                .recv_timeout(std::time::Duration::from_secs(60))
                .map_err(|_| anyhow!("timed out opening a mix block"))?;
            self.apply_ready(ready);
        }
        Ok(())
    }

    fn apply_ready(&mut self, ready: Opened) {
        if ready.generation != self.generation {
            return;
        }
        self.pending.remove(&ready.block_ix);
        let Some(mut decoder) = ready.decoder else {
            // Unreadable — a file deleted since the mix was built, say.
            // The rest of the mix plays; only this block is missing.
            self.finished.insert(ready.block_ix);
            return;
        };
        if self.active.len() >= MAX_VOICES {
            self.finished.insert(ready.block_ix);
            return;
        }
        // Late arrivals are nudged forward rather than dragging the
        // timeline back: the mix keeps time, this block just joins late.
        let mut next_frame = ready.enter_frame;
        if self.cursor > next_frame {
            let behind = (self.cursor - next_frame) as f64 / self.rate as f64;
            if behind > MAX_CATCH_UP_SECS {
                let plan_block = &self.plan.blocks[ready.block_ix];
                let into_block = self.decoded_secs() - plan_block.block.start_secs;
                let target = looped_source_secs(
                    plan_block.block.offset_secs,
                    into_block,
                    decoder.info.duration_secs,
                );
                if decoder.seek(target).is_err() {
                    self.finished.insert(ready.block_ix);
                    return;
                }
            } else {
                let loop_start = self.plan.blocks[ready.block_ix].block.offset_secs;
                if self.discard(&mut decoder, behind, loop_start).is_err() {
                    self.finished.insert(ready.block_ix);
                    return;
                }
            }
            next_frame = self.cursor;
        }
        let chain_ix = self.take_chain();
        self.active.push(Active {
            block_ix: ready.block_ix,
            decoder,
            chain_ix,
            next_frame,
        });
    }

    /// Read and throw away `secs` of audio, so a decoder that opened slowly
    /// lines up with the timeline again without an expensive seek.
    fn discard(
        &mut self,
        decoder: &mut TrackDecoder,
        secs: f64,
        loop_start_secs: f64,
    ) -> Result<()> {
        let mut left = (secs * self.rate as f64) as usize * CHANNELS;
        while left > 0 {
            let take = left.min(self.interleaved.len().max(CHANNELS));
            let got = decoder.read_looping(&mut self.interleaved[..take], loop_start_secs)?;
            if got == 0 {
                break;
            }
            left -= got;
        }
        Ok(())
    }

    /// Ask for the files of any block due to start soon.
    fn request_openings(&mut self) {
        let Some(open_tx) = self.open_tx.as_ref() else {
            return;
        };
        let horizon = self.decoded_secs() + LOOKAHEAD_SECS;
        for (block_ix, plan_block) in self.plan.blocks.iter().enumerate() {
            if plan_block.block.start_secs > horizon {
                // Sorted by start time, so nothing after this is due either.
                break;
            }
            if plan_block.lane_gain <= 0.0 {
                continue;
            }
            if self.active.len() + self.pending.len() >= MAX_VOICES {
                break;
            }
            let end_frame = frame_of(plan_block.block.end_secs(), self.rate);
            if end_frame <= self.cursor
                || self.finished.contains(&block_ix)
                || self.pending.contains(&block_ix)
                || self.active.iter().any(|v| v.block_ix == block_ix)
            {
                continue;
            }
            // Joining a block part-way through — after a seek into the middle
            // of one — means opening it at the matching point in the file.
            let enter_secs = plan_block.block.start_secs.max(self.decoded_secs());
            let into_block = enter_secs - plan_block.block.start_secs;
            self.pending.insert(block_ix);
            let _ = open_tx.send(OpenRequest {
                generation: self.generation,
                block_ix,
                path: plan_block.path.clone(),
                offset_secs: plan_block.block.offset_secs,
                into_block_secs: into_block,
                rate: self.rate,
                enter_frame: frame_of(enter_secs, self.rate),
            });
        }
    }

    fn render_active(&mut self, frames: usize) -> Result<()> {
        let block_start = self.cursor;
        let block_end = self.cursor + frames as u64;
        let mut retired: Vec<usize> = Vec::new();

        for slot in 0..self.active.len() {
            let (block_ix, chain_ix, next_frame) = {
                let voice = &self.active[slot];
                (voice.block_ix, voice.chain_ix, voice.next_frame)
            };
            let plan_block = &self.plan.blocks[block_ix];
            let start_frame = frame_of(plan_block.block.start_secs, self.rate);
            let end_frame = frame_of(plan_block.block.end_secs(), self.rate);

            if end_frame <= block_start {
                retired.push(slot);
                continue;
            }
            let from = start_frame.max(block_start).max(next_frame);
            let to = end_frame.min(block_end);
            if to <= from {
                continue;
            }
            let count = (to - from) as usize;
            let offset = (from - block_start) as usize;

            let got = {
                let voice = &mut self.active[slot];
                voice.decoder.read_looping(
                    &mut self.interleaved[..count * CHANNELS],
                    plan_block.block.offset_secs,
                )?
            };
            let got_frames = got / CHANNELS;
            for f in 0..count {
                for ch in 0..CHANNELS {
                    self.planar[ch][f] = if f < got_frames {
                        self.interleaved[f * CHANNELS + ch]
                    } else {
                        // The file ran out before the block did: silence, not
                        // a repeat of the last buffer.
                        0.0
                    };
                }
            }

            let plan_block = &self.plan.blocks[block_ix];
            let chain = &mut self.chains[chain_ix];
            chain.update(&plan_block.settings, plan_block.track_gain_db);
            if plan_block.settings.enabled {
                chain.process_music(&mut self.planar, count);
            }
            chain.apply_gain(&mut self.planar, count);

            // The envelope is sampled at each end of this stretch and
            // interpolated across it. At 512 frames that is ~10 ms, far below
            // anything audible as a stair-step, and it keeps a per-frame
            // automation lookup out of the inner loop.
            let lane = plan_block.lane_gain;
            let into_start = (from - start_frame) as f64 / self.rate as f64;
            let into_end = (to - start_frame) as f64 / self.rate as f64;
            let gain_from = plan_block.block.gain_at(into_start) * lane;
            let gain_to = plan_block.block.gain_at(into_end) * lane;
            let step = if count > 1 {
                (gain_to - gain_from) / (count - 1) as f32
            } else {
                0.0
            };

            for f in 0..count {
                let gain = gain_from + step * f as f32;
                for ch in 0..CHANNELS {
                    self.mix[ch][offset + f] += self.planar[ch][f] * gain;
                }
            }

            let voice = &mut self.active[slot];
            voice.next_frame = to;
            if to >= end_frame {
                retired.push(slot);
            }
        }

        // Back to front, so removing one does not shift the next.
        for slot in retired.into_iter().rev() {
            let voice = self.active.remove(slot);
            self.finished.insert(voice.block_ix);
            self.release_chain(voice.chain_ix);
        }
        Ok(())
    }

    fn take_chain(&mut self) -> usize {
        if let Some(ix) = self.free_chains.pop() {
            return ix;
        }
        let mut chain = Chain::new();
        chain.prepare(self.rate as f32);
        self.chains.push(chain);
        self.chains.len() - 1
    }

    /// Reset before it goes back in the pool, so one block's reverb tail can
    /// never bleed into the next block that happens to be handed this chain.
    fn release_chain(&mut self, ix: usize) {
        self.chains[ix].prepare(self.rate as f32);
        self.free_chains.push(ix);
    }
}

fn frame_of(secs: f64, rate: u32) -> u64 {
    (secs.max(0.0) * rate as f64).round() as u64
}

/// Source position for elapsed block time, repeating the slice from offset to EOF.
fn looped_source_secs(offset_secs: f64, into_block_secs: f64, source_duration_secs: f64) -> f64 {
    let loop_secs = source_duration_secs - offset_secs;
    if !loop_secs.is_finite() || loop_secs <= 0.0 {
        return offset_secs.max(0.0);
    }
    offset_secs.max(0.0) + into_block_secs.max(0.0) % loop_secs
}

/// Opens files for the timeline, off the DSP thread.
fn opener(requests: Receiver<OpenRequest>, ready: Sender<Opened>) {
    for request in requests.iter() {
        let opened = TrackDecoder::open(&request.path, request.rate)
            .and_then(|mut decoder| {
                let seek_secs = looped_source_secs(
                    request.offset_secs,
                    request.into_block_secs,
                    decoder.info.duration_secs,
                );
                if seek_secs > 0.0 {
                    decoder.seek(seek_secs)?;
                }
                Ok(decoder)
            })
            .map_err(|e| {
                eprintln!("timeline: {}: {e}", request.path.display());
                e
            })
            .ok();

        let sent = ready.send(Opened {
            generation: request.generation,
            block_ix: request.block_ix,
            enter_frame: request.enter_frame,
            decoder: opened.map(Box::new),
        });
        if sent.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::master_mix::BlockSource;

    fn plan_block(start: f64, duration: f64) -> PlanBlock {
        PlanBlock {
            path: PathBuf::from("/nowhere.flac"),
            block: Block {
                source: BlockSource::Entry { index: 0 },
                start_secs: start,
                duration_secs: duration,
                ..Default::default()
            },
            lane_gain: 1.0,
            settings: Arc::new(Resolved::default()),
            track_gain_db: 0.0,
        }
    }

    #[test]
    fn a_plan_is_as_long_as_its_last_block_ends() {
        let plan = Plan::new(vec![plan_block(0.0, 10.0), plan_block(30.0, 5.0)]);
        assert_eq!(plan.duration_secs, 35.0);
    }

    #[test]
    fn a_plan_orders_blocks_by_start_time() {
        let plan = Plan::new(vec![plan_block(30.0, 5.0), plan_block(0.0, 10.0)]);
        assert_eq!(plan.blocks[0].block.start_secs, 0.0);
        assert_eq!(plan.blocks[1].block.start_secs, 30.0);
    }

    /// Nothing here can be opened, so every block falls away and the mix is
    /// silence — but it must still be silence of the *right length*, ending
    /// exactly at the arrangement's end rather than stopping at the first
    /// missing file.
    #[test]
    fn a_mix_of_unreadable_files_still_runs_the_full_timeline() {
        let rate = 48_000;
        let mut source = TimelineSource::new(Plan::new(vec![plan_block(0.0, 0.5)]), rate);
        let mut out = vec![0.0f32; 512 * CHANNELS];

        let mut frames = 0usize;
        while !source.is_eof() {
            let got = source.read(&mut out).unwrap();
            if got == 0 {
                break;
            }
            frames += got / CHANNELS;
            assert!(out.iter().all(|s| *s == 0.0), "no file, so no sound");
        }
        assert_eq!(frames, (0.5 * rate as f64) as usize);
        assert!(source.is_eof());
    }

    #[test]
    fn a_silent_lane_keeps_its_duration_without_opening_a_voice() {
        let mut block = plan_block(0.0, 5.0);
        block.lane_gain = 0.0;
        let mut source = TimelineSource::new(Plan::new(vec![block]), 48_000);

        source.request_openings();

        assert!(source.pending.is_empty());
        assert_eq!(source.info().duration_secs, 5.0);
    }

    #[test]
    fn a_gap_in_the_arrangement_is_silence_rather_than_the_end() {
        let rate = 48_000;
        // Nothing until 10 s in: reads across the gap must keep returning
        // audio, or the engine would call the mix finished before it started.
        let mut source = TimelineSource::new(Plan::new(vec![plan_block(10.0, 1.0)]), rate);
        let mut out = vec![0.0f32; 512 * CHANNELS];
        assert!(source.read(&mut out).unwrap() > 0);
        assert!(!source.is_eof());
    }

    #[test]
    fn seeking_moves_the_reported_position_and_clamps_to_the_mix() {
        let mut source = TimelineSource::new(Plan::new(vec![plan_block(0.0, 20.0)]), 48_000);
        source.seek(12.0).unwrap();
        assert!((source.decoded_secs() - 12.0).abs() < 1e-6);

        source.seek(1_000.0).unwrap();
        assert!((source.decoded_secs() - 20.0).abs() < 1e-6);
        assert!(source.is_eof());

        source.seek(-5.0).unwrap();
        assert_eq!(source.decoded_secs(), 0.0);
        assert!(!source.is_eof());
    }

    #[test]
    fn source_time_repeats_from_the_offset_after_eof() {
        assert!((looped_source_secs(2.0, 0.0, 10.0) - 2.0).abs() < 1e-9);
        assert!((looped_source_secs(2.0, 7.0, 10.0) - 9.0).abs() < 1e-9);
        assert!((looped_source_secs(2.0, 8.0, 10.0) - 2.0).abs() < 1e-9);
        assert!((looped_source_secs(2.0, 19.0, 10.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn an_offset_at_eof_has_no_invalid_loop_span() {
        assert_eq!(looped_source_secs(10.0, 20.0, 10.0), 10.0);
    }

    #[test]
    fn an_empty_mix_is_immediately_finished() {
        let mut source = TimelineSource::new(Plan::default(), 48_000);
        let mut out = vec![0.0f32; 512 * CHANNELS];
        assert_eq!(source.read(&mut out).unwrap(), 0);
        assert!(source.is_eof());
    }

    #[test]
    fn no_more_than_the_voice_limit_is_ever_opened_at_once() {
        let rate = 48_000;
        let blocks: Vec<PlanBlock> = (0..MAX_VOICES * 3).map(|_| plan_block(0.0, 30.0)).collect();
        let mut source = TimelineSource::new(Plan::new(blocks), rate);
        let mut out = vec![0.0f32; 512 * CHANNELS];
        source.read(&mut out).unwrap();
        assert!(source.pending.len() + source.active.len() <= MAX_VOICES);
    }
}
