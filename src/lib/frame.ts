/**
 * Where the window actually ends.
 *
 * With client-side decorations the outer `--frame-inset` pixels of the
 * viewport are the transparent margin the window casts its shadow into — the
 * desktop, as far as the user is concerned. Anything positioned against the
 * viewport edge (a menu flipping to stay on screen, a popover clamped into
 * view) has to clamp against these bounds instead, or it lands outside the
 * window. The inset is zero where the platform decorates the window itself, so
 * callers need no platform check of their own.
 */

/** The shadow margin, in CSS pixels. */
export function frameInset(): number {
  const raw = getComputedStyle(document.documentElement).getPropertyValue("--frame-inset");
  return Number.parseFloat(raw) || 0;
}

export interface VisibleBounds {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
}

/** The visible window, in viewport coordinates. */
export function visibleBounds(): VisibleBounds {
  const inset = frameInset();
  return {
    left: inset,
    top: inset,
    right: window.innerWidth - inset,
    bottom: window.innerHeight - inset,
    width: window.innerWidth - inset * 2,
    height: window.innerHeight - inset * 2,
  };
}
