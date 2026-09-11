import { describe, expect, it } from "vitest";
import { tokenizeCode, type Token } from "./highlight";

const kinds = (line: Token[]) =>
  line.map((t) => [t.kind, t.text] as const);

describe("tokenizeCode", () => {
  it("highlights keywords, calls, numbers, strings and comments (JS)", () => {
    const [line] = tokenizeCode('t = setTimeout(() => fn(42), "ms"); // tick');
    // Consecutive plain characters merge into a single token.
    expect(kinds(line)).toEqual([
      [null, "t = "],
      ["fn", "setTimeout"],
      [null, "(() => "],
      // "fn" is in the (Rust-inclusive) keyword set — keyword beats call.
      ["kw", "fn"],
      [null, "("],
      ["num", "42"],
      [null, "), "],
      ["str", '"ms"'],
      [null, "); "],
      ["cm", "// tick"],
    ]);
  });

  it("tracks block comments across lines", () => {
    const lines = tokenizeCode("let a; /* start\nstill comment */ let b;");
    expect(kinds(lines[0])).toEqual([
      ["kw", "let"],
      [null, " a; "],
      ["cm", "/* start"],
    ]);
    expect(kinds(lines[1])).toEqual([
      ["cm", "still comment */"],
      [null, " "],
      ["kw", "let"],
      [null, " b;"],
    ]);
  });

  it("treats `#` as a comment only before a space (keeps #include, attrs)", () => {
    const [, py] = tokenizeCode("#include <stdio.h>\n# plain comment");
    expect(kinds(py)).toEqual([["cm", "# plain comment"]]);
    const rust = tokenizeCode("#[derive(Debug)]\nstruct S;");
    expect(kinds(rust[0])).toEqual([
      [null, "#["],
      ["fn", "derive"],
      [null, "(Debug)]"],
    ]);
    expect(kinds(rust[1])).toContainEqual(["kw", "struct"]);
  });

  it("honors backslash escapes inside strings", () => {
    const [line] = tokenizeCode(`const s = "a\\"b) // not a comment";`);
    const str = line.find((t) => t.kind === "str");
    expect(str!.text).toBe(`"a\\"b) // not a comment"`);
    expect(line.some((t) => t.kind === "cm")).toBe(false);
  });

  it("colors hex numbers and keywords case-insensitively", () => {
    const [line] = tokenizeCode("True False None 0xFF_00 mask");
    expect(kinds(line)).toEqual([
      ["kw", "True"],
      [null, " "],
      ["kw", "False"],
      [null, " "],
      ["kw", "None"],
      [null, " "],
      ["num", "0xFF_00"],
      [null, " mask"],
    ]);
  });

  it("handles empty and unterminated inputs without throwing", () => {
    expect(tokenizeCode("")).toEqual([[]]);
    const [line] = tokenizeCode('const s = "unterminated');
    expect(line.some((t) => t.kind === "str")).toBe(true);
    const blk = tokenizeCode("/* never closed");
    expect(kinds(blk[0])).toEqual([["cm", "/* never closed"]]);
  });
});
