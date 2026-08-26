/** Typed wrappers over the Tauri command surface. */

import { invoke } from "@tauri-apps/api/core";
import type {
  Album,
  Artist,
  FilterInfo,
  MixerSettings,
  MixerState,
  PlaybackSnapshot,
  PlayContext,
  PlaylistSummary,
  Preset,
  QueueView,
  Repeat,
  ResolvedMixer,
  ResolvedPlaylist,
  ScanReport,
  StreamInfo,
  Track,
} from "./types";

// -- library ---------------------------------------------------------------
export const listFolders = () => invoke<string[]>("list_folders");
export const addFolder = (path: string) => invoke<string[]>("add_folder", { path });
export const removeFolder = (path: string) => invoke<string[]>("remove_folder", { path });
export const scanLibrary = () => invoke<ScanReport>("scan_library");
export const listTracks = () => invoke<Track[]>("list_tracks");
export const listAlbums = () => invoke<Album[]>("list_albums");
export const listArtists = () => invoke<Artist[]>("list_artists");
export const albumTracks = (albumId: string) => invoke<Track[]>("album_tracks", { albumId });
export const artistTracks = (artistId: string) => invoke<Track[]>("artist_tracks", { artistId });
export const getTrack = (id: string) => invoke<Track | null>("get_track", { id });
export const search = (query: string) => invoke<Track[]>("search", { query });
export const enrichTrack = (id: string) => invoke<Track | null>("enrich_track", { id });

// -- playback --------------------------------------------------------------
export const playTracks = (request: {
  trackIds: string[];
  startIndex?: number;
  context?: PlayContext | null;
  contextMixer?: MixerSettings | null;
}) => invoke<void>("play_tracks", { request });
export const playQueueIndex = (index: number) => invoke<void>("play_queue_index", { index });
export const togglePlay = () => invoke<boolean>("toggle_play");
export const nextTrack = () => invoke<void>("next_track");
export const previousTrack = () => invoke<void>("previous_track");
export const seek = (positionSecs: number) => invoke<void>("seek", { positionSecs });
export const setVolume = (volume: number) => invoke<void>("set_volume", { volume });
export const playbackState = () => invoke<PlaybackSnapshot>("playback_state");
export const streamInfo = () => invoke<StreamInfo | null>("stream_info");

// -- queue -----------------------------------------------------------------
export const queueState = () => invoke<QueueView>("queue_state");
export const currentTrack = () => invoke<Track | null>("current_track");
export const playNext = (trackIds: string[]) => invoke<void>("play_next", { trackIds });
export const addToQueue = (trackIds: string[]) => invoke<void>("add_to_queue", { trackIds });
export const removeFromQueue = (index: number) => invoke<void>("remove_from_queue", { index });
export const moveInQueue = (from: number, to: number) =>
  invoke<void>("move_in_queue", { from, to });
export const clearQueue = () => invoke<void>("clear_queue");
export const setShuffle = (enabled: boolean) => invoke<void>("set_shuffle", { enabled });
export const setRepeat = (mode: Repeat) => invoke<void>("set_repeat", { mode });

// -- mixer -----------------------------------------------------------------
export const mixerState = () => invoke<MixerState>("mixer_state");
/** Just the cascade; no disk access, safe to call on every track change. */
export const mixerLayers = () =>
  invoke<Pick<MixerState, "global" | "context" | "track" | "effective">>("mixer_layers");
export const setGlobalMixer = (settings: MixerSettings) =>
  invoke<ResolvedMixer>("set_global_mixer", { settings });
export const setPlaylistMixer = (playlistId: string, settings: MixerSettings | null) =>
  invoke<void>("set_playlist_mixer", { playlistId, settings });
export const listPresets = () => invoke<Preset[]>("list_presets");
export const savePreset = (name: string, settings: MixerSettings) =>
  invoke<Preset[]>("save_preset", { name, settings });
export const deletePreset = (id: string) => invoke<Preset[]>("delete_preset", { id });
export const listFilters = () => invoke<FilterInfo[]>("list_filters");
export const filtersDirectory = () => invoke<string>("filters_directory");

// -- playlists -------------------------------------------------------------
export const listPlaylists = () => invoke<PlaylistSummary[]>("list_playlists");
export const getPlaylist = (id: string) => invoke<ResolvedPlaylist | null>("get_playlist", { id });
export const createPlaylist = (name: string, description?: string) =>
  invoke<PlaylistSummary>("create_playlist", { name, description });
export const updatePlaylist = (id: string, name?: string, description?: string) =>
  invoke<void>("update_playlist", { id, name, description });
export const deletePlaylist = (id: string) => invoke<void>("delete_playlist", { id });
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
export const importPlaylist = (path: string) => invoke<string>("import_playlist", { path });
export const exportPlaylist = (id: string, destination: string) =>
  invoke<void>("export_playlist", { id, destination });
export const playPlaylist = (id: string, startIndex?: number) =>
  invoke<void>("play_playlist", { id, startIndex });
