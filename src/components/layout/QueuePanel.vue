<script setup lang="ts">
/** Up-next list, opened from the queue button in the player bar. */
import { ref } from "vue";
import IconButton from "../ui/IconButton.vue";
import QueueList from "../media/QueueList.vue";
import { usePlayerStore } from "@/stores/player";
import { useSettingsStore } from "@/stores/settings";
import { useUiStore } from "@/stores/ui";
import { useQueueActions } from "@/composables/useQueueActions";

const player = usePlayerStore();
const settings = useSettingsStore();
const ui = useUiStore();

const { current, items, jump, remove, move, clear, saveAsPlaylist, openMenu } = useQueueActions();

/** The list owns the scrolling; the header only asks it to go there. */
const list = ref<InstanceType<typeof QueueList> | null>(null);
</script>

<template>
  <aside class="queue" role="complementary" aria-label="Playing next">
    <header class="queue__head">
      <div>
        <p class="eyebrow">Playing Next</p>
        <h2 class="queue__from truncate" :title="player.queue.context?.name ?? 'Your Library'">
          {{ player.queue.context?.name ?? "Your Library" }}
        </h2>
      </div>
      <div class="queue__actions">
        <IconButton
          v-if="current !== null"
          icon="locateCurrent"
          label="Jump to the playing song"
          :size="17"
          @click="list?.centreOnCurrent()"
        />
        <IconButton
          v-if="items.length"
          icon="addToPlaylist"
          label="Save queue as a playlist"
          :size="17"
          @click="saveAsPlaylist"
        />
        <button v-if="items.length" class="queue__clear" @click="clear">Clear</button>
        <IconButton icon="close" label="Close queue" :size="18" @click="ui.queueOpen = false" />
      </div>
    </header>

    <div v-if="items.length === 0" class="queue__empty">
      <PnmIcon name="queue" :size="26" />
      <p>Nothing queued yet.</p>
    </div>

    <div v-else class="queue__list scroll-area">
      <QueueList
        ref="list"
        :items="items"
        :current-index="current"
        :playing="player.playing"
        :position-secs="player.position"
        :follow-current="settings.preferences.queueFollowsCurrent"
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

/* Surface titling, as the mixer and EQ surfaces do it: accent eyebrow naming
   the surface, title naming what it is acting on. */
.queue__head h2 {
  margin: 2px 0 0;
  font-size: 14px;
  font-weight: 600;
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
