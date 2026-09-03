/**
 * The icon set.
 *
 * Every glyph is drawn on the same 24x24 grid with the same stroke weight and
 * round caps, so they read as one family. `mode` picks between an outlined
 * glyph and a solid one; the transport controls are solid because that is what
 * reads best at the size the player bar uses them.
 */

export type IconMode = "stroke" | "fill";

export interface IconDef {
  mode: IconMode;
  /** Path data, drawn in order. */
  d: readonly string[];
  /** Extra circles, which are clumsy to express as path data. */
  circles?: readonly { cx: number; cy: number; r: number; fill?: boolean }[];
  /** Small numerals, used by the skip-by-ten controls. */
  text?: { x: number; y: number; value: string; size: number };
}

const DEFS = {
  // -- navigation ---------------------------------------------------------
  home: {
    mode: "stroke",
    d: ["M3.2 10.6 12 3.4l8.8 7.2", "M5.6 9.5V20.2h12.8V9.5", "M9.9 20.2v-5.1h4.2v5.1"],
  },
  library: {
    mode: "stroke",
    d: [
      "M4.2 5.4a2 2 0 0 1 2-2h11.6a2 2 0 0 1 2 2v13.2a2 2 0 0 1-2 2H6.2a2 2 0 0 1-2-2z",
      "M14.6 7.6v6.9",
      "M14.6 7.6 9.4 8.9v6.9",
    ],
    circles: [
      { cx: 8, cy: 15.8, r: 1.9 },
      { cx: 13.2, cy: 14.5, r: 1.9 },
    ],
  },

  // -- transport ----------------------------------------------------------
  play: { mode: "fill", d: ["M7.6 4.9a1 1 0 0 1 1.53-.85l10 6.95a1.2 1.2 0 0 1 0 2l-10 6.95a1 1 0 0 1-1.53-.85z"] },
  pause: {
    mode: "fill",
    d: [
      "M7.4 4.6h2.6a1 1 0 0 1 1 1v12.8a1 1 0 0 1-1 1H7.4a1 1 0 0 1-1-1V5.6a1 1 0 0 1 1-1z",
      "M14 4.6h2.6a1 1 0 0 1 1 1v12.8a1 1 0 0 1-1 1H14a1 1 0 0 1-1-1V5.6a1 1 0 0 1 1-1z",
    ],
  },
  previous: {
    mode: "fill",
    d: [
      "M18.6 5.4a1 1 0 0 0-1.54-.84L8.8 10.2V5.6a1 1 0 0 0-1-1h-1a1 1 0 0 0-1 1v12.8a1 1 0 0 0 1 1h1a1 1 0 0 0 1-1v-4.6l8.26 5.64a1 1 0 0 0 1.54-.84z",
    ],
  },
  next: {
    mode: "fill",
    d: [
      "M5.4 5.4a1 1 0 0 1 1.54-.84l8.26 5.64V5.6a1 1 0 0 1 1-1h1a1 1 0 0 1 1 1v12.8a1 1 0 0 1-1 1h-1a1 1 0 0 1-1-1v-4.6l-8.26 5.64a1 1 0 0 1-1.54-.84z",
    ],
  },
  back10: {
    mode: "stroke",
    // The arc ends exactly where the arrowhead's corner sits, so they join.
    d: ["M3.8 12a8.2 8.2 0 1 0 2.7-6.1", "M6.5 2.2v4.7h3.7"],
    text: { x: 12, y: 12, value: "10", size: 7 },
  },
  forward10: {
    mode: "stroke",
    d: ["M20.2 12a8.2 8.2 0 1 1-2.7-6.1", "M17.5 2.2v4.7H13.8"],
    text: { x: 12, y: 12, value: "10", size: 7 },
  },
  shuffle: {
    mode: "stroke",
    // Each arrowhead's vertex lands on the endpoint of the line it terminates.
    d: [
      "M3.4 6.8h3.2c1.4 0 2.7.7 3.5 1.9l3.8 5.6c.8 1.2 2.1 1.9 3.5 1.9h3.2",
      "M3.4 17.2h3.2c1.4 0 2.7-.7 3.5-1.9l.9-1.3",
      "M13.2 8.7l.9-1.3c.8-1.2 2.1-1.9 3.5-1.9h3.2",
      "M17.8 2.7 20.6 5.5 17.8 8.3",
      "M17.8 13.4 20.6 16.2 17.8 19",
    ],
  },
  repeat: {
    mode: "stroke",
    // Arrowhead vertices sit on the ends of the loop they belong to.
    d: [
      "M17.2 3.4 20.8 7 17.2 10.6",
      "M20.8 7H7.4A3.6 3.6 0 0 0 3.8 10.6v1.2",
      "M6.8 20.6 3.2 17 6.8 13.4",
      "M3.2 17h13.4a3.6 3.6 0 0 0 3.6-3.6v-1.2",
    ],
  },
  repeatOne: {
    mode: "stroke",
    d: [
      "M17.2 3.4 20.8 7 17.2 10.6",
      "M20.8 7H7.4A3.6 3.6 0 0 0 3.8 10.6v1.2",
      "M6.8 20.6 3.2 17 6.8 13.4",
      "M3.2 17h13.4a3.6 3.6 0 0 0 3.6-3.6v-1.2",
    ],
    text: { x: 12, y: 12, value: "1", size: 7.2 },
  },

  // -- the mixer, echoing the app icon -------------------------------------
  mixer: {
    mode: "stroke",
    d: ["M6.2 3.8v16.4", "M12 3.8v16.4", "M17.8 3.8v16.4"],
    circles: [
      { cx: 6.2, cy: 15.6, r: 2.3, fill: true },
      { cx: 12, cy: 8.2, r: 2.3, fill: true },
      { cx: 17.8, cy: 13, r: 2.3, fill: true },
    ],
  },
  equaliser: {
    mode: "stroke",
    d: ["M4 18.4V13", "M4 9.6V5.6", "M12 18.4v-8", "M12 7V5.6", "M20 18.4v-3.2", "M20 11.8V5.6", "M2.4 11.3h3.2", "M10.4 8.7h3.2", "M18.4 13.5h3.2"],
  },

  // -- lists and queue ------------------------------------------------------
  queue: {
    mode: "stroke",
    d: ["M3.6 6.4h11.2", "M3.6 12h11.2", "M3.6 17.6h7"],
    circles: [{ cx: 18.6, cy: 16.2, r: 2.6 }],
  },
  // The top line is the short one: it stands for the new "next" slot, and the
  // hook curves up from below to point straight at it — inserted right after
  // what's currently playing, not appended at the end.
  playNext: {
    mode: "stroke",
    d: [
      "M3.6 7.6h5.6",
      "M3.6 12.6h8.4",
      "M3.6 17.6h8.4",
      "M20.6 17.4v-8a1.8 1.8 0 0 0 -1.8-1.8h-4",
      "M17.2 4.8 14.8 7.6 17.2 10.4",
    ],
  },
  // The mirror image of `playNext`: the short bottom line stands for the
  // newly appended slot, and the hook curves down from above to point
  // straight at it.
  addToQueue: {
    mode: "stroke",
    d: [
      "M3.6 6.4h8.4",
      "M3.6 11.4h8.4",
      "M3.6 16.4h5.6",
      "M20.6 6.6v8a1.8 1.8 0 0 1 -1.8 1.8h-4",
      "M17.2 13.6 14.8 16.4 17.2 19.2",
    ],
  },
  addToPlaylist: {
    mode: "stroke",
    d: ["M3.6 7.2h12", "M3.6 12h9", "M3.6 16.8h6.4", "M17.6 12.4v7.2", "M14 16h7.2"],
  },
  artist: {
    mode: "stroke",
    d: ["M5.4 20.4v-1.6a5.2 5.2 0 0 1 5.2-5.2h2.8a5.2 5.2 0 0 1 5.2 5.2v1.6"],
    circles: [{ cx: 12, cy: 7.6, r: 4 }],
  },
  album: {
    mode: "stroke",
    d: [],
    circles: [
      { cx: 12, cy: 12, r: 8.4 },
      { cx: 12, cy: 12, r: 2.2 },
    ],
  },

  // -- utility --------------------------------------------------------------
  settings: {
    mode: "stroke",
    d: [
      "M12 3.2v2.2",
      "M12 18.6v2.2",
      "M3.2 12h2.2",
      "M18.6 12h2.2",
      "M5.8 5.8l1.6 1.6",
      "M16.6 16.6l1.6 1.6",
      "M18.2 5.8l-1.6 1.6",
      "M7.4 16.6l-1.6 1.6",
    ],
    circles: [
      { cx: 12, cy: 12, r: 6.6 },
      { cx: 12, cy: 12, r: 2.7 },
    ],
  },
  info: { mode: "stroke", d: ["M12 11v5.4", "M12 7.9v.1"], circles: [{ cx: 12, cy: 12, r: 8.6 }] },
  warningCircle: {
    mode: "stroke",
    d: ["M12 7.2v6.2", "M12 16.8v.1"],
    circles: [{ cx: 12, cy: 12, r: 8.6 }],
  },
  duplicateFiles: {
    mode: "stroke",
    d: [
      "M7.2 6.2V5a1.8 1.8 0 0 1 1.8-1.8h9.8A1.8 1.8 0 0 1 20.6 5v11a1.8 1.8 0 0 1-1.8 1.8h-1.2",
      "M5.2 6.2H15a1.8 1.8 0 0 1 1.8 1.8v11A1.8 1.8 0 0 1 15 20.8H5.2A1.8 1.8 0 0 1 3.4 19V8a1.8 1.8 0 0 1 1.8-1.8z",
      "M11.8 10.2v5.1",
      "M11.8 10.2 8.4 11v5.1",
    ],
    circles: [
      { cx: 7.2, cy: 16.1, r: 1.5 },
      { cx: 10.6, cy: 15.3, r: 1.5 },
    ],
  },
  minimize: { mode: "stroke", d: ["M5.4 12h13.2"] },
  maximize: { mode: "stroke", d: ["M5.6 5.6h12.8v12.8H5.6z"] },
  close: { mode: "stroke", d: ["M6.4 6.4 17.6 17.6", "M17.6 6.4 6.4 17.6"] },
  plus: { mode: "stroke", d: ["M12 5.2v13.6", "M5.2 12h13.6"] },
  check: { mode: "stroke", d: ["M4.8 12.6 9.8 17.6 19.2 6.8"] },
  search: { mode: "stroke", d: ["M20.4 20.4 16.1 16.1"], circles: [{ cx: 10.6, cy: 10.6, r: 6.6 }] },
  more: {
    mode: "stroke",
    d: [],
    circles: [
      { cx: 5.4, cy: 12, r: 1.7, fill: true },
      { cx: 12, cy: 12, r: 1.7, fill: true },
      { cx: 18.6, cy: 12, r: 1.7, fill: true },
    ],
  },
  expand: { mode: "stroke", d: ["M14 4.6h5.4V10", "M19.4 4.6 13 11", "M10 19.4H4.6V14", "M4.6 19.4 11 13"] },
  collapse: { mode: "stroke", d: ["M19.4 10H14V4.6", "M14 10l5.4-5.4", "M4.6 14H10v5.4", "M10 14l-5.4 5.4"] },
  chevronRight: { mode: "stroke", d: ["M9.4 5.2 16.2 12l-6.8 6.8"] },
  chevronDown: { mode: "stroke", d: ["M5.2 9.4 12 16.2l6.8-6.8"] },
  chevronUp: { mode: "stroke", d: ["M5.2 14.6 12 7.8l6.8 6.8"] },
  chevronLeft: { mode: "stroke", d: ["M14.6 5.2 7.8 12l6.8 6.8"] },
  folder: { mode: "stroke", d: ["M3.4 6.6a1.8 1.8 0 0 1 1.8-1.8h3.7l2 2.6h7.9a1.8 1.8 0 0 1 1.8 1.8v8.2a1.8 1.8 0 0 1-1.8 1.8H5.2a1.8 1.8 0 0 1-1.8-1.8z"] },
  image: {
    mode: "stroke",
    d: [
      "M4.4 5.4h15.2v13.2H4.4z",
      "M4.4 15.6l4.2-4a1.4 1.4 0 0 1 1.9 0l5 4.8",
      "M14.2 13.4l1.6-1.5a1.4 1.4 0 0 1 1.9 0l1.9 1.8",
    ],
    circles: [{ cx: 9.2, cy: 9.4, r: 1.3 }],
  },
  trash: { mode: "stroke", d: ["M4.6 6.6h14.8", "M9.4 6.6V4.8a1.2 1.2 0 0 1 1.2-1.2h2.8a1.2 1.2 0 0 1 1.2 1.2v1.8", "M6.6 6.6l.9 12a1.8 1.8 0 0 0 1.8 1.6h5.4a1.8 1.8 0 0 0 1.8-1.6l.9-12", "M10.4 10.4v6", "M13.6 10.4v6"] },
  edit: { mode: "stroke", d: ["M4.6 15.6 15.9 4.3a2.1 2.1 0 0 1 3 3L7.6 18.6l-4 1z"] },
  share: { mode: "stroke", d: ["M12 15.4V3.8", "M8.4 7.4 12 3.8l3.6 3.6", "M5.4 12.6v6a1.8 1.8 0 0 0 1.8 1.8h9.6a1.8 1.8 0 0 0 1.8-1.8v-6"] },
  importFile: { mode: "stroke", d: ["M12 3.8v11.6", "M8.4 11.8 12 15.4l3.6-3.6", "M5.4 12.6v6a1.8 1.8 0 0 0 1.8 1.8h9.6a1.8 1.8 0 0 0 1.8-1.8v-6"] },
  sparkle: { mode: "stroke", d: ["M12 3.6 13.7 9 19.1 10.7 13.7 12.4 12 17.8 10.3 12.4 4.9 10.7 10.3 9z", "M18.4 16.2l.7 2.1 2.1.7-2.1.7-.7 2.1-.7-2.1-2.1-.7 2.1-.7z"] },
  volume: { mode: "stroke", d: ["M4.6 9.4h3.2L12.6 5v14L7.8 14.6H4.6z", "M16.2 9.2a4 4 0 0 1 0 5.6", "M18.9 6.5a7.8 7.8 0 0 1 0 11"] },
  volumeLow: {
    mode: "stroke",
    d: ["M4.6 9.4h3.2L12.6 5v14L7.8 14.6H4.6z", "M16.2 9.2a4 4 0 0 1 0 5.6"],
  },
  volumeOff: {
    mode: "stroke",
    // A struck-through speaker: unmistakably "no sound", not just "quiet".
    d: ["M4.6 9.4h3.2L12.6 5v14L7.8 14.6H4.6z", "M3.4 3.4 20.6 20.6"],
  },
  volumeMute: { mode: "stroke", d: ["M4.6 9.4h3.2L12.6 5v14L7.8 14.6H4.6z", "M16.4 9.8 21 14.4", "M21 9.8 16.4 14.4"] },
  music: { mode: "stroke", d: ["M9.4 18V6.2l10-2v11.4"], circles: [{ cx: 6.6, cy: 18, r: 2.8 }, { cx: 16.6, cy: 15.6, r: 2.8 }] },
  clock: { mode: "stroke", d: ["M12 7.4V12l3 1.8"], circles: [{ cx: 12, cy: 12, r: 8.6 }] },
  grip: {
    mode: "stroke",
    d: [],
    // Two columns of three dots, the conventional drag affordance.
    circles: [
      { cx: 9, cy: 6, r: 1.4, fill: true },
      { cx: 15, cy: 6, r: 1.4, fill: true },
      { cx: 9, cy: 12, r: 1.4, fill: true },
      { cx: 15, cy: 12, r: 1.4, fill: true },
      { cx: 9, cy: 18, r: 1.4, fill: true },
      { cx: 15, cy: 18, r: 1.4, fill: true },
    ],
  },
  filter: { mode: "stroke", d: ["M3.8 6.2h16.4", "M6.6 12h10.8", "M9.6 17.8h4.8"] },

  // -- master mixer -------------------------------------------------------
  /** Stacked lanes with a region on each: the playlist's timeline. */
  timeline: {
    mode: "stroke",
    d: [
      "M3.4 5.4h9.2v4H3.4z",
      "M8.4 14.6h12.2v4H8.4z",
      "M3.4 14.6h2",
      "M17.6 5.4h3",
    ],
  },
  /** The selection tool: a plain arrow, as in the drawing. */
  pointer: { mode: "stroke", d: ["M6.4 3.6 18.6 12.9l-5.3.8 2.9 5.6-2.4 1.2-2.9-5.6-3.5 3.8z"] },
  /** The blade: scissors, which is what splitting a region reads as. */
  blade: {
    mode: "stroke",
    d: ["M7.4 7.4 18.6 18.6", "M16.6 7.4 9.6 14.4"],
    circles: [
      { cx: 6.2, cy: 17.4, r: 2.6 },
      { cx: 17.8, cy: 17.4, r: 2.6 },
    ],
  },
  /** Bounce the mix to a file. */
  bounce: {
    mode: "stroke",
    d: [
      "M12 3.6v10.4",
      "M8.2 10.4 12 14.2l3.8-3.8",
      "M5.2 17.4h13.6",
    ],
  },
  /** Volume automation: a breakpoint envelope over a region. */
  automation: {
    mode: "stroke",
    d: ["M3.4 17.6 8.6 9.4l5.4 5 6.6-8"],
    circles: [
      { cx: 8.6, cy: 9.4, r: 1.7 },
      { cx: 14, cy: 14.4, r: 1.7 },
    ],
  },
  stop: { mode: "fill", d: ["M6.2 6.9a.7.7 0 0 1 .7-.7h10.2a.7.7 0 0 1 .7.7v10.2a.7.7 0 0 1-.7.7H6.9a.7.7 0 0 1-.7-.7z"] },
  undo: { mode: "stroke", d: ["M4.6 9.6h9.8a5.2 5.2 0 0 1 0 10.4H8.2", "M8.4 5.4 4.2 9.6l4.2 4.2"] },
  redo: { mode: "stroke", d: ["M19.4 9.6H9.6a5.2 5.2 0 0 0 0 10.4h6.2", "M15.6 5.4l4.2 4.2-4.2 4.2"] },
} as const;

export type IconName = keyof typeof DEFS;

/**
 * Widened to `IconDef` so consumers see the full shape, while `IconName`
 * still comes from the literal keys above.
 */
export const ICONS: Record<IconName, IconDef> = DEFS;
