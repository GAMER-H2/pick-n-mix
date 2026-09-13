import { beforeEach, describe, expect, it, vi } from "vitest";
import { installShortcuts } from "../keyboard";

/** Stand-in for the player store; only the methods the shortcuts use. */
function makePlayer() {
  return {
    toggle: vi.fn(),
    next: vi.fn(),
    previous: vi.fn(),
    seek: vi.fn(),
    setVolume: vi.fn(),
    position: 30,
    duration: 200,
    snapshot: { volume: 0.5 },
  };
}

type Player = ReturnType<typeof makePlayer>;

function makeRouter(routeName: string) {
  return {
    push: vi.fn(),
    back: vi.fn(),
    currentRoute: { value: { name: routeName } },
  };
}

function press(key: string, target?: EventTarget, modifiers: Partial<KeyboardEvent> = {}) {
  const event = new KeyboardEvent("keydown", { key, cancelable: true, ...modifiers });
  if (target) Object.defineProperty(event, "target", { value: target });
  window.dispatchEvent(event);
  return event;
}

describe("keyboard shortcuts", () => {
  let player: Player;
  let uninstall: () => void;

  beforeEach(() => {
    player = makePlayer();
    uninstall = installShortcuts(player);
    return () => uninstall();
  });

  it("space and K both toggle playback", () => {
    press(" ");
    press("k");
    press("K");
    expect(player.toggle).toHaveBeenCalledTimes(3);
  });

  it("L goes to the next track and J to the previous", () => {
    press("l");
    press("L");
    expect(player.next).toHaveBeenCalledTimes(2);

    press("j");
    press("J");
    expect(player.previous).toHaveBeenCalledTimes(2);
  });

  it("ignores keys while a text field has focus", () => {
    const input = document.createElement("input");
    document.body.append(input);

    press(" ", input);
    press("l", input);
    press("j", input);

    expect(player.toggle).not.toHaveBeenCalled();
    expect(player.next).not.toHaveBeenCalled();
    expect(player.previous).not.toHaveBeenCalled();
    input.remove();
  });

  it("ignores keys inside a contenteditable region", () => {
    const div = document.createElement("div");
    div.contentEditable = "true";
    Object.defineProperty(div, "isContentEditable", { value: true });
    document.body.append(div);

    press(" ", div);
    expect(player.toggle).not.toHaveBeenCalled();
    div.remove();
  });

  it("leaves unbound modifier combinations to the operating system", () => {
    press(" ", undefined, { metaKey: true });
    press("l", undefined, { ctrlKey: true });
    press("j", undefined, { altKey: true });

    expect(player.toggle).not.toHaveBeenCalled();
    expect(player.next).not.toHaveBeenCalled();
    expect(player.previous).not.toHaveBeenCalled();
  });

  it("prevents the page from scrolling on space", () => {
    const event = press(" ");
    expect(event.defaultPrevented).toBe(true);
  });

  it("arrow keys scrub and change volume within bounds", () => {
    press("ArrowRight");
    expect(player.seek).toHaveBeenCalledWith(40);

    press("ArrowLeft");
    expect(player.seek).toHaveBeenCalledWith(20);

    press("ArrowUp");
    expect(player.setVolume).toHaveBeenCalledWith(0.55);

    press("ArrowDown");
    expect(player.setVolume).toHaveBeenCalledWith(0.45);
  });

  it("reads the seek step on every press, like the bindings", () => {
    let step = 30;
    const off = installShortcuts(player, undefined, undefined, {
      seekStep: () => step,
    });

    press("ArrowRight");
    expect(player.seek).toHaveBeenLastCalledWith(60);

    // Changing the preference in settings applies without reinstalling.
    step = 7;
    press("ArrowRight");
    expect(player.seek).toHaveBeenLastCalledWith(37);
    off();
  });

  it("does not seek past the end or before the start", () => {
    player.position = 199;
    press("ArrowRight");
    expect(player.seek).toHaveBeenLastCalledWith(200);

    player.position = 2;
    press("ArrowLeft");
    expect(player.seek).toHaveBeenLastCalledWith(0);
  });

  it("stops responding once uninstalled", () => {
    uninstall();
    press(" ");
    expect(player.toggle).not.toHaveBeenCalled();
  });
});

describe("queue view shortcut", () => {
  it("opens the full-screen queue view and closes the compact panel", () => {
    const player = makePlayer();
    const ui = { queueOpen: true };
    const router = makeRouter("library");
    const off = installShortcuts(player, ui, router);

    const event = press("f");

    expect(event.defaultPrevented).toBe(true);
    expect(ui.queueOpen).toBe(false);
    expect(router.push).toHaveBeenCalledWith({ name: "nowPlaying" });
    expect(router.back).not.toHaveBeenCalled();
    off();
  });

  it("closes the full-screen queue view through history", () => {
    const player = makePlayer();
    const ui = { queueOpen: false };
    const router = makeRouter("nowPlaying");
    const off = installShortcuts(player, ui, router);

    press("F");

    expect(router.back).toHaveBeenCalledTimes(1);
    expect(router.push).not.toHaveBeenCalled();
    off();
  });

  it("does not navigate while typing or while shortcuts are suspended", () => {
    const player = makePlayer();
    const ui = { queueOpen: true };
    const router = makeRouter("library");
    let suspended = false;
    const off = installShortcuts(player, ui, router, {
      isSuspended: () => suspended,
    });
    const input = document.createElement("input");
    document.body.append(input);

    press("f", input);
    suspended = true;
    press("f");

    expect(router.push).not.toHaveBeenCalled();
    expect(router.back).not.toHaveBeenCalled();
    expect(ui.queueOpen).toBe(true);
    input.remove();
    off();
  });

  it("uses a rebound key instead of F", () => {
    const player = makePlayer();
    const ui = { queueOpen: false };
    const router = makeRouter("library");
    const off = installShortcuts(player, ui, router, {
      bindings: () => ({ toggleQueueView: ["Ctrl+Q"] }),
    });

    press("f");
    expect(router.push).not.toHaveBeenCalled();

    press("q", undefined, { ctrlKey: true });
    expect(router.push).toHaveBeenCalledWith({ name: "nowPlaying" });
    off();
  });
});

describe("escape closes overlays", () => {
  it("leaves the full-screen view before closing the side panel", () => {
    const player = makePlayer();
    const ui = { queueOpen: true };
    const router = makeRouter("nowPlaying");
    const off = installShortcuts(player, ui, router);

    // The full-screen player is a route, so leaving it is a navigation.
    press("Escape");
    expect(router.back).toHaveBeenCalledTimes(1);
    expect(ui.queueOpen).toBe(true);

    router.currentRoute.value.name = "library";
    press("Escape");
    expect(ui.queueOpen).toBe(false);
    off();
  });

  it("does nothing when no overlay is open", () => {
    const player = makePlayer();
    const ui = { queueOpen: false };
    const router = makeRouter("library");
    const off = installShortcuts(player, ui, router);
    const event = press("Escape");
    expect(event.defaultPrevented).toBe(false);
    expect(router.back).not.toHaveBeenCalled();
    expect(player.toggle).not.toHaveBeenCalled();
    off();
  });
})

describe("rebound shortcuts", () => {
  it("runs the user's binding and not the default it replaced", () => {
    const player = makePlayer();
    const off = installShortcuts(player, undefined, undefined, {
      bindings: () => ({ playPause: ["Ctrl+Shift+P"] }),
    });

    press("p", undefined, { ctrlKey: true, shiftKey: true });
    expect(player.toggle).toHaveBeenCalledTimes(1);

    // Space belonged to play/pause and was given up when it was rebound.
    press(" ");
    expect(player.toggle).toHaveBeenCalledTimes(1);

    // Actions the user has not touched keep their defaults.
    press("l");
    expect(player.next).toHaveBeenCalledTimes(1);
    off();
  });
});

/**
 * The Master Mixer runs its own transport against the same engine. With both
 * handlers live, one space bar reached the engine twice — and depending on
 * which landed second, a pause turned into a resume or a stop.
 */
describe("standing aside for another transport", () => {
  it("ignores every key while playback is owned elsewhere", () => {
    const player = makePlayer();
    let suspended = true;
    const off = installShortcuts(player, undefined, undefined, {
      isSuspended: () => suspended,
    });

    press(" ");
    press("l");
    press("ArrowRight");
    expect(player.toggle).not.toHaveBeenCalled();
    expect(player.next).not.toHaveBeenCalled();
    expect(player.seek).not.toHaveBeenCalled();

    suspended = false;
    press(" ");
    expect(player.toggle).toHaveBeenCalledTimes(1);
    off();
  });
});
