/**
 * Queue behaviour shared by the compact side panel and the full-screen
 * player: jump, remove, move and the per-row context menu, plus the two
 * whole-queue verbs — clear it, or keep it as a playlist — against the player
 * store's queue.
 */
import { computed } from "vue";
import * as api from "@/lib/api";
import { usePlayerStore } from "@/stores/player";
import { useUiStore } from "@/stores/ui";

export function useQueueActions() {
  const player = usePlayerStore();
  const ui = useUiStore();

  const current = computed(() => player.queue.currentIndex);
  const items = computed(() => player.queue.items);

  /** Clicking the row that is already playing toggles it instead of restarting. */
  async function jump(index: number, positionSecs?: number) {
    // A chapter inside the playing mix is a place to go, not a play/pause.
    if (positionSecs === undefined && index === current.value && player.playing) {
      await player.toggle();
      return;
    }
    await api.playQueueIndex(index, positionSecs);
  }

  async function remove(index: number) {
    await api.removeFromQueue(index);
    await player.refreshQueue();
  }

  async function move(from: number, to: number) {
    await api.moveInQueue(from, to);
    await player.refreshQueue();
  }

  async function clear() {
    await api.clearQueue();
    await player.refreshQueue();
  }

  function saveAsPlaylist() {
    ui.saveQueueOpen = true;
  }

  function openMenu(index: number, event: MouseEvent) {
    // A mix has no track menu: nothing in it can be reordered or sent
    // elsewhere on its own.
    const row = items.value[index];
    if (row?.kind !== "track") return;
    ui.openContextMenu({ x: event.clientX, y: event.clientY, tracks: [row.track] });
  }

  return { current, items, jump, remove, move, clear, saveAsPlaylist, openMenu };
}
