<script setup lang="ts">
/** Placeholder until the home experience is designed. */
import PnmIcon from "@/components/icons/PnmIcon.vue";
</script>

<template>
  <div class="home">
    <PnmIcon name="home" :size="44" class="home__icon" />
    <h1>Home</h1>
    <p>
      This is where recently played music, new additions and mixes will live.
      For now, head to your Library or pick a playlist from the sidebar.
    </p>
    <RouterLink to="/library" class="pill-button">Open Library</RouterLink>
  </div>
</template>

<style scoped>
.home {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  height: 100%;
  text-align: center;
  color: var(--text-secondary);
}

.home__icon {
  color: var(--text-tertiary);
}

h1 {
  margin: 4px 0 0;
  font-size: 22px;
  font-weight: 600;
  color: var(--text);
}

p {
  margin: 0;
  max-width: 380px;
  font-size: 13px;
  line-height: 1.55;
}

.pill-button {
  margin-top: 8px;
  text-decoration: none;
}
</style>
