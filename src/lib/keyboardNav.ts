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

/** Parse "Ctrl+Alt+K" into the modifiers + key code it expects from a
 * KeyboardEvent. Same combo grammar as the backend's parse_hotkey. */
export function parseCombo(
  combo: string,
): { ctrl: boolean; alt: boolean; shift: boolean; meta: boolean; code: string } | null {
  let ctrl = false,
    alt = false,
    shift = false,
    meta = false;
  let key: string | null = null;
  for (const part of combo.split("+")) {
    const p = part.trim().toLowerCase();
    if (p === "ctrl" || p === "control") ctrl = true;
    else if (p === "alt") alt = true;
    else if (p === "shift") shift = true;
    else if (p === "super" || p === "win" || p === "meta" || p === "cmd") meta = true;
    else if (p === "") return null;
    else if (key === null) key = p;
    else return null;
  }
  if (!key) return null;
  const up = key.toUpperCase();
  const code =
    up.length === 1 && /[A-Z]/.test(up)
      ? `Key${up}`
      : up.length === 1 && /[0-9]/.test(up)
        ? `Digit${up}`
        : up; // F1..F12 pass through
  return { ctrl, alt, shift, meta, code };
}

/** True when `e` matches the recorded combo (e.g. the Actions shortcut). */
export function matchesCombo(e: KeyboardEvent, combo: string): boolean {
  const m = parseCombo(combo);
  if (!m) return false;
  return (
    e.ctrlKey === m.ctrl &&
    e.altKey === m.alt &&
    e.shiftKey === m.shift &&
    e.metaKey === m.meta &&
    e.code === m.code
  );
}
