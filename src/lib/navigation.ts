/**
 * Back/forward state for the sidebar's navigation arrows.
 *
 * Vue Router does not expose whether there is anywhere to go, so we track our
 * own position in the stack. Router writes a `position` into history state on
 * every navigation, which survives reloads and tells us where we are; the
 * furthest position we have seen tells us whether forward is possible.
 */

import { ref } from "vue";
import type { Router } from "vue-router";

const position = ref(0);
const furthest = ref(0);

export const canGoBack = ref(false);
export const canGoForward = ref(false);

function sync() {
  canGoBack.value = position.value > 0;
  canGoForward.value = position.value < furthest.value;
}

export function trackNavigation(router: Router) {
  router.afterEach(() => {
    const state = window.history.state as { position?: number } | null;
    const next = typeof state?.position === "number" ? state.position : position.value + 1;

    // Navigating somewhere new truncates anything that was ahead of us.
    if (next > position.value) {
      furthest.value = Math.max(next, furthest.value);
    } else if (next < position.value) {
      // Went back: whatever was ahead is still reachable.
      furthest.value = Math.max(furthest.value, position.value);
    }
    position.value = next;
    sync();
  });
  sync();
}

/** Reset, used by tests. */
export function resetNavigation() {
  position.value = 0;
  furthest.value = 0;
  sync();
}
