export function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

/** "Today" / "Yesterday" / short date — sidebar group headers + info rows. */
export function dayLabel(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  const startOfDay = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86_400_000);
  if (days <= 0) return "Today";
  if (days === 1) return "Yesterday";
  return d.toLocaleDateString([], { month: "short", day: "numeric" });
}

export function extractColor(content: string): string | null {
  const m = content.trim().match(/^(#[0-9a-fA-F]{3,8}|rgba?\([^)]+\))$/);
  return m ? m[1] : null;
}

/**
 * Truncate text for card preview. Backs off one code unit when the cut lands
 * inside a UTF-16 surrogate pair — a split pair renders as a broken `�`
 * glyph (the "weird icon" on long clips).
 */
export function truncateText(text: string, max: number): string {
  if (text.length <= max) return text;
  const last = text.charCodeAt(max - 1);
  // High surrogate at the cut means its low surrogate got cut off.
  const cut = last >= 0xd800 && last <= 0xdbff ? max - 1 : max;
  return text.slice(0, cut) + "…";
}

/**
 * Trim a sanitized HTML fragment for card preview. Cuts at a tag boundary —
 * never mid-tag: a half-open `<ul style="...` makes the WebView swallow the
 * rest as attribute junk and the card renders blank (seen live with a 2.5k
 * Tailwind-styled list fragment). Returns null when nothing visible would
 * survive (caller falls back to plain text); the stored full HTML used for
 * copy-back is untouched.
 */
export function richPreviewHtml(html: string | null, max = 2000): string | null {
  if (!html) return null;
  let cut = html;
  if (cut.length > max) {
    const end = cut.lastIndexOf(">", max);
    if (end < 0) return null;
    // The cut lands right after an ASCII '>', so it can never split a
    // surrogate pair — no extra back-off needed here (unlike truncateText).
    cut = cut.slice(0, end + 1);
  }
  // Unclosed elements auto-close on innerHTML parse — only a total lack of
  // visible text (bare <ul></ul>, empty style shells) needs the fallback.
  if (cut.replace(/<[^>]*>/g, "").trim().length === 0) return null;
  return cut;
}

/** Human byte counts for file cards: 512 → "512 B", 2048 → "2 KB". */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const v = bytes / 1024 ** i;
  return `${Number.isInteger(v) ? v : v.toFixed(1)} ${units[i]}`;
}
