<script setup lang="ts">
/**
 * The strip along the top of a full-window overlay that still drags the
 * window.
 *
 * Where the app draws its own decorations — Linux, and macOS's overlaid
 * traffic lights — the only thing to move the window by is the title bar the
 * app draws itself, and an open modal covers it. Without this, a modal has to
 * be closed before the window can be moved at all.
 *
 * Drop it in as the *first* child of a scrim. It is deliberately behind the
 * dialog (which takes its own stacking position), so a tall dialog reaching
 * into the title bar keeps its clicks; only the dead space around it drags.
 * Clicks land on this rather than on the scrim, so a scrim that closes on
 * click stays open while the window is being moved.
 */
</script>

<template>
  <div class="window-drag-region" data-tauri-drag-region />
</template>

<style scoped>
.window-drag-region {
  position: absolute;
  top: 0;
  /* Clear of the window buttons, wherever the platform puts them: macOS
     floats them over the left, the app draws its own on the right. Both are
     zero when the platform decorates the window for us. */
  left: var(--overlay-controls);
  right: var(--titlebar-controls);
  /* Zero — and so not there at all — where the platform draws the title bar
     outside the webview and there is nothing here to drag by. */
  height: var(--titlebar-height);
}

/* macOS keeps its decorations but floats the traffic lights over the webview,
   so this strip is ours to drag by even though the window is decorated.
   Matches `.app__titlebar`, which reserves the same band. */
:global(html.is-mac-overlay) .window-drag-region {
  height: 30px;
}
</style>
