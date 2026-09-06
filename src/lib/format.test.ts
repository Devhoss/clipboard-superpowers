import { describe, it, expect } from "vitest";
import { extractColor, formatTime } from "./format";

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
});
