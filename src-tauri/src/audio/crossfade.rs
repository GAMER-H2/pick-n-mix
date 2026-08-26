//! The global crossfade curve.
//!
//! Two envelopes, one per song, sharing a single time axis anchored at the
//! outgoing song's own natural end (`x = 0`). Negative `x` is "before that
//! song ends"; positive `x` only means anything for the incoming song, since
//! the outgoing one has no audio to play there.
//!
//! Four points, in the same spirit as the lofi sketch this was drawn from:
//! when the outgoing song starts fading, when it finishes (reaches silence),
//! when the incoming song starts becoming audible, and when it reaches full
//! volume. Because the outgoing song cannot play past its own end, its two
//! points are constrained to `x <= 0`; the incoming song's final point is the
//! only one allowed to sit after the boundary, since it is free to keep
//! rising once the outgoing song is gone.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrossfadeCurve {
    /// When the outgoing song starts fading out. Always <= `fade_out_end`.
    pub fade_out_start: f32,
    /// When the outgoing song reaches silence. Always <= 0: a track cannot
    /// play, let alone fade, past its own end.
    pub fade_out_end: f32,
    /// When the incoming song starts becoming audible. Always <= 0.
    pub fade_in_start: f32,
    /// When the incoming song reaches full volume. May be positive.
    pub fade_in_end: f32,
}

impl CrossfadeCurve {
    /// The classic overlapping crossfade: both songs ramp across the whole
    /// window, meeting at the boundary. This is what the simple length slider
    /// alone produces; the advanced graph lets it be pulled apart into a gap,
    /// an instant handoff, or a longer overlap on one side than the other.
    pub fn symmetric(length_secs: f32) -> Self {
        let length = length_secs.max(0.0);
        CrossfadeCurve {
            fade_out_start: -length,
            fade_out_end: 0.0,
            fade_in_start: -length,
            fade_in_end: 0.0,
        }
    }

    /// Keep the curve physically playable and internally ordered after an
    /// edit: each song's own two points stay ordered, the outgoing song's
    /// points stay at or before its own end, and no point sits further than
    /// `length_secs` before the boundary.
    pub fn clamp(&self, length_secs: f32) -> Self {
        let length = length_secs.max(0.0);
        let floor = -length;

        let fade_out_end = self.fade_out_end.clamp(floor, 0.0);
        let fade_out_start = self.fade_out_start.clamp(floor, fade_out_end);

        let fade_in_start = self.fade_in_start.clamp(floor, 0.0);
        let fade_in_end = self.fade_in_end.clamp(fade_in_start, length);

        CrossfadeCurve { fade_out_start, fade_out_end, fade_in_start, fade_in_end }
    }

    /// Equal-power gain for the outgoing song at time `x`.
    ///
    /// Quarter-cosine rather than linear or smoothstep: two independent
    /// linear or smoothstep ramps sum to roughly a 3 dB dip in the middle of
    /// an overlap for uncorrelated material, the classic audible crossfade
    /// sag. Equal-power avoids it in the symmetric case, and stays the
    /// least-wrong choice once the two sides are pulled apart.
    pub fn gain_out(&self, x: f32) -> f32 {
        ease_down(inverse_lerp(self.fade_out_start, self.fade_out_end, x))
    }

    /// Equal-power gain for the incoming song at time `x`.
    pub fn gain_in(&self, x: f32) -> f32 {
        ease_up(inverse_lerp(self.fade_in_start, self.fade_in_end, x))
    }

    /// How long before the boundary the earliest fade in this curve begins.
    /// The engine uses this to decide when to start preparing the next track.
    pub fn lead_secs(&self) -> f32 {
        (-self.fade_out_start.min(self.fade_in_start)).max(0.0)
    }
}

impl Default for CrossfadeCurve {
    fn default() -> Self {
        CrossfadeCurve::symmetric(0.0)
    }
}

/// 0 before `start`, 1 after `end`, linearly interpolated between, clamped.
/// A degenerate (`start == end`) window snaps straight to the far side,
/// rather than dividing by zero, so an instant handoff is well-defined.
fn inverse_lerp(start: f32, end: f32, x: f32) -> f32 {
    if end <= start {
        return if x < start { 0.0 } else { 1.0 };
    }
    ((x - start) / (end - start)).clamp(0.0, 1.0)
}

fn ease_down(t: f32) -> f32 {
    (t * std::f32::consts::FRAC_PI_2).cos()
}

fn ease_up(t: f32) -> f32 {
    (t * std::f32::consts::FRAC_PI_2).sin()
}

/// The user-facing crossfade setting.
///
/// Global only, not layered through the [`crate::audio::params`] mixer
/// cascade: a crossfade happens between two tracks that may belong to two
/// different playlists, so there is no single track it could sensibly attach
/// to. Kept out of `MixerSettings` for a second reason too — that struct is
/// captured wholesale into saved presets, and a crossfade length has no
/// business travelling along with a EQ/reverb preset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CrossfadeSettings {
    /// 0 disables crossfading: tracks change with an instant cut, as before
    /// this feature existed.
    pub length_secs: f32,
    pub curve: CrossfadeCurve,
}

impl Default for CrossfadeSettings {
    fn default() -> Self {
        CrossfadeSettings { length_secs: 0.0, curve: CrossfadeCurve::default() }
    }
}

impl CrossfadeSettings {
    pub fn enabled(&self) -> bool {
        self.length_secs > 0.01
    }

    pub fn lead_secs(&self) -> f32 {
        if self.enabled() {
            self.curve.lead_secs()
        } else {
            0.0
        }
    }

    /// Apply a new length from the simple slider.
    ///
    /// If the curve has not been customised in the advanced graph, the result
    /// stays exactly symmetric at the new length — dragging the slider should
    /// keep behaving the same way whether or not the graph has ever been
    /// opened. If it has been customised, points are scaled proportionally so
    /// a gap or an asymmetric overlap keeps its shape rather than snapping
    /// back to symmetric.
    pub fn with_length(&self, length_secs: f32) -> Self {
        let length_secs = length_secs.max(0.0);

        if self.curve == CrossfadeCurve::symmetric(self.length_secs) {
            return CrossfadeSettings { length_secs, curve: CrossfadeCurve::symmetric(length_secs) };
        }

        let old_length = self.length_secs.max(1e-6);
        let scale = length_secs / old_length;
        let curve = CrossfadeCurve {
            fade_out_start: self.curve.fade_out_start * scale,
            fade_out_end: self.curve.fade_out_end * scale,
            fade_in_start: self.curve.fade_in_start * scale,
            fade_in_end: self.curve.fade_in_end * scale,
        }
        .clamp(length_secs);
        CrossfadeSettings { length_secs, curve }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symmetric_curve_is_full_at_the_start_and_silent_at_the_boundary() {
        let curve = CrossfadeCurve::symmetric(5.0);
        assert_eq!(curve.fade_out_start, -5.0);
        assert_eq!(curve.fade_out_end, 0.0);
        assert_eq!(curve.fade_in_start, -5.0);
        assert_eq!(curve.fade_in_end, 0.0);

        assert!((curve.gain_out(-5.0) - 1.0).abs() < 1e-6);
        assert!(curve.gain_out(0.0).abs() < 1e-6);
        assert!(curve.gain_in(-5.0).abs() < 1e-6);
        assert!((curve.gain_in(0.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn symmetric_curve_is_equal_power_at_every_point() {
        // sin^2 + cos^2 == 1 for any shared t, which is exactly what the
        // quarter-cosine curves give when both sides share one window.
        let curve = CrossfadeCurve::symmetric(6.0);
        for i in 0..=20 {
            let x = -6.0 + i as f32 * 0.3;
            let power = curve.gain_out(x).powi(2) + curve.gain_in(x).powi(2);
            assert!((power - 1.0).abs() < 1e-4, "x={x} power={power}");
        }
    }

    #[test]
    fn outside_the_window_gains_stay_flat() {
        let curve = CrossfadeCurve::symmetric(4.0);
        assert_eq!(curve.gain_out(-10.0), 1.0, "long before the fade, still full");
        // A quarter-cosine's endpoint is ~0 rather than exactly 0 in f32.
        assert!(curve.gain_out(1.0).abs() < 1e-6, "past the boundary, the outgoing song is gone");
        assert_eq!(curve.gain_in(-10.0), 0.0, "long before the fade, still silent");
        assert_eq!(curve.gain_in(10.0), 1.0, "long after, fully up");
    }

    #[test]
    fn a_track_can_never_be_asked_to_play_past_its_own_end() {
        // Attempt to push both of the outgoing song's points into the future.
        let broken = CrossfadeCurve {
            fade_out_start: 2.0,
            fade_out_end: 3.0,
            fade_in_start: 1.0,
            fade_in_end: 4.0,
        };
        let clamped = broken.clamp(5.0);
        assert!(clamped.fade_out_end <= 0.0);
        assert!(clamped.fade_out_start <= clamped.fade_out_end);
        assert!(clamped.fade_in_start <= 0.0);
    }

    #[test]
    fn clamp_keeps_each_songs_own_points_ordered() {
        let reversed = CrossfadeCurve {
            fade_out_start: -1.0,
            fade_out_end: -4.0,
            fade_in_start: -1.0,
            fade_in_end: -4.0,
        };
        let clamped = reversed.clamp(5.0);
        assert!(clamped.fade_out_start <= clamped.fade_out_end);
        assert!(clamped.fade_in_start <= clamped.fade_in_end);
    }

    #[test]
    fn clamp_does_not_forbid_a_gap_or_an_overlap() {
        // A silent gap in the middle: the sketch's literal shape.
        let gap = CrossfadeCurve {
            fade_out_start: -3.0,
            fade_out_end: -1.0,
            fade_in_start: 1.0,
            fade_in_end: 3.0,
        }
        .clamp(5.0);
        assert!(gap.fade_out_end < 0.0 || gap.fade_in_start <= 0.0);
        // Both songs silent between fade_out_end and fade_in_start.
        assert!(gap.gain_out(gap.fade_out_end) < 1e-6);
        assert!(gap.gain_in(gap.fade_out_end) < 1e-6);

        // A long overlap where both are audible at once: point2 > point3.
        let overlap = CrossfadeCurve {
            fade_out_start: -4.0,
            fade_out_end: -0.5,
            fade_in_start: -3.5,
            fade_in_end: 0.0,
        }
        .clamp(5.0);
        let mid = -2.0;
        assert!(overlap.gain_out(mid) > 0.0 && overlap.gain_in(mid) > 0.0);
    }

    #[test]
    fn zero_length_collapses_every_point_to_the_boundary() {
        let curve = CrossfadeCurve::symmetric(10.0).clamp(0.0);
        assert_eq!(curve, CrossfadeCurve::default());
    }

    #[test]
    fn dragging_the_simple_slider_stays_symmetric() {
        let settings = CrossfadeSettings::default().with_length(4.0).with_length(8.0);
        assert_eq!(settings.curve, CrossfadeCurve::symmetric(8.0));
    }

    #[test]
    fn dragging_the_slider_after_a_custom_edit_scales_the_shape() {
        let mut settings = CrossfadeSettings::default().with_length(4.0);
        settings.curve.fade_in_start = -1.0; // half the window: a customised gap.
        let doubled = settings.with_length(8.0);
        // The gap's relative position (a quarter of the way through the
        // outgoing fade) should carry over rather than resetting to symmetric.
        assert!((doubled.curve.fade_in_start - -2.0).abs() < 1e-4);
        assert_ne!(doubled.curve, CrossfadeCurve::symmetric(8.0));
    }

    #[test]
    fn lead_time_is_the_earliest_of_the_two_starts() {
        let curve = CrossfadeCurve {
            fade_out_start: -6.0,
            fade_out_end: -1.0,
            fade_in_start: -3.0,
            fade_in_end: 2.0,
        };
        assert_eq!(curve.lead_secs(), 6.0);
    }

    #[test]
    fn disabled_settings_have_no_lead_time() {
        assert_eq!(CrossfadeSettings::default().lead_secs(), 0.0);
        assert!(!CrossfadeSettings::default().enabled());
    }
}
