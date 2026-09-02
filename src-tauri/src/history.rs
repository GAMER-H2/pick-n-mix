//! Measuring what was actually listened to.
//!
//! The home page's mixes are only as good as the history behind them, and the
//! thing that ruins listening history is counting things that were not
//! listened to. Skipping through twenty songs looking for one must not leave
//! twenty plays behind, or every "most played" shelf fills with music that was
//! rejected on sight.
//!
//! So time is accumulated rather than inferred: the tracker is ticked while
//! audio is running and adds only the wall time that elapsed between ticks.
//! Pausing stops the clock, and seeking moves the playhead without moving the
//! clock at all — which means scrubbing to the end of a song cannot fake
//! having heard it.

use std::time::Instant;

use parking_lot::Mutex;

use crate::library::model::Play;

/// How long a song must actually be heard before it counts as played rather
/// than skipped.
pub const MIN_PLAY_SECS: f64 = 25.0;

/// Fraction of a song that also counts as a play, for songs too short to ever
/// reach [`MIN_PLAY_SECS`]. Without this a 20-second track could be played
/// daily for a year and never appear in a single shelf.
const FINISHED_FRACTION: f64 = 0.9;

/// Largest gap a single tick may contribute.
///
/// The tick interval is a fraction of this; anything longer means the process
/// was not running normally — the machine slept, or the thread was starved —
/// and the wall clock is no longer evidence that anyone was listening.
const MAX_TICK_SECS: f64 = 1.0;

#[derive(Debug)]
struct InFlight {
    song_id: String,
    duration_secs: f64,
    context_kind: Option<String>,
    context_id: Option<String>,
    listened_secs: f64,
    /// Set while audio is running; `None` whenever it is not, which is what
    /// stops a pause from being counted as listening.
    running_since: Option<Instant>,
}

impl InFlight {
    fn into_play(self, played_at: i64) -> Play {
        let listened = self.listened_secs;
        let fraction = if self.duration_secs > 0.0 {
            (listened / self.duration_secs).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let finished =
            self.duration_secs > 0.0 && listened >= self.duration_secs * FINISHED_FRACTION;
        Play {
            song_id: self.song_id,
            played_at,
            seconds_played: listened,
            fraction,
            counted: listened >= MIN_PLAY_SECS || finished,
            context_kind: self.context_kind,
            context_id: self.context_id,
        }
    }
}

#[derive(Default)]
pub struct PlayTracker {
    current: Mutex<Option<InFlight>>,
}

impl PlayTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Start following a new song, returning the finished play for whatever
    /// was playing before it so the caller can persist it.
    #[must_use = "the returned play is the previous song's history and is lost if dropped"]
    pub fn begin(
        &self,
        song_id: &str,
        duration_secs: f64,
        context_kind: Option<String>,
        context_id: Option<String>,
        played_at: i64,
    ) -> Option<Play> {
        let mut slot = self.current.lock();
        let previous = slot.take().map(|entry| entry.into_play(played_at));
        *slot = Some(InFlight {
            song_id: song_id.to_string(),
            duration_secs,
            context_kind,
            context_id,
            listened_secs: 0.0,
            running_since: None,
        });
        previous
    }

    /// Advance the clock. Called on the playback ticker, whose `playing` flag
    /// is the engine's own, so this follows real audio rather than intent.
    pub fn tick(&self, playing: bool) {
        let mut slot = self.current.lock();
        let Some(entry) = slot.as_mut() else { return };
        let now = Instant::now();
        match (playing, entry.running_since) {
            (true, Some(since)) => {
                entry.listened_secs += now.duration_since(since).as_secs_f64().min(MAX_TICK_SECS);
                entry.running_since = Some(now);
            }
            // Either playback just started, or it just stopped. Both only move
            // the marker: the gap either has not been measured yet, or was
            // silence.
            (true, None) => entry.running_since = Some(now),
            (false, _) => entry.running_since = None,
        }
    }

    /// Stop following the current song and hand back its play.
    #[must_use = "the returned play is this song's history and is lost if dropped"]
    pub fn finish(&self, played_at: i64) -> Option<Play> {
        self.current
            .lock()
            .take()
            .map(|entry| entry.into_play(played_at))
    }

    /// Forget listening accumulated before a history clear while continuing to
    /// follow the current song from this point onward. When `song_id` is set,
    /// only reset that song's in-flight entry.
    pub fn reset_progress(&self, song_id: Option<&str>) {
        let mut slot = self.current.lock();
        let Some(entry) = slot.as_mut() else { return };
        if song_id.is_some_and(|id| id != entry.song_id) {
            return;
        }
        entry.listened_secs = 0.0;
        entry.running_since = None;
    }

    /// Which song is being followed, if any.
    pub fn song_id(&self) -> Option<String> {
        self.current.lock().as_ref().map(|e| e.song_id.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    /// Drive the tracker as the ticker would, without waiting in real time:
    /// `tick` measures the wall clock, so tests that need many seconds of
    /// "listening" set the accumulator directly instead.
    fn listen(tracker: &PlayTracker, secs: f64) {
        let mut slot = tracker.current.lock();
        slot.as_mut().unwrap().listened_secs = secs;
    }

    fn begin(tracker: &PlayTracker, duration: f64) {
        assert!(tracker.begin("song", duration, None, None, 100).is_none());
    }

    #[test]
    fn a_song_heard_past_the_threshold_counts() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);
        listen(&tracker, MIN_PLAY_SECS + 1.0);

        let play = tracker.finish(500).unwrap();
        assert!(play.counted);
        assert_eq!(play.played_at, 500);
        assert!((play.fraction - 26.0 / 200.0).abs() < 1e-6);
    }

    #[test]
    fn a_song_dropped_before_the_threshold_is_a_skip() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);
        listen(&tracker, MIN_PLAY_SECS - 1.0);

        let play = tracker.finish(500).unwrap();
        assert!(!play.counted);
        // Still recorded: an abandoned song is information, not nothing.
        assert!(play.seconds_played > 0.0);
    }

    /// A jingle shorter than the threshold has to be able to count, or it
    /// could be played forever and never appear anywhere.
    #[test]
    fn a_song_shorter_than_the_threshold_counts_when_finished() {
        let tracker = PlayTracker::new();
        begin(&tracker, 10.0);
        listen(&tracker, 9.5);

        let play = tracker.finish(500).unwrap();
        assert!(play.counted);
        assert!((play.fraction - 0.95).abs() < 1e-6);
    }

    #[test]
    fn half_of_a_short_song_is_still_a_skip() {
        let tracker = PlayTracker::new();
        begin(&tracker, 10.0);
        listen(&tracker, 5.0);

        assert!(!tracker.finish(500).unwrap().counted);
    }

    #[test]
    fn clearing_progress_keeps_tracking_without_restoring_old_history() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);
        listen(&tracker, 60.0);

        tracker.reset_progress(None);
        assert_eq!(tracker.song_id().as_deref(), Some("song"));
        let play = tracker.finish(500).unwrap();
        assert_eq!(play.seconds_played, 0.0);
        assert!(!play.counted);
    }

    #[test]
    fn clearing_another_songs_progress_leaves_the_current_entry_alone() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);
        listen(&tracker, 60.0);

        tracker.reset_progress(Some("another-song"));
        assert_eq!(tracker.finish(500).unwrap().seconds_played, 60.0);
    }

    #[test]
    fn beginning_another_song_hands_back_the_previous_one() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);
        listen(&tracker, 60.0);

        let previous = tracker
            .begin("second", 200.0, None, None, 500)
            .expect("the first song's play");
        assert_eq!(previous.song_id, "song");
        assert!(previous.counted);
        assert_eq!(tracker.song_id().as_deref(), Some("second"));

        // The new song starts from nothing rather than inheriting.
        let next = tracker.finish(600).unwrap();
        assert_eq!(next.song_id, "second");
        assert_eq!(next.seconds_played, 0.0);
    }

    #[test]
    fn time_only_accumulates_while_playing() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);

        // Paused: ticks pass, nothing accrues.
        for _ in 0..3 {
            tracker.tick(false);
            sleep(Duration::from_millis(10));
        }
        assert_eq!(tracker.current.lock().as_ref().unwrap().listened_secs, 0.0);

        // Playing: the gap between consecutive ticks is counted.
        tracker.tick(true);
        sleep(Duration::from_millis(30));
        tracker.tick(true);
        let heard = tracker.current.lock().as_ref().unwrap().listened_secs;
        assert!(heard >= 0.02, "expected ~30ms of listening, got {heard}");
        assert!(heard < 0.5, "counted far more than elapsed: {heard}");
    }

    /// The first tick after resuming must not bill the whole pause.
    #[test]
    fn resuming_does_not_count_the_time_spent_paused() {
        let tracker = PlayTracker::new();
        begin(&tracker, 200.0);

        tracker.tick(true);
        sleep(Duration::from_millis(20));
        tracker.tick(false); // paused; marker cleared

        sleep(Duration::from_millis(60)); // a long pause

        tracker.tick(true); // resumed; only re-marks
        let after_resume = tracker.current.lock().as_ref().unwrap().listened_secs;
        tracker.tick(true);
        let heard = tracker.current.lock().as_ref().unwrap().listened_secs;

        assert!(
            heard - after_resume < 0.03,
            "the pause leaked into the count: {after_resume} -> {heard}"
        );
    }

    #[test]
    fn a_single_tick_cannot_contribute_more_than_its_cap() {
        let tracker = PlayTracker::new();
        begin(&tracker, 6000.0);

        // Stand in for the machine having been asleep between two ticks.
        {
            let mut slot = tracker.current.lock();
            let entry = slot.as_mut().unwrap();
            entry.running_since = Some(Instant::now() - Duration::from_secs(3600));
        }
        tracker.tick(true);

        let heard = tracker.current.lock().as_ref().unwrap().listened_secs;
        assert!(
            heard <= MAX_TICK_SECS,
            "an hour of sleep was counted: {heard}"
        );
    }

    #[test]
    fn a_song_of_unknown_length_still_counts_on_time_alone() {
        let tracker = PlayTracker::new();
        begin(&tracker, 0.0);
        listen(&tracker, MIN_PLAY_SECS + 5.0);

        let play = tracker.finish(500).unwrap();
        assert!(play.counted);
        assert_eq!(play.fraction, 0.0);
    }

    #[test]
    fn there_is_nothing_to_report_when_nothing_was_playing() {
        let tracker = PlayTracker::new();
        assert!(tracker.finish(100).is_none());
        tracker.tick(true);
        assert!(tracker.finish(100).is_none());
    }

    #[test]
    fn the_playback_context_travels_with_the_play() {
        let tracker = PlayTracker::new();
        assert!(tracker
            .begin(
                "song",
                200.0,
                Some("playlist".into()),
                Some("pl-1".into()),
                100,
            )
            .is_none());
        listen(&tracker, 60.0);

        let play = tracker.finish(500).unwrap();
        assert_eq!(play.context_kind.as_deref(), Some("playlist"));
        assert_eq!(play.context_id.as_deref(), Some("pl-1"));
    }
}
