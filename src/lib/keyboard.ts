/**
 * Global keyboard shortcuts.
 *
 * Space and K play/pause, L skips forward, J skips back. Keys are ignored
 * while a text field has focus, so typing a playlist name never scrubs the
 * music, and modifier combinations are left to the OS.
 *
 * They are also ignored entirely while something else owns the transport —
 * the Master Mixer, which has its own space bar and its own idea of what is
 * playing. Two handlers reaching the engine on the same key press is how a
 * pause turns into a stop.
 */

import type { Router } from "vue-router";
import type { usePlayerStore } from "@/stores/player";
import type { useUiStore } from "@/stores/ui";

type Player = ReturnType<typeof usePlayerStore>;
type Ui = ReturnType<typeof useUiStore>;

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

export function installShortcuts(
  player: Player,
  ui?: Ui,
  router?: Router,
  /** True while another view owns playback and these keys must not fire. */
  isSuspended?: () => boolean,
): () => void {
  async function onKeydown(event: KeyboardEvent) {
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    if (isTyping(event.target)) return;
    if (isSuspended?.()) return;

    switch (event.key) {
      case "Escape":
        // Back out of the full-screen view first, then any open panel.
        if (router && router.currentRoute.value.name === "nowPlaying") {
          event.preventDefault();
          router.back();
        } else if (ui?.queueOpen) {
          event.preventDefault();
          ui.queueOpen = false;
        }
        break;
      case " ":
      case "k":
      case "K":
        event.preventDefault();
        await player.toggle();
        break;
      case "l":
      case "L":
        event.preventDefault();
        await player.next();
        break;
      case "j":
      case "J":
        event.preventDefault();
        await player.previous();
        break;
      case "ArrowRight":
        event.preventDefault();
        await player.seek(Math.min(player.duration, player.position + 5));
        break;
      case "ArrowLeft":
        event.preventDefault();
        await player.seek(Math.max(0, player.position - 5));
        break;
      case "ArrowUp":
        event.preventDefault();
        await player.setVolume(Math.min(1, player.snapshot.volume + 0.05));
        break;
      case "ArrowDown":
        event.preventDefault();
        await player.setVolume(Math.max(0, player.snapshot.volume - 0.05));
        break;
    }
  }

  window.addEventListener("keydown", onKeydown);
  return () => window.removeEventListener("keydown", onKeydown);
}
