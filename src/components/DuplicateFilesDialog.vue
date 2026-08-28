<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import Artwork from "./Artwork.vue";
import PnmIcon from "./icons/PnmIcon.vue";
import * as api from "@/lib/api";
import { formatBytes, formatDuration, formatHz, subtitleFor } from "@/lib/format";
import type { Track, TrackFile } from "@/lib/types";
import { useLibraryStore } from "@/stores/library";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";

const ui = useUiStore();
const library = useLibraryStore();
const player = usePlayerStore();

const dialog = ref<HTMLElement | null>(null);
const versions = ref<TrackFile[]>([]);
const loading = ref(false);
const loadError = ref<string | null>(null);
const activePreviewId = ref<string | null>(null);
const busyKey = ref<string | null>(null);
const closing = ref(false);
const pendingRelink = ref<{ fileId: string; path: string } | null>(null);
let loadRequest = 0;

const song = computed(() => ui.duplicateTrack);
const songSubtitle = computed(() => {
  const current = song.value;
  if (!current) return "";
  return subtitleFor([current.artist, current.album]);
});

const effectiveReason = computed(() => {
  const current = song.value;
  if (!current?.effectiveFileId) return "No version is currently available for playback.";
  if (current.preferredFileId === current.effectiveFileId) {
    return "This version was selected manually and will be used whenever it is available.";
  }
  if (current.preferredFileId) {
    return "The preferred version is missing, so the best available version is being used as a fallback.";
  }
  return "Automatic selection is using the best available quality.";
});

type TechnicalKey =
  | "format"
  | "bitsPerSample"
  | "sampleRate"
  | "bitrateKbps"
  | "channels"
  | "fileSize"
  | "durationSecs"
  | "location";

function differs(key: TechnicalKey): boolean {
  return new Set(versions.value.map((version) => version[key])).size > 1;
}

function isMissing(version: TrackFile): boolean {
  return version.missing || !version.available;
}

function isCurrent(version: TrackFile): boolean {
  return (
    activePreviewId.value === null &&
    player.track?.id === song.value?.id &&
    version.effective
  );
}

function channelLabel(channels: number | null): string {
  if (channels === null) return "Unknown";
  if (channels === 1) return "1 (Mono)";
  if (channels === 2) return "2 (Stereo)";
  return `${channels}`;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

async function loadVersions(songId: string) {
  const request = ++loadRequest;
  loading.value = true;
  loadError.value = null;
  try {
    const files = await api.listTrackFiles(songId);
    if (request === loadRequest && song.value?.id === songId) versions.value = files;
  } catch (error) {
    if (request === loadRequest) {
      loadError.value = errorMessage(error);
      ui.notify(`Could not load duplicate files: ${errorMessage(error)}`, "error");
    }
  } finally {
    if (request === loadRequest) loading.value = false;
  }
}

async function stopPreview(force = false) {
  if (!force && activePreviewId.value === null) return;
  try {
    await api.stopTrackFilePreview();
  } finally {
    activePreviewId.value = null;
  }
}

async function preview(version: TrackFile) {
  if (busyKey.value || isMissing(version)) return;
  if (activePreviewId.value === version.id) {
    busyKey.value = `preview:${version.id}`;
    try {
      await stopPreview();
    } catch (error) {
      ui.notify(`Could not stop preview: ${errorMessage(error)}`, "error");
    } finally {
      busyKey.value = null;
    }
    return;
  }

  const current = song.value;
  if (!current) return;
  busyKey.value = `preview:${version.id}`;
  try {
    await api.previewTrackFile(current.id, version.id);
    activePreviewId.value = version.id;
  } catch (error) {
    ui.notify(`Could not preview file: ${errorMessage(error)}`, "error");
  } finally {
    busyKey.value = null;
  }
}

async function closeDialog() {
  if (closing.value) return;
  closing.value = true;
  try {
    await stopPreview(true);
  } catch (error) {
    ui.notify(`Could not restore playback: ${errorMessage(error)}`, "error");
  } finally {
    pendingRelink.value = null;
    ui.duplicateTrack = null;
    closing.value = false;
  }
}

async function mutate(
  label: string,
  action: () => Promise<Track | null>,
): Promise<boolean> {
  if (busyKey.value) return false;
  busyKey.value = label;
  try {
    await stopPreview();
    const updated = await action();

    if (updated === null) {
      await library.refresh();
      await closeDialog();
      return true;
    }

    ui.duplicateTrack = updated;
    const [files] = await Promise.all([api.listTrackFiles(updated.id), library.refresh()]);
    if (song.value?.id === updated.id) {
      versions.value = files;
      loadError.value = null;
    }
    return true;
  } catch (error) {
    ui.notify(`${label} failed: ${errorMessage(error)}`, "error");
    return false;
  } finally {
    busyKey.value = null;
  }
}

async function useVersion(version: TrackFile) {
  const current = song.value;
  if (!current || version.preferred || isMissing(version)) return;
  await mutate("Use this version", () => api.setPreferredTrackFile(current.id, version.id));
}

async function useAutomatic() {
  const current = song.value;
  if (!current || current.preferredFileId === null) return;
  await mutate("Automatic selection", () => api.setPreferredTrackFile(current.id, null));
}

async function trash(version: TrackFile) {
  if (!window.confirm(`Move “${version.location}” to Trash?`)) return;
  await mutate("Move to Trash", () => api.trashTrackFile(version.id));
}

async function forget(version: TrackFile) {
  if (!window.confirm(`Forget the missing file “${version.location}”?`)) return;
  await mutate("Forget missing file", () => api.forgetMissingTrackFile(version.id));
}

async function relink(fileId: string, path: string, destinationFolder: string | null) {
  const succeeded = await mutate("Locate file", () =>
    api.relinkTrackFile(fileId, path, destinationFolder),
  );
  if (succeeded) pendingRelink.value = null;
}

async function locate(version: TrackFile) {
  if (busyKey.value) return;
  const key = `locate:${version.id}`;
  busyKey.value = key;
  try {
    const selected = await open({
      multiple: false,
      directory: false,
      title: `Locate ${song.value?.title ?? "audio file"}`,
      filters: [
        {
          name: "Audio files",
          extensions: ["aac", "aif", "aiff", "alac", "flac", "m4a", "mp3", "ogg", "opus", "wav"],
        },
      ],
    });
    if (typeof selected !== "string") return;

    const needsDestination = await api.restoreNeedsDestination(selected);
    if (!needsDestination) {
      busyKey.value = null;
      await relink(version.id, selected, null);
      return;
    }

    if (library.folders.length === 0) {
      ui.notify("Add a library folder before restoring this file.", "error");
      return;
    }
    pendingRelink.value = { fileId: version.id, path: selected };
  } catch (error) {
    ui.notify(`Locate file failed: ${errorMessage(error)}`, "error");
  } finally {
    if (busyKey.value === key) busyKey.value = null;
  }
}

async function chooseDestination(folder: string) {
  const pending = pendingRelink.value;
  if (!pending) return;
  await relink(pending.fileId, pending.path, folder);
}

function onKeydown(event: KeyboardEvent) {
  if (event.key !== "Escape" || !song.value) return;
  event.preventDefault();
  event.stopPropagation();
  void closeDialog();
}

watch(
  () => song.value?.id ?? null,
  async (songId) => {
    pendingRelink.value = null;
    activePreviewId.value = null;
    versions.value = [];
    loadError.value = null;
    if (!songId) {
      loadRequest += 1;
      return;
    }
    await nextTick();
    dialog.value?.focus();
    await loadVersions(songId);
  },
  { immediate: true },
);

onMounted(() => window.addEventListener("keydown", onKeydown, true));
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown, true);
  if (song.value) void api.stopTrackFilePreview();
});
</script>

<template>
  <Transition name="fade">
    <div v-if="song" class="duplicate-scrim" @click.self="closeDialog">
      <section
        ref="dialog"
        class="duplicate-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="duplicate-dialog-title"
        tabindex="-1"
      >
        <header class="duplicate-dialog__header">
          <Artwork :artwork-id="song.artworkId" :size="58" :radius="7" shadow />
          <div class="duplicate-dialog__heading">
            <p class="duplicate-dialog__eyebrow">Duplicate files</p>
            <h2 id="duplicate-dialog-title" class="truncate">{{ song.title }}</h2>
            <p class="truncate" :title="songSubtitle">{{ songSubtitle }}</p>
          </div>
          <button class="icon-button" aria-label="Close duplicate files" @click="closeDialog">
            <PnmIcon name="close" :size="17" />
          </button>
        </header>

        <div class="duplicate-dialog__selection">
          <div>
            <strong>Version in use</strong>
            <p>{{ effectiveReason }}</p>
          </div>
          <button
            class="pill-button is-secondary"
            :disabled="song.preferredFileId === null || busyKey !== null"
            aria-label="Use automatic file selection"
            @click="useAutomatic"
          >
            Automatic
          </button>
        </div>

        <section
          v-if="pendingRelink"
          class="destination"
          aria-labelledby="restore-destination-title"
        >
          <div class="destination__head">
            <div>
              <h3 id="restore-destination-title">Choose a library folder</h3>
              <p>The restored file will be copied into Pick n Mix Restored.</p>
            </div>
            <button
              class="icon-button"
              aria-label="Cancel destination selection"
              @click="pendingRelink = null"
            >
              <PnmIcon name="close" :size="15" />
            </button>
          </div>
          <div class="destination__folders">
            <button
              v-for="folder in library.folders"
              :key="folder"
              class="destination__folder"
              :disabled="busyKey !== null"
              :title="folder"
              @click="chooseDestination(folder)"
            >
              <PnmIcon name="folder" :size="16" />
              <span class="truncate">{{ folder }}</span>
            </button>
          </div>
        </section>

        <div class="duplicate-dialog__body scroll-area" aria-live="polite">
          <p v-if="loading" class="duplicate-dialog__state">Loading file versions…</p>
          <div v-else-if="loadError" class="duplicate-dialog__state is-error">
            <PnmIcon name="warningCircle" :size="18" />
            <span>{{ loadError }}</span>
            <button class="pill-button is-plain" @click="loadVersions(song.id)">Retry</button>
          </div>
          <p v-else-if="versions.length === 0" class="duplicate-dialog__state">
            No file versions were found.
          </p>

          <article
            v-for="(version, index) in versions"
            v-else
            :key="version.id"
            class="version"
            :class="{
              'is-effective': version.effective,
              'is-missing': isMissing(version),
              'is-previewing': activePreviewId === version.id,
            }"
          >
            <header class="version__header">
              <div class="version__title">
                <span>Version {{ index + 1 }}</span>
                <span v-if="version.preferred" class="status is-preferred">Preferred</span>
                <span v-if="version.effective" class="status is-effective">Effective</span>
                <span v-if="isCurrent(version)" class="status is-current">Current</span>
                <span v-if="activePreviewId === version.id" class="status is-previewing">
                  Previewing
                </span>
              </div>
              <div v-if="isMissing(version)" class="version__missing">
                <PnmIcon name="warningCircle" :size="16" />
                <span>File missing</span>
              </div>
            </header>

            <dl class="technical">
              <div :class="{ 'is-different': differs('format') }">
                <dt>Format</dt>
                <dd>{{ version.format ?? "Unknown" }}</dd>
              </div>
              <div :class="{ 'is-different': differs('bitsPerSample') }">
                <dt>Bit depth</dt>
                <dd>{{ version.bitsPerSample === null ? "Unknown" : `${version.bitsPerSample} bit` }}</dd>
              </div>
              <div :class="{ 'is-different': differs('sampleRate') }">
                <dt>Sample rate</dt>
                <dd>{{ formatHz(version.sampleRate) }}</dd>
              </div>
              <div :class="{ 'is-different': differs('bitrateKbps') }">
                <dt>Bitrate</dt>
                <dd>{{ version.bitrateKbps === null ? "Unknown" : `${version.bitrateKbps} kbps` }}</dd>
              </div>
              <div :class="{ 'is-different': differs('channels') }">
                <dt>Channels</dt>
                <dd>{{ channelLabel(version.channels) }}</dd>
              </div>
              <div :class="{ 'is-different': differs('fileSize') }">
                <dt>File size</dt>
                <dd>{{ formatBytes(version.fileSize) }}</dd>
              </div>
              <div :class="{ 'is-different': differs('durationSecs') }">
                <dt>Duration</dt>
                <dd>{{ formatDuration(version.durationSecs) }}</dd>
              </div>
              <div class="technical__path" :class="{ 'is-different': differs('location') }">
                <dt>Path</dt>
                <dd :title="version.location">{{ version.location }}</dd>
              </div>
            </dl>

            <footer class="version__actions">
              <template v-if="!isMissing(version)">
                <button
                  class="pill-button is-plain"
                  :disabled="busyKey !== null"
                  :aria-label="`${activePreviewId === version.id ? 'Stop previewing' : 'Preview'} version ${index + 1}`"
                  @click="preview(version)"
                >
                  <PnmIcon :name="activePreviewId === version.id ? 'pause' : 'play'" :size="12" />
                  {{ activePreviewId === version.id ? "Stop Preview" : "Preview" }}
                </button>
                <button
                  class="pill-button is-secondary"
                  :disabled="version.preferred || busyKey !== null"
                  :aria-label="`Use version ${index + 1}`"
                  @click="useVersion(version)"
                >
                  Use this version
                </button>
                <button
                  class="version__text-action is-danger"
                  :disabled="busyKey !== null"
                  @click="trash(version)"
                >
                  Move to Trash…
                </button>
              </template>
              <template v-else>
                <button
                  class="pill-button is-secondary"
                  :disabled="busyKey !== null"
                  :aria-label="`Locate missing version ${index + 1}`"
                  @click="locate(version)"
                >
                  <PnmIcon name="folder" :size="14" />
                  Locate File…
                </button>
                <button
                  class="version__text-action is-danger"
                  :disabled="busyKey !== null"
                  @click="forget(version)"
                >
                  Forget
                </button>
              </template>
            </footer>
          </article>
        </div>
      </section>
    </div>
  </Transition>
</template>

<style scoped>
.duplicate-scrim {
  position: fixed;
  inset: 0;
  z-index: 520;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 22px;
  background: rgba(0, 0, 0, 0.34);
  backdrop-filter: blur(4px);
}

.duplicate-dialog {
  width: min(920px, 100%);
  max-height: min(86vh, 820px);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  border: 0.5px solid var(--separator);
  border-radius: var(--radius-lg);
  outline: none;
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
}

.duplicate-dialog__header {
  display: flex;
  align-items: center;
  gap: 13px;
  padding: 17px 18px 14px;
  border-bottom: 1px solid var(--separator);
}

.duplicate-dialog__heading {
  flex: 1;
  min-width: 0;
}

.duplicate-dialog__heading h2 {
  margin: 1px 0 2px;
  font-size: 18px;
  font-weight: 650;
}

.duplicate-dialog__heading p {
  margin: 0;
  color: var(--text-secondary);
  font-size: 12.5px;
}

.duplicate-dialog__eyebrow {
  color: var(--accent) !important;
  font-size: 10.5px !important;
  font-weight: 650;
  letter-spacing: 0.06em;
  text-transform: uppercase;
}

.duplicate-dialog__selection {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 18px;
  padding: 11px 18px;
  background: var(--bg-sunken);
  border-bottom: 1px solid var(--separator);
}

.duplicate-dialog__selection strong {
  display: block;
  margin-bottom: 2px;
  font-size: 12.5px;
}

.duplicate-dialog__selection p {
  margin: 0;
  color: var(--text-secondary);
  font-size: 11.5px;
  line-height: 1.4;
}

.duplicate-dialog__selection button:disabled,
.version__actions button:disabled,
.destination button:disabled {
  cursor: default;
  opacity: 0.42;
}

.destination {
  padding: 13px 18px;
  border-bottom: 1px solid var(--separator);
  background: var(--accent-tint);
}

.destination__head {
  display: flex;
  align-items: start;
  justify-content: space-between;
  gap: 12px;
}

.destination h3 {
  margin: 0 0 2px;
  font-size: 13px;
}

.destination p {
  margin: 0;
  color: var(--text-secondary);
  font-size: 11.5px;
}

.destination__folders {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 7px;
  margin-top: 10px;
}

.destination__folder {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  padding: 8px 10px;
  border: 1px solid var(--separator);
  border-radius: var(--radius-sm);
  background: var(--bg-elevated);
  font-size: 11.5px;
  text-align: left;
}

.destination__folder:hover {
  border-color: var(--accent);
}

.duplicate-dialog__body {
  min-height: 190px;
  padding: 14px 18px 18px;
}

.duplicate-dialog__state {
  min-height: 150px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 9px;
  margin: 0;
  color: var(--text-tertiary);
  font-size: 12.5px;
}

.duplicate-dialog__state.is-error {
  color: #d7373f;
}

.version {
  margin-bottom: 11px;
  overflow: hidden;
  border: 1px solid var(--separator);
  border-radius: var(--radius);
  background: var(--bg-elevated);
}

.version:last-child {
  margin-bottom: 0;
}

.version.is-effective {
  border-color: color-mix(in srgb, var(--accent) 46%, var(--separator));
  box-shadow: inset 3px 0 0 var(--accent);
}

.version.is-missing {
  border-color: color-mix(in srgb, #d7373f 42%, var(--separator));
}

.version.is-previewing {
  box-shadow: inset 3px 0 0 var(--accent-secondary);
}

.version__header {
  min-height: 40px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--separator);
  background: var(--bg-hover);
}

.version__title,
.version__missing {
  display: flex;
  align-items: center;
  gap: 6px;
}

.version__title > span:first-child {
  font-size: 12.5px;
  font-weight: 650;
}

.status {
  padding: 2px 6px;
  border-radius: 999px;
  background: var(--bg-active);
  color: var(--text-secondary);
  font-size: 9.5px;
  font-weight: 650;
  letter-spacing: 0.02em;
  text-transform: uppercase;
}

.status.is-preferred,
.status.is-effective {
  background: var(--accent-tint);
  color: var(--accent);
}

.status.is-current {
  background: var(--accent-secondary-tint);
  color: var(--accent-secondary);
}

.status.is-previewing {
  background: var(--accent-secondary-tint);
  color: var(--accent-secondary);
}

.version__missing {
  flex: none;
  color: #d7373f;
  font-size: 11.5px;
  font-weight: 600;
}

.technical {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0;
  margin: 0;
}

.technical > div {
  min-width: 0;
  padding: 10px 12px;
  border-right: 1px solid var(--separator);
  border-bottom: 1px solid var(--separator);
}

.technical > div:nth-child(4n) {
  border-right: 0;
}

.technical > div.is-different {
  background: var(--accent-tint);
}

.technical dt {
  margin-bottom: 3px;
  color: var(--text-tertiary);
  font-size: 10.5px;
}

.technical dd {
  min-width: 0;
  margin: 0;
  font-size: 11.5px;
  user-select: text;
}

.technical__path {
  grid-column: 1 / -1;
  border-right: 0 !important;
}

.technical__path dd {
  overflow-wrap: anywhere;
  color: var(--text-secondary);
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 10.5px;
  line-height: 1.45;
}

.version__actions {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 12px;
}

.version__text-action {
  margin-left: auto;
  padding: 6px 5px;
  color: var(--text-secondary);
  font-size: 11.5px;
}

.version__text-action:hover {
  color: var(--text);
}

.version__text-action.is-danger {
  color: #d7373f;
}

@media (max-width: 720px) {
  .duplicate-scrim {
    padding: 10px;
  }

  .technical {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .technical > div:nth-child(4n) {
    border-right: 1px solid var(--separator);
  }

  .technical > div:nth-child(2n) {
    border-right: 0;
  }

  .destination__folders {
    grid-template-columns: 1fr;
  }
}
</style>
