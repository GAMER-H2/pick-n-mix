import { afterEach, beforeEach, describe, expect, it } from "vitest";
import type { Router } from "vue-router";
import { registerScroller, resetScrollPositions, trackScrollPositions } from "../viewState";

/**
 * A router stand-in that lets a test fire `beforeEach`/`afterEach` guards by
 * hand, so the history-state race that only shows up on back/forward can be
 * reproduced deterministically instead of needing a real popstate event.
 */
function makeRouter() {
  const before: Array<(to: unknown, from: unknown, next: () => void) => void> = [];
  const after: Array<() => void> = [];
  return {
    router: {
      beforeEach: (fn: (typeof before)[number]) => before.push(fn),
      afterEach: (fn: (typeof after)[number]) => after.push(fn),
    } as unknown as Router,
    fireBefore: () => before.forEach((fn) => fn(null, null, () => {})),
    fireAfter: () => after.forEach((fn) => fn()),
  };
}

function setHistoryPosition(position: number) {
  window.history.replaceState({ position }, "");
}

describe("trackScrollPositions", () => {
  let scroller: HTMLElement;

  beforeEach(() => {
    // Runs synchronously so `restore()`'s single rAF settles within the test.
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (globalThis as any).requestAnimationFrame = (cb: FrameRequestCallback) => {
      cb(0);
      return 0;
    };
    setHistoryPosition(0);
    resetScrollPositions();
    scroller = document.createElement("div");
    registerScroller(scroller);
  });

  afterEach(() => {
    registerScroller(null);
  });

  it("restores the offset of the page being left, not the destination, across a back navigation", () => {
    const { router, fireBefore, fireAfter } = makeRouter();
    trackScrollPositions(router);

    // Library (position 0), scrolled down.
    scroller.scrollTop = 100;

    // Push to Album (position 1): a real push only updates history.state
    // *after* the guards resolve, so this mirrors vue-router's own ordering.
    fireBefore();
    setHistoryPosition(1);
    fireAfter();
    expect(scroller.scrollTop).toBe(0); // a fresh page starts at the top

    scroller.scrollTop = 50;

    // Back to Library (position 0): a real popstate updates history.state to
    // the *destination* before any guard runs, which is the race this guards
    // against.
    setHistoryPosition(0);
    fireBefore();
    fireAfter();

    expect(scroller.scrollTop).toBe(100);
  });

  it("still saves the correct offset for a plain forward push", () => {
    const { router, fireBefore, fireAfter } = makeRouter();
    trackScrollPositions(router);

    scroller.scrollTop = 30;
    fireBefore();
    setHistoryPosition(1);
    fireAfter();
    expect(scroller.scrollTop).toBe(0);

    setHistoryPosition(0);
    fireBefore();
    fireAfter();
    expect(scroller.scrollTop).toBe(30);
  });
});
