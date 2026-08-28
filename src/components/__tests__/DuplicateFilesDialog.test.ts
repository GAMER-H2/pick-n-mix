import { beforeEach, describe, expect, it, vi } from "vitest";
import { mount, type VueWrapper } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import DuplicateFilesDialog from "../DuplicateFilesDialog.vue";
import type { Track, TrackFile } from "@/lib/types";
import { useLibraryStore } from "@/stores/library";
import { useUiStore } from "@/stores/ui";

const listTrackFiles = vi.fn();
const setPreferredTrackFile = vi.fn();
const previewTrackFile = vi.fn();
const stopTrackFilePreview = vi.fn();
const restoreNeedsDestination = vi.fn();
const relinkTrackFile = vi.fn();
const trashTrackFile = vi.fn();
const forgetMissingTrackFile = vi.fn();
const listTracks = vi.fn();
const listAlbums = vi.fn();
const listArtists = vi.fn();
const listFolders = vi.fn();
const openFile = vi.fn();

vi.mock("@/lib/api", () => ({
  listTrackFiles: (...args: unknown[]) => listTrackFiles(...args),
  setPreferredTrackFile: (...args: unknown[]) => setPreferredTrackFile(...args),
  previewTrackFile: (...args: unknown[]) => previewTrackFile(...args),
  stopTrackFilePreview: (...args: unknown[]) => stopTrackFilePreview(...args),
  restoreNeedsDestination: (...args: unknown[]) => restoreNeedsDestination(...args),
  relinkTrackFile: (...args: unknown[]) => relinkTrackFile(...args),
  trashTrackFile: (...args: unknown[]) => trashTrackFile(...args),
  forgetMissingTrackFile: (...args: unknown[]) => forgetMissingTrackFile(...args),
  listTracks: (...args: unknown[]) => listTracks(...args),
  listAlbums: (...args: unknown[]) => listAlbums(...args),
  listArtists: (...args: unknown[]) => listArtists(...args),
  listFolders: (...args: unknown[]) => listFolders(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openFile(...args),
}));

function song(overrides: Partial<Track> = {}): Track {
  return {
    id: "song-1",
    sourceId: "local",
    location: "/Music/album/song.flac",
    title: "Duplicate Song",
    artist: "The Artist",
    albumArtist: "The Artist",
    album: "The Album",
    trackNumber: 1,
    discNumber: 1,
    year: 2024,
    genre: "Rock",
    durationSecs: 201,
    sampleRate: 96000,
    channels: 2,
    bitsPerSample: 24,
    bitrateKbps: 2800,
    fileSize: 70_000_000,
    format: "FLAC",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 1,
    fileCount: 2,
    missingFileCount: 0,
    effectiveFileId: "file-1",
    preferredFileId: null,
    ...overrides,
  };
}

function trackFile(overrides: Partial<TrackFile> = {}): TrackFile {
  return {
    id: "file-1",
    songId: "song-1",
    sourceId: "local",
    location: "/Music/album/song.flac",
    title: "Duplicate Song",
    artist: "The Artist",
    albumArtist: "The Artist",
    album: "The Album",
    trackNumber: 1,
    discNumber: 1,
    year: 2024,
    genre: "Rock",
    durationSecs: 201,
    sampleRate: 96000,
    channels: 2,
    bitsPerSample: 24,
    bitrateKbps: 2800,
    fileSize: 70_000_000,
    format: "FLAC",
    artworkId: null,
    musicbrainzRecordingId: null,
    musicbrainzReleaseId: null,
    gainDb: null,
    addedAt: 1,
    modifiedAt: 2,
    available: true,
    missing: false,
    preferred: false,
    effective: true,
    ...overrides,
  };
}

async function settle() {
  await Promise.resolve();
  await Promise.resolve();
  await new Promise((resolve) => window.setTimeout(resolve, 0));
}

async function mountDialog(currentSong: Track, files: TrackFile[]): Promise<VueWrapper> {
  listTrackFiles.mockResolvedValueOnce(files);
  const wrapper = mount(DuplicateFilesDialog);
  useUiStore().duplicateTrack = currentSong;
  await settle();
  return wrapper;
}

function buttonWithText(wrapper: VueWrapper, label: string) {
  return wrapper.findAll("button").find((button) => button.text().trim() === label);
}

describe("duplicate files dialog", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
    vi.resetAllMocks();
    previewTrackFile.mockResolvedValue(undefined);
    stopTrackFilePreview.mockResolvedValue(undefined);
    listTracks.mockResolvedValue([]);
    listAlbums.mockResolvedValue([]);
    listArtists.mockResolvedValue([]);
    listFolders.mockResolvedValue(["/Music", "/Archive"]);
  });

  it("shows every version, technical differences, and the missing-preference fallback", async () => {
    const files = [
      trackFile(),
      trackFile({
        id: "file-2",
        location: "/Old/song.mp3",
        format: "MP3",
        sampleRate: 44100,
        bitsPerSample: null,
        bitrateKbps: 320,
        fileSize: 8_000_000,
        available: false,
        missing: true,
        preferred: true,
        effective: false,
      }),
    ];
    const wrapper = await mountDialog(
      song({ preferredFileId: "file-2", missingFileCount: 1 }),
      files,
    );

    expect(wrapper.get("[role='dialog']").attributes("aria-modal")).toBe("true");
    expect(wrapper.text()).toContain("Duplicate Song");
    expect(wrapper.text()).toContain("best available version is being used as a fallback");
    expect(wrapper.text()).toContain("File missing");
    expect(wrapper.text()).toContain("96 kHz");
    expect(wrapper.text()).toContain("44.1 kHz");
    expect(wrapper.findAll("article.version")).toHaveLength(2);
    expect(wrapper.findAll(".technical .is-different").length).toBeGreaterThan(0);
  });

  it("switches previews and stops the temporary preview when closed", async () => {
    const wrapper = await mountDialog(song(), [
      trackFile(),
      trackFile({ id: "file-2", location: "/Music/album/song.wav", effective: false }),
    ]);

    const previewButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().trim() === "Preview");
    expect(previewButtons).toHaveLength(2);
    if (previewButtons.length !== 2) throw new Error("Expected two preview buttons");

    await previewButtons[0].trigger("click");
    await settle();
    const remainingPreview = buttonWithText(wrapper, "Preview");
    if (!remainingPreview) throw new Error("Expected the second preview button");
    await remainingPreview.trigger("click");
    await settle();

    expect(previewTrackFile.mock.calls).toEqual([
      ["song-1", "file-1"],
      ["song-1", "file-2"],
    ]);
    expect(wrapper.text()).toContain("Stop Preview");

    await wrapper.get("[aria-label='Close duplicate files']").trigger("click");
    await settle();
    expect(stopTrackFilePreview).toHaveBeenCalledTimes(1);
    expect(useUiStore().duplicateTrack).toBeNull();
  });

  it("sets a preferred version and refreshes versions and the library", async () => {
    const initialFiles = [
      trackFile(),
      trackFile({ id: "file-2", location: "/Music/album/song.wav", effective: false }),
    ];
    const updatedSong = song({ preferredFileId: "file-2", effectiveFileId: "file-2" });
    const updatedFiles = [
      trackFile({ effective: false }),
      trackFile({
        id: "file-2",
        location: "/Music/album/song.wav",
        preferred: true,
        effective: true,
      }),
    ];
    setPreferredTrackFile.mockResolvedValue(updatedSong);
    const wrapper = await mountDialog(song(), initialFiles);
    listTrackFiles.mockResolvedValueOnce(updatedFiles);

    const useButtons = wrapper
      .findAll("button")
      .filter((button) => button.text().trim() === "Use this version");
    expect(useButtons).toHaveLength(2);
    if (useButtons.length !== 2) throw new Error("Expected two version selection buttons");
    await useButtons[1].trigger("click");
    await settle();

    expect(setPreferredTrackFile).toHaveBeenCalledWith("song-1", "file-2");
    expect(listTrackFiles).toHaveBeenCalledTimes(2);
    expect(listTracks).toHaveBeenCalledTimes(1);
    expect(useUiStore().duplicateTrack?.preferredFileId).toBe("file-2");
    expect(wrapper.text()).toContain("selected manually");
  });

  it("asks for a configured destination when a located file must be restored", async () => {
    const missing = trackFile({
      id: "file-2",
      location: "/Old/song.flac",
      available: false,
      missing: true,
      effective: false,
    });
    openFile.mockResolvedValue("/Downloads/recovered.flac");
    restoreNeedsDestination.mockResolvedValue(true);
    relinkTrackFile.mockResolvedValue(song());

    const wrapper = await mountDialog(song({ missingFileCount: 1 }), [trackFile(), missing]);
    listTrackFiles.mockResolvedValueOnce([trackFile(), trackFile({ id: "file-2" })]);
    useLibraryStore().folders = ["/Music", "/Archive"];

    const locateButton = buttonWithText(wrapper, "Locate File…");
    if (!locateButton) throw new Error("Expected a locate button");
    await locateButton.trigger("click");
    await settle();

    expect(restoreNeedsDestination).toHaveBeenCalledWith("/Downloads/recovered.flac");
    expect(wrapper.text()).toContain("Choose a library folder");
    const archiveButton = buttonWithText(wrapper, "/Archive");
    if (!archiveButton) throw new Error("Expected the Archive destination");
    await archiveButton.trigger("click");
    await settle();

    expect(relinkTrackFile).toHaveBeenCalledWith(
      "file-2",
      "/Downloads/recovered.flac",
      "/Archive",
    );
    expect(listTracks).toHaveBeenCalledTimes(1);
  });
});
