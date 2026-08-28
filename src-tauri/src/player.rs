//! Queue and transport logic.
//!
//! The queue lives here rather than in the frontend so that "next track" is
//! decided in the same place that knows a track just ended, and so keyboard
//! shortcuts stay correct even if the UI is mid-render.

use serde::{Deserialize, Serialize};

use crate::audio::params::MixerSettings;
use crate::library::model::Track;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Repeat {
    Off,
    All,
    One,
}

impl Default for Repeat {
    fn default() -> Self {
        Repeat::Off
    }
}

/// Where the current queue came from, so playlist-level mixer overrides and
/// the "playing from" label both know what to say.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    pub kind: String,
    pub id: String,
    pub name: String,
}

/// A track sitting in the queue, plus any override it inherited from the
/// playlist entry it came from.
///
/// The override travels with the queue entry rather than with the track,
/// because the same song in two playlists can be mixed two different ways, and
/// played from the library it should be mixed not at all.
#[derive(Debug, Clone)]
pub struct QueueItem {
    pub track: Track,
    pub mixer: Option<MixerSettings>,
}

impl From<Track> for QueueItem {
    fn from(track: Track) -> Self {
        QueueItem { track, mixer: None }
    }
}

#[derive(Debug, Default)]
pub struct Player {
    /// Queue entries in the order they were added.
    queue: Vec<QueueItem>,
    /// Indices into `queue` giving playback order; differs when shuffling.
    order: Vec<usize>,
    /// Position within `order`.
    cursor: usize,
    shuffle: bool,
    repeat: Repeat,
    pub context: Option<Context>,
    /// Mixer override carried by the playlist the queue came from.
    pub context_mixer: Option<MixerSettings>,
    pub global_mixer: MixerSettings,
}

/// Queue state shaped for the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueView {
    pub items: Vec<Track>,
    /// Index into `items` of the track playing now.
    pub current_index: Option<usize>,
    /// Upcoming tracks in play order, which is what the queue panel shows.
    pub upcoming: Vec<Track>,
    pub shuffle: bool,
    pub repeat: Repeat,
    pub context: Option<Context>,
}

impl Player {
    pub fn new() -> Self {
        Player::default()
    }

    pub fn current(&self) -> Option<&Track> {
        self.current_item().map(|item| &item.track)
    }

    pub fn current_item(&self) -> Option<&QueueItem> {
        self.order.get(self.cursor).and_then(|&i| self.queue.get(i))
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    pub fn repeat(&self) -> Repeat {
        self.repeat
    }

    /// Replace the queue and start at `start_index` (an index into `tracks`).
    pub fn set_queue(&mut self, tracks: Vec<Track>, start_index: usize) {
        self.set_queue_items(
            tracks.into_iter().map(QueueItem::from).collect(),
            start_index,
        );
    }

    /// Replace the queue with entries that may carry their own overrides.
    pub fn set_queue_items(&mut self, items: Vec<QueueItem>, start_index: usize) {
        self.queue = items;
        self.rebuild_order(Some(start_index.min(self.queue.len().saturating_sub(1))));
    }

    /// Update the override on whichever entry holds `track_id`, used when a
    /// playlist entry is edited while that playlist is playing.
    pub fn set_entry_mixer(&mut self, track_id: &str, mixer: Option<MixerSettings>) -> bool {
        let mut changed = false;
        for item in self.queue.iter_mut().filter(|i| i.track.id == track_id) {
            item.mixer = mixer.clone();
            changed = true;
        }
        changed
    }

    pub fn set_shuffle(&mut self, on: bool) {
        if self.shuffle == on {
            return;
        }
        self.shuffle = on;
        // Keep playing whatever is playing; only what comes after changes.
        let playing = self.order.get(self.cursor).copied();
        self.rebuild_order(playing);
    }

    pub fn set_repeat(&mut self, repeat: Repeat) {
        self.repeat = repeat;
    }

    /// Rebuild play order, keeping `keep` (an index into `queue`) current.
    fn rebuild_order(&mut self, keep: Option<usize>) {
        self.order = (0..self.queue.len()).collect();
        if self.shuffle {
            shuffle_in_place(&mut self.order);
            if let Some(keep) = keep {
                // Move the track that should be playing to the front of the
                // shuffled order rather than jumping to a different song.
                if let Some(pos) = self.order.iter().position(|&i| i == keep) {
                    self.order.swap(0, pos);
                }
            }
            self.cursor = 0;
        } else {
            self.cursor = keep.unwrap_or(0).min(self.order.len().saturating_sub(1));
        }
    }

    /// Insert directly after the current track.
    pub fn play_next(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        if self.queue.is_empty() {
            self.set_queue(tracks, 0);
            return;
        }
        let insert_at = self.queue.len();
        self.queue
            .extend(tracks.iter().cloned().map(QueueItem::from));
        let new_indices: Vec<usize> = (insert_at..self.queue.len()).collect();
        for (offset, index) in new_indices.into_iter().enumerate() {
            self.order.insert(self.cursor + 1 + offset, index);
        }
    }

    /// Append to the very end of the play order.
    pub fn add_to_queue(&mut self, tracks: Vec<Track>) {
        if tracks.is_empty() {
            return;
        }
        if self.queue.is_empty() {
            self.set_queue(tracks, 0);
            return;
        }
        let start = self.queue.len();
        self.queue.extend(tracks.into_iter().map(QueueItem::from));
        self.order.extend(start..self.queue.len());
    }

    /// Remove by index into the *play order*, which is what the queue shows.
    pub fn remove_at(&mut self, order_index: usize) {
        if order_index >= self.order.len() {
            return;
        }
        let removed = self.order.remove(order_index);
        self.queue.remove(removed);
        // Every stored index above the removed one has now shifted down.
        for index in self.order.iter_mut() {
            if *index > removed {
                *index -= 1;
            }
        }
        if order_index < self.cursor {
            self.cursor = self.cursor.saturating_sub(1);
        }
        self.cursor = self.cursor.min(self.order.len().saturating_sub(1));
    }

    /// Move an entry within the play order, keeping the current track current.
    pub fn move_item(&mut self, from: usize, to: usize) -> bool {
        if from >= self.order.len() || to >= self.order.len() || from == to {
            return false;
        }
        let playing = self.order.get(self.cursor).copied();
        let moved = self.order.remove(from);
        self.order.insert(to, moved);

        // The cursor addresses a position, so follow the track it pointed at.
        if let Some(playing) = playing {
            if let Some(pos) = self.order.iter().position(|&i| i == playing) {
                self.cursor = pos;
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.order.clear();
        self.cursor = 0;
        self.context = None;
        self.context_mixer = None;
    }

    /// Step forward. `automatic` is true when a track ended by itself, which
    /// is the only case where repeat-one and end-of-queue behaviour differ.
    pub fn advance(&mut self, automatic: bool) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }
        if automatic && self.repeat == Repeat::One {
            return self.current();
        }
        if self.cursor + 1 < self.order.len() {
            self.cursor += 1;
            return self.current();
        }
        match self.repeat {
            Repeat::All => {
                // Reshuffle on wrap so a repeated shuffle is not the same order.
                if self.shuffle {
                    self.rebuild_order(None);
                } else {
                    self.cursor = 0;
                }
                self.current()
            }
            // Pressing next at the end of the queue stops rather than wrapping.
            _ => None,
        }
    }

    pub fn previous(&mut self) -> Option<&Track> {
        if self.order.is_empty() {
            return None;
        }
        if self.cursor > 0 {
            self.cursor -= 1;
        } else if self.repeat == Repeat::All {
            self.cursor = self.order.len() - 1;
        }
        self.current()
    }

    /// Jump to a position in the play order.
    pub fn jump_to(&mut self, order_index: usize) -> Option<&Track> {
        if order_index >= self.order.len() {
            return None;
        }
        self.cursor = order_index;
        self.current()
    }

    /// The item that `advance(true)` would land on, without moving the
    /// cursor. Used to prepare a crossfade ahead of a track actually ending.
    ///
    /// Returns `None` at points where the real destination cannot be known in
    /// advance: the end of a non-repeating queue, and the wrap point of a
    /// *shuffled* `Repeat::All` queue, since wrapping there reshuffles
    /// (`rebuild_order`) and a plain peek cannot predict what a fresh shuffle
    /// will produce. The caller falls back to an instant cut in both cases,
    /// exactly as it always has at the end of the queue.
    pub fn peek_next(&self) -> Option<(usize, &QueueItem)> {
        if self.order.is_empty() {
            return None;
        }
        if self.repeat == Repeat::One {
            // Loops back into itself; a second, independent decode of the
            // same file starting from zero, which is what a seamless loop
            // via crossfade should be.
            return self.item_at(self.cursor);
        }
        if self.cursor + 1 < self.order.len() {
            return self.item_at(self.cursor + 1);
        }
        if self.repeat == Repeat::All && !self.shuffle {
            return self.item_at(0);
        }
        None
    }

    fn item_at(&self, order_index: usize) -> Option<(usize, &QueueItem)> {
        self.order
            .get(order_index)
            .and_then(|&i| self.queue.get(i))
            .map(|item| (order_index, item))
    }

    /// Refresh one queue snapshot without disturbing its playlist mixer or any
    /// other occurrence of the same logical song. The id guard prevents an I/O
    /// result racing a queue mutation from overwriting a different entry.
    pub fn refresh_track_at(&mut self, order_index: usize, track: Track) -> bool {
        let Some(queue_index) = self.order.get(order_index).copied() else {
            return false;
        };
        let Some(item) = self.queue.get_mut(queue_index) else {
            return false;
        };
        if item.track.id != track.id {
            return false;
        }
        item.track = track;
        true
    }

    pub fn refresh_current_track(&mut self, track: Track) -> bool {
        self.refresh_track_at(self.cursor, track)
    }

    pub fn view(&self) -> QueueView {
        let items: Vec<Track> = self
            .order
            .iter()
            .filter_map(|&i| self.queue.get(i).map(|item| item.track.clone()))
            .collect();
        let upcoming = items.iter().skip(self.cursor + 1).cloned().collect();
        QueueView {
            current_index: (!items.is_empty()).then_some(self.cursor),
            items,
            upcoming,
            shuffle: self.shuffle,
            repeat: self.repeat,
            context: self.context.clone(),
        }
    }

    /// The full cascade for the track playing now: global settings, then the
    /// playlist's override, then the playlist entry's own.
    pub fn effective_mixer(&self) -> crate::audio::params::Resolved {
        let empty = MixerSettings::default();
        // The innermost layer belongs to the queue entry, not the track, so the
        // same song played from the library carries no override at all.
        let entry = self
            .current_item()
            .and_then(|item| item.mixer.as_ref())
            .unwrap_or(&empty);
        self.resolve_mixer(entry)
    }

    /// The cascade as it would apply to `item`, using the same global and
    /// playlist layers as [`Player::effective_mixer`]. Lets the crossfade
    /// engine resolve the next voice's effects before that item becomes
    /// current, i.e. before `current_item` would return it.
    pub fn effective_mixer_for(&self, item: &QueueItem) -> crate::audio::params::Resolved {
        let empty = MixerSettings::default();
        self.resolve_mixer(item.mixer.as_ref().unwrap_or(&empty))
    }

    fn resolve_mixer(&self, entry: &MixerSettings) -> crate::audio::params::Resolved {
        let empty = MixerSettings::default();
        let context = self.context_mixer.as_ref().unwrap_or(&empty);
        MixerSettings::resolve(&[&self.global_mixer, context, entry])
    }

    /// Normalisation gain for the current track, from its ReplayGain tag.
    pub fn current_gain_db(&self) -> f32 {
        self.current().and_then(|t| t.gain_db).unwrap_or(0.0)
    }
}

/// Fisher-Yates using a cheap xorshift seeded from the clock. Playlist order
/// does not need cryptographic randomness, and this avoids a dependency.
fn shuffle_in_place(items: &mut [usize]) {
    if items.len() < 2 {
        return;
    }
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x2545_F491_4F6C_DD1D)
        | 1;

    for i in (1..items.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        items.swap(i, j);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::params::Reverb;

    fn tracks(n: usize) -> Vec<Track> {
        (0..n)
            .map(|i| Track {
                id: format!("t{i}"),
                title: format!("Track {i}"),
                location: format!("/m/{i}.flac"),
                ..Default::default()
            })
            .collect()
    }

    #[test]
    fn playing_from_the_middle_starts_there() {
        let mut p = Player::new();
        p.set_queue(tracks(5), 2);
        assert_eq!(p.current().unwrap().id, "t2");
    }

    #[test]
    fn next_stops_at_the_end_unless_repeating() {
        let mut p = Player::new();
        p.set_queue(tracks(2), 0);
        assert_eq!(p.advance(false).unwrap().id, "t1");
        assert!(p.advance(false).is_none());

        p.set_queue(tracks(2), 0);
        p.set_repeat(Repeat::All);
        p.advance(false);
        assert_eq!(p.advance(false).unwrap().id, "t0", "repeat all wraps");
    }

    #[test]
    fn repeat_one_only_applies_when_a_track_ends_by_itself() {
        let mut p = Player::new();
        p.set_queue(tracks(3), 0);
        p.set_repeat(Repeat::One);
        assert_eq!(p.advance(true).unwrap().id, "t0", "auto-advance repeats");
        assert_eq!(
            p.advance(false).unwrap().id,
            "t1",
            "pressing next still moves on"
        );
    }

    #[test]
    fn play_next_lands_immediately_after_the_current_track() {
        let mut p = Player::new();
        p.set_queue(tracks(3), 0);
        let extra = vec![Track {
            id: "x".into(),
            ..Default::default()
        }];
        p.play_next(extra);
        assert_eq!(p.advance(false).unwrap().id, "x");
        assert_eq!(p.advance(false).unwrap().id, "t1");
    }

    #[test]
    fn add_to_queue_goes_to_the_back() {
        let mut p = Player::new();
        p.set_queue(tracks(2), 0);
        p.add_to_queue(vec![Track {
            id: "x".into(),
            ..Default::default()
        }]);
        assert_eq!(p.advance(false).unwrap().id, "t1");
        assert_eq!(p.advance(false).unwrap().id, "x");
    }

    #[test]
    fn turning_on_shuffle_keeps_the_current_track_playing() {
        let mut p = Player::new();
        p.set_queue(tracks(20), 7);
        let before = p.current().unwrap().id.clone();
        p.set_shuffle(true);
        assert_eq!(p.current().unwrap().id, before);
    }

    #[test]
    fn removing_an_entry_keeps_the_rest_addressable() {
        let mut p = Player::new();
        p.set_queue(tracks(4), 0);
        p.remove_at(1);
        let view = p.view();
        assert_eq!(view.items.len(), 3);
        assert_eq!(
            view.items.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["t0", "t2", "t3"]
        );
        assert_eq!(p.advance(false).unwrap().id, "t2");
    }

    #[test]
    fn removing_before_the_cursor_does_not_change_the_current_track() {
        let mut p = Player::new();
        p.set_queue(tracks(4), 2);
        assert_eq!(p.current().unwrap().id, "t2");
        p.remove_at(0);
        assert_eq!(p.current().unwrap().id, "t2");
    }

    #[test]
    fn refreshing_one_queue_snapshot_preserves_identity_and_other_entries() {
        let mut p = Player::new();
        p.set_queue(tracks(3), 1);
        let mut refreshed = p.current().unwrap().clone();
        refreshed.location = "/new/effective.flac".into();
        refreshed.file_count = 2;

        assert!(p.refresh_current_track(refreshed));
        assert_eq!(p.current().unwrap().location, "/new/effective.flac");
        assert_eq!(p.view().items[0].location, "/m/0.flac");
        assert_eq!(p.view().items[2].location, "/m/2.flac");

        let mut wrong = p.current().unwrap().clone();
        wrong.id = "different-song".into();
        assert!(!p.refresh_track_at(1, wrong));
        assert_eq!(p.current().unwrap().id, "t1");
    }

    #[test]
    fn the_mixer_cascade_runs_global_then_playlist_then_track() {
        let mut p = Player::new();
        let entry = QueueItem {
            track: Track {
                id: "t0".into(),
                ..Default::default()
            },
            mixer: Some(MixerSettings {
                reverb: Some(Reverb {
                    enabled: true,
                    mix: 0.9,
                    ..Default::default()
                }),
                ..Default::default()
            }),
        };
        p.set_queue_items(vec![entry], 0);

        p.global_mixer = MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.1,
                ..Default::default()
            }),
            ..Default::default()
        };
        p.context_mixer = Some(MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        });

        // The track's own override is innermost and wins.
        assert_eq!(p.effective_mixer().reverb.mix, 0.9);
    }

    #[test]
    fn a_playlist_override_applies_when_the_track_has_none() {
        let mut p = Player::new();
        p.set_queue(
            vec![Track {
                id: "t0".into(),
                ..Default::default()
            }],
            0,
        );
        p.global_mixer = MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.1,
                ..Default::default()
            }),
            ..Default::default()
        };
        p.context_mixer = Some(MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        });
        assert_eq!(p.effective_mixer().reverb.mix, 0.5);
    }

    #[test]
    fn shuffle_permutes_without_losing_or_duplicating() {
        let mut order: Vec<usize> = (0..50).collect();
        shuffle_in_place(&mut order);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, (0..50).collect::<Vec<_>>());
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    use crate::audio::params::Reverb;

    fn track(id: &str) -> Track {
        Track {
            id: id.into(),
            ..Default::default()
        }
    }

    fn wet(mix: f32) -> MixerSettings {
        MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn a_song_played_from_the_library_carries_no_override() {
        let mut p = Player::new();
        p.global_mixer = wet(0.1);
        p.set_queue(vec![track("t0")], 0);

        // No playlist, so nothing between the global mixer and the song.
        assert_eq!(p.effective_mixer().reverb.mix, 0.1);
        assert!(p.current_item().unwrap().mixer.is_none());
    }

    #[test]
    fn the_same_song_can_be_mixed_differently_in_two_playlists() {
        let mut p = Player::new();
        p.global_mixer = wet(0.1);

        p.set_queue_items(
            vec![QueueItem {
                track: track("shared"),
                mixer: Some(wet(0.8)),
            }],
            0,
        );
        assert_eq!(p.effective_mixer().reverb.mix, 0.8);

        // Re-queued from a different playlist with its own override.
        p.set_queue_items(
            vec![QueueItem {
                track: track("shared"),
                mixer: Some(wet(0.3)),
            }],
            0,
        );
        assert_eq!(p.effective_mixer().reverb.mix, 0.3);
    }

    #[test]
    fn editing_an_entry_applies_to_the_live_queue() {
        let mut p = Player::new();
        p.set_queue_items(
            vec![QueueItem {
                track: track("t0"),
                mixer: None,
            }],
            0,
        );
        assert_eq!(p.effective_mixer().reverb.mix, 0.25);
        assert!(!p.effective_mixer().reverb.enabled);

        assert!(p.set_entry_mixer("t0", Some(wet(0.6))));
        assert_eq!(p.effective_mixer().reverb.mix, 0.6);

        // Clearing it falls back through the cascade again.
        assert!(p.set_entry_mixer("t0", None));
        assert!(!p.effective_mixer().reverb.enabled);
    }

    #[test]
    fn editing_an_entry_that_is_not_queued_changes_nothing() {
        let mut p = Player::new();
        p.set_queue(vec![track("t0")], 0);
        assert!(!p.set_entry_mixer("somebody-else", Some(wet(0.9))));
    }

    #[test]
    fn reordering_keeps_the_current_track_playing() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b"), track("c"), track("d")], 1);
        assert_eq!(p.current().unwrap().id, "b");

        // Drag the last entry to the top.
        assert!(p.move_item(3, 0));
        assert_eq!(
            p.current().unwrap().id,
            "b",
            "the playing track must not change"
        );

        let view = p.view();
        let ids: Vec<&str> = view.items.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["d", "a", "b", "c"]);
        assert_eq!(p.advance(false).unwrap().id, "c");
    }

    #[test]
    fn reordering_moves_the_entry_and_keeps_its_override() {
        let mut p = Player::new();
        p.set_queue_items(
            vec![
                QueueItem {
                    track: track("a"),
                    mixer: None,
                },
                QueueItem {
                    track: track("b"),
                    mixer: Some(wet(0.7)),
                },
            ],
            0,
        );
        assert!(p.move_item(1, 0));
        assert_eq!(p.view().items[0].id, "b");
        // Playing it now should still pick up its override.
        p.jump_to(0);
        assert_eq!(p.effective_mixer().reverb.mix, 0.7);
    }

    #[test]
    fn out_of_range_moves_are_ignored() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b")], 0);
        assert!(!p.move_item(0, 0));
        assert!(!p.move_item(5, 0));
        assert!(!p.move_item(0, 9));
    }

    #[test]
    fn peek_next_matches_what_advance_would_do() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b"), track("c")], 0);

        let (idx, item) = p.peek_next().expect("a middle track has a successor");
        assert_eq!(item.track.id, "b");
        assert_eq!(p.advance(true).unwrap().id, "b");
        assert_eq!(idx, 1);
    }

    #[test]
    fn peek_next_is_none_at_the_end_of_a_non_repeating_queue() {
        let mut p = Player::new();
        p.set_queue(vec![track("a")], 0);
        assert!(p.peek_next().is_none());
        assert!(p.advance(true).is_none(), "peek must agree with advance");
    }

    #[test]
    fn peek_next_wraps_for_an_unshuffled_repeating_queue() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b")], 1);
        p.set_repeat(Repeat::All);

        let (idx, item) = p.peek_next().expect("repeat-all wraps");
        assert_eq!(item.track.id, "a");
        assert_eq!(idx, 0);
        assert_eq!(p.advance(true).unwrap().id, "a");
    }

    #[test]
    fn peek_next_gives_up_at_a_shuffled_wrap_since_it_would_reshuffle() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b"), track("c")], 0);
        p.set_shuffle(true);
        p.set_repeat(Repeat::All);

        // Walk to the last position without wrapping. Bounded deliberately:
        // under `Repeat::All`, `advance` never returns `None` (wrapping just
        // reshuffles and keeps going), so a `while ... is_some()` loop here
        // would spin forever rather than reaching "the end".
        let len = p.view().items.len();
        for _ in 0..len - 1 {
            p.advance(false);
        }
        assert_eq!(
            p.view().current_index,
            Some(len - 1),
            "should be at the last position"
        );
        assert!(
            p.peek_next().is_none(),
            "a shuffled repeat-all wrap reshuffles and cannot be predicted"
        );
    }

    #[test]
    fn peek_next_loops_a_track_into_itself_under_repeat_one() {
        let mut p = Player::new();
        p.set_queue(vec![track("a"), track("b")], 0);
        p.set_repeat(Repeat::One);

        let (idx, item) = p.peek_next().expect("repeat-one loops");
        assert_eq!(item.track.id, "a");
        assert_eq!(idx, 0);
    }

    #[test]
    fn effective_mixer_for_a_peeked_item_uses_its_own_override_not_the_currents() {
        let mut p = Player::new();
        p.global_mixer = wet(0.1);
        p.set_queue_items(
            vec![
                QueueItem {
                    track: track("a"),
                    mixer: Some(wet(0.9)),
                },
                QueueItem {
                    track: track("b"),
                    mixer: Some(wet(0.4)),
                },
            ],
            0,
        );

        assert_eq!(
            p.effective_mixer().reverb.mix,
            0.9,
            "current track's own override"
        );
        let (_, next) = p.peek_next().unwrap();
        let next = next.clone();
        assert_eq!(
            p.effective_mixer_for(&next).reverb.mix,
            0.4,
            "the peeked track's own override, not the current track's"
        );
    }
}
