<script setup lang="ts">
/**
 * The shared preset dropdown: a trigger button, a `MenuSurface` of grouped
 * built-in entries, a "Yours" section whose rows carry a delete affordance,
 * and an inline save row. Both mixer preset selects (mixer presets and EQ
 * curves) render through this so the trigger, dismissal, save form and delete
 * affordance exist once.
 *
 * Presentational by design: stores stay in the feature wrappers, which map
 * their state onto the props and handle `select` / `delete` / `save`.
 */
import { nextTick, ref } from "vue";
import PnmIcon from "../icons/PnmIcon.vue";
import MenuSurface, { type MenuGroup, type MenuItem } from "./MenuSurface.vue";
import { useDismiss } from "@/lib/dismiss";

const props = withDefaults(
  defineProps<{
    /** Text shown on the trigger button. */
    label: string;
    /** Non-deletable entries, already grouped (e.g. "Built In"). */
    groups: MenuGroup[];
    /** The user's own entries, rendered under "Yours" with a delete affordance. */
    custom: MenuItem[];
    /** Placeholder and accessible name of the inline save row's input. */
    savePlaceholder: string;
    /** Label of the row that opens the save form. */
    saveActionLabel: string;
    /** Accessible name of the per-row delete control. */
    deleteLabel?: string;
    /** Fill the parent's width (the mixer panels) instead of hugging content. */
    stretch?: boolean;
  }>(),
  { deleteLabel: "Delete preset", stretch: false },
);

const emit = defineEmits<{
  select: [id: string];
  delete: [id: string];
  save: [name: string];
}>();

const open = ref(false);
const naming = ref(false);
const draftName = ref("");
const menuEl = ref<HTMLElement | null>(null);

useDismiss(
  () => open.value,
  () => (open.value = false),
  menuEl,
);

async function toggle() {
  open.value = !open.value;
  if (!open.value) return;
  naming.value = false;
  // Focus the current entry so the arrow keys have somewhere to start.
  await nextTick();
  menuEl.value?.querySelector<HTMLElement>("[aria-checked='true']")?.focus();
}

function onSelect(id: string) {
  open.value = false;
  emit("select", id);
}

function save() {
  const name = draftName.value.trim();
  if (!name) return;
  emit("save", name);
}

/**
 * Called by the parent once a save has actually succeeded, so a failed save
 * keeps the form open with the draft intact.
 */
function closeSaveRow() {
  naming.value = false;
  draftName.value = "";
}

defineExpose({ closeSaveRow });
</script>

<template>
  <div class="preset" :class="{ 'preset--stretch': props.stretch }">
    <button class="preset__button" aria-haspopup="menu" :aria-expanded="open" @click="toggle">
      <span class="truncate">{{ label }}</span>
      <PnmIcon name="chevronDown" :size="14" />
    </button>

    <Transition name="pop">
      <div v-if="open" ref="menuEl" class="preset__menu">
        <MenuSurface :groups="groups" @select="onSelect">
          <template v-if="custom.length">
            <div class="preset__separator" />
            <div class="preset__group">Yours</div>
            <button
              v-for="item in custom"
              :key="item.id"
              class="preset__item"
              role="menuitem"
              @click="emit('select', item.id)"
            >
              <span class="truncate">{{ item.label }}</span>
              <span class="preset__actions">
                <PnmIcon v-if="item.checked" name="check" :size="14" />
                <span class="preset__delete" :title="deleteLabel" @click.stop="emit('delete', item.id)">
                  <PnmIcon name="trash" :size="13" />
                </span>
              </span>
            </button>
          </template>

          <div class="preset__separator" />
          <div v-if="naming" class="preset__save">
            <input
              v-model="draftName"
              class="text-field"
              :placeholder="savePlaceholder"
              :aria-label="savePlaceholder"
              autofocus
              @keydown.enter="save"
              @keydown.esc="naming = false"
            />
            <button class="pill-button" @click="save">Save</button>
          </div>
          <button v-else class="preset__item" role="menuitem" @click="naming = true">
            <PnmIcon name="plus" :size="14" />
            <span>{{ saveActionLabel }}</span>
          </button>
        </MenuSurface>
      </div>
    </Transition>
  </div>
</template>

<style scoped>
.preset {
  position: relative;
}

.preset--stretch {
  width: 100%;
}

.preset__button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  min-width: 160px;
  height: 30px;
  padding: 0 10px;
  border-radius: var(--radius-sm);
  border: 1px solid var(--separator);
  background: var(--bg-elevated);
  font-size: 12.5px;
  color: var(--text);
}

.preset__button:hover {
  border-color: var(--separator-strong);
}

.preset--stretch .preset__button {
  width: 100%;
}

.preset__menu {
  position: absolute;
  z-index: var(--z-popover);
  top: calc(100% + 5px);
  right: 0;
  width: 230px;
  max-height: min(360px, 70vh);
  overflow-y: auto;
}

.preset--stretch .preset__menu {
  left: 0;
  right: 0;
  width: auto;
  max-height: 320px;
}

.preset__group {
  padding: 6px 9px 3px;
  font-size: 11px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
  color: var(--text-tertiary);
}

.preset__item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  padding: 7px 9px;
  border-radius: var(--radius-sm);
  font-size: 13px;
  text-align: left;
  color: var(--text);
}

.preset__item:hover {
  background: var(--accent-tint);
  color: var(--accent);
}

.preset__actions {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.preset__delete {
  display: inline-flex;
  color: var(--text-tertiary);
  opacity: 0;
}

.preset__item:hover .preset__delete,
.preset__delete:focus-visible {
  opacity: 1;
}

.preset__delete:hover {
  color: #e0383e;
}

.preset__separator {
  height: 1px;
  margin: 5px 8px;
  background: var(--separator);
}

.preset__save {
  display: flex;
  gap: 6px;
  padding: 4px;
}

.preset__save input {
  min-width: 0;
}
</style>
