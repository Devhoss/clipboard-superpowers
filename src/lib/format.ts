export function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function extractColor(content: string): string | null {
  const m = content.trim().match(/^(#[0-9a-fA-F]{3,8}|rgba?\([^)]+\))$/);
  return m ? m[1] : null;
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
    cut = cut.slice(0, end + 1);
  }
  // Unclosed elements auto-close on innerHTML parse — only a total lack of
  // visible text (bare <ul></ul>, empty style shells) needs the fallback.
  if (cut.replace(/<[^>]*>/g, "").trim().length === 0) return null;
  return cut;
}
