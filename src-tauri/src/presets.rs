//! Mixer presets: the built-in set plus whatever the user saves.
//!
//! A preset is just a partial `MixerSettings`, so it layers through the same
//! cascade as everything else. A preset that only mentions reverb leaves the
//! EQ exactly as the user left it.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::audio::params::{
    default_bands, BandKind, Delay, Eq, EqBand, Lofi, MixerSettings, Normalisation, Pitch, Reverb,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    /// Built-ins cannot be deleted or overwritten.
    pub built_in: bool,
    pub settings: MixerSettings,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            id: String::new(),
            name: "Untitled".into(),
            built_in: false,
            settings: MixerSettings::default(),
        }
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
    Eq { enabled: true, preamp_db: 0.0, bands }
}

pub fn built_ins() -> Vec<Preset> {
    vec![
        Preset {
            id: "flat".into(),
            name: "Flat".into(),
            built_in: true,
            settings: MixerSettings {
                enabled: Some(true),
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
            settings: MixerSettings {
                enabled: Some(true),
                // Slightly slow and low, the classic lo-fi treatment.
                pitch: Some(Pitch { semitones: -1.0, cents: 0.0 }),
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
                normalisation: Some(Normalisation { enabled: true, ..Default::default() }),
                ..Default::default()
            },
        },
        Preset {
            id: "chopped".into(),
            name: "Chopped & Screwed".into(),
            built_in: true,
            settings: MixerSettings {
                enabled: Some(true),
                pitch: Some(Pitch { semitones: -4.0, cents: 0.0 }),
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
            settings: MixerSettings {
                enabled: Some(true),
                pitch: Some(Pitch { semitones: 4.0, cents: 0.0 }),
                eq: Some(shelved_eq(1.0, 0.0, 2.0)),
                ..Default::default()
            },
        },
        Preset {
            id: "club".into(),
            name: "Club".into(),
            built_in: true,
            settings: MixerSettings {
                enabled: Some(true),
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
            settings: MixerSettings {
                enabled: Some(true),
                eq: Some(Eq {
                    enabled: true,
                    preamp_db: -1.0,
                    bands: vec![
                        EqBand { kind: BandKind::LowShelf, freq: 90.0, gain_db: -3.0, q: 0.7, enabled: true },
                        EqBand { kind: BandKind::Peak, freq: 300.0, gain_db: -2.0, q: 1.0, enabled: true },
                        EqBand { kind: BandKind::Peak, freq: 1800.0, gain_db: 3.5, q: 0.9, enabled: true },
                        EqBand { kind: BandKind::Peak, freq: 3500.0, gain_db: 4.0, q: 1.1, enabled: true },
                        EqBand { kind: BandKind::HighShelf, freq: 9000.0, gain_db: 1.5, q: 0.7, enabled: true },
                    ],
                }),
                ..Default::default()
            },
        },
        Preset {
            id: "cathedral".into(),
            name: "Cathedral".into(),
            built_in: true,
            settings: MixerSettings {
                enabled: Some(true),
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
            settings: MixerSettings {
                enabled: Some(true),
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
            settings: MixerSettings {
                enabled: Some(true),
                eq: Some(Eq {
                    enabled: true,
                    preamp_db: 2.0,
                    bands: vec![
                        EqBand { kind: BandKind::HighPass, freq: 500.0, gain_db: 0.0, q: 0.8, enabled: true },
                        EqBand { kind: BandKind::Peak, freq: 1600.0, gain_db: 6.0, q: 1.2, enabled: true },
                        EqBand { kind: BandKind::LowPass, freq: 3600.0, gain_db: 0.0, q: 0.8, enabled: true },
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

/// Add or replace a user preset by name.
pub fn upsert(path: &Path, name: &str, settings: MixerSettings) -> Result<Vec<Preset>> {
    let mut user = load_user(path);
    let id = crate::library::model::stable_id("ps", &crate::library::model::normalise(name));
    match user.iter_mut().find(|p| p.id == id) {
        Some(existing) => {
            existing.name = name.to_string();
            existing.settings = settings;
        }
        None => user.push(Preset {
            id,
            name: name.to_string(),
            built_in: false,
            settings,
        }),
    }
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
        assert!(all.iter().any(|p| p.name == "Lo-Fi Study"));
    }

    #[test]
    fn a_preset_only_touches_the_sections_it_names() {
        // Tape Echo says nothing about EQ, so the user's EQ must survive it.
        let tape = built_ins().into_iter().find(|p| p.id == "tape-echo").unwrap();
        assert!(tape.settings.eq.is_none());

        let user_eq = MixerSettings { eq: Some(shelved_eq(6.0, 0.0, 0.0)), ..Default::default() };
        let resolved = MixerSettings::resolve(&[&user_eq, &tape.settings]);
        assert_eq!(resolved.eq.bands[0].gain_db, 6.0, "the user's EQ survived");
        assert!(resolved.delay.enabled, "the preset's delay applied");
    }

    #[test]
    fn saving_a_preset_then_reloading_returns_it() {
        let path = tempfile();
        let settings = MixerSettings {
            reverb: Some(Reverb { enabled: true, mix: 0.66, ..Default::default() }),
            ..Default::default()
        };
        upsert(&path, "My Mix", settings).unwrap();

        let all = load_all(&path);
        let mine = all.iter().find(|p| p.name == "My Mix").unwrap();
        assert!(!mine.built_in);
        assert_eq!(mine.settings.reverb.as_ref().unwrap().mix, 0.66);
    }

    #[test]
    fn saving_the_same_name_twice_replaces_rather_than_duplicates() {
        let path = tempfile();
        upsert(&path, "My Mix", MixerSettings::default()).unwrap();
        let all = upsert(
            &path,
            "My Mix",
            MixerSettings {
                reverb: Some(Reverb { enabled: true, mix: 0.9, ..Default::default() }),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(all.iter().filter(|p| p.name == "My Mix").count(), 1);
    }

    #[test]
    fn built_ins_cannot_be_deleted() {
        let path = tempfile();
        upsert(&path, "My Mix", MixerSettings::default()).unwrap();
        let all = delete(&path, "lofi-study").unwrap();
        assert!(all.iter().any(|p| p.id == "lofi-study"), "built-ins are not stored on disk");

        let id = crate::library::model::stable_id("ps", "my mix");
        let all = delete(&path, &id).unwrap();
        assert!(!all.iter().any(|p| p.name == "My Mix"));
    }
}
