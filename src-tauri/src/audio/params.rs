//! Mixer parameters and the global -> playlist -> track override cascade.
//!
//! Every section is `Option`al so a playlist or track can override just the
//! parts it cares about. Crossfade is the exception: it is global or playlist
//! scoped, never an entry override. Every field inside a section has a serde
//! default so older files keep loading as new fields are added.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::audio::crossfade::CrossfadeSettings;

/// One layer of mixer settings. `None` sections fall through to the layer below.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct MixerSettings {
    /// Master bypass for this layer. `Some(false)` mutes every effect.
    pub enabled: Option<bool>,
    /// Name of the preset this layer was loaded from, for UI display only.
    pub preset: Option<String>,
    pub pitch: Option<Pitch>,
    pub eq: Option<Eq>,
    pub reverb: Option<Reverb>,
    pub delay: Option<Delay>,
    pub normalisation: Option<Normalisation>,
    pub lofi: Option<Lofi>,
    /// Crossfade is layered globally and per playlist, but never per entry.
    pub crossfade: Option<CrossfadeSettings>,
    pub filters: Option<Vec<Filter>>,
    /// Anything written by a newer version of the app is kept here and written
    /// back out untouched, so sharing a playlist between versions is lossless.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl MixerSettings {
    /// Layer `over` on top of `self`, section by section.
    pub fn overlay(&self, over: &MixerSettings) -> MixerSettings {
        let mut extra = self.extra.clone();
        for (k, v) in &over.extra {
            extra.insert(k.clone(), v.clone());
        }
        MixerSettings {
            enabled: over.enabled.or(self.enabled),
            preset: over.preset.clone().or_else(|| self.preset.clone()),
            pitch: over.pitch.clone().or_else(|| self.pitch.clone()),
            eq: over.eq.clone().or_else(|| self.eq.clone()),
            reverb: over.reverb.clone().or_else(|| self.reverb.clone()),
            delay: over.delay.clone().or_else(|| self.delay.clone()),
            normalisation: over
                .normalisation
                .clone()
                .or_else(|| self.normalisation.clone()),
            lofi: over.lofi.clone().or_else(|| self.lofi.clone()),
            crossfade: over.crossfade.clone().or_else(|| self.crossfade.clone()),
            filters: over.filters.clone().or_else(|| self.filters.clone()),
            extra,
        }
    }

    /// Collapse the cascade into a fully-populated set of values for the DSP chain.
    pub fn resolve(layers: &[&MixerSettings]) -> Resolved {
        let merged = layers
            .iter()
            .fold(MixerSettings::default(), |acc, l| acc.overlay(l));
        // Crossfade applies globally and per playlist, but never per entry.
        // Keep that scope at the resolver boundary so legacy or hand-edited
        // entry data cannot affect playback.
        let crossfade = layers
            .iter()
            .take(2)
            .fold(MixerSettings::default(), |acc, layer| acc.overlay(layer))
            .crossfade
            .unwrap_or_default();
        let on = merged.enabled.unwrap_or(true);
        Resolved {
            enabled: on,
            pitch: merged.pitch.unwrap_or_default(),
            eq: merged.eq.unwrap_or_default(),
            reverb: merged.reverb.unwrap_or_default(),
            delay: merged.delay.unwrap_or_default(),
            normalisation: merged.normalisation.unwrap_or_default(),
            lofi: merged.lofi.unwrap_or_default(),
            crossfade,
            filters: merged.filters.unwrap_or_default(),
        }
    }
}

/// Varispeed: pitch and tempo move together, as decided in the design notes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Pitch {
    /// Semitone offset. 0 is unmodified.
    pub semitones: f32,
    /// Extra fine adjustment in cents, added to `semitones`.
    pub cents: f32,
}

impl Default for Pitch {
    fn default() -> Self {
        Pitch {
            semitones: 0.0,
            cents: 0.0,
        }
    }
}

impl Pitch {
    /// Playback rate multiplier. 1.0 is unmodified.
    pub fn ratio(&self) -> f64 {
        let semis = self.semitones as f64 + self.cents as f64 / 100.0;
        (2.0f64).powf(semis / 12.0).clamp(0.25, 4.0)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum BandKind {
    LowShelf,
    Peak,
    HighShelf,
    LowPass,
    HighPass,
}

impl Default for BandKind {
    fn default() -> Self {
        BandKind::Peak
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct EqBand {
    pub kind: BandKind,
    pub freq: f32,
    pub gain_db: f32,
    pub q: f32,
    pub enabled: bool,
}

impl Default for EqBand {
    fn default() -> Self {
        EqBand {
            kind: BandKind::Peak,
            freq: 1000.0,
            gain_db: 0.0,
            q: 1.0,
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Eq {
    pub enabled: bool,
    /// Output trim applied after the band chain, to claw back headroom.
    pub preamp_db: f32,
    pub bands: Vec<EqBand>,
}

impl Default for Eq {
    fn default() -> Self {
        Eq {
            enabled: true,
            preamp_db: 0.0,
            bands: default_bands(),
        }
    }
}

/// The six bands the simple mixer's sliders drive, in the classic
/// low-to-high spread. The advanced panel edits the same list but can
/// change frequency, Q and band type as well.
pub fn default_bands() -> Vec<EqBand> {
    const FREQS: [f32; 6] = [60.0, 170.0, 500.0, 1500.0, 4000.0, 10000.0];
    FREQS
        .iter()
        .enumerate()
        .map(|(i, &freq)| EqBand {
            kind: match i {
                0 => BandKind::LowShelf,
                5 => BandKind::HighShelf,
                _ => BandKind::Peak,
            },
            freq,
            gain_db: 0.0,
            q: 0.9,
            enabled: true,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Reverb {
    pub enabled: bool,
    /// Tail length, 0..1.
    pub size: f32,
    /// High-frequency absorption, 0..1.
    pub damping: f32,
    /// Stereo spread of the tail, 0..1.
    pub width: f32,
    /// Wet/dry balance, 0..1.
    pub mix: f32,
    /// Pre-delay before the tail starts, milliseconds.
    pub predelay_ms: f32,
}

impl Default for Reverb {
    fn default() -> Self {
        Reverb {
            enabled: false,
            size: 0.5,
            damping: 0.5,
            width: 1.0,
            mix: 0.25,
            predelay_ms: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Delay {
    pub enabled: bool,
    pub time_ms: f32,
    pub feedback: f32,
    pub mix: f32,
    /// Low-pass cutoff inside the feedback path, so repeats darken.
    pub tone_hz: f32,
    /// Offsets the right channel's delay time for a ping-pong feel, 0..1.
    pub spread: f32,
}

impl Default for Delay {
    fn default() -> Self {
        Delay {
            enabled: false,
            time_ms: 350.0,
            feedback: 0.35,
            mix: 0.25,
            tone_hz: 6000.0,
            spread: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Normalisation {
    pub enabled: bool,
    /// Target level in dBFS that per-track gain aims for.
    pub target_db: f32,
    /// Manual trim on top of the computed per-track gain.
    pub gain_db: f32,
    pub limiter_enabled: bool,
    /// Ceiling the limiter holds the signal below, in dBFS.
    pub limiter_ceiling_db: f32,
    pub limiter_release_ms: f32,
}

impl Default for Normalisation {
    fn default() -> Self {
        Normalisation {
            enabled: false,
            target_db: -14.0,
            gain_db: 0.0,
            limiter_enabled: true,
            limiter_ceiling_db: -0.3,
            limiter_release_ms: 120.0,
        }
    }
}

/// The creative "Sample Rate" control: a decimator plus bit crusher.
/// Distinct from the audio device's real output rate, which lives in settings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Lofi {
    pub enabled: bool,
    /// Effective sample rate to fold down to, in Hz.
    pub sample_rate_hz: f32,
    /// Quantisation depth in bits. 16 or above is transparent.
    pub bit_depth: f32,
    pub mix: f32,
}

impl Default for Lofi {
    fn default() -> Self {
        Lofi {
            enabled: false,
            sample_rate_hz: 44100.0,
            bit_depth: 16.0,
            mix: 1.0,
        }
    }
}

/// An ambience bed layered under the music (rain, vinyl crackle, cafe, ...).
/// `id` maps to an audio file the user drops into the filters directory.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Filter {
    pub id: String,
    pub enabled: bool,
    /// Level of the bed, 0..1.
    pub volume: f32,
    /// Optional high/low pass on the bed so it can sit behind the music.
    pub tone_hz: f32,
}

impl Default for Filter {
    fn default() -> Self {
        Filter {
            id: String::new(),
            enabled: false,
            volume: 0.4,
            tone_hz: 20000.0,
        }
    }
}

/// Fully-populated settings handed to the DSP chain.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resolved {
    pub enabled: bool,
    pub pitch: Pitch,
    pub eq: Eq,
    pub reverb: Reverb,
    pub delay: Delay,
    pub normalisation: Normalisation,
    pub lofi: Lofi,
    /// Fully resolved so the audio engine can always apply a concrete setting.
    pub crossfade: CrossfadeSettings,
    pub filters: Vec<Filter>,
}

impl Default for Resolved {
    fn default() -> Self {
        MixerSettings::resolve(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_layer_wins_over_playlist_and_global() {
        let global = MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.1,
                ..Default::default()
            }),
            ..Default::default()
        };
        let playlist = MixerSettings {
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let track = MixerSettings {
            delay: Some(Delay {
                enabled: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        let r = MixerSettings::resolve(&[&global, &playlist, &track]);
        // Playlist overrides global's reverb; the track said nothing about reverb.
        assert_eq!(r.reverb.mix, 0.5);
        // The track's own delay applies.
        assert!(r.delay.enabled);
    }

    #[test]
    fn missing_sections_fall_back_to_defaults() {
        let r = MixerSettings::resolve(&[&MixerSettings::default()]);
        assert!(r.enabled);
        assert_eq!(r.pitch.ratio(), 1.0);
        assert_eq!(r.eq.bands.len(), 6);
        assert_eq!(r.crossfade, CrossfadeSettings::default());
    }

    #[test]
    fn playlist_crossfade_overrides_global_but_not_an_entry() {
        let global = MixerSettings {
            crossfade: Some(CrossfadeSettings::default().with_length(3.0)),
            ..Default::default()
        };
        let playlist = MixerSettings {
            crossfade: Some(CrossfadeSettings::default().with_length(1.5)),
            ..Default::default()
        };
        let entry = MixerSettings {
            crossfade: Some(CrossfadeSettings::default().with_length(8.0)),
            ..Default::default()
        };

        let resolved = MixerSettings::resolve(&[&global, &playlist, &entry]);
        assert_eq!(resolved.crossfade.length_secs, 1.5);
    }

    #[test]
    fn unknown_fields_survive_a_round_trip() {
        let json = r#"{"reverb":{"mix":0.7},"somethingFromTheFuture":{"a":1}}"#;
        let parsed: MixerSettings = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.reverb.as_ref().unwrap().mix, 0.7);
        let out = serde_json::to_string(&parsed).unwrap();
        assert!(out.contains("somethingFromTheFuture"));
    }

    #[test]
    fn semitones_map_to_octave_ratios() {
        assert!(
            (Pitch {
                semitones: 12.0,
                cents: 0.0
            }
            .ratio()
                - 2.0)
                .abs()
                < 1e-9
        );
        assert!(
            (Pitch {
                semitones: -12.0,
                cents: 0.0
            }
            .ratio()
                - 0.5)
                .abs()
                < 1e-9
        );
    }
}
