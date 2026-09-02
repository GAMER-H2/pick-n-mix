/**
 * Scroll restoration for the back and forward buttons.
 *
 * Vue Router's own `scrollBehavior` restores `window`, but the app scrolls an
 * inner element (`.app__main`), so it never sees the offset that matters. This
 * keeps its own record instead, keyed by the same history `position` that
 * `navigation.ts` reads, so going back lands on the entry it was saved for.
 *
 * Search text is *not* kept here. It lives in the route query, so it is part of
 * the history entry itself and comes back with it for free.
 */

import type { Router } from "vue-router";

/** Offsets keyed by history position. */
const offsets = new Map<number, number>();

/** How many frames to keep trying before giving up on a restore. */
const RESTORE_ATTEMPTS = 12;

let scroller: HTMLElement | null = null;
let pending: number | null = null;

/** Point the module at the element that actually scrolls. */
export function registerScroller(element: HTMLElement | null) {
  scroller = element;
}

function currentPosition(): number {
  const state = window.history.state as { position?: number } | null;
  return typeof state?.position === "number" ? state.position : 0;
}

/**
 * The position we are currently sitting on, cached rather than re-read from
 * `window.history.state` inside `beforeEach`.
 *
 * On a back/forward navigation the browser updates `history.state` to the
 * *destination* entry before `popstate` fires, i.e. before any guard runs. A
 * `beforeEach` that re-read `currentPosition()` would therefore save the
 * outgoing page's scroll offset under the *incoming* page's key, clobbering
 * whatever was saved for it. This is only ever advanced in `afterEach`, once
 * a navigation has actually completed and `history.state` is trustworthy for
 * both push and pop alike.
 */
let currentPos = currentPosition();

/**
 * Restore an offset once the view has rendered enough content to reach it.
 *
 * Lists here use `content-visibility`, so the scroll height grows over the
 * first few frames after a route renders. Setting `scrollTop` too early would
 * silently clamp to a shorter page, so this retries for a handful of frames and
 * stops as soon as the offset sticks.
 */
function restore(target: number, attempt = 0) {
  if (pending !== null) cancelAnimationFrame(pending);
  pending = requestAnimationFrame(() => {
    pending = null;
    const element = scroller;
    if (!element) return;

    element.scrollTop = target;
    // `scrollTop` clamps silently, so compare rather than trust the write.
    const reached = Math.abs(element.scrollTop - target) <= 1;
    if (!reached && attempt < RESTORE_ATTEMPTS) restore(target, attempt + 1);
  });
}

/**
 * Record the offset before leaving a page and put it back on arrival.
 *
 * Registered once, from the app shell.
 */
export function trackScrollPositions(router: Router) {
  router.beforeEach((_to, _from, next) => {
    if (scroller) offsets.set(currentPos, scroller.scrollTop);
    next();
  });

  router.afterEach(() => {
    currentPos = currentPosition();
    // A brand-new page has no saved offset and should start at the top, which
    // is also what the router's own `scrollBehavior` asks for.
    restore(offsets.get(currentPos) ?? 0);
  });
}

/** Reset, used by tests. */
export function resetScrollPositions() {
  offsets.clear();
  scroller = null;
  currentPos = currentPosition();
  if (pending !== null) cancelAnimationFrame(pending);
  pending = null;
}
