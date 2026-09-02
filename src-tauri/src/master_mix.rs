//! The per-playlist master mix: a Logic-style timeline over a playlist.
//!
//! A playlist normally plays as a list — one song, then the next, joined by
//! the global crossfade curve. A *master mix* replaces that list with a
//! timeline: lanes stacked vertically, each holding audio blocks placed at
//! explicit times. Two blocks that overlap are heard together, which is how a
//! hand-made crossfade is built.
//!
//! The whole document lives inside the playlist file under `masterMix`. Rule 2
//! of that format (every field has a default, unknown fields ride along in
//! `extra`) means a playlist written here still opens in a build that predates
//! the feature — it simply plays as a plain list.
//!
//! Two invariants are worth stating up front, because everything else follows
//! from them:
//!
//! 1. **Block times are absolute timeline seconds**, not offsets from a join.
//!    Moving one block never re-times another, and a mix stays readable after
//!    an edit anywhere in it.
//! 2. **A block that plays a playlist song references it by entry index**, not
//!    by a copy of its identity. Reordering or deleting playlist entries
//!    therefore has to remap those indices, which [`MasterMix::entry_removed`]
//!    and [`MasterMix::entry_moved`] do.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::audio::params::MixerSettings;

/// Ceilings that exist so a hand-edited or malicious playlist file cannot make
/// the app allocate without bound. All are far above anything a person would
/// build by hand.
pub const MAX_LANES: usize = 64;
pub const MAX_BLOCKS_PER_LANE: usize = 512;
pub const MAX_AUTOMATION_POINTS: usize = 512;
/// A day. Long enough for any real mix, short enough to bound a render.
pub const MAX_TIMELINE_SECS: f64 = 24.0 * 60.0 * 60.0;
/// Shorter than this and a block is not audible as anything but a click.
pub const MIN_BLOCK_SECS: f64 = 0.02;
/// Gain range shared by lanes, blocks and automation points.
pub const MIN_GAIN_DB: f32 = -60.0;
pub const MAX_GAIN_DB: f32 = 12.0;
/// Below this a gain is treated as silence rather than a very quiet signal.
pub const SILENT_DB: f32 = MIN_GAIN_DB;

/// Where a block's audio comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BlockSource {
    /// One of the playlist's own entries, by index into `Playlist::tracks`.
    /// Resolved through the library like any other entry, so the block follows
    /// the listener's own copy of the song.
    Entry { index: usize },
    /// A file imported into this playlist's assets folder. `file` is a bare
    /// file name inside that folder, never a path, so a mix stays portable and
    /// a playlist file can never point outside its own directory.
    Asset { file: String },
}

impl Default for BlockSource {
    fn default() -> Self {
        BlockSource::Entry { index: 0 }
    }
}

/// One point on a block's volume envelope. Reserved for the automation
/// overlay; defined now so the file format does not have to change when it
/// arrives.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AutomationPoint {
    /// Seconds from the start of the block, not of the timeline: a point
    /// survives the block being dragged somewhere else.
    pub at_secs: f64,
    pub gain_db: f32,
    /// Shape of the segment *leaving* this point. 1 is linear; below 1 leans
    /// early, above 1 leans late.
    pub curve: f32,
}

impl Default for AutomationPoint {
    fn default() -> Self {
        AutomationPoint {
            at_secs: 0.0,
            gain_db: 0.0,
            curve: 1.0,
        }
    }
}

/// One audio region on the timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Block {
    pub id: String,
    pub source: BlockSource,
    /// Where this block starts on the timeline.
    pub start_secs: f64,
    /// How far into the source the block begins. Splitting a block leaves the
    /// right-hand half with a non-zero offset; trimming from the left does the
    /// same.
    pub offset_secs: f64,
    /// How long it plays for, from `offset_secs`.
    pub duration_secs: f64,
    pub gain_db: f32,
    pub fade_in_secs: f64,
    pub fade_out_secs: f64,
    /// Mixer override for this block alone, layered over the playlist's.
    pub mixer: Option<MixerSettings>,
    /// Volume envelope, in block time. Empty means "just `gain_db`".
    pub automation: Vec<AutomationPoint>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Block {
    fn default() -> Self {
        Block {
            id: new_id("blk"),
            source: BlockSource::default(),
            start_secs: 0.0,
            offset_secs: 0.0,
            duration_secs: 0.0,
            gain_db: 0.0,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            mixer: None,
            automation: Vec::new(),
            extra: Map::new(),
        }
    }
}

impl Block {
    pub fn end_secs(&self) -> f64 {
        self.start_secs + self.duration_secs
    }

    /// Linear gain at `at_secs` from the start of the block, folding the
    /// block's own gain, its fades and its automation envelope together.
    pub fn gain_at(&self, at_secs: f64) -> f32 {
        let mut gain = db_to_gain(self.gain_db) * self.automation_gain(at_secs);

        if self.fade_in_secs > 0.0 && at_secs < self.fade_in_secs {
            gain *= (at_secs / self.fade_in_secs).clamp(0.0, 1.0) as f32;
        }
        let from_end = self.duration_secs - at_secs;
        if self.fade_out_secs > 0.0 && from_end < self.fade_out_secs {
            gain *= (from_end / self.fade_out_secs).clamp(0.0, 1.0) as f32;
        }
        gain
    }

    /// The envelope alone, as a linear multiplier. Points are held sorted by
    /// [`MasterMix::normalise`], so this is a straight walk.
    fn automation_gain(&self, at_secs: f64) -> f32 {
        let points = &self.automation;
        match points.len() {
            0 => 1.0,
            1 => db_to_gain(points[0].gain_db),
            _ => {
                if at_secs <= points[0].at_secs {
                    return db_to_gain(points[0].gain_db);
                }
                let last = &points[points.len() - 1];
                if at_secs >= last.at_secs {
                    return db_to_gain(last.gain_db);
                }
                let ix = points.partition_point(|p| p.at_secs <= at_secs).max(1) - 1;
                let (a, b) = (&points[ix], &points[ix + 1]);
                let span = b.at_secs - a.at_secs;
                let t = if span > 0.0 {
                    ((at_secs - a.at_secs) / span).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                // Interpolated in dB, so a fade between two points sounds like
                // a fader move rather than a linear-amplitude ramp.
                let shaped = t.powf(a.curve.clamp(0.05, 8.0) as f64) as f32;
                db_to_gain(a.gain_db + (b.gain_db - a.gain_db) * shaped)
            }
        }
    }
}

/// One horizontal lane. The mute and solo buttons in the drawing live here.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Lane {
    pub id: String,
    pub name: String,
    pub muted: bool,
    pub soloed: bool,
    pub gain_db: f32,
    pub blocks: Vec<Block>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Lane {
    fn default() -> Self {
        Lane {
            id: new_id("lane"),
            name: String::new(),
            muted: false,
            soloed: false,
            gain_db: 0.0,
            blocks: Vec::new(),
            extra: Map::new(),
        }
    }
}

impl Lane {
    pub fn named(name: impl Into<String>) -> Self {
        Lane {
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn end_secs(&self) -> f64 {
        self.blocks
            .iter()
            .map(Block::end_secs)
            .fold(0.0, |a: f64, b| a.max(b))
    }
}

/// The whole timeline for one playlist.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MasterMix {
    /// Off until the user asks for it. A playlist with a master mix that is
    /// switched off plays as a plain list, and the timeline is kept so turning
    /// it back on does not lose the work.
    pub enabled: bool,
    /// Bumped by [`MasterMix::touch`] on every edit, so a rendered bounce or a
    /// cached preview can tell whether it is stale without diffing the mix.
    pub revision: u64,
    pub lanes: Vec<Lane>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MasterMix {
    /// The default mix for a playlist: one lane per entry, each holding that
    /// entry's whole song, laid end to end so it plays exactly like the
    /// playlist did before the mix existed.
    ///
    /// `durations` is one entry per playlist track. A zero duration (an entry
    /// whose song is not in this library, or a hand-written file that omitted
    /// it) still gets a lane, so the mix keeps a slot for it, but contributes
    /// no block — there is nothing to place.
    pub fn build(titles: &[String], durations: &[f64]) -> Self {
        let mut mix = MasterMix {
            enabled: true,
            revision: 1,
            lanes: Vec::new(),
            extra: Map::new(),
        };
        let mut cursor = 0.0;
        for (index, duration) in durations.iter().enumerate().take(MAX_LANES) {
            let name = titles
                .get(index)
                .filter(|t| !t.trim().is_empty())
                .cloned()
                .unwrap_or_else(|| format!("Track {}", index + 1));
            let mut lane = Lane::named(name);
            if *duration > MIN_BLOCK_SECS {
                lane.blocks.push(Block {
                    source: BlockSource::Entry { index },
                    start_secs: cursor,
                    duration_secs: *duration,
                    ..Default::default()
                });
                cursor += *duration;
            }
            mix.lanes.push(lane);
        }
        mix
    }

    pub fn duration_secs(&self) -> f64 {
        self.lanes
            .iter()
            .map(Lane::end_secs)
            .fold(0.0, |a: f64, b| a.max(b))
    }

    pub fn touch(&mut self) {
        self.revision = self.revision.wrapping_add(1);
    }

    /// Whether any lane is soloed. When one is, every lane that is not soloed
    /// is silent, exactly as in a hardware mixer.
    pub fn has_solo(&self) -> bool {
        self.lanes.iter().any(|lane| lane.soloed)
    }

    /// Whether a lane is actually heard, accounting for solo elsewhere.
    pub fn lane_audible(&self, lane: &Lane) -> bool {
        if lane.muted {
            return false;
        }
        !self.has_solo() || lane.soloed
    }

    /// A new playlist entry arrived. Per the brief it becomes its own lane at
    /// the end of the mix, so adding a song to a playlist that has a master
    /// mix never disturbs the arrangement already built.
    pub fn append_entry(&mut self, index: usize, title: &str, duration_secs: f64) {
        if self.lanes.len() >= MAX_LANES || duration_secs <= MIN_BLOCK_SECS {
            return;
        }
        let start = self.duration_secs();
        let name = if title.trim().is_empty() {
            format!("Track {}", self.lanes.len() + 1)
        } else {
            title.to_string()
        };
        let mut lane = Lane::named(name);
        lane.blocks.push(Block {
            source: BlockSource::Entry { index },
            start_secs: start,
            duration_secs,
            ..Default::default()
        });
        self.lanes.push(lane);
        self.touch();
    }

    /// A playlist entry was deleted. Blocks that played it go with it; blocks
    /// pointing past it shuffle down. An emptied lane is kept, because it may
    /// still hold the user's own imported blocks — only a lane that is now
    /// completely empty *and* was only ever that entry's is dropped.
    pub fn entry_removed(&mut self, index: usize) {
        for lane in self.lanes.iter_mut() {
            lane.blocks.retain(|block| match block.source {
                BlockSource::Entry { index: i } => i != index,
                BlockSource::Asset { .. } => true,
            });
            for block in lane.blocks.iter_mut() {
                if let BlockSource::Entry { index: i } = &mut block.source {
                    if *i > index {
                        *i -= 1;
                    }
                }
            }
        }
        self.lanes.retain(|lane| !lane.blocks.is_empty());
        self.touch();
    }

    /// A playlist entry moved to a different position in the list. Only the
    /// indices change: nothing on the timeline moves, because the arrangement
    /// is the user's and the list order is not what it is built from.
    pub fn entry_moved(&mut self, from: usize, to: usize) {
        if from == to {
            return;
        }
        for lane in self.lanes.iter_mut() {
            for block in lane.blocks.iter_mut() {
                if let BlockSource::Entry { index } = &mut block.source {
                    *index = remap_index(*index, from, to);
                }
            }
        }
        self.touch();
    }

    /// Every asset file the mix refers to, for garbage-collecting the assets
    /// folder after blocks are deleted.
    pub fn asset_files(&self) -> Vec<String> {
        let mut files: Vec<String> = self
            .lanes
            .iter()
            .flat_map(|lane| lane.blocks.iter())
            .filter_map(|block| match &block.source {
                BlockSource::Asset { file } => Some(file.clone()),
                BlockSource::Entry { .. } => None,
            })
            .collect();
        files.sort();
        files.dedup();
        files
    }

    /// Make a mix that arrived from the webview (or from a hand-edited file)
    /// safe to play: every value in range, blocks ordered, ids unique, and
    /// nothing pointing at an entry that does not exist.
    ///
    /// This is the only validation gate. Commands call it before saving, so
    /// the renderer downstream can assume the mix is sane.
    pub fn normalise(&mut self, entry_count: usize) {
        self.lanes.truncate(MAX_LANES);

        let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for lane in self.lanes.iter_mut() {
            if lane.id.trim().is_empty() || !seen_ids.insert(lane.id.clone()) {
                lane.id = new_id("lane");
                seen_ids.insert(lane.id.clone());
            }
            lane.gain_db = clamp_db(lane.gain_db);
            lane.blocks.truncate(MAX_BLOCKS_PER_LANE);

            lane.blocks.retain(|block| match &block.source {
                BlockSource::Entry { index } => *index < entry_count,
                // A bare file name, so a playlist can never reach outside its
                // own assets folder.
                BlockSource::Asset { file } => is_safe_file_name(file),
            });

            for block in lane.blocks.iter_mut() {
                if block.id.trim().is_empty() || !seen_ids.insert(block.id.clone()) {
                    block.id = new_id("blk");
                    seen_ids.insert(block.id.clone());
                }
                block.start_secs = finite(block.start_secs).clamp(0.0, MAX_TIMELINE_SECS);
                block.offset_secs = finite(block.offset_secs).clamp(0.0, MAX_TIMELINE_SECS);
                block.duration_secs = finite(block.duration_secs)
                    .clamp(MIN_BLOCK_SECS, MAX_TIMELINE_SECS - block.start_secs);
                block.gain_db = clamp_db(block.gain_db);
                // Fades are trimmed to the block and then to each other, so
                // the two can never overlap and multiply into a notch.
                block.fade_in_secs = finite(block.fade_in_secs).clamp(0.0, block.duration_secs);
                block.fade_out_secs = finite(block.fade_out_secs)
                    .clamp(0.0, block.duration_secs - block.fade_in_secs);

                block.automation.truncate(MAX_AUTOMATION_POINTS);
                for point in block.automation.iter_mut() {
                    point.at_secs = finite(point.at_secs).clamp(0.0, block.duration_secs);
                    point.gain_db = clamp_db(point.gain_db);
                    point.curve = if point.curve.is_finite() {
                        point.curve.clamp(0.05, 8.0)
                    } else {
                        1.0
                    };
                }
                block
                    .automation
                    .sort_by(|a, b| a.at_secs.total_cmp(&b.at_secs));
            }

            lane.blocks
                .sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
        }
    }
}

/// Where `index` ends up after the element at `from` is moved to `to`.
fn remap_index(index: usize, from: usize, to: usize) -> usize {
    if index == from {
        return to;
    }
    if from < to && index > from && index <= to {
        index - 1
    } else if to < from && index >= to && index < from {
        index + 1
    } else {
        index
    }
}

/// A name that is a plain file inside one folder: no separators, no parent
/// hops, nothing hidden.
pub fn is_safe_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.contains(['/', '\\', '\0'])
        && name != ".."
}

fn finite(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

fn clamp_db(db: f32) -> f32 {
    if db.is_finite() {
        db.clamp(MIN_GAIN_DB, MAX_GAIN_DB)
    } else {
        0.0
    }
}

fn db_to_gain(db: f32) -> f32 {
    if db <= SILENT_DB {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

fn new_id(prefix: &str) -> String {
    let uuid = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}_{}", &uuid[..12])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mix_of(durations: &[f64]) -> MasterMix {
        let titles: Vec<String> = (0..durations.len()).map(|i| format!("Song {i}")).collect();
        MasterMix::build(&titles, durations)
    }

    #[test]
    fn the_default_mix_plays_the_playlist_back_to_back() {
        let mix = mix_of(&[100.0, 200.0, 50.0]);
        assert_eq!(mix.lanes.len(), 3);
        assert_eq!(mix.lanes[0].blocks[0].start_secs, 0.0);
        assert_eq!(mix.lanes[1].blocks[0].start_secs, 100.0);
        assert_eq!(mix.lanes[2].blocks[0].start_secs, 300.0);
        assert_eq!(mix.duration_secs(), 350.0);
    }

    #[test]
    fn an_entry_with_no_duration_still_gets_a_lane_but_no_block() {
        let mix = mix_of(&[100.0, 0.0, 50.0]);
        assert_eq!(mix.lanes.len(), 3);
        assert!(mix.lanes[1].blocks.is_empty());
        // The third song is not pushed out by the gap the missing one left.
        assert_eq!(mix.lanes[2].blocks[0].start_secs, 100.0);
    }

    #[test]
    fn adding_a_song_appends_a_lane_at_the_end_of_the_mix() {
        let mut mix = mix_of(&[100.0, 100.0]);
        mix.append_entry(2, "New", 60.0);
        assert_eq!(mix.lanes.len(), 3);
        assert_eq!(mix.lanes[2].blocks[0].start_secs, 200.0);
        assert_eq!(mix.duration_secs(), 260.0);
    }

    #[test]
    fn appending_lands_after_the_last_block_not_the_last_lane() {
        // A hand-built mix where lane 0 runs longest: the new lane must go
        // after *all* audio, not after whichever lane happens to be last.
        let mut mix = mix_of(&[300.0, 10.0]);
        mix.lanes[1].blocks[0].start_secs = 0.0;
        mix.append_entry(2, "New", 5.0);
        assert_eq!(mix.lanes[2].blocks[0].start_secs, 300.0);
    }

    #[test]
    fn deleting_a_song_removes_its_blocks_and_renumbers_the_rest() {
        let mut mix = mix_of(&[10.0, 10.0, 10.0]);
        mix.entry_removed(0);
        assert_eq!(mix.lanes.len(), 2);
        let sources: Vec<_> = mix
            .lanes
            .iter()
            .map(|lane| lane.blocks[0].source.clone())
            .collect();
        assert_eq!(
            sources,
            vec![
                BlockSource::Entry { index: 0 },
                BlockSource::Entry { index: 1 }
            ]
        );
    }

    #[test]
    fn deleting_a_song_leaves_imported_blocks_alone() {
        let mut mix = mix_of(&[10.0, 10.0]);
        mix.lanes[0].blocks.push(Block {
            source: BlockSource::Asset {
                file: "riser.wav".into(),
            },
            start_secs: 4.0,
            duration_secs: 2.0,
            ..Default::default()
        });
        mix.entry_removed(0);
        assert_eq!(mix.lanes.len(), 2, "the lane survives its imported block");
        assert_eq!(mix.lanes[0].blocks.len(), 1);
        assert!(matches!(
            mix.lanes[0].blocks[0].source,
            BlockSource::Asset { .. }
        ));
    }

    #[test]
    fn reordering_the_playlist_repoints_blocks_without_moving_them() {
        let mut mix = mix_of(&[10.0, 10.0, 10.0]);
        let starts: Vec<f64> = mix
            .lanes
            .iter()
            .map(|lane| lane.blocks[0].start_secs)
            .collect();

        mix.entry_moved(0, 2);

        assert_eq!(
            mix.lanes[0].blocks[0].source,
            BlockSource::Entry { index: 2 }
        );
        assert_eq!(
            mix.lanes[1].blocks[0].source,
            BlockSource::Entry { index: 0 }
        );
        assert_eq!(
            mix.lanes[2].blocks[0].source,
            BlockSource::Entry { index: 1 }
        );
        let after: Vec<f64> = mix
            .lanes
            .iter()
            .map(|lane| lane.blocks[0].start_secs)
            .collect();
        assert_eq!(
            starts, after,
            "the arrangement is the user's, not the list's"
        );
    }

    #[test]
    fn normalise_drops_blocks_pointing_at_entries_that_are_gone() {
        let mut mix = mix_of(&[10.0, 10.0, 10.0]);
        mix.normalise(2);
        assert_eq!(mix.lanes[2].blocks.len(), 0);
        assert_eq!(mix.lanes[0].blocks.len(), 1);
    }

    #[test]
    fn normalise_rejects_asset_names_that_could_escape_the_folder() {
        let mut mix = mix_of(&[10.0]);
        for name in ["../secrets.wav", "/etc/passwd", "sub/dir.wav", ".hidden"] {
            mix.lanes[0].blocks.push(Block {
                source: BlockSource::Asset { file: name.into() },
                duration_secs: 1.0,
                ..Default::default()
            });
        }
        mix.normalise(1);
        assert_eq!(
            mix.lanes[0].blocks.len(),
            1,
            "only the entry block survives"
        );
    }

    #[test]
    fn normalise_tames_values_the_webview_should_never_have_sent() {
        let mut mix = mix_of(&[10.0]);
        let block = &mut mix.lanes[0].blocks[0];
        block.start_secs = f64::NAN;
        block.duration_secs = -5.0;
        block.gain_db = 400.0;
        block.fade_in_secs = 1e9;
        block.fade_out_secs = 1e9;
        mix.normalise(1);

        let block = &mix.lanes[0].blocks[0];
        assert_eq!(block.start_secs, 0.0);
        assert_eq!(block.duration_secs, MIN_BLOCK_SECS);
        assert_eq!(block.gain_db, MAX_GAIN_DB);
        assert!(block.fade_in_secs + block.fade_out_secs <= block.duration_secs);
    }

    #[test]
    fn normalise_makes_duplicate_ids_unique() {
        let mut mix = mix_of(&[10.0, 10.0]);
        let shared = mix.lanes[0].blocks[0].id.clone();
        mix.lanes[1].blocks[0].id = shared.clone();
        mix.normalise(2);
        assert_ne!(mix.lanes[0].blocks[0].id, mix.lanes[1].blocks[0].id);
    }

    #[test]
    fn normalise_orders_blocks_by_time() {
        let mut mix = mix_of(&[10.0]);
        let template = mix.lanes[0].blocks[0].clone();
        mix.lanes[0].blocks = vec![
            Block {
                start_secs: 30.0,
                ..template.clone()
            },
            Block {
                start_secs: 5.0,
                id: new_id("blk"),
                ..template.clone()
            },
            Block {
                start_secs: 12.0,
                id: new_id("blk"),
                ..template
            },
        ];
        mix.normalise(1);
        let starts: Vec<f64> = mix.lanes[0].blocks.iter().map(|b| b.start_secs).collect();
        assert_eq!(starts, vec![5.0, 12.0, 30.0]);
    }

    #[test]
    fn solo_silences_every_lane_that_is_not_soloed() {
        let mut mix = mix_of(&[10.0, 10.0, 10.0]);
        assert!(mix.lanes.iter().all(|lane| mix.lane_audible(lane)));

        mix.lanes[1].soloed = true;
        assert!(!mix.lane_audible(&mix.lanes[0]));
        assert!(mix.lane_audible(&mix.lanes[1]));

        // Mute still wins over the lane's own solo.
        mix.lanes[1].muted = true;
        assert!(!mix.lane_audible(&mix.lanes[1]));
    }

    #[test]
    fn fades_ramp_from_silence_to_the_blocks_own_gain() {
        let block = Block {
            duration_secs: 10.0,
            fade_in_secs: 2.0,
            fade_out_secs: 4.0,
            ..Default::default()
        };
        assert_eq!(block.gain_at(0.0), 0.0);
        assert!((block.gain_at(1.0) - 0.5).abs() < 1e-6);
        assert!((block.gain_at(5.0) - 1.0).abs() < 1e-6);
        // Four seconds of fade-out means half way down two seconds from the end.
        assert!((block.gain_at(8.0) - 0.5).abs() < 1e-6);
        assert!((block.gain_at(10.0)).abs() < 1e-6);
    }

    #[test]
    fn automation_holds_flat_outside_its_points_and_interpolates_between_them() {
        let block = Block {
            duration_secs: 10.0,
            automation: vec![
                AutomationPoint {
                    at_secs: 2.0,
                    gain_db: 0.0,
                    curve: 1.0,
                },
                AutomationPoint {
                    at_secs: 6.0,
                    gain_db: -12.0,
                    curve: 1.0,
                },
            ],
            ..Default::default()
        };
        // Before the first point and after the last, the envelope holds.
        assert!((block.gain_at(0.0) - 1.0).abs() < 1e-6);
        assert!((block.gain_at(9.0) - db_to_gain(-12.0)).abs() < 1e-6);
        // Halfway is halfway *in decibels*, which is what a fader does.
        assert!((block.gain_at(4.0) - db_to_gain(-6.0)).abs() < 1e-5);
    }

    #[test]
    fn a_full_mix_survives_a_json_round_trip() {
        let mut mix = mix_of(&[120.0, 90.0]);
        mix.lanes[1].blocks[0].start_secs = 110.0;
        mix.lanes[1].blocks[0].fade_in_secs = 6.0;
        mix.lanes[0].blocks[0].fade_out_secs = 6.0;
        mix.lanes[0].blocks[0].automation.push(AutomationPoint {
            at_secs: 100.0,
            gain_db: -3.0,
            curve: 1.4,
        });

        let json = serde_json::to_string(&mix).unwrap();
        let back: MasterMix = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lanes.len(), 2);
        assert_eq!(back.lanes[1].blocks[0].start_secs, 110.0);
        assert_eq!(back.lanes[0].blocks[0].automation[0].curve, 1.4);
    }

    #[test]
    fn a_lane_color_round_trips_as_forward_compatible_metadata() {
        let json = r#"{
            "enabled": true,
            "lanes": [{ "id": "lane_a", "name": "One", "colorHue": 165 }]
        }"#;
        let mix: MasterMix = serde_json::from_str(json).unwrap();
        let round_tripped = serde_json::to_value(&mix).unwrap();
        assert_eq!(round_tripped["lanes"][0]["colorHue"], 165);
    }

    #[test]
    fn a_mix_from_a_newer_version_keeps_the_fields_this_one_does_not_know() {
        let json = r#"{
            "enabled": true,
            "tempo": 128,
            "lanes": [ { "id": "lane_a", "instrument": "sax", "blocks": [
                { "id": "blk_a", "source": {"kind":"entry","index":0},
                  "durationSecs": 10, "reverse": true }
            ] } ]
        }"#;
        let mix: MasterMix = serde_json::from_str(json).unwrap();
        let round_tripped = serde_json::to_string(&mix).unwrap();
        assert!(round_tripped.contains("tempo"));
        assert!(round_tripped.contains("instrument"));
        assert!(round_tripped.contains("reverse"));
    }
}
