/** Mirrors the Rust types in `src-tauri/src`. Keep the two in step. */

export type ThemePreference = "system" | "light" | "dark";
export type FadeMode = "off" | "play" | "pause" | "both";

/** Durable visual, playback, and recommendation preferences. */
export interface AppPreferences {
  theme: ThemePreference;
  accent: string;
  fadeMode: FadeMode;
  /** Let reverb and delay tails ring out after a pause. */
  keepReverbOnPause: boolean;
  /** Output device by name; empty means the system default. */
  outputDevice: string;
  mixLength: number;
  replayDays: number;
  replayMinPlays: number;
  archiveDays: number;
  archiveMinPlays: number;
  discoverMaxPlays: number;
  hiddenBuiltInPresetIds: string[];
  hiddenBuiltInFilterIds: string[];
}

interface TrackFields {
  sourceId: string;
  location: string;
  title: string;
  artist: string;
  albumArtist: string;
  album: string;
  trackNumber: number | null;
  discNumber: number | null;
  year: number | null;
  genre: string | null;
  durationSecs: number;
  sampleRate: number | null;
  channels: number | null;
  bitsPerSample: number | null;
  bitrateKbps: number | null;
  fileSize: number | null;
  format: string | null;
  artworkId: string | null;
  musicbrainzRecordingId: string | null;
  musicbrainzReleaseId: string | null;
  gainDb: number | null;
  addedAt: number;
}

/** A merged song returned by library and playback commands. */
export interface Track extends TrackFields {
  id: string;
  fileCount: number;
  missingFileCount: number;
  effectiveFileId: string | null;
  preferredFileId: string | null;
}

/** One physical version of a merged song. */
export interface TrackFile extends TrackFields {
  id: string;
  songId: string;
  modifiedAt: number;
  available: boolean;
  missing: boolean;
  preferred: boolean;
  effective: boolean;
}

export interface Album {
  id: string;
  name: string;
  artist: string;
  year: number | null;
  trackCount: number;
  durationSecs: number;
  artworkId: string | null;
}

export interface Artist {
  id: string;
  name: string;
  albumCount: number;
  trackCount: number;
  artworkId: string | null;
}

export interface StreamInfo {
  sampleRate: number;
  channels: number;
  durationSecs: number;
  codec: string;
  bitsPerSample: number | null;
  bitrateKbps: number | null;
}

export interface PlaybackSnapshot {
  playing: boolean;
  positionSecs: number;
  durationSecs: number;
  volume: number;
  speed: number;
  limiterReductionDb: number;
  deviceName: string;
  deviceSampleRate: number;
  stream: StreamInfo | null;
}

export type Repeat = "off" | "all" | "one";

export interface PlayContext {
  kind: string;
  id: string;
  name: string;
}

export interface QueueView {
  items: Track[];
  currentIndex: number | null;
  upcoming: Track[];
  shuffle: boolean;
  repeat: Repeat;
  context: PlayContext | null;
}

// -- mixer -------------------------------------------------------------------

export type BandKind = "lowShelf" | "peak" | "highShelf" | "lowPass" | "highPass";

export interface EqBand {
  kind: BandKind;
  freq: number;
  gainDb: number;
  q: number;
  enabled: boolean;
}

export interface Eq {
  enabled: boolean;
  preampDb: number;
  bands: EqBand[];
}

/**
 * One frame of the output spectrum, tapped after the whole effect chain.
 *
 * The axis travels with the data rather than being duplicated as constants on
 * this side, so the UI cannot disagree with the engine about what a bin means.
 */
export interface AnalyserFrame {
  /** Magnitudes in dBFS, log-spaced from `minHz` to `maxHz`. */
  bins: number[];
  minHz: number;
  maxHz: number;
  floorDb: number;
}

export interface Pitch {
  semitones: number;
  cents: number;
}

export interface Reverb {
  enabled: boolean;
  size: number;
  damping: number;
  width: number;
  mix: number;
  predelayMs: number;
}

export interface Delay {
  enabled: boolean;
  timeMs: number;
  feedback: number;
  mix: number;
  toneHz: number;
  spread: number;
}

export interface Normalisation {
  enabled: boolean;
  targetDb: number;
  gainDb: number;
  limiterEnabled: boolean;
  limiterCeilingDb: number;
  limiterReleaseMs: number;
}

export interface Lofi {
  enabled: boolean;
  sampleRateHz: number;
  bitDepth: number;
  mix: number;
}

export interface FilterSetting {
  id: string;
  enabled: boolean;
  volume: number;
  toneHz: number;
}

/** A partial layer of the cascade. Missing sections fall through. */
export interface MixerSettings {
  enabled?: boolean | null;
  preset?: string | null;
  pitch?: Pitch | null;
  eq?: Eq | null;
  reverb?: Reverb | null;
  delay?: Delay | null;
  normalisation?: Normalisation | null;
  lofi?: Lofi | null;
  /** Global default or playlist override; entry layers are ignored by the backend. */
  crossfade?: CrossfadeSettings | null;
  filters?: FilterSetting[] | null;
  [extra: string]: unknown;
}

/** The cascade collapsed: every field populated. */
export interface ResolvedMixer {
  enabled: boolean;
  pitch: Pitch;
  eq: Eq;
  reverb: Reverb;
  delay: Delay;
  normalisation: Normalisation;
  lofi: Lofi;
  crossfade: CrossfadeSettings;
  filters: FilterSetting[];
}

export type PresetKind = "mixer" | "eq";

export interface Preset {
  id: string;
  name: string;
  builtIn: boolean;
  kind: PresetKind;
  settings: MixerSettings;
}

export interface FilterInfo {
  id: string;
  name: string;
  builtIn: boolean;
  available: boolean;
  path: string | null;
}

export interface MixerState {
  global: MixerSettings;
  context: MixerSettings | null;
  track: MixerSettings | null;
  effective: ResolvedMixer;
  presets: Preset[];
  filters: FilterInfo[];
}

// -- crossfade -----------------------------------------------------------
//
// A global mixer section with an optional playlist override. Entry-level
// mixer settings deliberately cannot override it because a crossfade spans
// two playlist entries.

/**
 * Four points sharing one time axis anchored at the outgoing song's own
 * natural end (`x = 0`, seconds). Negative is "before that song ends";
 * positive only ever applies to the incoming song, which is free to keep
 * rising after the outgoing one is gone.
 */
export interface CrossfadeCurve {
  /** When the outgoing song starts fading out. Always <= fadeOutEnd. */
  fadeOutStart: number;
  /** When the outgoing song reaches silence. Always <= 0. */
  fadeOutEnd: number;
  /** When the incoming song starts becoming audible. Always <= 0. */
  fadeInStart: number;
  /** When the incoming song reaches full volume. May be positive. */
  fadeInEnd: number;
  /** Time-shape exponent for the outgoing envelope; 1 preserves equal-power timing. */
  fadeOutShape: number;
  /** Time-shape exponent for the incoming envelope; 1 preserves equal-power timing. */
  fadeInShape: number;
}

export interface CrossfadeSettings {
  /** 0 disables crossfading: tracks change with an instant cut. */
  lengthSecs: number;
  curve: CrossfadeCurve;
}

// -- playlists ---------------------------------------------------------------

export interface PlaylistSummary {
  id: string;
  name: string;
  description: string;
  trackCount: number;
  artwork: string | null;
  hasMixer: boolean;
  /** Whether a timeline has ever been built for this playlist. */
  hasMasterMix: boolean;
  /** Whether that timeline is what plays. */
  masterMixEnabled: boolean;
  /** Ignore the stored order and reshuffle on every play. */
  shuffleOnly: boolean;
  path: string;
}

// -- master mix ---------------------------------------------------------------
//
// The Logic-style timeline behind a playlist. Mirrors `src-tauri/src/master_mix.rs`;
// see that file for what each field means and what keeps it in range.

/** Where a block's audio comes from: a playlist entry, or an imported file. */
export type BlockSource =
  | { kind: "entry"; index: number }
  | { kind: "asset"; file: string };

export interface AutomationPoint {
  /** Seconds from the start of the block, so a point survives a drag. */
  atSecs: number;
  gainDb: number;
  /** Shape of the segment leaving this point; 1 is linear. */
  curve: number;
}

export interface MixBlock {
  id: string;
  source: BlockSource;
  /** Absolute position on the timeline. */
  startSecs: number;
  /** How far into the source this block begins. */
  offsetSecs: number;
  durationSecs: number;
  gainDb: number;
  fadeInSecs: number;
  fadeOutSecs: number;
  mixer: MixerSettings | null;
  automation: AutomationPoint[];
  [extra: string]: unknown;
}

export interface MixLane {
  id: string;
  name: string;
  muted: boolean;
  soloed: boolean;
  gainDb: number;
  blocks: MixBlock[];
  [extra: string]: unknown;
}

export interface MasterMix {
  enabled: boolean;
  /** Bumped on every saved edit, so caches can tell they are stale. */
  revision: number;
  lanes: MixLane[];
  [extra: string]: unknown;
}

/** One playlist entry as the timeline needs it. */
export interface MixEntry {
  index: number;
  title: string;
  artist: string;
  artworkId: string | null;
  /** The song's full length: the longest a block of it can be. */
  durationSecs: number;
  /** False when nothing in this library matched; its blocks are silent. */
  available: boolean;
}

export interface MasterMixView {
  playlistId: string;
  playlistName: string;
  mix: MasterMix;
  entries: MixEntry[];
  durationSecs: number;
  /** False for the default arrangement of a playlist never mixed before. */
  saved: boolean;
}

/** Peak magnitudes for drawing one song's waveform. */
export interface Waveform {
  /** 0-255, one per 1/peaksPerSec of audio. */
  peaks: number[];
  peaksPerSec: number;
  durationSecs: number;
}

// -- home ---------------------------------------------------------------------

/** The generated mixes, in the order the home page shows them. */
export type MixKind = "replay" | "archive" | "discover";

export interface MixSummary {
  kind: MixKind;
  name: string;
  description: string;
  trackCount: number;
  /** Covers of the first few songs, for the card's artwork. */
  artworkIds: string[];
  pinned: boolean;
}

/** One recommendation, which is either a song or a whole album. */
export interface HomePick {
  kind: "song" | "album";
  /** Song id, or the stable album id the album view routes by. */
  id: string;
  title: string;
  subtitle: string;
  artworkId: string | null;
  /** Why this was picked, shown next to it. */
  reason: string;
  trackIds: string[];
}

export interface HomeShelves {
  mixes: MixSummary[];
  picks: HomePick[];
  recentPlaylists: PlaylistSummary[];
  /** Counted plays overall, to tell "no history yet" from "empty shelf". */
  playTotal: number;
}

export interface Play {
  songId: string;
  playedAt: number;
  secondsPlayed: number;
  fraction: number;
  /** Whether this passed the bar to count as a play rather than a skip. */
  counted: boolean;
  contextKind: string | null;
  contextId: string | null;
}

export interface PlayRecord {
  play: Play;
  /** Null once the song has left the library — history outlives it. */
  track: Track | null;
}

export interface PlaylistEntry {
  title: string;
  artist: string;
  album: string;
  albumArtist: string;
  durationSecs: number;
  trackNumber: number | null;
  discNumber: number | null;
  year: number | null;
  musicbrainzRecordingId: string | null;
  localPath: string | null;
  mixer: MixerSettings | null;
  addedAt: number;
}

export interface ResolvedEntry {
  index: number;
  entry: PlaylistEntry;
  /** Null when nothing in this library matched. */
  track: Track | null;
}

export interface ResolvedPlaylist {
  id: string;
  name: string;
  description: string;
  artwork: string | null;
  createdAt: number;
  updatedAt: number;
  shuffleOnly: boolean;
  mixer: MixerSettings | null;
  /** Null until the master mixer has been opened for this playlist. */
  masterMix: MasterMix | null;
  items: ResolvedEntry[];
  missingCount: number;
}

export interface ScanReport {
  scanned: number;
  added: number;
  updated: number;
  skipped: number;
  errors: string[];
}
