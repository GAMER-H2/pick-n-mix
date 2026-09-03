<script setup lang="ts">
/**
 * Bounces happening in the background.
 *
 * A render of a long mix takes minutes, and the app is meant to stay usable
 * for all of them — so this is a strip above the player bar rather than
 * anything modal. Collapsed it is a single line of progress with no text,
 * which is enough to know something is still going on.
 */
import { computed } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import { useBounceStore } from "@/stores/bounce";

const bounce = useBounceStore();

/** One bar for everything running, so a collapsed strip still means something. */
const overall = computed(() => {
  const running = bounce.running;
  if (running.length === 0) return 1;
  return running.reduce((sum, job) => sum + job.fraction, 0) / running.length;
});

const summary = computed(() => {
  const running = bounce.running.length;
  if (running === 0) return "Bounce finished";
  if (running === 1) return `Bouncing ${bounce.running[0].name}`;
  return `Bouncing ${running} mixes`;
});

function percent(fraction: number): string {
  return `${Math.round(Math.min(1, Math.max(0, fraction)) * 100)}%`;
}

/** The file's own name; the full path is the row's tooltip. */
function fileName(path: string): string {
  return path.split(/[/\\]/).pop() || path;
}
</script>

<template>
  <section
    v-if="bounce.jobs.length > 0"
    class="bounce"
    :class="{ 'is-collapsed': bounce.collapsed }"
    aria-label="Bounces in progress"
  >
    <header class="bounce__head">
      <button
        class="bounce__toggle"
        type="button"
        :aria-expanded="!bounce.collapsed"
        :title="bounce.collapsed ? 'Show bounce details' : 'Collapse'"
        @click="bounce.collapsed = !bounce.collapsed"
      >
        <PnmIcon :name="bounce.collapsed ? 'chevronUp' : 'chevronDown'" :size="14" />
        <span class="truncate">{{ summary }}</span>
      </button>
      <span v-if="bounce.active" class="bounce__percent">{{ percent(overall) }}</span>
      <button
        v-if="!bounce.active"
        class="bounce__dismiss"
        type="button"
        aria-label="Dismiss finished bounces"
        @click="bounce.dismissFinished()"
      >
        <PnmIcon name="close" :size="13" />
      </button>
    </header>

    <!-- Collapsed, this line is the whole component. -->
    <div class="bounce__track" :aria-valuenow="Math.round(overall * 100)" role="progressbar">
      <div class="bounce__fill" :style="{ width: percent(overall) }" />
    </div>

    <ul v-if="!bounce.collapsed" class="bounce__list">
      <li v-for="job in bounce.jobs" :key="job.id" class="bounce__job">
        <span class="bounce__name truncate">{{ job.name }}</span>
        <span v-if="job.error" class="bounce__error truncate">{{ job.error }}</span>
        <span v-else-if="job.done" class="bounce__saved truncate" :title="job.path">
          Saved as {{ fileName(job.path) }}
        </span>
        <span v-else class="bounce__job-percent">{{ percent(job.fraction) }}</span>
        <button
          v-if="job.done"
          class="bounce__dismiss"
          type="button"
          :aria-label="`Dismiss ${job.name}`"
          @click="bounce.dismiss(job.id)"
        >
          <PnmIcon name="close" :size="12" />
        </button>
      </li>
    </ul>
  </section>
</template>

<style scoped>
/*
 * As wide as the sidebar and no wider.
 *
 * A bounce is background work, so it belongs in the app's quiet column rather
 * than as a band across everything: at full width it reads as a modal state
 * the whole window is in, which is exactly what it is not.
 */
.bounce {
  flex: none;
  width: var(--sidebar-width);
  padding: 7px 12px 6px;
  border-top: 0.5px solid var(--separator);
  border-right: 0.5px solid var(--separator);
  background: var(--bg-sidebar);
}

.bounce.is-collapsed {
  padding-bottom: 4px;
}

.bounce__head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 5px;
}

.bounce__toggle {
  display: flex;
  align-items: center;
  gap: 6px;
  min-width: 0;
  flex: 1;
  font-size: 12px;
  color: var(--text-secondary);
}

.bounce__toggle:hover {
  color: var(--text);
}

.bounce__percent,
.bounce__job-percent {
  flex: none;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
  color: var(--text-tertiary);
}

.bounce__track {
  height: 3px;
  border-radius: 999px;
  background: var(--control-track);
  overflow: hidden;
}

.bounce__fill {
  height: 100%;
  border-radius: 999px;
  background: var(--accent);
  transition: width 0.25s var(--ease);
}

.bounce__list {
  display: flex;
  flex-direction: column;
  gap: 2px;
  margin: 6px 0 0;
  padding: 0;
  list-style: none;
}

.bounce__job {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--text-secondary);
}

.bounce__name {
  flex: 1;
  min-width: 0;
}

.bounce__error {
  flex: 2;
  min-width: 0;
  color: var(--accent);
}

.bounce__saved {
  flex: 2;
  min-width: 0;
  text-align: right;
  color: var(--text-tertiary);
}

.bounce__dismiss {
  flex: none;
  display: flex;
  color: var(--text-tertiary);
}

.bounce__dismiss:hover {
  color: var(--text);
}
</style>
