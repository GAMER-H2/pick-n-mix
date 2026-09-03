# Pick n Mix — UI Design System

This document is the contract for building or changing UI in this application.
Read it before writing any component. It exists so that every surface — a new
view, a dialog, a popover, a settings row — comes out looking and behaving like
it belongs to the same app, and so that logic is never copied between components
when it could be shared.

The visual language is Apple Music's proportions and restraint, with orange in
place of its red. When in doubt, do less: fewer borders, fewer colours, fewer
weights.

---

## 1. Directory layout

```
src/
  components/
    ui/           Design-system primitives. No feature knowledge, no stores.
    icons/        PnmIcon — the only source of iconography.
    media/        Artwork, PlaylistArtwork, TrackRow, QueueList, MixCard.
    layout/       App chrome: Sidebar, NowPlayingBar, NowPlayingScreen,
                  QueuePanel, BounceProgress.
    collections/  Shared building blocks for collection pages
                  (CollectionHeader, TrackList).
    dialogs/      Small task dialogs (AddToPlaylist, DuplicateFiles, Bounce).
    overlays/     ContextMenu.
    mixer/        Mixer feature components.
    mastermix/    Master mixer feature components.
    settings/     Settings feature components.
  composables/    Reusable behaviour (useWindowChrome, useBackendEvents,
                  useQueueActions, useMenu, …).
  views/          Routed pages. Orchestrate stores + shared components;
                  contain only logic unique to that page.
  stores/         Pinia stores. UI state that outlives a component lives here.
  lib/            Pure functions and types. No Vue, no DOM.
  styles/         theme.css — every design token lives here.
```

**Dependency rule:** `ui/` may not import from `stores/`, `lib/api`, or any
feature folder. Feature components may use `ui/`. Views may use everything.
If a `ui/` component needs app state, the design is wrong — lift the state to
the caller and pass it as props.

---

## 2. Design tokens

Everything visual is driven by CSS custom properties in `src/styles/theme.css`.
Never hardcode a colour, radius, shadow, duration, or z-index in a component.
If a value you need is missing, add a token there first and use it.

### Colour

| Token | Use |
|---|---|
| `--accent` / `--accent-hover` / `--accent-active` | The single brand colour, in the role Apple Music gives red. |
| `--accent-tint` / `--accent-tint-strong` | Accent-tinted backgrounds (active states, secondary buttons). |
| `--accent-secondary` | Only where two things must be told apart at a glance (the two songs in a crossfade). Never a second brand colour. |
| `--bg`, `--bg-elevated`, `--bg-sidebar`, `--bg-bar`, `--bg-sunken` | Surfaces, from lowest to highest elevation. |
| `--bg-hover`, `--bg-active` | Interactive hover/pressed fills. |
| `--text`, `--text-secondary`, `--text-tertiary` | Three levels of text emphasis. Nothing gets a fourth. |
| `--separator`, `--separator-strong` | Hairlines. |

### Z-index scale

Layering is a token, never a magic number:

| Token | Value | Layer |
|---|---|---|
| `--z-panel` | 300 | Slide-in side panels (queue, mixer). |
| `--z-popover` | 600 | Anchored popovers and menus (teleported to `<body>`). |
| `--z-modal` | 520 | Modal dialogs and workspace modals. |
| `--z-modal-top` | 530 | A modal that must sit above another modal (Master Mix over EQ). |
| `--z-context` | 540 | The context menu — it can be opened from anywhere, including inside modals. |

### Type, space, motion

- Base font size is 13px; titles step 17 / 22 / 28 / 34px. Secondary text is
  12–12.5px in `--text-secondary`; hints and metadata are 11.5–12px in
  `--text-tertiary`.
- Page padding is `6px 26px 40px`; every routed view's root element carries it
  (see the page recipe in §6).
- All transitions use `var(--ease)`; durations are 0.10–0.24s. The global
  `prefers-reduced-motion` rule already neutralises animation — do not add
  motion that carries meaning without a text alternative.
- Radii: `--radius-sm` (controls), `--radius` (menus), `--radius-lg`
  (popovers/dialogs), `--radius-xl` (large surfaces).

---

## 3. HCI principles (and how this app applies them)

These are not decoration; each maps to a concrete rule below.

1. **Consistency and standards.** Same action, same control, everywhere.
   Play is always the accent pill; destructive is always `is-danger`; a menu is
   always a `MenuSurface`. Never restyle a primitive per-feature.
2. **Visibility of system status.** Long operations show progress (scanning,
   bouncing); disabled controls say why in their `title`; the player bar always
   shows what is playing and which context it came from.
3. **Recognition, not recall.** Icon buttons always carry `title` **and**
   `aria-label` with the same human text. Sort/search state lives in the URL so
   Back/Forward restore it.
4. **User control and freedom.** Every modal closes on Escape and scrim click
   (except where a task must not be abandoned mid-write). Destructive actions
   get a confirm or an undo path.
5. **Error prevention and recovery.** Disable actions that cannot succeed
   (`:disabled` + opacity is the standard look). Errors surface as toasts via
   `ui.notify(message, "error")`, never as `alert()`.
6. **Flexibility and efficiency.** Every list row supports dblclick-to-play and
   right-click-for-more. Keyboard shortcuts are installed once, in
   `lib/keyboard.ts`, and stand down while the Master Mixer owns transport.
7. **Accessibility.** Icon-only controls get `aria-label`. Dialogs get
   `role="dialog"`, `aria-modal`, an accessible name, initial focus, and focus
   restore on close. Knobs and sliders are keyboard operable. Hit targets are
   ≥ 26px; icon buttons are 30px.
8. **Progressive disclosure.** Advanced controls live behind the mixer panel,
   popover, or modal — not on the main surface. Hover reveals secondary row
   actions; the current row's actions stay visible.

---

## 4. UI primitives (`src/components/ui/`)

These are the only approved ways to build the things they cover. If you find
yourself writing a scrim, a menu surface, or an icon-button triplet by hand,
stop and use the primitive.

### IconButton

```vue
<IconButton icon="close" label="Close" @click="close" />
<IconButton icon="mixer" label="Mixer" :active="mixerOpen" :disabled="!ready" />
```
Wraps the global `.icon-button` class + `PnmIcon`. `label` becomes both
`title` and `aria-label`. `active` applies the accent-tinted state.

### BaseModal

The single modal implementation: scrim, dialog shell, Escape (capture-phase,
`stopPropagation`), scrim click, initial focus, focus restore, `fade`
transition, teleport to `<body>`, `role="dialog"` + `aria-modal`.

```vue
<BaseModal :open="open" title="Bounce Playlist" subtitle="Render to a single file"
           :width="380" @close="open = false">
  <!-- body -->
  <template #footer>
    <button class="pill-button is-plain" @click="open = false">Cancel</button>
    <button class="pill-button" @click="go">Bounce</button>
  </template>
</BaseModal>
```

- Props: `open`, `title?`, `subtitle?`, `width?` (px), `labelledby?` (when the
  title lives in custom header content), `closeOnScrim?` (default true),
  `closeOnEsc?` (default true), `layer?: "modal" | "modal-top"`.
- Slots: `default`, `header` (replaces the title row), `footer`.
- A modal with its own global key handling (Master Mix) sets `close-on-esc`
  to false and keeps its own handler; everything else defers to BaseModal.
- Footer buttons: primary = `.pill-button`, secondary = `.pill-button.is-secondary`,
  plain = `.pill-button.is-plain`, destructive = `.pill-button.is-danger`.

### MenuSurface

The one menu look: elevated surface, 5px padding, hover rows, check ticks,
danger rows, separators. Used by ContextMenu, SelectMenu, and preset selects.

```vue
<MenuSurface :groups="[{ label: 'Built In', items }, { items: mine }]"
             @select="id => apply(id)" />
```
- Item shape: `{ id, label, icon?, checked?, danger?, disabled? }`.
- Custom rows (e.g. an inline "save as" form) go in the default slot.
- Anchoring/positioning is the caller's job (see Popover recipe, §6).

### EmptyState

```vue
<EmptyState icon="folder" title="Add your music"
            message="Choose a folder and Pick n Mix will index everything in it.">
  <button class="pill-button" @click="choose">Choose Folder</button>
</EmptyState>
```
- Full-page variant (icon 44, centred, min-height 60–70vh) by default;
  `compact` for inline "nothing matches this filter" messages inside lists
  and grids.

### MediaCard

Artwork-over-title-over-subtitle card used by every grid (home playlists,
library albums, artists).

```vue
<MediaCard :title="album.name" :subtitle="album.artist"
           @open="nav" @menu="openMenu($event, { tracks })">
  <Artwork :artwork-id="album.artworkId" :size="152" :radius="7" shadow />
</MediaCard>
```
Owns the hover lift, truncation, and grid-cell behaviour. Round artwork
(artists) via the `round` prop on the Artwork you slot in.

### SearchField

```vue
<SearchField v-model="query" placeholder="Search songs" />
```
Pill-shaped, sunken background, search icon. Debouncing and URL sync stay in
the caller (see `LibraryView` for the pattern).

### Tabs

```vue
<Tabs :tabs="[{ id: 'songs', label: 'Songs' }, …]" v-model="tab" />
```
Underline style, accent active indicator. Tab state belongs in the route query,
not local state.

### SliderRow / FormRow

```vue
<SliderRow label="Pitch" v-model="pitch" :min="-12" :max="12" :origin="0"
           :format="(v) => `${v > 0 ? '+' : ''}${v.toFixed(1)} st`" />
<FormRow label="Crossfade" hint="Blend between songs">
  <AppToggle v-model="enabled" />
</FormRow>
```
`SliderRow` = label + `AppSlider` + formatted value (all extra `AppSlider`
props pass through). `FormRow` = label + control + optional hint, the standard
settings/dialog field row.

### Controls: AppSlider, AppKnob, AppToggle

Live in `ui/` because they are featureless controls. Rules:

- Always bind with `v-model` (never the spelled-out
  `:model-value`/`@update:model-value` pair).
- Detent behaviour is identical in concept between slider and knob; if you
  touch either, keep the two `snap()` implementations in agreement (or extract
  a shared `useDetents` composable rather than letting them drift).
- Knobs must remain keyboard-operable (arrow keys), like sliders.

---

## 5. Media & collection components

| Component | Purpose |
|---|---|
| `media/Artwork` | Single cover, DPR-aware, placeholder + icon fallback. |
| `media/PlaylistArtwork` | Single cover **or** 4-cover quilt. The quilt rule (`>= 4 ids → quilt, slice(0,4)`) lives only here. |
| `media/TrackRow` | One row in any track list. Emits `play` / `menu` / `mixer`; renders current-state, missing-state, index, duration. |
| `media/QueueList` | The queue's row list (drag grips, remove buttons). The only place queue rows exist. |
| `media/MixCard` | Home's three mix banners. Uses `PlaylistArtwork` for its quilt. |
| `collections/CollectionHeader` | Playlist/album/artist/mix page header: artwork, title, subtitle, meta, Play/Shuffle/Mixer/More. |
| `collections/TrackList` | `TrackRow` list with current/playing wiring, empty state, optional drag-reorder. Views do not hand-roll `v-for` + TrackRow anymore. |

**Rule:** artwork is never an `<img>`; it is `Artwork` or `PlaylistArtwork`.
**Rule:** a track is never rendered by hand; it is `TrackRow` (or `QueueList`
in the queue).

---

## 6. Recipes

### A new page (view)

1. Create `src/views/ThingView.vue`, add a lazy route in `src/router.ts`.
2. Compose the page from primitives; keep only page-unique logic in the file:

```vue
<template>
  <div class="page">                      <!-- padding: 6px 26px 40px -->
    <EmptyState v-if="empty" … />
    <template v-else>
      <CollectionHeader … @menu="openMenu($event, { tracks })" />
      <TrackList :tracks="tracks" :current-id="player.track?.id ?? null"
                 :playing="player.playing" show-artwork
                 @play="playFrom" @menu="(e, t) => openMenu(e, { tracks: [t] })" />
    </template>
  </div>
</template>
```

3. Use the shared composables instead of re-deriving behaviour:
   - `useMenu()` — `openMenu(event, payload)` wraps `ui.openContextMenu` and
     the `clientX/clientY` extraction.
   - `useCollectionMeta(tracks)` — song counts, total duration, the
     "N songs · 1 hr 12 min" string.
   - `useRouteParamLoader` — watch a route param, fetch, manage `loading`.
   - Playback: toggle-if-current idiom and shuffle-and-play come from
     `useCollectionPlayback` (or the store-backed equivalent for playlists and
     mixes, which play through the backend).
4. State that should survive Back/Forward (tab, query, sort) goes in the URL
   query, `replace`d while typing, `push`ed on discrete changes.

### A dialog or modal

Use `BaseModal`. Never write a scrim, an Escape handler, or a focus routine.
Open state for global dialogs lives in the `ui` store; task-local dialogs may
keep local state. Footer order: Cancel (plain) left of the primary action.

### A popover or anchored menu

1. Teleport to `<body>` (backdrop-filter on the player bar creates stacking
   contexts; anchoring inside it breaks).
2. Dismiss with `useDismiss(ref, { onDismiss, ignore: [triggerRef] })` from
   `lib/dismiss.ts` — never a full-window scrim div, never hand-rolled
   listeners.
3. Position with CSS relative to a positioned ancestor when the anchor is
   stable (`.pnm-popover` pattern); measure and clamp only when the anchor
   moves (context menu).
4. z-index: `var(--z-popover)`.

### A context menu

Build the payload and call `ui.openContextMenu({ x, y, … })` — always via
`useMenu().openMenu(event, payload)` so coordinates are extracted once.
The menu itself renders `MenuSurface` inside `overlays/ContextMenu.vue`.

### A settings row or mixer control row

`FormRow` for toggle/label pairs, `SliderRow` for sliders with readouts,
`SectionHeader` (mixer) for section titles with enable-toggles. Do not create
new label/control CSS.

### Surface titling

Every mixer/audio surface — and any future one — titles itself the same way:
a small-caps accent **eyebrow** naming the surface, above a **title** naming
what the surface is acting on:

| Surface | Eyebrow | Title |
|---|---|---|
| Settings modal | `Pick n Mix` | Settings |
| Settings pane | pane description | pane name |
| Mixer popover | `DJ Mixer` | target (playlist/track/Global) |
| Advanced mixer panel | `Advanced DJ Mixer` (or `EQ Preset Editor`) | target |
| Equaliser modal | `Equaliser` | target |
| Master mixer modal | `Master Mixer` | playlist name |

Use the global `.eyebrow` class (theme.css); never restyle it per component.
Sub-sections *inside* a surface keep `SectionHeader` (mixer) or
`.section-heading h4` (settings) — the eyebrow pattern is for the surface
itself.

### An icon

Add the path to `components/icons/paths.ts` and use `<PnmIcon>`. Inline SVGs
are forbidden except for data visualisations (graphs, waveforms, knob dials).

---

## 7. Code conventions

- `<script setup lang="ts">` with a one-paragraph doc comment at the top
  explaining what the component is for and any non-obvious decision.
- Strict types: no `any`, no `as` casts without justification, props declared
  with `defineProps<{…}>()` + `withDefaults`, emits with the typed
  `defineEmits<{…}>()` syntax.
- Control bindings use `v-model`.
- Scoped styles, class names in BEM-ish `block__element` form, design tokens
  for every value they cover.
- Stores are the only cross-component state; a component that receives state
  as props stays presentational.
- Events are named as intents (`play`, `close`, `select`), not DOM names.

## 8. Accessibility checklist

- [ ] Icon-only control has `aria-label` (and `title`) — use `IconButton`.
- [ ] Dialog: `role="dialog"`, `aria-modal`, accessible name, Escape works,
      focus moves in on open and back on close.
- [ ] Menu: `role="menu"`/`menuitem`, arrow-key navigation where the menu is
      keyboard-reachable.
- [ ] Every image-bearing control has a text alternative (title/aria-label).
- [ ] Colour is never the only carrier of meaning (current track = accent
      *and* play glyph; mixer override = dot *and* active tint).
- [ ] New motion respects `prefers-reduced-motion` (automatic via theme.css).
- [ ] Hit targets ≥ 26px; spacing between adjacent targets ≥ 8px.

## 9. Anti-patterns (do not do these)

- Copying a scrim, dialog shell, menu surface, or close button from another
  component instead of using the primitive.
- A new z-index number. Add a token if the scale genuinely needs a new layer.
- Feature logic in `ui/` components, or store imports inside them.
- A second button system (`.primary-button` etc.) — the pill-button variants
  are the only buttons.
- Hand-rolled dismissal (window scrims, bubble-phase listeners) instead of
  `useDismiss`.
- Rendering artwork or track rows by hand.
- `v-model`-less bindings on `AppSlider`/`AppKnob`/`AppToggle` in new code.
