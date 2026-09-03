import { defineStore } from "pinia";
import { computed, ref } from "vue";
import * as api from "@/lib/api";
import type { BounceOptions } from "@/lib/types";

/** One render, from the moment it is asked for to the moment it is dismissed. */
export interface BounceJob {
  id: string;
  /** The playlist being rendered, for the label. */
  name: string;
  /** Where it is being written, shown when it is done. */
  path: string;
  fraction: number;
  done: boolean;
  error: string | null;
}

/**
 * Bounces in flight.
 *
 * A render takes minutes for a long mix, so it happens in the background and
 * reports through events. The store keeps a job per render rather than a
 * single one: starting a second bounce while the first is running is allowed,
 * and quietly replacing the first one's progress would be a lie.
 */
export const useBounceStore = defineStore("bounce", () => {
  const jobs = ref<BounceJob[]>([]);
  /** Shrunk to a bar with no detail. The user's choice, kept for the session. */
  const collapsed = ref(false);

  const running = computed(() => jobs.value.filter((job) => !job.done));
  const active = computed(() => running.value.length > 0);

  /** Start a render and return its id. Throws only if it could not be started. */
  async function start(
    playlistId: string,
    name: string,
    destination: string,
    options: BounceOptions,
  ): Promise<string> {
    const id = await api.bounceMasterMix(playlistId, destination, options);
    jobs.value = [
      ...jobs.value,
      { id, name, path: destination, fraction: 0, done: false, error: null },
    ];
    collapsed.value = false;
    return id;
  }

  function onProgress(id: string, fraction: number) {
    jobs.value = jobs.value.map((job) =>
      // A late event for a job already finished must not reopen it.
      job.id === id && !job.done ? { ...job, fraction } : job,
    );
  }

  function onFinished(id: string, path: string, error: string | null) {
    jobs.value = jobs.value.map((job) =>
      job.id === id
        ? { ...job, path: path || job.path, fraction: error ? job.fraction : 1, done: true, error }
        : job,
    );
  }

  function dismiss(id: string) {
    jobs.value = jobs.value.filter((job) => job.id !== id);
  }

  /** Clear everything that has stopped, leaving what is still running. */
  function dismissFinished() {
    jobs.value = jobs.value.filter((job) => !job.done);
  }

  return {
    jobs,
    collapsed,
    running,
    active,
    start,
    onProgress,
    onFinished,
    dismiss,
    dismissFinished,
  };
});
