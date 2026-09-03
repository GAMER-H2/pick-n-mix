import { useUiStore, type ContextMenuState } from "@/stores/ui";

/** A context-menu payload, without the coordinates taken from the event. */
export type MenuPayload = Omit<ContextMenuState, "x" | "y">;

/**
 * Opens the shared context menu.
 *
 * Every caller used to hand-roll `ui.openContextMenu({ x: $event.clientX,
 * y: $event.clientY, … })`; this extracts the pointer coordinates once so the
 * payload is the only thing a caller builds.
 */
export function useMenu() {
  const ui = useUiStore();

  function openMenu(event: MouseEvent, payload: MenuPayload) {
    ui.openContextMenu({ x: event.clientX, y: event.clientY, ...payload });
  }

  return { openMenu };
}
