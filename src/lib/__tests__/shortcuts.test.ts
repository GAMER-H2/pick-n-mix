import { describe, expect, it } from "vitest";
import { actionFor, bindingFor, bindingLabel, bindingsFor, SHORTCUTS } from "../shortcuts";

function press(key: string, modifiers: Partial<KeyboardEvent> = {}) {
  return new KeyboardEvent("keydown", { key, ...modifiers });
}

describe("bindings from key presses", () => {
  it("normalises a key so the same press always reads the same", () => {
    expect(bindingFor(press("k"))).toBe("K");
    expect(bindingFor(press("K"))).toBe("K");
    expect(bindingFor(press(" "))).toBe("Space");
    expect(bindingFor(press("ArrowRight"))).toBe("ArrowRight");
  });

  it("names modifiers in a fixed order", () => {
    const binding = bindingFor(press("p", { ctrlKey: true, shiftKey: true, altKey: true }));
    expect(binding).toBe("Ctrl+Alt+Shift+P");
  });

  it("is nothing while only a modifier is held", () => {
    expect(bindingFor(press("Shift", { shiftKey: true }))).toBeNull();
  });
});

describe("resolving a binding to an action", () => {
  it("falls back to the defaults for an action the user has not changed", () => {
    expect(bindingsFor("playPause", {})).toEqual(["Space", "K"]);
    expect(actionFor("Space", {})).toBe("playPause");
    expect(actionFor("L", {})).toBe("nextTrack");
  });

  /** The point of an override: the default it replaced stops working. */
  it("uses an override in place of the defaults, not alongside them", () => {
    const overrides = { playPause: ["Ctrl+Space"] };
    expect(bindingsFor("playPause", overrides)).toEqual(["Ctrl+Space"]);
    expect(actionFor("Ctrl+Space", overrides)).toBe("playPause");
    expect(actionFor("Space", overrides)).toBeNull();
    // Untouched actions are unaffected.
    expect(actionFor("L", overrides)).toBe("nextTrack");
  });

  it("has nothing bound to an unknown key", () => {
    expect(actionFor("Q", {})).toBeNull();
  });
});

describe("what the user is shown", () => {
  it("draws arrows and the space bar as symbols", () => {
    expect(bindingLabel("ArrowRight")).toBe("→");
    expect(bindingLabel("Ctrl+Shift+P")).toBe("Ctrl + Shift + P");
    expect(bindingLabel("Space")).toBe("Space");
  });

  it("gives every action a label and at least one default", () => {
    for (const shortcut of SHORTCUTS) {
      expect(shortcut.label.length).toBeGreaterThan(0);
      expect(shortcut.defaults.length).toBeGreaterThan(0);
    }
  });

  /** Two actions on one key means one of them silently never runs. */
  it("ships no default bound to two actions", () => {
    const seen = new Set<string>();
    for (const shortcut of SHORTCUTS) {
      for (const binding of shortcut.defaults) {
        expect(seen.has(binding)).toBe(false);
        seen.add(binding);
      }
    }
  });
});
