//! Ambience beds: rain, vinyl crackle, fireplace and friends.
//!
//! Built-in beds are packaged with the application. Users can add custom
//! ambience files to the app-data `filters` directory; matching IDs override
//! the packaged file. All beds are decoded once, kept in memory at the device
//! sample rate, and looped underneath the music.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::audio::decode::decode_whole;
use crate::audio::dsp::{db_to_gain, Biquad, Smoothed, CHANNELS};
use crate::audio::params::{BandKind, Filter};

/// Beds the UI offers out of the box. A bed with no matching file is shown
/// greyed out rather than hidden, so it is obvious what to supply.
pub const BUILT_IN: &[(&str, &str, &str)] = &[
    ("rain", "Rain", "rain.mp3"),
    ("fireplace", "Fireplace", "fireplace.mp3"),
    ("forest", "Forest", "forest.mp3"),
    ("city", "City", "city.mp3"),
    ("ocean", "Ocean", "ocean.mp3"),
    ("vinyl", "Vinyl Crackle", "vinyl_crackle.mp3"),
];

const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "mp3", "ogg", "m4a", "aiff", "aif", "opus"];

pub fn is_supported_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| AUDIO_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterInfo {
    pub id: String,
    pub name: String,
    pub built_in: bool,
    /// False when no audio file has been supplied for this bed yet.
    pub available: bool,
    pub path: Option<String>,
}

/// Decoded, device-rate, stereo-interleaved beds keyed by ambience ID.
pub type Bank = HashMap<String, Arc<Vec<f32>>>;

/// Lists packaged ambience plus custom app-data ambience. A custom file with
/// a built-in ID takes precedence over the packaged file while retaining its
/// built-in classification.
pub fn catalogue(bundled_dir: Option<&Path>, custom_dir: &Path) -> Vec<FilterInfo> {
    let built_in: HashMap<&str, PathBuf> = bundled_dir
        .into_iter()
        .flat_map(|dir| {
            BUILT_IN
                .iter()
                .map(move |(id, _, file)| (*id, dir.join(file)))
        })
        .filter_map(|(id, path)| path.is_file().then_some((id, path)))
        .collect();
    let custom = audio_files(custom_dir);

    let mut out: Vec<FilterInfo> = BUILT_IN
        .iter()
        .map(|(id, name, _)| {
            let path = custom.get(*id).or_else(|| built_in.get(*id));
            FilterInfo {
                id: (*id).to_string(),
                name: (*name).to_string(),
                built_in: true,
                available: path.is_some(),
                path: path.map(|path| path.display().to_string()),
            }
        })
        .collect();

    // Anything the user added beyond the built-in set shows up too.
    let mut extra: Vec<_> = custom
        .iter()
        .filter(|(id, _)| !BUILT_IN.iter().any(|(built_in_id, _, _)| built_in_id == id))
        .collect();
    extra.sort_by(|a, b| a.0.cmp(b.0));
    for (id, path) in extra {
        out.push(FilterInfo {
            id: id.clone(),
            name: titleise(id),
            built_in: false,
            available: true,
            path: Some(path.display().to_string()),
        });
    }
    out
}

fn audio_files(dir: &Path) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !is_supported_audio(&path) {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
                found.insert(slugify(stem), path);
            }
        }
    }
    found
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

/// Tracks which ambience beds have been asked for and when.
///
/// Deliberately not a permanent "already requested" memo. A decode can fail, a
/// file can be replaced, and a bed can leave the bank entirely when the
/// listener hides or deletes it — a memo would leave that atmosphere silent
/// for the rest of the session with no way back, which is exactly the failure
/// this exists to prevent. Requests repeat on an interval until the bed lands.
#[derive(Debug, Default)]
pub struct BedRequests {
    asked: HashMap<String, Instant>,
}

impl BedRequests {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether `id` should be asked for now, recording the request if so.
    pub fn due(&mut self, id: &str, now: Instant, retry: Duration) -> bool {
        let due = self
            .asked
            .get(id)
            .map(|last| now.duration_since(*last) >= retry)
            .unwrap_or(true);
        if due {
            self.asked.insert(id.to_string(), now);
        }
        due
    }

    /// Forget anything that has since arrived, so the map cannot grow without
    /// bound and a bed removed later is asked for again from scratch.
    pub fn settled(&mut self, bank: &Bank) {
        self.asked.retain(|id, _| !bank.contains_key(id));
    }
}

/// One ambience bed currently sounding, with its own playhead and gain glide.
struct ActiveBed {
    id: String,
    samples: Arc<Vec<f32>>,
    pos: usize,
    gain: Smoothed,
    tone: [Biquad; CHANNELS],
    /// Set when the ambience bed has been switched off and is fading out.
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

    /// Reconcile the sounding ambience beds with the requested list.
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

    /// IDs of enabled ambience beds that are not in the bank yet, so the caller can
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
    fn supported_audio_extensions_are_case_insensitive() {
        assert!(is_supported_audio(Path::new("bed.FLAC")));
        assert!(!is_supported_audio(Path::new("notes.txt")));
    }

    #[test]
    fn catalogue_resolves_packaged_built_in_assets() {
        let root = temp_dir("packaged-ambience");
        let bundled = root.join("bundled");
        let custom = root.join("custom");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(bundled.join("rain.mp3"), []).unwrap();
        std::fs::write(bundled.join("vinyl_crackle.mp3"), []).unwrap();

        let ambience = catalogue(Some(&bundled), &custom);
        let rain = ambience.iter().find(|item| item.id == "rain").unwrap();
        let vinyl = ambience.iter().find(|item| item.id == "vinyl").unwrap();

        assert!(rain.built_in && rain.available);
        assert_eq!(rain.path.as_deref(), bundled.join("rain.mp3").to_str());
        assert!(vinyl.built_in && vinyl.available);
        assert_eq!(
            vinyl.path.as_deref(),
            bundled.join("vinyl_crackle.mp3").to_str()
        );

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn custom_ambience_overrides_packaged_built_in_asset() {
        let root = temp_dir("ambience-override");
        let bundled = root.join("bundled");
        let custom = root.join("custom");
        std::fs::create_dir_all(&bundled).unwrap();
        std::fs::create_dir_all(&custom).unwrap();
        std::fs::write(bundled.join("rain.mp3"), []).unwrap();
        std::fs::write(custom.join("rain.wav"), []).unwrap();
        std::fs::write(custom.join("coffee-shop.mp3"), []).unwrap();

        let ambience = catalogue(Some(&bundled), &custom);
        let rain = ambience.iter().find(|item| item.id == "rain").unwrap();
        let coffee = ambience
            .iter()
            .find(|item| item.id == "coffee-shop")
            .unwrap();

        assert!(rain.built_in && rain.available);
        assert_eq!(rain.path.as_deref(), custom.join("rain.wav").to_str());
        assert!(!coffee.built_in && coffee.available);
        assert_eq!(coffee.name, "Coffee Shop");

        std::fs::remove_dir_all(root).unwrap();
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "pnm-{prefix}-{}",
            crate::library::model::stable_id("d", &format!("{:?}", std::time::Instant::now()))
        ))
    }

    const RETRY: Duration = Duration::from_secs(2);

    #[test]
    fn a_bed_is_only_asked_for_once_while_it_is_still_arriving() {
        let mut requests = BedRequests::new();
        let start = Instant::now();

        assert!(requests.due("rain", start, RETRY));
        // Decoding takes a moment; the worker runs hundreds of blocks in it.
        assert!(!requests.due("rain", start + Duration::from_millis(10), RETRY));
        assert!(!requests.due("rain", start + Duration::from_millis(500), RETRY));
    }

    #[test]
    fn a_bed_that_never_arrives_is_asked_for_again() {
        let mut requests = BedRequests::new();
        let start = Instant::now();

        assert!(requests.due("rain", start, RETRY));
        // A decode that failed, or a request that went nowhere, must not leave
        // the atmosphere silent for the rest of the session.
        assert!(requests.due("rain", start + RETRY, RETRY));
    }

    /// The bug this type exists for: hiding or deleting an atmosphere drops it
    /// from the bank, and turning it back on has to fetch it again.
    #[test]
    fn a_bed_removed_from_the_bank_is_asked_for_from_scratch() {
        let mut requests = BedRequests::new();
        let start = Instant::now();
        assert!(requests.due("rain", start, RETRY));

        // It arrives, so it stops being chased.
        let mut bank = Bank::new();
        bank.insert("rain".into(), Arc::new(vec![0.0; 8]));
        requests.settled(&bank);

        // It is then removed. Asking again must not be blocked by the earlier
        // request, and must not have to wait out the retry interval either.
        bank.remove("rain");
        requests.settled(&bank);
        assert!(requests.due("rain", start + Duration::from_millis(1), RETRY));
    }

    #[test]
    fn arrived_beds_are_forgotten_so_the_map_cannot_grow_forever() {
        let mut requests = BedRequests::new();
        let now = Instant::now();
        for id in ["rain", "forest", "city"] {
            assert!(requests.due(id, now, RETRY));
        }

        let mut bank = Bank::new();
        bank.insert("rain".into(), Arc::new(vec![0.0; 8]));
        bank.insert("forest".into(), Arc::new(vec![0.0; 8]));
        requests.settled(&bank);

        assert_eq!(requests.asked.len(), 1);
        assert!(requests.asked.contains_key("city"));
    }

    #[test]
    fn each_bed_is_chased_independently() {
        let mut requests = BedRequests::new();
        let start = Instant::now();
        assert!(requests.due("rain", start, RETRY));
        // A second atmosphere turned on later gets its own first ask straight
        // away rather than inheriting the first one's timer.
        assert!(requests.due("fireplace", start + Duration::from_millis(10), RETRY));
        assert!(!requests.due("rain", start + Duration::from_millis(10), RETRY));
    }

    /// A bed enabled before its audio has decoded must join once it lands,
    /// rather than being missed because it was absent on the first block.
    #[test]
    fn a_bed_joins_on_a_later_block_when_its_audio_arrives_late() {
        let mut mixer = AmbienceMixer::new();
        mixer.prepare(48000.0);
        let wanted = vec![Filter {
            id: "rain".into(),
            enabled: true,
            volume: 1.0,
            tone_hz: 20000.0,
        }];

        let empty = Bank::new();
        mixer.sync(&wanted, &empty);
        assert!(mixer.is_silent());
        assert_eq!(mixer.missing(&wanted, &empty), vec!["rain"]);

        let mut bank = Bank::new();
        bank.insert("rain".into(), Arc::new(vec![1.0; 8]));
        mixer.sync(&wanted, &bank);

        assert!(!mixer.is_silent());
        assert!(mixer.missing(&wanted, &bank).is_empty());
    }

    #[test]
    fn switching_an_atmosphere_off_and_on_again_keeps_it_sounding() {
        let mut mixer = AmbienceMixer::new();
        mixer.prepare(48000.0);
        let mut bank = Bank::new();
        bank.insert("rain".into(), Arc::new(vec![1.0; 8]));

        let on = vec![Filter {
            id: "rain".into(),
            enabled: true,
            volume: 1.0,
            tone_hz: 20000.0,
        }];
        let off = vec![Filter {
            enabled: false,
            ..on[0].clone()
        }];

        mixer.sync(&on, &bank);
        let mut buf = vec![vec![0.0f32; 64]; CHANNELS];
        mixer.process(&mut buf, 64);
        assert!(buf[0][63].abs() > 0.0);

        // Off: it fades rather than cutting, so it is still there mid-glide.
        mixer.sync(&off, &bank);
        mixer.process(&mut buf, 64);

        // Back on before the fade finished: it must recover rather than being
        // left stuck in a retiring state.
        mixer.sync(&on, &bank);
        let mut after = vec![vec![0.0f32; 64]; CHANNELS];
        mixer.process(&mut after, 64);
        assert!(
            after[0][63].abs() > 0.0,
            "the atmosphere went silent after being toggled off and on"
        );
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
