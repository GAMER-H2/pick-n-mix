<script setup lang="ts">
/** Up-next list, opened from the queue button in the player bar. */
import { computed } from "vue";
import PnmIcon from "./icons/PnmIcon.vue";
import QueueList from "./QueueList.vue";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";

const player = usePlayerStore();
const ui = useUiStore();

const current = computed(() => player.queue.currentIndex);
const items = computed(() => player.queue.items);

async function jump(index: number) {
  await api.playQueueIndex(index);
}

async function remove(index: number) {
  await api.removeFromQueue(index);
  await player.refreshQueue();
}

async function clear() {
  await api.clearQueue();
  await player.refreshQueue();
}

async function move(from: number, to: number) {
  await api.moveInQueue(from, to);
  await player.refreshQueue();
}

function openMenu(index: number, event: MouseEvent) {
  const track = items.value[index];
  if (track) ui.openContextMenu({ x: event.clientX, y: event.clientY, tracks: [track] });
}
</script>

<template>
  <aside class="queue" role="complementary" aria-label="Playing next">
    <header class="queue__head">
      <div>
        <h2>Playing Next</h2>
        <p v-if="player.queue.context" class="queue__context truncate">
          From {{ player.queue.context.name }}
        </p>
      </div>
      <div class="queue__actions">
        <button v-if="items.length" class="queue__clear" @click="clear">Clear</button>
        <button class="icon-button" aria-label="Close queue" @click="ui.queueOpen = false">
          <PnmIcon name="close" :size="18" />
        </button>
      </div>
    </header>

    <div v-if="items.length === 0" class="queue__empty">
      <PnmIcon name="queue" :size="26" />
      <p>Nothing queued yet.</p>
    </div>

    <div v-else class="queue__list scroll-area">
      <QueueList
        :items="items"
        :current-index="current"
        :playing="player.playing"
        @play="jump"
        @remove="remove"
        @move="move"
        @menu="openMenu"
      />
    </div>
  </aside>
</template>

<style scoped>
.queue {
  display: flex;
  flex-direction: column;
  width: 320px;
  flex: none;
  border-left: 1px solid var(--separator);
  background: var(--bg-elevated);
}

.queue__head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 10px;
  padding: 14px 12px 10px 16px;
  border-bottom: 1px solid var(--separator);
}

.queue__head h2 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.queue__context {
  margin: 3px 0 0;
  font-size: 11px;
  color: var(--text-tertiary);
  max-width: 180px;
}

.queue__actions {
  display: flex;
  align-items: center;
  gap: 4px;
}

.queue__clear {
  font-size: 11.5px;
  color: var(--accent);
}

.queue__empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  color: var(--text-tertiary);
}

.queue__empty p {
  margin: 0;
  font-size: 12px;
}

.queue__list {
  flex: 1;
  padding: 6px;
}

</style>
