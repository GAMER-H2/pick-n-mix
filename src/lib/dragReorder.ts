import { computed, ref, type Ref } from "vue";

/**
 * Pointer-based drag-to-reorder for a list, driven by an explicit grip handle
 * so a drag is never mistaken for a click on the row itself.
 *
 * Shared by the queue and playlists so both flavours of "drag by handle, drop
 * in a gap" behave identically. The container only needs each row to carry a
 * `data-row` attribute, in document order.
 */
export function useDragReorder(
  container: Ref<HTMLElement | null>,
  onMove: (from: number, to: number) => void,
) {
  const dragFrom = ref<number | null>(null);
  const dropAt = ref<number | null>(null);

  const isDragging = computed(() => dragFrom.value !== null);

  /** Which gap the pointer is currently over, 0..rowCount. */
  function gapAt(clientY: number): number {
    const element = container.value;
    if (!element) return 0;
    const rows = Array.from(element.querySelectorAll<HTMLElement>("[data-row]"));
    for (let i = 0; i < rows.length; i += 1) {
      const rect = rows[i].getBoundingClientRect();
      if (clientY < rect.top + rect.height / 2) return i;
    }
    return rows.length;
  }

  function onHandleDown(event: PointerEvent, index: number) {
    event.preventDefault();
    dragFrom.value = index;
    dropAt.value = index;
    (event.currentTarget as HTMLElement).setPointerCapture(event.pointerId);
  }

  function onHandleMove(event: PointerEvent) {
    if (dragFrom.value === null) return;
    dropAt.value = gapAt(event.clientY);
  }

  function onHandleUp(event: PointerEvent) {
    const from = dragFrom.value;
    const gap = dropAt.value;
    (event.currentTarget as HTMLElement).releasePointerCapture(event.pointerId);
    dragFrom.value = null;
    dropAt.value = null;
    if (from === null || gap === null) return;

    // A gap index above the dragged row means the row shifts down by one.
    const to = gap > from ? gap - 1 : gap;
    if (to !== from) onMove(from, to);
  }

  function onHandleCancel() {
    dragFrom.value = null;
    dropAt.value = null;
  }

  return { dragFrom, dropAt, isDragging, onHandleDown, onHandleMove, onHandleUp, onHandleCancel };
}
