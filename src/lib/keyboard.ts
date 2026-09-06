/**
 * Global keyboard shortcuts.
 *
 * What each key does is decided by the catalogue in `lib/shortcuts.ts` and the
 * user's own bindings, which are read fresh on every press: rebinding a key in
 * settings takes effect immediately, with nothing to reinstall.
 *
 * Keys are ignored while a text field has focus, so typing a playlist name
 * never scrubs the music. Escape is deliberately not in the catalogue — it
 * backs out of whatever is open, which is a fixed part of how the app is
 * navigated rather than a preference.
 *
 * They are also ignored entirely while something else owns the transport —
 * the Master Mixer, which has its own space bar and its own idea of what is
 * playing. Two handlers reaching the engine on the same key press is how a
 * pause turns into a stop.
 */

import type { Router } from "vue-router";
import { actionFor, bindingFor } from "@/lib/shortcuts";
import type { usePlayerStore } from "@/stores/player";
import type { useUiStore } from "@/stores/ui";

type Player = ReturnType<typeof usePlayerStore>;
type Ui = ReturnType<typeof useUiStore>;

/** How far the seek keys jump, and how much the volume keys move. */
const SEEK_SECONDS = 5;
const VOLUME_STEP = 0.05;

function isTyping(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable === true
  );
}

export interface ShortcutOptions {
  /** True while another view owns playback and these keys must not fire. */
  isSuspended?: () => boolean;
  /** The user's rebindings, read on every press so changes apply at once. */
  bindings?: () => Record<string, string[]>;
}

export function installShortcuts(
  player: Player,
  ui?: Ui,
  router?: Router,
  options: ShortcutOptions = {},
): () => void {
  async function onKeydown(event: KeyboardEvent) {
    if (isTyping(event.target)) return;
    if (options.isSuspended?.()) return;

    if (event.key === "Escape") {
      // Back out of the full-screen view first, then any open panel.
      if (router && router.currentRoute.value.name === "nowPlaying") {
        event.preventDefault();
        router.back();
      } else if (ui?.queueOpen) {
        event.preventDefault();
        ui.queueOpen = false;
      }
      return;
    }

    const binding = bindingFor(event);
    if (!binding) return;
    const action = actionFor(binding, options.bindings?.() ?? {});
    if (!action) return;
    event.preventDefault();

    switch (action) {
      case "playPause":
        await player.toggle();
        break;
      case "nextTrack":
        await player.next();
        break;
      case "previousTrack":
        await player.previous();
        break;
      case "seekForward":
        await player.seek(Math.min(player.duration, player.position + SEEK_SECONDS));
        break;
      case "seekBackward":
        await player.seek(Math.max(0, player.position - SEEK_SECONDS));
        break;
      case "volumeUp":
        await player.setVolume(Math.min(1, player.snapshot.volume + VOLUME_STEP));
        break;
      case "volumeDown":
        await player.setVolume(Math.max(0, player.snapshot.volume - VOLUME_STEP));
        break;
    }
  }

  window.addEventListener("keydown", onKeydown);
  return () => window.removeEventListener("keydown", onKeydown);
}
