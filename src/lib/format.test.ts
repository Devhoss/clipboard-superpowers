import { describe, it, expect } from "vitest";
import { extractColor, formatTime, richPreviewHtml } from "./format";

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
});
