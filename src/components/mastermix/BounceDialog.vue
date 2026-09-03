<script setup lang="ts">
/**
 * Export a playlist's master mix as a single audio file.
 *
 * Quality settings live here so the save dialog that follows can be just a
 * location picker. The bounce itself is offline: the same graph used for
 * audition, driven as fast as the CPU allows.
 */
import { computed, onMounted, ref } from "vue";
import { save } from "@tauri-apps/plugin-dialog";
import PnmIcon from "../icons/PnmIcon.vue";
import SelectMenu from "../SelectMenu.vue";
import * as api from "@/lib/api";
import { useBounceStore } from "@/stores/bounce";
import type { BounceFormat, BounceOptions, FfmpegStatus } from "@/lib/types";

const props = defineProps<{
  playlistId: string;
  playlistName: string;
  /** Whether this playlist has a picture of its own to embed as the cover. */
  hasArtwork?: boolean;
}>();

const emit = defineEmits<{ close: []; bounced: [path: string] }>();

const bounce = useBounceStore();

const format = ref<BounceFormat>("wav");
const sampleRate = ref<"44100" | "48000" | "96000">("48000");
const wavBitDepth = ref<"16" | "24" | "32">("24");
const flacCompression = ref<"0" | "5" | "8">("5");
const mp3Bitrate = ref<"128" | "192" | "256" | "320">("320");
const busy = ref(false);
const error = ref<string | null>(null);

/**
 * WAV and FLAC are written by the app itself. MP3 is not — there is no encoder
 * with a licence worth linking into this binary — so it is handed to whatever
 * ffmpeg the machine has. Asking up front means the format either is not
 * offered or works, rather than failing after a render that took minutes.
 */
const ffmpeg = ref<FfmpegStatus | null>(null);
const checking = ref(false);
const canMp3 = computed(() => ffmpeg.value?.mp3 === true);

async function checkFfmpeg(again = false) {
  checking.value = true;
  try {
    ffmpeg.value = await api.ffmpegStatus(again);
    if (!canMp3.value && format.value === "mp3") format.value = "wav";
  } catch {
    // Treated exactly like a missing ffmpeg: MP3 is simply not on offer.
    ffmpeg.value = { path: "", available: false, mp3: false, version: "" };
  } finally {
    checking.value = false;
  }
}

onMounted(() => void checkFfmpeg());

const mp3Note = computed(() => {
  const status = ffmpeg.value;
  if (!status || canMp3.value) return null;
  return status.available
    ? "MP3 needs ffmpeg built with libmp3lame; this system's was not."
    : "MP3 needs ffmpeg, which was not found. WAV and FLAC need nothing installed.";
});

// MP3 is left out rather than shown greyed: an option that cannot be chosen
// says less than the note underneath, which says what to install.
const formatOptions = computed(() => [
  { id: "wav", label: "WAV" },
  { id: "flac", label: "FLAC" },
  ...(canMp3.value ? [{ id: "mp3", label: "MP3" }] : []),
]);
const rateOptions = [
  { id: "44100", label: "44.1 kHz" },
  { id: "48000", label: "48 kHz" },
  { id: "96000", label: "96 kHz" },
];
const depthOptions = [
  { id: "16", label: "16-bit" },
  { id: "24", label: "24-bit" },
  { id: "32", label: "32-bit float" },
];
const flacOptions = [
  { id: "0", label: "Fast" },
  { id: "5", label: "Default" },
  { id: "8", label: "Best" },
];
const mp3Options = [
  { id: "128", label: "128 kbps" },
  { id: "192", label: "192 kbps" },
  { id: "256", label: "256 kbps" },
  { id: "320", label: "320 kbps" },
];

const extension = computed(() => format.value);
const defaultName = computed(
  () => `${props.playlistName.replace(/[/\\?%*:|"<>]/g, "-")} mix.${extension.value}`,
);

function options(): BounceOptions {
  return {
    format: format.value,
    sampleRate: Number(sampleRate.value) as BounceOptions["sampleRate"],
    wavBitDepth: Number(wavBitDepth.value) as BounceOptions["wavBitDepth"],
    flacCompression: Number(flacCompression.value),
    mp3Bitrate: Number(mp3Bitrate.value) as BounceOptions["mp3Bitrate"],
  };
}

/**
 * Start the render and get out of the way.
 *
 * The render itself happens in the background and reports to the strip above
 * the player bar, so this dialog closes as soon as the job is accepted rather
 * than holding the app still for the minutes a long mix takes. Only a failure
 * to *start* is shown here, since after that there is somewhere better for it.
 */
async function startBounce() {
  error.value = null;
  const destination = await save({
    title: "Bounce mix",
    defaultPath: defaultName.value,
    filters: [{ name: format.value.toUpperCase(), extensions: [extension.value] }],
  });
  if (typeof destination !== "string") return;
  busy.value = true;
  try {
    await bounce.start(props.playlistId, props.playlistName, destination, options());
    emit("bounced", destination);
  } catch (e) {
    error.value = String(e);
  } finally {
    busy.value = false;
  }
}
</script>

<template>
  <div class="scrim" @click.self="emit('close')">
    <div class="dialog" role="dialog" aria-label="Bounce mix">
      <header class="dialog__head">
        <h2>Bounce mix</h2>
        <button class="icon-button" type="button" aria-label="Close" @click="emit('close')">
          <PnmIcon name="close" :size="17" />
        </button>
      </header>
      <p class="dialog__subtitle">
        Render {{ playlistName }} to a single file, with mixer settings, fades and
        keyframes baked in.<template v-if="props.hasArtwork">
          The playlist's image goes in as the file's cover art.</template>
      </p>

      <div class="dialog__fields">
        <SelectMenu v-model="format" label="Format" :options="formatOptions" />
        <SelectMenu v-model="sampleRate" label="Sample rate" :options="rateOptions" />
        <SelectMenu
          v-if="format === 'wav'"
          v-model="wavBitDepth"
          label="Bit depth"
          :options="depthOptions"
        />
        <SelectMenu
          v-if="format === 'flac'"
          v-model="flacCompression"
          label="Compression"
          :options="flacOptions"
        />
        <SelectMenu
          v-if="format === 'mp3'"
          v-model="mp3Bitrate"
          label="Bitrate"
          :options="mp3Options"
        />
      </div>

      <p v-if="mp3Note" class="dialog__note">
        {{ mp3Note }}
        <button class="dialog__recheck" type="button" :disabled="checking" @click="checkFfmpeg(true)">
          {{ checking ? "Checking…" : "Check again" }}
        </button>
      </p>

      <p v-if="error" class="dialog__error" role="alert">{{ error }}</p>

      <footer class="dialog__foot">
        <button class="pill-button is-secondary" type="button" :disabled="busy" @click="emit('close')">
          Cancel
        </button>
        <button class="pill-button" type="button" :disabled="busy" @click="startBounce">
          {{ busy ? "Starting…" : "Choose location…" }}
        </button>
      </footer>
    </div>
  </div>
</template>

<style scoped>
.scrim {
  position: fixed;
  inset: 0;
  z-index: 520;
  display: flex;
  align-items: center;
  justify-content: center;
  background: rgba(0, 0, 0, 0.28);
  backdrop-filter: blur(3px);
}

.dialog {
  width: 380px;
  display: flex;
  flex-direction: column;
  padding: 16px;
  border-radius: var(--radius-lg);
  background: var(--bg-elevated);
  box-shadow: var(--shadow-popover);
}

.dialog__head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.dialog__head h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.dialog__subtitle {
  margin: 4px 0 14px;
  font-size: 12px;
  color: var(--text-tertiary);
  line-height: 1.45;
}

.dialog__fields {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.dialog__note {
  margin: 10px 0 0;
  font-size: 11.5px;
  line-height: 1.45;
  color: var(--text-tertiary);
}

.dialog__recheck {
  color: var(--accent);
  font-size: 11.5px;
}

.dialog__recheck:disabled {
  opacity: 0.5;
  cursor: default;
}

.dialog__error {
  margin: 10px 0 0;
  font-size: 12px;
  color: var(--danger, #c44);
}

.dialog__foot {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
  margin-top: 16px;
}
</style>
