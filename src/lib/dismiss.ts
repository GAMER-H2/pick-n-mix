/**
 * Close-on-outside-click for popovers.
 *
 * Listening in the capture phase means a click on some other control both
 * closes the popover and still does its own job, rather than being swallowed
 * by a scrim.
 */

import { onBeforeUnmount, onMounted, type Ref } from "vue";

export interface DismissOptions {
  /** Elements that count as "inside", so clicking them does not close. */
  ignore?: Ref<HTMLElement | null>[];
}

export function useDismiss(
  isOpen: () => boolean,
  close: () => void,
  element: Ref<HTMLElement | null>,
  options: DismissOptions = {},
) {
  function isInside(target: Node) {
    if (element.value?.contains(target)) return true;
    return (options.ignore ?? []).some((candidate) => candidate.value?.contains(target));
  }

  function onPointerDown(event: PointerEvent) {
    if (!isOpen()) return;
    const target = event.target as Node | null;
    if (target && isInside(target)) return;
    close();
  }

  function onKeydown(event: KeyboardEvent) {
    if (isOpen() && event.key === "Escape") {
      event.stopPropagation();
      close();
    }
  }

  onMounted(() => {
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeydown, true);
  });

  onBeforeUnmount(() => {
    window.removeEventListener("pointerdown", onPointerDown, true);
    window.removeEventListener("keydown", onKeydown, true);
  });
}
