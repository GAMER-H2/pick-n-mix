/**
 * The catalogue of rebindable keyboard shortcuts.
 *
 * One list, used three ways: `lib/keyboard.ts` matches key presses against it,
 * the settings pane lists it, and the recorder writes overrides back into
 * preferences. Adding a shortcut means adding an action here and a case in the
 * handler — nothing else needs to know.
 *
 * A binding is written as its modifiers then its key, joined by `+`
 * ("Ctrl+Shift+K"). Keys are normalised so that the same physical press always
 * produces the same string: letters uppercase, the space bar "Space".
 */

export type ShortcutAction =
  | "playPause"
  | "nextTrack"
  | "previousTrack"
  | "seekForward"
  | "seekBackward"
  | "volumeUp"
  | "volumeDown";

export interface ShortcutDefinition {
  id: ShortcutAction;
  label: string;
  /** What the action does, in the settings list. */
  description: string;
  /**
   * What it is bound to out of the box. Several, where a second key has always
   * worked as well — rebinding replaces the lot with the one key recorded.
   */
  defaults: string[];
}

export const SHORTCUTS: ReadonlyArray<ShortcutDefinition> = [
  {
    id: "playPause",
    label: "Play / Pause",
    description: "Start or pause what is playing",
    defaults: ["Space", "K"],
  },
  { id: "nextTrack", label: "Next song", description: "Skip to the next song", defaults: ["L"] },
  {
    id: "previousTrack",
    label: "Previous song",
    description: "Back to the previous song",
    defaults: ["J"],
  },
  {
    id: "seekForward",
    label: "Seek forward",
    description: "Jump ahead by the configured skip step",
    defaults: ["ArrowRight"],
  },
  {
    id: "seekBackward",
    label: "Seek back",
    description: "Jump back by the configured skip step",
    defaults: ["ArrowLeft"],
  },
  {
    id: "volumeUp",
    label: "Volume up",
    description: "Raise the volume by 5%",
    defaults: ["ArrowUp"],
  },
  {
    id: "volumeDown",
    label: "Volume down",
    description: "Lower the volume by 5%",
    defaults: ["ArrowDown"],
  },
];

/** Modifier keys, which are never a binding on their own. */
const MODIFIER_KEYS = new Set(["Control", "Alt", "Shift", "Meta"]);

/**
 * The binding a key press stands for, or null if the press is only modifiers
 * being held down.
 */
export function bindingFor(event: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;

  const parts: string[] = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Meta");
  parts.push(normaliseKey(event.key));
  return parts.join("+");
}

/** " " is unreadable in a settings list, and "k" and "K" are the same key. */
function normaliseKey(key: string): string {
  if (key === " " || key === "Spacebar") return "Space";
  return key.length === 1 ? key.toUpperCase() : key;
}

/** The bindings in force for an action: the user's, or the defaults. */
export function bindingsFor(
  action: ShortcutAction,
  overrides: Record<string, string[]>,
): string[] {
  const override = overrides[action];
  if (override && override.length > 0) return override;
  return SHORTCUTS.find((shortcut) => shortcut.id === action)?.defaults ?? [];
}

/** The action a binding runs, or null when it is bound to nothing. */
export function actionFor(
  binding: string,
  overrides: Record<string, string[]>,
): ShortcutAction | null {
  for (const shortcut of SHORTCUTS) {
    if (bindingsFor(shortcut.id, overrides).includes(binding)) return shortcut.id;
  }
  return null;
}

/** Arrows and the space bar read better as symbols than as their key names. */
const KEY_LABELS: Record<string, string> = {
  ArrowLeft: "←",
  ArrowRight: "→",
  ArrowUp: "↑",
  ArrowDown: "↓",
  Space: "Space",
  Escape: "Esc",
};

/** A binding as it is shown to the user. */
export function bindingLabel(binding: string): string {
  return binding
    .split("+")
    .map((part) => KEY_LABELS[part] ?? part)
    .join(" + ");
}
