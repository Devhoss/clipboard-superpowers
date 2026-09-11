import { describe, expect, it } from "vitest";
import { clampSelection, matchesCombo, moveSelection, parseCombo } from "./keyboardNav";

describe("moveSelection", () => {
  it("moves down by delta", () => {
    expect(moveSelection(0, 1, 5)).toBe(1);
    expect(moveSelection(1, 2, 5)).toBe(3);
  });

  it("moves up by delta", () => {
    expect(moveSelection(3, -1, 5)).toBe(2);
  });

  it("clamps at the top instead of wrapping", () => {
    expect(moveSelection(0, -1, 5)).toBe(0);
  });

  it("clamps at the bottom instead of wrapping", () => {
    expect(moveSelection(4, 1, 5)).toBe(4);
    expect(moveSelection(2, 99, 5)).toBe(4);
  });

  it("returns 0 for an empty list", () => {
    expect(moveSelection(0, 1, 0)).toBe(0);
    expect(moveSelection(3, -1, 0)).toBe(0);
  });
});

describe("clampSelection", () => {
  it("clamps an out-of-range index after the list shrinks", () => {
    expect(clampSelection(9, 3)).toBe(2);
  });

  it("keeps a valid index untouched", () => {
    expect(clampSelection(1, 3)).toBe(1);
  });

  it("returns 0 for an empty list", () => {
    expect(clampSelection(2, 0)).toBe(0);
  });
});

describe("parseCombo / matchesCombo", () => {
  it("parses modifiers and maps keys to codes", () => {
    expect(parseCombo("Ctrl+K")).toEqual({
      ctrl: true, alt: false, shift: false, meta: false, code: "KeyK",
    });
    expect(parseCombo("Ctrl+Alt+7")!.code).toBe("Digit7");
    expect(parseCombo("Alt+F9")!.code).toBe("F9");
    expect(parseCombo("Ctrl+")).toBeNull();
    expect(parseCombo("K")).toEqual({ ctrl: false, alt: false, shift: false, meta: false, code: "KeyK" });
  });

  it("matches real key events against the combo", () => {
    const ev = (init: Partial<KeyboardEvent> & { code: string }) =>
      ({ ctrlKey: false, altKey: false, shiftKey: false, metaKey: false, ...init }) as KeyboardEvent;
    expect(matchesCombo(ev({ code: "KeyK", ctrlKey: true }), "Ctrl+K")).toBe(true);
    expect(matchesCombo(ev({ code: "KeyK" }), "Ctrl+K")).toBe(false);
    expect(matchesCombo(ev({ code: "KeyK", ctrlKey: true, shiftKey: true }), "Ctrl+K")).toBe(false);
    expect(matchesCombo(ev({ code: "Digit7", ctrlKey: true, altKey: true }), "Ctrl+Alt+7")).toBe(true);
    expect(matchesCombo(ev({ code: "KeyK" }), "bogus")).toBe(false);
  });
});
