import { defineStore } from "pinia";
import { ref } from "vue";
import * as api from "@/lib/api";
import { defaultCrossfade } from "@/lib/crossfadeCurve";
import type { CrossfadeCurve, CrossfadeSettings } from "@/lib/types";

/**
 * Global crossfade settings.
 *
 * Deliberately its own store, not folded into `useMixerStore`: a crossfade
 * happens between two tracks that may belong to two different playlists, so
 * unlike every other mixer section it has no playlist or track layer to
 * cascade through — there is exactly one value, always.
 */
export const useCrossfadeStore = defineStore("crossfade", () => {
  const settings = ref<CrossfadeSettings>(defaultCrossfade());

  async function refresh() {
    settings.value = await api.crossfadeSettings();
  }

  /** From the simple slider in the DJ Mixer popup. */
  async function setLength(lengthSecs: number) {
    settings.value = await api.setCrossfadeLength(lengthSecs);
  }

  /** From dragging a handle in the advanced graph. */
  async function setCurve(curve: CrossfadeCurve) {
    settings.value = await api.setCrossfadeCurve(curve);
  }

  /** Apply the crossfade snapshot carried by a mixer preset. Length is set
   * first because the backend validates the curve against its current range. */
  async function setSettings(next: CrossfadeSettings) {
    settings.value = await api.setCrossfadeLength(next.lengthSecs);
    settings.value = await api.setCrossfadeCurve(next.curve);
  }

  return { settings, refresh, setLength, setCurve, setSettings };
});
