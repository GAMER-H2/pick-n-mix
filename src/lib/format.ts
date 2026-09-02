/** Small display helpers shared across views. */

export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return "--:--";
  const total = Math.floor(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = total % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

/** "1 hr 12 min", used for album and playlist totals. */
export function formatTotal(seconds: number): string {
  const total = Math.round(seconds);
  const hours = Math.floor(total / 3600);
  const minutes = Math.round((total % 3600) / 60);
  if (hours > 0) return `${hours} hr ${minutes} min`;
  if (minutes > 0) return `${minutes} min`;
  return `${total} sec`;
}

export function formatBytes(bytes: number | null): string {
  if (bytes == null) return "Unknown";
  const units = ["B", "KB", "MB", "GB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 2)} ${units[unit]}`;
}

export function formatHz(hz: number | null): string {
  if (hz == null) return "Unknown";
  return hz >= 1000 ? `${(hz / 1000).toFixed(hz % 1000 === 0 ? 0 : 1)} kHz` : `${hz} Hz`;
}

/**
 * URL for a cached cover, served by the app's `art://` handler.
 *
 * `width` asks the backend for a downscaled copy sized for where it is
 * actually displayed — the embedded picture in a file can be several
 * megapixels, and decoding that for every row of a long list is expensive
 * enough to visibly stutter scrolling. Omit it for the rare case the full
 * original is wanted (e.g. a backdrop that is blown up past its own size).
 */
export function artUrl(artworkId: string | null | undefined, width?: number): string | null {
  if (!artworkId) return null;
  const query = width ? `?w=${width}` : "";
  return `art://localhost/${encodeURIComponent(artworkId)}${query}`;
}

/** "Album - Artist - Year", skipping whatever is missing. */
export function subtitleFor(parts: (string | number | null | undefined)[]): string {
  return parts.filter((p) => p !== null && p !== undefined && `${p}`.trim() !== "").join(" · ");
}

export function semitonesLabel(semitones: number, cents: number): string {
  const total = semitones + cents / 100;
  if (Math.abs(total) < 0.005) return "0";
  return `${total > 0 ? "+" : ""}${total.toFixed(2).replace(/\.?0+$/, "")}`;
}
