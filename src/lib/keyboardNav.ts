// Pure selection-index math for keyboard navigation (PR1).
// Clamp, never wrap: with hundreds of rows a wrap-around jump is
// disorienting, and clamping keeps Up/Down WYSIWYG.

/** Move `current` by `delta` rows, clamped to `[0, length - 1]`. Empty list → 0. */
export function moveSelection(current: number, delta: number, length: number): number {
  if (length <= 0) return 0;
  return Math.min(Math.max(current + delta, 0), length - 1);
}

/** Clamp a possibly stale index (e.g. list shrank after a filter/delete). */
export function clampSelection(index: number, length: number): number {
  if (length <= 0) return 0;
  return Math.min(Math.max(index, 0), length - 1);
}
