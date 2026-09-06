export function formatTime(rfc3339: string): string {
  const d = new Date(rfc3339);
  if (Number.isNaN(d.getTime())) return "";
  return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

export function extractColor(content: string): string | null {
  const m = content.trim().match(/^(#[0-9a-fA-F]{3,8}|rgba?\([^)]+\))$/);
  return m ? m[1] : null;
}
