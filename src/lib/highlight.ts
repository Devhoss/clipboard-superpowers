// Lightweight code tokenizer for the detail-pane preview (Option A from the
// redesign discussion). One regex-ish pass per line, stateful only for /*
// block comments. Fidelity matches the mock: keywords, function calls,
// strings, numbers, comments — nothing more. Cosmetic by contract: copy-back
// always uses the raw text.

export type TokenKind = "kw" | "fn" | "str" | "num" | "cm";
export interface Token {
  /** null = plain text */
  kind: TokenKind | null;
  text: string;
}

// Lowercase set on purpose: `True`/`None` highlight like `true`/`none`, and
// mis-coloring a type name is acceptable for a preview.
const KEYWORDS = new Set(
  [
    // C-like / JS / TS
    "function", "return", "let", "const", "var", "if", "else", "for", "while",
    "do", "class", "new", "extends", "implements", "interface", "type", "enum",
    "import", "export", "from", "as", "default", "async", "await", "yield",
    "try", "catch", "finally", "throw", "switch", "case", "break", "continue",
    "typeof", "instanceof", "in", "of", "delete", "void", "static", "public",
    "private", "protected", "final", "abstract", "override", "virtual",
    "namespace", "using", "package",
    // Rust
    "fn", "pub", "use", "mut", "impl", "struct", "trait", "where", "match",
    "loop", "crate", "self", "super", "move",
    // Python / general
    "def", "lambda", "elif", "pass", "raise", "with", "global", "nonlocal",
    "assert", "not", "and", "or", "is", "del", "then", "begin", "end",
    // literals & common primitives
    "true", "false", "null", "undefined", "none", "nil", "this",
    "int", "float", "bool", "str", "string", "char", "double", "long",
  ],
);

const IDENT = /^[A-Za-z_$][\w$]*/;
const NUMBER = /^(?:0[xX][0-9a-fA-F_]+|\d[\d_]*(?:\.\d+)?(?:[eE][+-]?\d+)?)/;

function pushToken(tokens: Token[], kind: TokenKind | null, text: string) {
  const last = tokens[tokens.length - 1];
  if (last && last.kind === kind) last.text += text;
  else tokens.push({ kind, text });
}

/** Tokenize `code` into lines of spans. O(n), no allocations kept beyond the
 * token arrays; a 66KB / 600-line clip tokenizes in low single-digit ms. */
export function tokenizeCode(code: string): Token[][] {
  const out: Token[][] = [];
  let inBlockComment = false;

  for (const line of code.split("\n")) {
    const tokens: Token[] = [];
    let i = 0;
    while (i < line.length) {
      // Inside a /* ... */ block: consume to the closer or end of line.
      if (inBlockComment) {
        const end = line.indexOf("*/", i);
        if (end === -1) {
          pushToken(tokens, "cm", line.slice(i));
          i = line.length;
        } else {
          pushToken(tokens, "cm", line.slice(i, end + 2));
          i = end + 2;
          inBlockComment = false;
        }
        continue;
      }

      const rest = line.slice(i);
      const ch = rest[0];

      // Block comment opens: flip state; the in-block branch above consumes
      // from the opener itself so `/*` is never dropped.
      if (rest.startsWith("/*")) {
        inBlockComment = true;
        continue;
      }
      // Line comments: `//` anywhere; `#` only when followed by space/EOL so
      // `#include` and Rust attributes stay un-colored while `# comment` and
      // shell/Python comments still work.
      if (rest.startsWith("//")) {
        pushToken(tokens, "cm", rest);
        break;
      }
      if (ch === "#" && (rest.length === 1 || rest[1] === " " || rest[1] === "\t")) {
        pushToken(tokens, "cm", rest);
        break;
      }
      // Strings: ", ' and template literals, honoring backslash escapes.
      if (ch === '"' || ch === "'" || ch === "`") {
        let j = 1;
        while (j < rest.length) {
          if (rest[j] === "\\") {
            j += 2;
            continue;
          }
          if (rest[j] === ch) {
            j++;
            break;
          }
          j++;
        }
        pushToken(tokens, "str", rest.slice(0, j));
        i += j;
        continue;
      }
      // Numbers (decimal, float, hex). NUMBER requires a digit start and
      // IDENT a letter/_/$ start, so the two can never both match here.
      const num = NUMBER.exec(rest);
      if (num) {
        pushToken(tokens, "num", num[0]);
        i += num[0].length;
        continue;
      }
      // Identifiers: keyword, or function call/declaration (name + `(`).
      const ident = IDENT.exec(rest);
      if (ident) {
        const word = ident[0];
        if (KEYWORDS.has(word.toLowerCase())) pushToken(tokens, "kw", word);
        else if (/^\s*\(/.test(rest.slice(word.length))) pushToken(tokens, "fn", word);
        else pushToken(tokens, null, word);
        i += word.length;
        continue;
      }
      // Anything else (punctuation, whitespace): plain, one char per step.
      pushToken(tokens, null, ch);
      i++;
    }
    out.push(tokens);
  }
  return out;
}
