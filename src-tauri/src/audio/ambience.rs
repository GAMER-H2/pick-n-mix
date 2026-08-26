//! Ambience beds ("filters"): rain, vinyl crackle, fireplace and friends.
//!
//! Beds are ordinary audio files the user drops into the app's `filters`
//! directory. They are decoded once, kept in memory at the device sample rate,
//! and looped underneath the music.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::audio::decode::decode_whole;
use crate::audio::dsp::{db_to_gain, Biquad, Smoothed, CHANNELS};
use crate::audio::params::{BandKind, Filter};

/// Beds the UI offers out of the box. A bed with no matching file is shown
/// greyed out rather than hidden, so it is obvious what to supply.
pub const BUILT_IN: &[(&str, &str)] = &[
    ("rain", "Rain"),
    ("tv-static", "TV Static"),
    ("fireplace", "Fireplace"),
    ("forest", "Forest"),
    ("city", "City"),
    ("ocean", "Ocean"),
    ("vinyl", "Vinyl Crackle"),
    ("cafe", "Cafe"),
];

const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg", "m4a", "aiff", "aif", "opus"];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterInfo {
    pub id: String,
    pub name: String,
    /// False when no audio file has been supplied for this bed yet.
    pub available: bool,
    pub path: Option<String>,
}

/// Decoded, device-rate, stereo-interleaved beds keyed by filter id.
pub type Bank = HashMap<String, Arc<Vec<f32>>>;

/// Lists the beds on disk, merged with the built-in names.
pub fn catalogue(dir: &Path) -> Vec<FilterInfo> {
    let mut found: HashMap<String, PathBuf> = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_audio = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
                .unwrap_or(false);
            if !is_audio {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                found.insert(slugify(stem), path);
            }
        }
    }

    let mut out: Vec<FilterInfo> = BUILT_IN
        .iter()
        .map(|(id, name)| FilterInfo {
            id: (*id).to_string(),
            name: (*name).to_string(),
            available: found.contains_key(*id),
            path: found.get(*id).map(|p| p.display().to_string()),
        })
        .collect();

    // Anything the user added beyond the built-in set shows up too.
    let mut extra: Vec<_> = found
        .iter()
        .filter(|(id, _)| !BUILT_IN.iter().any(|(b, _)| b == id))
        .collect();
    extra.sort_by(|a, b| a.0.cmp(b.0));
    for (id, path) in extra {
        out.push(FilterInfo {
            id: id.clone(),
            name: titleise(id),
            available: true,
            path: Some(path.display().to_string()),
        });
    }
    out
}

pub fn load_bed(path: &Path, target_rate: u32) -> anyhow::Result<Arc<Vec<f32>>> {
    let samples = decode_whole(&path.to_path_buf(), target_rate)?;
    Ok(Arc::new(samples))
}

fn slugify(s: &str) -> String {
    s.trim()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

fn titleise(id: &str) -> String {
    id.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// One bed currently sounding, with its own playhead and gain glide.
struct ActiveBed {
    id: String,
    samples: Arc<Vec<f32>>,
    pos: usize,
    gain: Smoothed,
    tone: [Biquad; CHANNELS],
    /// Set when the bed has been switched off and is fading out.
    retiring: bool,
}

/// Mixes every enabled bed into the music bus.
pub struct AmbienceMixer {
    beds: Vec<ActiveBed>,
    sample_rate: f32,
}

impl AmbienceMixer {
    pub fn new() -> Self {
        AmbienceMixer {
            beds: Vec::new(),
            sample_rate: 48000.0,
        }
    }

    pub fn prepare(&mut self, sample_rate: f32) {
        self.sample_rate = sample_rate;
        self.beds.clear();
    }

    /// Reconcile the sounding beds with the requested filter list.
    pub fn sync(&mut self, wanted: &[Filter], bank: &Bank) {
        for bed in self.beds.iter_mut() {
            let still_on = wanted.iter().any(|f| f.id == bed.id && f.enabled);
            if !still_on {
                bed.retiring = true;
                bed.gain.set_target(0.0);
            }
        }

        for filter in wanted.iter().filter(|f| f.enabled) {
            let gain = db_to_gain(-20.0 * (1.0 - filter.volume.clamp(0.0, 1.0)) - 6.0);
            if let Some(bed) = self.beds.iter_mut().find(|b| b.id == filter.id) {
                bed.retiring = false;
                bed.gain.set_target(gain);
                for ch in 0..CHANNELS {
                    bed.tone[ch].set(
                        BandKind::LowPass,
                        self.sample_rate,
                        filter.tone_hz,
                        0.0,
                        0.707,
                    );
                }
                continue;
            }
            let Some(samples) = bank.get(&filter.id) else {
                continue; // Not decoded yet; it will join on a later block.
            };
            if samples.is_empty() {
                continue;
            }
            let mut smoothed = Smoothed::new(0.0);
            smoothed.prepare(self.sample_rate, 60.0);
            smoothed.set_target(gain);
            let mut tone = [Biquad::bypass(); CHANNELS];
            for t in tone.iter_mut() {
                t.set(
                    BandKind::LowPass,
                    self.sample_rate,
                    filter.tone_hz,
                    0.0,
                    0.707,
                );
            }
            self.beds.push(ActiveBed {
                id: filter.id.clone(),
                samples: Arc::clone(samples),
                pos: 0,
                gain: smoothed,
                tone,
                retiring: false,
            });
        }

        // Drop beds that have finished fading out.
        self.beds.retain(|b| !(b.retiring && b.gain.is_settled()));
    }

    /// Ids of enabled beds that are not in the bank yet, so the caller can
    /// kick off decoding for them off the audio path.
    pub fn missing<'a>(&self, wanted: &'a [Filter], bank: &Bank) -> Vec<&'a str> {
        wanted
            .iter()
            .filter(|f| f.enabled && !bank.contains_key(&f.id))
            .map(|f| f.id.as_str())
            .collect()
    }

    pub fn is_silent(&self) -> bool {
        self.beds.is_empty()
    }

    pub fn process(&mut self, buf: &mut [Vec<f32>], frames: usize) {
        for bed in self.beds.iter_mut() {
            let len = bed.samples.len();
            if len < CHANNELS {
                continue;
            }
            let total_frames = len / CHANNELS;
            for i in 0..frames {
                let g = bed.gain.next();
                for ch in 0..CHANNELS {
                    let s = bed.samples[bed.pos * CHANNELS + ch];
                    buf[ch][i] += bed.tone[ch].process(s) * g;
                }
                bed.pos += 1;
                if bed.pos >= total_frames {
                    bed.pos = 0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_handles_spaces_and_case() {
        assert_eq!(slugify("TV Static"), "tv-static");
        assert_eq!(slugify(" Rain "), "rain");
    }

    #[test]
    fn titles_are_derived_from_slugs() {
        assert_eq!(titleise("tv-static"), "Tv Static");
        assert_eq!(titleise("rain"), "Rain");
    }

    #[test]
    fn beds_loop_rather_than_running_out() {
        let mut mixer = AmbienceMixer::new();
        mixer.prepare(48000.0);
        let mut bank = Bank::new();
        // Four frames of stereo, constant value.
        bank.insert("rain".into(), Arc::new(vec![1.0; 8]));
        let wanted = vec![Filter {
            id: "rain".into(),
            enabled: true,
            volume: 1.0,
            tone_hz: 20000.0,
        }];
        mixer.sync(&wanted, &bank);

        let mut buf = vec![vec![0.0f32; 64]; CHANNELS];
        mixer.process(&mut buf, 64);
        // Every frame got contribution from the looping bed.
        assert!(buf[0][63].abs() > 0.0);
    }
}
