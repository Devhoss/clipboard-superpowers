export function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function extractColor(content: string): string | null {
  const m = content.trim().match(/^(#[0-9a-fA-F]{3,8}|rgba?\([^)]+\))$/);
  return m ? m[1] : null;
}

<<<<<<< HEAD
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
