//! Mixer presets: the built-in set plus whatever the user saves.
//!
//! A preset contains a partial `MixerSettings`, including its crossfade
//! section, so every setting layers through the normal cascade.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::{
    crossfade::{CrossfadeCurve, CrossfadeSettings},
    params::{
        default_bands, BandKind, Delay, Eq, EqBand, Lofi, MixerSettings, Normalisation, Pitch,
        Reverb,
    },
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresetKind {
    #[default]
    Mixer,
    Eq,
}

#[derive(Debug, Clone, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// Built-ins cannot be deleted or overwritten.
    pub built_in: bool,
    pub kind: PresetKind,
    pub settings: MixerSettings,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            id: String::new(),
            name: "Untitled".into(),
            built_in: false,
            kind: PresetKind::default(),
            settings: MixerSettings::default(),
        }
    }
}

impl<'de> Deserialize<'de> for Preset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize, Default)]
        #[serde(default, rename_all = "camelCase")]
        struct StoredPreset {
            id: String,
            name: String,
            built_in: bool,
            kind: PresetKind,
            settings: MixerSettings,
            // Read the legacy location once, but only write `settings.crossfade`.
            crossfade: Option<CrossfadeSettings>,
        }

        let stored = StoredPreset::deserialize(deserializer)?;
        let mut settings = stored.settings;
        if settings.crossfade.is_none() {
            settings.crossfade = stored.crossfade;
        }
        Ok(Preset {
            id: stored.id,
            name: stored.name,
            built_in: stored.built_in,
            kind: stored.kind,
            settings,
        })
    }
}

fn shelved_eq(low_db: f32, mid_db: f32, high_db: f32) -> Eq {
    let mut bands = default_bands();
    for (i, band) in bands.iter_mut().enumerate() {
        band.gain_db = match i {
            0 | 1 => low_db,
            2 | 3 => mid_db,
            _ => high_db,
        };
    }
    Eq {
        enabled: true,
        preamp_db: 0.0,
        bands,
    }
}

pub fn built_ins() -> Vec<Preset> {
    vec![
        Preset {
            id: "flat".into(),
            name: "Flat".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                pitch: Some(Pitch::default()),
                eq: Some(Eq::default()),
                reverb: Some(Reverb::default()),
                delay: Some(Delay::default()),
                lofi: Some(Lofi::default()),
                normalisation: Some(Normalisation::default()),
                filters: Some(Vec::new()),
                ..Default::default()
            },
        },
        Preset {
            id: "lofi-study".into(),
            name: "Lo-Fi Study".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings {
                    length_secs: 2.0,
                    curve: CrossfadeCurve::symmetric(2.0),
                }),
                // Slightly slow and low, the classic lo-fi treatment.
                pitch: Some(Pitch {
                    semitones: -1.0,
                    cents: 0.0,
                }),
                eq: Some(shelved_eq(2.5, -1.0, -5.0)),
                reverb: Some(Reverb {
                    enabled: true,
                    size: 0.45,
                    damping: 0.7,
                    width: 0.8,
                    mix: 0.18,
                    predelay_ms: 15.0,
                }),
                lofi: Some(Lofi {
                    enabled: true,
                    sample_rate_hz: 16000.0,
                    bit_depth: 12.0,
                    mix: 0.7,
                }),
                normalisation: Some(Normalisation {
                    enabled: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "chopped".into(),
            name: "Chopped & Screwed".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                pitch: Some(Pitch {
                    semitones: -4.0,
                    cents: 0.0,
                }),
                eq: Some(shelved_eq(5.0, 0.0, -3.0)),
                reverb: Some(Reverb {
                    enabled: true,
                    size: 0.6,
                    damping: 0.5,
                    width: 1.0,
                    mix: 0.22,
                    predelay_ms: 25.0,
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "nightcore".into(),
            name: "Nightcore".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                pitch: Some(Pitch {
                    semitones: 4.0,
                    cents: 0.0,
                }),
                eq: Some(shelved_eq(1.0, 0.0, 2.0)),
                ..Default::default()
            },
        },
        Preset {
            id: "club".into(),
            name: "Club".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                eq: Some(shelved_eq(6.0, -1.5, 3.5)),
                normalisation: Some(Normalisation {
                    enabled: true,
                    limiter_enabled: true,
                    limiter_ceiling_db: -0.5,
                    ..Default::default()
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "vocal".into(),
            name: "Vocal Boost".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                eq: Some(Eq {
                    enabled: true,
                    preamp_db: -1.0,
                    bands: vec![
                        EqBand {
                            kind: BandKind::LowShelf,
                            freq: 90.0,
                            gain_db: -3.0,
                            q: 0.7,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::Peak,
                            freq: 300.0,
                            gain_db: -2.0,
                            q: 1.0,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::Peak,
                            freq: 1800.0,
                            gain_db: 3.5,
                            q: 0.9,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::Peak,
                            freq: 3500.0,
                            gain_db: 4.0,
                            q: 1.1,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::HighShelf,
                            freq: 9000.0,
                            gain_db: 1.5,
                            q: 0.7,
                            enabled: true,
                        },
                    ],
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "cathedral".into(),
            name: "Cathedral".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                reverb: Some(Reverb {
                    enabled: true,
                    size: 0.95,
                    damping: 0.25,
                    width: 1.0,
                    mix: 0.45,
                    predelay_ms: 60.0,
                }),
                eq: Some(shelved_eq(-1.0, 0.0, 1.0)),
                ..Default::default()
            },
        },
        Preset {
            id: "tape-echo".into(),
            name: "Tape Echo".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                delay: Some(Delay {
                    enabled: true,
                    time_ms: 375.0,
                    feedback: 0.45,
                    mix: 0.3,
                    tone_hz: 3200.0,
                    spread: 0.35,
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "am-radio".into(),
            name: "AM Radio".into(),
            built_in: true,
            kind: PresetKind::Mixer,
            settings: MixerSettings {
                enabled: Some(true),
                crossfade: Some(CrossfadeSettings::default()),
                eq: Some(Eq {
                    enabled: true,
                    preamp_db: 2.0,
                    bands: vec![
                        EqBand {
                            kind: BandKind::HighPass,
                            freq: 500.0,
                            gain_db: 0.0,
                            q: 0.8,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::Peak,
                            freq: 1600.0,
                            gain_db: 6.0,
                            q: 1.2,
                            enabled: true,
                        },
                        EqBand {
                            kind: BandKind::LowPass,
                            freq: 3600.0,
                            gain_db: 0.0,
                            q: 0.8,
                            enabled: true,
                        },
                    ],
                }),
                lofi: Some(Lofi {
                    enabled: true,
                    sample_rate_hz: 11025.0,
                    bit_depth: 10.0,
                    mix: 0.85,
                }),
                ..Default::default()
            },
        },
    ]
}

/// User presets on disk, plus the built-ins, built-ins first.
pub fn load_all(path: &Path) -> Vec<Preset> {
    let mut all = built_ins();
    all.extend(load_user(path));
    all
}

fn load_user(path: &Path) -> Vec<Preset> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    match serde_json::from_str::<Vec<Preset>>(&raw) {
        Ok(mut presets) => {
            // Guard against a hand-edited file claiming to be built in.
            for p in presets.iter_mut() {
                p.built_in = false;
            }
            presets
        }
        Err(e) => {
            eprintln!("presets: ignoring unreadable {}: {e}", path.display());
            Vec::new()
        }
    }
}

pub fn save_user(path: &Path, presets: &[Preset]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let user: Vec<&Preset> = presets.iter().filter(|p| !p.built_in).collect();
    let json = serde_json::to_string_pretty(&user).context("serialising presets")?;
    std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Add or replace a mixer preset by name.
pub fn upsert(path: &Path, name: &str, settings: MixerSettings) -> Result<Vec<Preset>> {
    upsert_with_kind(path, name, PresetKind::Mixer, settings)
}

/// Add or replace a user preset by category and name.
pub fn upsert_with_kind(
    path: &Path,
    name: &str,
    kind: PresetKind,
    mut settings: MixerSettings,
) -> Result<Vec<Preset>> {
    settings.preset = None;
    let mut user = load_user(path);
    let normalised_name = crate::library::model::normalise(name);
    let id = match kind {
        // Preserve the historical mixer ID so existing presets update in place.
        PresetKind::Mixer => crate::library::model::stable_id("ps", &normalised_name),
        PresetKind::Eq => crate::library::model::stable_id("ps", &format!("eq:{normalised_name}")),
    };
    match user
        .iter_mut()
        .find(|preset| preset.kind == kind && preset.id == id)
    {
        Some(existing) => {
            existing.name = name.to_string();
            existing.settings = settings;
        }
        None => user.push(Preset {
            id,
            name: name.to_string(),
            built_in: false,
            kind,
            settings,
        }),
    }
    save_user(path, &user)?;
    let mut all = built_ins();
    all.extend(user);
    Ok(all)
}

/// Update an existing user preset by its stable ID.
pub fn update_user(
    path: &Path,
    id: &str,
    name: &str,
    mut settings: MixerSettings,
) -> Result<Vec<Preset>> {
    if name.trim().is_empty() {
        bail!("preset name cannot be empty");
    }
    if built_ins().iter().any(|preset| preset.id == id) {
        bail!("cannot update built-in preset: {id}");
    }

    let mut user = load_user(path);
    let Some(existing) = user.iter_mut().find(|preset| preset.id == id) else {
        bail!("user preset not found: {id}");
    };
    settings.preset = None;
    existing.name = name.to_string();
    existing.settings = settings;

    save_user(path, &user)?;
    let mut all = built_ins();
    all.extend(user);
    Ok(all)
}

pub fn delete(path: &Path, id: &str) -> Result<Vec<Preset>> {
    let mut user = load_user(path);
    user.retain(|p| p.id != id);
    save_user(path, &user)?;
    let mut all = built_ins();
    all.extend(user);
    Ok(all)
}

pub fn presets_path(data_dir: &Path) -> PathBuf {
    data_dir.join("presets.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tempfile() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pnm-presets-{}",
            crate::library::model::stable_id("d", &format!("{:?}", std::time::Instant::now()))
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("presets.json")
    }

    #[test]
    fn built_ins_are_all_present_and_marked() {
        let all = built_ins();
        assert!(all.len() >= 8);
        assert!(all.iter().all(|p| p.built_in));
        assert!(all.iter().all(|p| p.kind == PresetKind::Mixer));
        assert!(all.iter().any(|p| p.name == "Lo-Fi Study"));
    }

    #[test]
    fn lofi_study_has_a_two_second_symmetric_crossfade() {
        let lofi = built_ins()
            .into_iter()
            .find(|p| p.id == "lofi-study")
            .unwrap();
        assert_eq!(
            lofi.settings.crossfade,
            Some(CrossfadeSettings {
                length_secs: 2.0,
                curve: CrossfadeCurve::symmetric(2.0),
            })
        );
        assert!(built_ins()
            .iter()
            .filter(|p| p.id != "lofi-study")
            .all(|p| p.settings.crossfade == Some(CrossfadeSettings::default())));
    }

    #[test]
    fn a_preset_only_touches_the_sections_it_names() {
        // Tape Echo says nothing about EQ, so the user's EQ must survive it.
        let tape = built_ins()
            .into_iter()
            .find(|p| p.id == "tape-echo")
            .unwrap();
        assert!(tape.settings.eq.is_none());

        let user_eq = MixerSettings {
            eq: Some(shelved_eq(6.0, 0.0, 0.0)),
            ..Default::default()
        };
        let resolved = MixerSettings::resolve(&[&user_eq, &tape.settings]);
        assert_eq!(resolved.eq.bands[0].gain_db, 6.0, "the user's EQ survived");
        assert!(resolved.delay.enabled, "the preset's delay applied");
    }

    #[test]
    fn saving_a_preset_then_reloading_returns_it() {
        let path = tempfile();
        let crossfade = CrossfadeSettings {
            length_secs: 3.5,
            curve: CrossfadeCurve {
                fade_out_start: -3.5,
                fade_out_end: -0.25,
                fade_in_start: -2.0,
                fade_in_end: 1.0,
                fade_out_shape: 1.5,
                fade_in_shape: 0.75,
            },
        };
        let settings = MixerSettings {
            preset: Some("runtime display value".into()),
            reverb: Some(Reverb {
                enabled: true,
                mix: 0.66,
                ..Default::default()
            }),
            crossfade: Some(crossfade.clone()),
            ..Default::default()
        };
        upsert(&path, "My Mix", settings).unwrap();

        let all = load_all(&path);
        let mine = all.iter().find(|p| p.name == "My Mix").unwrap();
        assert!(!mine.built_in);
        assert_eq!(mine.settings.preset, None);
        assert_eq!(mine.settings.reverb.as_ref().unwrap().mix, 0.66);
        assert_eq!(mine.settings.crossfade, Some(crossfade));
    }

    #[test]
    fn legacy_preset_without_kind_defaults_to_mixer() {
        let preset: Preset = serde_json::from_str(
            r#"{"id":"legacy","name":"Legacy","builtIn":false,"settings":{}}"#,
        )
        .unwrap();

        assert_eq!(preset.kind, PresetKind::Mixer);
        assert_eq!(
            serde_json::to_value(&preset).unwrap()["kind"],
            serde_json::json!("mixer")
        );
    }

    #[test]
    fn legacy_top_level_crossfade_migrates_to_settings() {
        let path = tempfile();
        std::fs::write(
            &path,
            r#"[{"id":"legacy","name":"Legacy","settings":{},"crossfade":{"lengthSecs":2.0,"curve":{"fadeOutStart":-2.0,"fadeOutEnd":0.0,"fadeInStart":-2.0,"fadeInEnd":0.0}}}]"#,
        )
        .unwrap();

        let legacy = load_all(&path)
            .into_iter()
            .find(|preset| preset.id == "legacy")
            .unwrap();
        assert_eq!(legacy.settings.crossfade.unwrap().length_secs, 2.0);
    }

    #[test]
    fn saving_the_same_name_twice_replaces_rather_than_duplicates() {
        let path = tempfile();
        upsert(&path, "My Mix", MixerSettings::default()).unwrap();
        let all = upsert(
            &path,
            "My Mix",
            MixerSettings {
                reverb: Some(Reverb {
                    enabled: true,
                    mix: 0.9,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.iter().filter(|p| p.name == "My Mix").count(), 1);
    }

    #[test]
    fn same_name_mixer_and_eq_presets_coexist() {
        let path = tempfile();
        let mixer_id = crate::library::model::stable_id("ps", "shared name");

        upsert(&path, "Shared Name", MixerSettings::default()).unwrap();
        let all = upsert_with_kind(
            &path,
            "Shared Name",
            PresetKind::Eq,
            MixerSettings::default(),
        )
        .unwrap();
        let matching: Vec<&Preset> = all
            .iter()
            .filter(|preset| preset.name == "Shared Name")
            .collect();

        assert_eq!(matching.len(), 2);
        assert!(matching
            .iter()
            .any(|preset| { preset.kind == PresetKind::Mixer && preset.id == mixer_id }));
        assert!(matching
            .iter()
            .any(|preset| { preset.kind == PresetKind::Eq && preset.id != mixer_id }));
    }

    #[test]
    fn updating_a_user_preset_preserves_its_id_and_replaces_its_contents() {
        let path = tempfile();
        let created =
            upsert_with_kind(&path, "Original", PresetKind::Eq, MixerSettings::default()).unwrap();
        let id = created
            .iter()
            .find(|preset| preset.name == "Original")
            .unwrap()
            .id
            .clone();

        let updated = update_user(
            &path,
            &id,
            "Renamed",
            MixerSettings {
                preset: Some("runtime display value".into()),
                reverb: Some(Reverb {
                    enabled: true,
                    mix: 0.42,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap();
        let renamed = updated.iter().find(|preset| preset.id == id).unwrap();

        assert_eq!(renamed.name, "Renamed");
        assert_eq!(renamed.kind, PresetKind::Eq);
        assert_eq!(renamed.settings.preset, None);
        assert_eq!(renamed.settings.reverb.as_ref().unwrap().mix, 0.42);
        assert!(!updated.iter().any(|preset| preset.name == "Original"));
    }

    #[test]
    fn updating_a_user_preset_rejects_empty_names_and_non_user_ids() {
        let path = tempfile();
        let created = upsert(&path, "Mine", MixerSettings::default()).unwrap();
        let id = created
            .iter()
            .find(|preset| preset.name == "Mine")
            .unwrap()
            .id
            .clone();

        assert_eq!(
            update_user(&path, &id, "  ", MixerSettings::default())
                .unwrap_err()
                .to_string(),
            "preset name cannot be empty"
        );
        assert_eq!(
            update_user(&path, "flat", "Changed", MixerSettings::default())
                .unwrap_err()
                .to_string(),
            "cannot update built-in preset: flat"
        );
        assert_eq!(
            update_user(&path, "missing", "Changed", MixerSettings::default())
                .unwrap_err()
                .to_string(),
            "user preset not found: missing"
        );
    }

    #[test]
    fn built_ins_cannot_be_deleted() {
        let path = tempfile();
        upsert(&path, "My Mix", MixerSettings::default()).unwrap();
        let all = delete(&path, "lofi-study").unwrap();
        assert!(
            all.iter().any(|p| p.id == "lofi-study"),
            "built-ins are not stored on disk"
        );

        let id = crate::library::model::stable_id("ps", "my mix");
        let all = delete(&path, &id).unwrap();
        assert!(!all.iter().any(|p| p.name == "My Mix"));
    }
}
