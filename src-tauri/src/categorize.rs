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
    Regex::new(r"rgba?\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*(,\s*[\d.]+\s*)?\)").unwrap()
});
// Secrets (PR3): key=value credential shapes, standalone OTP digits, PEM
// private-key headers. The keyword must stand alone or be _-separated
// (client_secret matches, mytoken doesn't) and must be followed by = or :
// ("my password is long" doesn't) so normal prose and code survive.
static RE_SECRET_KV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^A-Za-z0-9])(passw(or)?d|passwd|pwd|api[-_]?key|secret|token)\b\s*[:=]\s*\S+")
        .unwrap()
});
static RE_OTP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4,8}$").unwrap());
static RE_PEM_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----").unwrap()
});

const CODE_TOKENS: &[&str] = &[
    "{", "}", "=>", "function ", "const ", "let ", "var ", "import ", "class ", "def ", "fn ",
    "return ", "public ", "private ", "#include", "SELECT ", "INSERT ",
];

/// True for markdown list items (`- foo`, `* foo`, `1. foo`, `2) foo`).
/// Chat/paste list indentation is prose structure, not code indentation —
/// counting it classified bulleted Discord pastes as code (seen live).
fn is_list_item(line: &str) -> bool {
    let t = line.trim_start();
    if t.starts_with("- ") || t.starts_with("* ") || t.starts_with("+ ") {
        return true;
    }
    let mut chars = t.chars();
    let mut digits = 0;
    for c in chars.by_ref() {
        if c.is_ascii_digit() {
            digits += 1;
        } else {
            break;
        }
    }
    if digits == 0 || digits > 9 {
        return false;
    }
    let rest: String = chars.collect();
    rest.starts_with(". ") || rest.starts_with(") ")
}

/// True when text looks like a credential worth protecting (PR3).
/// Links and emails are checked first: a password-reset URL is a link,
/// not a secret — swallowing it would be a false positive.
pub fn is_secret(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    if RE_LINK.is_match(trimmed) || RE_EMAIL.is_match(trimmed) {
        return false;
    }
    RE_SECRET_KV.is_match(s) || RE_OTP.is_match(trimmed) || RE_PEM_KEY.is_match(s)
}

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
    // List-item indentation is prose structure — only real indentation counts,
    // and even that needs a code token to convict (indentation alone used to
    // classify bulleted chat pastes as code).
    let looks_indented = line_count >= 3
        && trimmed
            .lines()
            .filter(|l| {
                (l.starts_with(' ') || l.starts_with('\t')) && !is_list_item(l)
            })
            .count()
            >= 2;
    if has_code_token
        && (looks_indented || line_count >= 2 || trimmed.contains(';'))
    {
        return "code";
    }
    "plain"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_key_value_secrets() {
        assert!(is_secret("password=hunter2"));
        assert!(is_secret("Password: hunter2"));
        assert!(is_secret("API_KEY=sk-live-abc123"));
        assert!(is_secret("api-key: xyz"));
        assert!(is_secret("token=eyJhbGciOiJIUzI1NiJ9"));
        assert!(is_secret("client_secret=abc123")); // bare keyword needs = or :
        assert!(is_secret("-----BEGIN RSA PRIVATE KEY-----\nMIIE..."));
    }

    #[test]
    fn detects_standalone_otp() {
        assert!(is_secret("482910"));
        assert!(is_secret("  1234  "));
    }

    #[test]
    fn rejects_lookalikes_and_non_secrets() {
        // Near-miss identifiers must not nuke normal clips.
        assert!(!is_secret("tokenize() the string"));
        assert!(!is_secret("my password is long and memorable"));
        assert!(!is_secret("order 482910 confirmed"));
        assert!(!is_secret("123"));
        assert!(!is_secret("123456789"));
        // Links/emails win over secret patterns — a reset URL is a link.
        assert!(!is_secret("https://example.com/reset?password=1"));
        assert!(!is_secret("const x = 1;"));
        assert!(!is_secret("hello world"));
    }

    #[test]
    fn categorizes_link() {
        assert_eq!(categorize("https://example.com"), "link");
        assert_eq!(categorize("  http://foo.bar/baz?q=1  "), "link");
    }

    #[test]
    fn bulleted_chat_pastes_are_plain_not_code() {
        // Live: 22-line Discord paste, 10 indented lines, zero code tokens.
        let chat = "hey guys @everyone this is a read only session\n\
            @researcher do a deep search and read hermes agent docs\n\
            1. Hermes skin system:\n               - E:\\hermes\\skins\\\n               - skin_engine.py\n            2. more items here\n               - nested note";
        assert_eq!(categorize(chat), "plain");
        // Indented prose without any token is plain too.
        assert_eq!(categorize("a thought\n  continued gently\n  and more"), "plain");
        // ...but real indented code still convicts via its tokens.
        assert_eq!(categorize("if x:\n    return 1\n    return 2"), "code");
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
