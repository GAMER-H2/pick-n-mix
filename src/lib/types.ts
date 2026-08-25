/** Mirrors the Rust types in `src-tauri/src`. Keep the two in step. */

export interface Track {
  id: string;
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
  filters: FilterSetting[];
}

export interface Preset {
  id: string;
  name: string;
  builtIn: boolean;
  settings: MixerSettings;
}

export interface FilterInfo {
  id: string;
  name: string;
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

// -- playlists ---------------------------------------------------------------

export interface PlaylistSummary {
  id: string;
  name: string;
  description: string;
  trackCount: number;
  artwork: string | null;
  hasMixer: boolean;
  path: string;
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
  mixer: MixerSettings | null;
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
