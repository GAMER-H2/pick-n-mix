<script setup lang="ts">
/**
 * Route wrapper for the full-screen player.
 *
 * It is a route rather than a flag so the back and forward buttons treat it
 * like any other page, which is what closing it should feel like.
 */
import NowPlayingScreen from "@/components/NowPlayingScreen.vue";
</script>

<template>
  <NowPlayingScreen />
</template>
