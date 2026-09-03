/** Typed wrappers over the Tauri command surface. */

import { invoke } from "@tauri-apps/api/core";
import type {
  Album,
  AnalyserFrame,
  AppPreferences,
  Artist,
  CrossfadeCurve,
  CrossfadeSettings,
  FfmpegStatus,
  FilterInfo,
  HomeShelves,
  BounceOptions,
  MasterMix,
  MasterMixNowPlaying,
  MasterMixView,
  MixAsset,
  MixerSettings,
  MixerState,
  MixKind,
  MixSummary,
  PlayRecord,
  PlaybackSnapshot,
  PlayContext,
  PlaylistSummary,
  Preset,
  PresetKind,
  QueueView,
  Repeat,
  ResolvedMixer,
  ResolvedPlaylist,
  ScanReport,
  StreamInfo,
  Track,
  TrackFile,
  Waveform,
} from "./types";

// -- library ---------------------------------------------------------------
export const listFolders = () => invoke<string[]>("list_folders");
export const addFolder = (path: string) =>
  invoke<string[]>("add_folder", { path });
export const removeFolder = (path: string) =>
  invoke<string[]>("remove_folder", { path });
export const scanLibrary = () => invoke<ScanReport>("scan_library");
export const listTracks = () => invoke<Track[]>("list_tracks");
export const listAlbums = () => invoke<Album[]>("list_albums");
export const listArtists = () => invoke<Artist[]>("list_artists");
export const albumTracks = (albumId: string) =>
  invoke<Track[]>("album_tracks", { albumId });
export const artistTracks = (artistId: string) =>
  invoke<Track[]>("artist_tracks", { artistId });
export const getTrack = (id: string) =>
  invoke<Track | null>("get_track", { id });
export const search = (query: string) => invoke<Track[]>("search", { query });
export const enrichTrack = (id: string) =>
  invoke<Track | null>("enrich_track", { id });

// -- duplicate files -------------------------------------------------------
export const listTrackFiles = (songId: string) =>
  invoke<TrackFile[]>("list_track_files", { songId });
export const setPreferredTrackFile = (songId: string, fileId: string | null) =>
  invoke<Track>("set_preferred_track_file", { songId, fileId });
export const previewTrackFile = (songId: string, fileId: string) =>
  invoke<void>("preview_track_file", { songId, fileId });
export const stopTrackFilePreview = () =>
  invoke<void>("stop_track_file_preview");
export const restoreNeedsDestination = (path: string) =>
  invoke<boolean>("restore_needs_destination", { path });
export const relinkTrackFile = (
  fileId: string,
  path: string,
  destinationFolder: string | null,
) => invoke<Track>("relink_track_file", { fileId, path, destinationFolder });
export const trashTrackFile = (fileId: string) =>
  invoke<Track | null>("trash_track_file", { fileId });
export const forgetMissingTrackFile = (fileId: string) =>
  invoke<Track | null>("forget_missing_track_file", { fileId });

// -- playback --------------------------------------------------------------
export const playTracks = (request: {
  trackIds: string[];
  startIndex?: number;
  context?: PlayContext | null;
  contextMixer?: MixerSettings | null;
}) => invoke<void>("play_tracks", { request });
/** Jump to a queue row, optionally to a time inside it — a mix's chapter. */
export const playQueueIndex = (index: number, positionSecs?: number) =>
  invoke<void>("play_queue_index", { index, positionSecs });
export const togglePlay = () => invoke<boolean>("toggle_play");
export const nextTrack = () => invoke<void>("next_track");
export const previousTrack = () => invoke<void>("previous_track");
export const seek = (positionSecs: number) =>
  invoke<void>("seek", { positionSecs });
export const setVolume = (volume: number) =>
  invoke<void>("set_volume", { volume });
export const playbackState = () => invoke<PlaybackSnapshot>("playback_state");
export const streamInfo = () => invoke<StreamInfo | null>("stream_info");

/** The spectrum is only maintained while something is drawing it. */
export const setAnalyserEnabled = (enabled: boolean) =>
  invoke<void>("set_analyser_enabled", { enabled });
export const analyserFrame = () => invoke<AnalyserFrame>("analyser_frame");

// -- queue -----------------------------------------------------------------
export const queueState = () => invoke<QueueView>("queue_state");
export const currentTrack = () => invoke<Track | null>("current_track");
export const playNext = (trackIds: string[]) =>
  invoke<void>("play_next", { trackIds });
export const addToQueue = (trackIds: string[]) =>
  invoke<void>("add_to_queue", { trackIds });
export const removeFromQueue = (index: number) =>
  invoke<void>("remove_from_queue", { index });
export const moveInQueue = (from: number, to: number) =>
  invoke<void>("move_in_queue", { from, to });
export const clearQueue = () => invoke<void>("clear_queue");
export const setShuffle = (enabled: boolean) =>
  invoke<void>("set_shuffle", { enabled });
export const setRepeat = (mode: Repeat) => invoke<void>("set_repeat", { mode });

// -- settings --------------------------------------------------------------
export const appPreferences = () => invoke<AppPreferences>("app_preferences");
export const setAppPreferences = (preferences: AppPreferences) =>
  invoke<AppPreferences>("set_app_preferences", { preferences });
/** Output devices available right now, by name. */
export const outputDevices = () => invoke<string[]>("output_devices");

// -- mixer -----------------------------------------------------------------
export const mixerState = () => invoke<MixerState>("mixer_state");
/** Just the cascade; no disk access, safe to call on every track change. */
export const mixerLayers = () =>
  invoke<Pick<MixerState, "global" | "context" | "track" | "effective">>(
    "mixer_layers",
  );
export const setGlobalMixer = (settings: MixerSettings) =>
  invoke<ResolvedMixer>("set_global_mixer", { settings });
export const setPlaylistMixer = (
  playlistId: string,
  settings: MixerSettings | null,
) => invoke<void>("set_playlist_mixer", { playlistId, settings });
export const listPresets = () => invoke<Preset[]>("list_presets");
export const savePreset = (
  name: string,
  settings: MixerSettings,
  kind: PresetKind = "mixer",
) => invoke<Preset[]>("save_preset", { name, settings, kind });
export const deletePreset = (id: string) =>
  invoke<Preset[]>("delete_preset", { id });
export const updatePreset = (
  id: string,
  name: string,
  settings: MixerSettings,
) => invoke<Preset[]>("update_preset", { id, name, settings });
export const listFilters = () => invoke<FilterInfo[]>("list_filters");
export const filtersDirectory = () => invoke<string>("filters_directory");
export const importFilter = (sourcePath: string) =>
  invoke<FilterInfo[]>("import_filter", { sourcePath });
export const deleteFilter = (id: string) =>
  invoke<FilterInfo[]>("delete_filter", { id });

// -- crossfade ---------------------------------------------------------------
// Global only, never part of the mixer cascade above.
export const crossfadeSettings = () =>
  invoke<CrossfadeSettings>("crossfade_settings");
export const setCrossfadeLength = (lengthSecs: number) =>
  invoke<CrossfadeSettings>("set_crossfade_length", { lengthSecs });
export const setCrossfadeCurve = (curve: CrossfadeCurve) =>
  invoke<CrossfadeSettings>("set_crossfade_curve", { curve });

// -- home ------------------------------------------------------------------
export const homeShelves = () => invoke<HomeShelves>("home_shelves");
export const mixTracks = (kind: MixKind) =>
  invoke<Track[]>("mix_tracks", { kind });
export const playMix = (kind: MixKind, startIndex?: number) =>
  invoke<void>("play_mix", { kind, startIndex });
/** Discard the held mixes so the next request builds them from fresh history. */
export const refreshMixes = () => invoke<void>("refresh_mixes");
export const listPinnedMixes = () => invoke<MixSummary[]>("list_pinned_mixes");
export const setMixPinned = (kind: MixKind, pinned: boolean) =>
  invoke<MixKind[]>("set_mix_pinned", { kind, pinned });
/** Freeze a mix into a playlist: a new one when `playlistId` is omitted. */
export const saveMixToPlaylist = (
  kind: MixKind,
  playlistId?: string,
  name?: string,
) =>
  invoke<PlaylistSummary>("save_mix_to_playlist", { kind, playlistId, name });

export const listeningHistory = (limit?: number) =>
  invoke<PlayRecord[]>("listening_history", { limit });
export const clearListeningHistory = () =>
  invoke<void>("clear_listening_history");
export const clearListeningHistoryForSong = (songId: string) =>
  invoke<void>("clear_listening_history_for_song", { songId });

// -- playlists -------------------------------------------------------------
export const listPlaylists = () => invoke<PlaylistSummary[]>("list_playlists");
export const getPlaylist = (id: string) =>
  invoke<ResolvedPlaylist | null>("get_playlist", { id });
export const createPlaylist = (name: string, description?: string) =>
  invoke<PlaylistSummary>("create_playlist", { name, description });
export const updatePlaylist = (
  id: string,
  name?: string,
  description?: string,
) => invoke<void>("update_playlist", { id, name, description });
export const deletePlaylist = (id: string) =>
  invoke<void>("delete_playlist", { id });
export const addToPlaylist = (playlistId: string, trackIds: string[]) =>
  invoke<number>("add_to_playlist", { playlistId, trackIds });
export const removeFromPlaylist = (playlistId: string, index: number) =>
  invoke<void>("remove_from_playlist", { playlistId, index });
export const moveInPlaylist = (playlistId: string, from: number, to: number) =>
  invoke<void>("move_in_playlist", { playlistId, from, to });
export const setPlaylistEntryMixer = (
  playlistId: string,
  index: number,
  settings: MixerSettings | null,
) => invoke<void>("set_playlist_entry_mixer", { playlistId, index, settings });
export const setPlaylistShuffleOnly = (id: string, enabled: boolean) =>
  invoke<void>("set_playlist_shuffle_only", { id, enabled });
export const setPlaylistArtwork = (id: string, sourcePath: string) =>
  invoke<string>("set_playlist_artwork", { id, sourcePath });
export const clearPlaylistArtwork = (id: string) =>
  invoke<void>("clear_playlist_artwork", { id });
/** Queue a whole playlist: its songs, or its mix as one block. */
export const queuePlaylist = (playlistId: string, next: boolean) =>
  invoke<void>("queue_playlist", { playlistId, next });

/** Queue one entry, carrying that playlist's mixer for this play only. */
export const queuePlaylistEntry = (
  playlistId: string,
  index: number,
  next: boolean,
) => invoke<void>("queue_playlist_entry", { playlistId, index, next });
export const importPlaylist = (path: string) =>
  invoke<string>("import_playlist", { path });
export const exportPlaylist = (id: string, destination: string) =>
  invoke<void>("export_playlist", { id, destination });
export const playPlaylist = (id: string, startIndex?: number) =>
  invoke<void>("play_playlist", { id, startIndex });

// -- master mix ------------------------------------------------------------
// The timeline is edited whole: the modal owns the arrangement while it is
// open and hands it back, rather than there being a command per drag.
export const masterMix = (playlistId: string) =>
  invoke<MasterMixView>("master_mix", { playlistId });
export const setMasterMix = (playlistId: string, mix: MasterMix) =>
  invoke<MasterMixView>("set_master_mix", { playlistId, mix });
export const setMasterMixEnabled = (playlistId: string, enabled: boolean) =>
  invoke<MasterMixView>("set_master_mix_enabled", { playlistId, enabled });
export const resetMasterMix = (playlistId: string) =>
  invoke<MasterMixView>("reset_master_mix", { playlistId });
/** Peaks for one song, cached on disk; slow only the first time. */
export const entryWaveform = (playlistId: string, index: number) =>
  invoke<Waveform>("entry_waveform", { playlistId, index });
/** Capture normal playback for the lifetime of an open Master Mixer. */
/** The playlist being played as a mix, or null when a normal track plays. */
export const masterMixNowPlaying = () =>
  invoke<MasterMixNowPlaying | null>("master_mix_now_playing");
export const beginMasterMixSession = () =>
  invoke<string>("begin_master_mix_session");
/** Close the editor and restore the playback captured by its matching session. */
export const endMasterMixSession = (token: string) =>
  invoke<boolean>("end_master_mix_session", { token });
/** Audition the arrangement, including edits that have not been saved yet. */
export const playMasterMix = (
  playlistId: string,
  mix: MasterMix | null,
  positionSecs: number,
  token: string,
) =>
  invoke<number>("play_master_mix", { playlistId, mix, positionSecs, token });
export const setMasterMixPlaying = (playing: boolean, token: string) =>
  invoke<boolean>("set_master_mix_playing", { playing, token });
export const stopMasterMix = (token: string) =>
  invoke<void>("stop_master_mix", { token });
export const importMixAsset = (playlistId: string, path: string) =>
  invoke<MixAsset>("import_mix_asset", { playlistId, path });
export const assetWaveform = (playlistId: string, file: string) =>
  invoke<Waveform>("asset_waveform", { playlistId, file });
/** Start a render. Returns the job id its progress events carry. */
export const bounceMasterMix = (
  playlistId: string,
  destination: string,
  options: BounceOptions,
) => invoke<string>("bounce_master_mix", { playlistId, destination, options });
/**
 * Whether an ffmpeg is present, and whether it can write MP3.
 *
 * MP3 is the one bounce format this app does not carry its own encoder for,
 * so the dialog asks before offering it rather than failing at the end of a
 * long render. `refresh` re-probes for someone who has just installed it.
 */
export const ffmpegStatus = (refresh = false) =>
  invoke<FfmpegStatus>("ffmpeg_status", { refresh });
