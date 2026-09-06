use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

static RE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://\S+$").unwrap());
static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap());
static RE_HEX_COLOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").unwrap());
static RE_RGB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"rgb\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*\)").unwrap()
});

const CODE_TOKENS: &[&str] = &[
    "{", "}", "=>", "function ", "const ", "let ", "var ", "import ", "class ", "def ", "fn ",
    "return ", "public ", "private ", "#include", "SELECT ", "INSERT ",
];

pub fn hash_content(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn categorize(s: &str) -> &'static str {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return "plain";
    }
    if RE_LINK.is_match(trimmed) {
        return "link";
    }
    if RE_EMAIL.is_match(trimmed) {
        return "email";
    }
    if RE_HEX_COLOR.is_match(trimmed) || RE_RGB.is_match(trimmed) {
        return "color";
    }
    let line_count = trimmed.lines().count();
    let has_code_token = CODE_TOKENS.iter().any(|t| trimmed.contains(t));
    let looks_indented = line_count >= 3
        && trimmed
            .lines()
            .filter(|l| l.starts_with(' ') || l.starts_with('\t'))
            .count()
            >= 2;
    if looks_indented || (line_count >= 2 && has_code_token) || (has_code_token && trimmed.contains(';')) {
        return "code";
    }
    "plain"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categorizes_link() {
        assert_eq!(categorize("https://example.com"), "link");
        assert_eq!(categorize("  http://foo.bar/baz?q=1  "), "link");
    }

    #[test]
    fn categorizes_email() {
        assert_eq!(categorize("hi@test.com"), "email");
    }

    #[test]
    fn categorizes_color() {
        assert_eq!(categorize("#ff00ff"), "color");
        assert_eq!(categorize("#FF00FFAA"), "color");
        assert_eq!(categorize("rgb(12, 34, 56)"), "color");
    }

    #[test]
    fn categorizes_code() {
        assert_eq!(categorize("function foo() {\n  return 42;\n}"), "code");
        assert_eq!(categorize("const x = 1;\nconst y = 2;"), "code");
        assert_eq!(categorize("def main():\n    print('hi')\n    return 0"), "code");
    }

    #[test]
    fn categorizes_plain() {
        assert_eq!(categorize("hello world"), "plain");
        assert_eq!(categorize("a single line with { braces }"), "plain");
        assert_eq!(categorize(""), "plain");
    }

    #[test]
    fn hashes_deterministically() {
        assert_eq!(hash_content(b"hello"), hash_content(b"hello"));
        assert_ne!(hash_content(b"hello"), hash_content(b"world"));
        assert_eq!(hash_content(b"hello").len(), 64);
    }
}
