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
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    uninstall = installShortcuts(player as any);
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

  it("leaves modifier combinations to the operating system", () => {
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
    expect(player.seek).toHaveBeenCalledWith(35);

    press("ArrowLeft");
    expect(player.seek).toHaveBeenCalledWith(25);

    press("ArrowUp");
    expect(player.setVolume).toHaveBeenCalledWith(0.55);

    press("ArrowDown");
    expect(player.setVolume).toHaveBeenCalledWith(0.45);
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

describe("escape closes overlays", () => {
  function makeUi() {
    return { nowPlayingOpen: false, queueOpen: false };
  }

  it("closes the full-screen view before the side panel", () => {
    const player = makePlayer();
    const ui = makeUi();
    ui.nowPlayingOpen = true;
    ui.queueOpen = true;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const off = installShortcuts(player as any, ui as any);

    press("Escape");
    expect(ui.nowPlayingOpen).toBe(false);
    expect(ui.queueOpen).toBe(true);

    press("Escape");
    expect(ui.queueOpen).toBe(false);
    off();
  });

  it("does nothing when no overlay is open", () => {
    const player = makePlayer();
    const ui = makeUi();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const off = installShortcuts(player as any, ui as any);
    const event = press("Escape");
    expect(event.defaultPrevented).toBe(false);
    expect(player.toggle).not.toHaveBeenCalled();
    off();
  });
})
