import { describe, it, expect } from "vitest";
import { dayLabel, extractColor, formatBytes, formatTime, richPreviewHtml, truncateText } from "./format";

describe("format", () => {
  it("extracts hex colors", () => {
    expect(extractColor("#ff00ff")).toBe("#ff00ff");
    expect(extractColor("  #ABC  ")).toBe("#ABC");
  });

  it("extracts rgb and rgba", () => {
    expect(extractColor("rgb(12, 34, 56)")).toBe("rgb(12, 34, 56)");
    expect(extractColor("rgba(1,2,3,0.5)")).toBe("rgba(1,2,3,0.5)");
  });

  it("returns null for non-colors", () => {
    expect(extractColor("hello")).toBeNull();
    expect(extractColor("")).toBeNull();
  });

  it("formats RFC3339 times, empty on invalid", () => {
    expect(formatTime("not-a-date")).toBe("");
    expect(formatTime("2026-01-01T12:00:00Z")).toMatch(/\d/);
  });

  it("passes short fragments through", () => {
    expect(richPreviewHtml("<h1>Hi</h1>")).toBe("<h1>Hi</h1>");
    expect(richPreviewHtml(null)).toBeNull();
  });

  it("never cuts mid-tag (blank-card regression)", () => {
    // Live shape: one giant <ul style="..."> with no ">" inside the budget.
    const frag = `<ul style="${"x".repeat(2500)}"><li>Poodle</li></ul>`;
    const out = richPreviewHtml(frag, 2000);
    // Kept whole tags only — no half-open tag for the WebView to choke on.
    expect(out === null || !/<[^>]*$/.test(out)).toBe(true);
  });

  it("falls back when no visible text would survive", () => {
    expect(richPreviewHtml("<ul></ul>")).toBeNull();
    const shells = `<ul style="${"x".repeat(2500)}">`;
    expect(richPreviewHtml(shells, 2000)).toBeNull();
  });

  it("labels day groups Today / Yesterday / date", () => {
    const now = new Date();
    const iso = (d: Date) => d.toISOString();
    expect(dayLabel(iso(now))).toBe("Today");
    const yesterday = new Date(now.getTime() - 24 * 3_600_000);
    expect(dayLabel(iso(yesterday))).toBe("Yesterday");
    expect(dayLabel("not-a-date")).toBe("");
    // Far past renders as a short date (locale-dependent — just assert shape).
    expect(dayLabel("2020-01-02T00:00:00Z")).not.toBe("Today");
  });

  it("truncates without splitting surrogate pairs", () => {
    // "a" + 😀 (surrogate pair) cut at 2 would split the pair → broken glyph.
    const emoji = "a\u{1F600}b";
    expect(truncateText(emoji, 2)).toBe("a…");
    expect(truncateText(emoji, 3)).toBe("a\u{1F600}…");
    expect(truncateText(emoji, 99)).toBe(emoji);
    // Plain ASCII passes through unchanged.
    expect(truncateText("hello", 3)).toBe("hel…");
  });

  it("formats byte counts", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(12_582_912)).toBe("12 MB");
    expect(formatBytes(3_221_225_472)).toBe("3 GB");
  });
});
