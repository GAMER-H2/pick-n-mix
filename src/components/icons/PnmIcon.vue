<script setup lang="ts">
/**
 * Single entry point for every icon, so weight, sizing and colour are decided
 * in one place rather than drifting between call sites.
 */
import { computed } from "vue";
import { ICONS, type IconName } from "./paths";

const props = withDefaults(
  defineProps<{
    name: IconName;
    /** Rendered size in pixels. */
    size?: number;
    /** Overrides the default weight; useful for very small or very large uses. */
    weight?: number;
  }>(),
  { size: 20 },
);

const icon = computed(() => ICONS[props.name]);

// Thin the stroke as the icon grows so the visual weight stays even.
const strokeWidth = computed(() => {
  if (props.weight) return props.weight;
  if (props.size >= 40) return 1.4;
  if (props.size <= 16) return 1.9;
  return 1.7;
});
</script>

<template>
  <svg
    :width="size"
    :height="size"
    viewBox="0 0 24 24"
    fill="none"
    aria-hidden="true"
    focusable="false"
    class="pnm-icon"
  >
    <g
      v-if="icon.mode === 'stroke'"
      stroke="currentColor"
      :stroke-width="strokeWidth"
      stroke-linecap="round"
      stroke-linejoin="round"
    >
      <path v-for="(d, i) in icon.d" :key="i" :d="d" />
      <circle
        v-for="(c, i) in icon.circles ?? []"
        :key="`c${i}`"
        :cx="c.cx"
        :cy="c.cy"
        :r="c.r"
        :fill="c.fill ? 'currentColor' : 'none'"
      />
    </g>

    <g v-else fill="currentColor">
      <path v-for="(d, i) in icon.d" :key="i" :d="d" />
      <circle
        v-for="(c, i) in icon.circles ?? []"
        :key="`c${i}`"
        :cx="c.cx"
        :cy="c.cy"
        :r="c.r"
      />
    </g>

    <!-- `dy` rather than dominant-baseline: baseline handling differs between
         renderers, and a shifted numeral is very visible at this size. -->
    <text
      v-if="icon.text"
      :x="icon.text.x"
      :y="icon.text.y"
      dy="0.35em"
      :font-size="icon.text.size"
      fill="currentColor"
      text-anchor="middle"
      font-weight="600"
      font-family="inherit"
    >
      {{ icon.text.value }}
    </text>
  </svg>
</template>

<style scoped>
.pnm-icon {
  display: block;
  flex: none;
}
</style>
