use regex::Regex;
use sha2::{Digest, Sha256};
use std::sync::LazyLock;

static RE_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^https?://\S+$").unwrap());
static RE_EMAIL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[^\s@]+@[^\s@]+\.[^\s@]+$").unwrap());
static RE_HEX_COLOR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9a-fA-F]{3}|[0-9a-fA-F]{6}|[0-9a-fA-F]{8})$").unwrap());
// Anchored: an UNanchored copy classified ANY text containing `rgb(` as a
// color — one `rgb(255, 99, 71)` comment in a 600-line code paste sent the
// whole clip to the Colors tab (seen live).
static RE_RGB: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^rgba?\(\s*\d+\s*,\s*\d+\s*,\s*\d+\s*(,\s*[\d.]+\s*)?\)$").unwrap()
});
// Secrets (PR3): key=value credential shapes, standalone OTP digits, PEM
// private-key headers, plus high-confidence bare tokens (JWTs, provider
// prefixes). The keyword must stand alone or be _-separated (client_secret
// matches, mytoken doesn't) and must be followed by = or : ("my password is
// long" doesn't) so normal prose and code survive. RE_SECRET_KV is only
// trusted for SINGLE-LINE clips — for multi-line text a credential line must
// START with the keyword (see `credential_line`), otherwise any code paste
// containing `token:` mid-expression was swallowed whole and auto-deleted.
static RE_SECRET_KV: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^A-Za-z0-9])(passw(or)?d|passwd|pwd|api[-_]?key|secret|token)\b\s*[:=]\s*\S+")
        .unwrap()
});
static RE_OTP: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4,8}$").unwrap());
static RE_PEM_KEY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-----BEGIN [A-Z0-9 ]*PRIVATE KEY-----").unwrap()
});
// Bare tokens (no keyword in sight). Each is provider-specific or
// structurally unmistakable; a generic "long random string" rule is
// deliberately NOT included — it would swallow git SHAs and file hashes,
// and a false positive here self-destructs via the secret TTL.
static RE_JWT: LazyLock<Regex> = LazyLock::new(|| {
    // `eyJ` is base64 of `{"` — every JWT header starts with it.
    Regex::new(r"^eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]*$").unwrap()
});
static RE_BEARER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^bearer\s+\S{15,}$").unwrap());
static RE_PROVIDER_TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(",
        r"sk-[A-Za-z0-9_-]{20,}",            // OpenAI
        r"|ghp_[A-Za-z0-9]{20,}",            // GitHub classic PAT
        r"|gho_[A-Za-z0-9]{20,}",            // GitHub OAuth token
        r"|github_pat_[A-Za-z0-9_]{20,}",    // GitHub fine-grained PAT
        r"|xox[abprs]-[A-Za-z0-9-]{10,}",    // Slack
        r"|glpat-[A-Za-z0-9_-]{15,}",        // GitLab
        r"|AIza[A-Za-z0-9_-]{30,}",          // Google API key
        r"|npm_[A-Za-z0-9]{30,}",            // npm
        r"|AKIA[A-Z0-9]{16}",                // AWS access key id
        r")$"
    ))
    .unwrap()
});

/// Keywords that introduce a credential on their own line. Longest-prefix
/// forms first where they share a prefix (`password` before nothing else
/// overlaps; each entry is distinct).
const SECRET_KEYWORDS: &[&str] = &[
    "password", "passwd", "pwd", "api_key", "api-key", "apikey", "secret", "token",
];

/// True when `line` STARTS with a credential keyword whose value doesn't
/// look like code. `password: hunter2` convicts; `password = get_password()`
/// and `token: config.token` are code and must not nuke a whole paste.
fn credential_line(line: &str) -> bool {
    let lower = line.trim_start().to_lowercase();
    for kw in SECRET_KEYWORDS {
        let Some(mut rest) = lower.strip_prefix(kw) else {
            continue;
        };
        // Word boundary: the keyword must be followed by the separator,
        // optionally spaced — `tokenize = x` is an identifier, not a secret.
        if !rest.starts_with(':') && !rest.starts_with('=') {
            let spaced = rest.trim_start_matches([' ', '\t']);
            if !spaced.starts_with(':') && !spaced.starts_with('=') {
                continue;
            }
            rest = spaced;
        }
        let value = rest[1..].trim().trim_end_matches([',', ';']);
        if value.is_empty() || value.contains('(') {
            return false;
        }
        // Identifier-shaped value with no digit (`config.token`,
        // `process.env.PW`) is a code reference, not a credential. Real
        // secrets virtually always carry a digit, symbol, or quoting.
        let identifier_shaped = value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
        let no_digit = !value.chars().any(|c| c.is_ascii_digit());
        return !(identifier_shaped && no_digit);
    }
    false
}

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
/// Multi-line clips only convict via a line that STARTS with the keyword
/// (`credential_line`) — the loose KV regex is trusted on single lines only.
pub fn is_secret(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    if RE_LINK.is_match(trimmed) || RE_EMAIL.is_match(trimmed) {
        return false;
    }
    if RE_OTP.is_match(trimmed) || RE_PEM_KEY.is_match(s) {
        return true;
    }
    if !trimmed.contains('\n') {
        return RE_SECRET_KV.is_match(s)
            || RE_JWT.is_match(trimmed)
            || RE_BEARER.is_match(trimmed)
            || RE_PROVIDER_TOKEN.is_match(trimmed);
    }
    s.lines().any(credential_line)
}

/// True for a markdown fenced code block (` ``` ` on its own line, any
/// indentation). Rescues JSON/YAML/TOML/config pastes that carry no
/// CODE_TOKENS from landing in Plain. Inline `backticks` don't count.
fn has_code_fence(s: &str) -> bool {
    s.lines().any(|l| l.trim_start().starts_with("```"))
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
    let line_count = trimmed.lines().count();
    // A color value is a single token; multi-line text that merely mentions
    // one is prose/code, not a color.
    if line_count == 1 && (RE_HEX_COLOR.is_match(trimmed) || RE_RGB.is_match(trimmed)) {
        return "color";
    }
    let has_code_token = CODE_TOKENS.iter().any(|t| trimmed.contains(t));
    // List-item indentation is prose structure — only real indentation counts,
    // and even that needs a code token to convict (indentation alone used to
    // classify bulleted chat pastes as code). A ``` fence convicts on its own.
    let looks_indented = line_count >= 3
        && trimmed
            .lines()
            .filter(|l| {
                (l.starts_with(' ') || l.starts_with('\t')) && !is_list_item(l)
            })
            .count()
            >= 2;
    if (has_code_token && (looks_indented || line_count >= 2 || trimmed.contains(';')))
        || has_code_fence(trimmed)
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
    fn multiline_paste_with_mid_expression_kv_is_not_a_secret() {
        // Live bug: a code/config paste containing `token:` anywhere was
        // swallowed whole and auto-deleted by the secret TTL.
        let code = "const config = {\n  token: config.token,\n  password: process.env.PW,\n};";
        assert!(!is_secret(code));
        let py = "def login():\n    password = get_password()\n    return password";
        assert!(!is_secret(py));
    }

    #[test]
    fn multiline_paste_with_dedicated_credential_line_is_a_secret() {
        assert!(is_secret("here are the creds:\npassword: hunter2\ntoken: abc123xyz"));
        assert!(is_secret("settings:\n  api_key = sk-live-abc123"));
        // Punctuation after the value must not defeat the check.
        assert!(is_secret("token: abc123xyz,\nhost: example.com"));
    }

    #[test]
    fn detects_bare_provider_tokens_and_jwts() {
        assert!(is_secret("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJVadQssw5c"));
        assert!(is_secret("sk-proj-abcdefghijklmnopqrstuvwxyz123456"));
        assert!(is_secret("ghp_abcdefghijklmnopqrstuvwxyz1234567890"));
        assert!(is_secret("github_pat_abcdefghijklmnopqrstuvwxyz12"));
        // Shape-only fixture (no digit segments) — a realistic-looking Slack
        // token trips GitHub push protection even though it's fake.
        assert!(is_secret("xoxb-example-token-abcdefghijklmnop"));
        assert!(is_secret("glpat-abcdefghijklmnopqrstuvwxyz"));
        assert!(is_secret("AIzaSyABCDEFGHIJKLMNOPQRSTUVWXYZ1234567"));
        assert!(is_secret("AKIAIOSFODNN7EXAMPLE"));
        assert!(is_secret("Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.abc"));
    }

    #[test]
    fn rejects_hashes_and_prose_lookalikes_as_bare_tokens() {
        // Git SHAs and file hashes are constantly copied — a generic
        // "long random string" rule would silently destroy them.
        assert!(!is_secret("a94a8fe5ccb19ba61c4c0873d391e987982fbbd3"));
        assert!(!is_secret("the bearer of bad news arrived"));
        assert!(!is_secret("sk- short")); // too short after the prefix
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
        assert_eq!(categorize("rgba(12, 34, 56, 0.5)"), "color");
    }

    #[test]
    fn multiline_text_mentioning_colors_is_not_a_color() {
        // Live bug: one rgb() anywhere sent 600-line code to the Colors tab.
        let code = "function render() {\n  const c = theme.primary; // rgb(255, 99, 71)\n  return c;\n}";
        assert_eq!(categorize(code), "code");
        assert_eq!(categorize("#ff00ff is purple\n#00ff00 is green"), "plain");
    }

    #[test]
    fn fenced_blocks_are_code_even_without_code_tokens() {
        let yaml = "```\nhost: db.example.com\nport: 5432\nuser: admin\n```";
        assert_eq!(categorize(yaml), "code");
        let indented_fence = "text before\n  ```toml\n  key = \"value\"\n  ```";
        assert_eq!(categorize(indented_fence), "code");
        // Inline single backticks are prose, not a fenced block.
        assert_eq!(categorize("use `const` instead of `var` here"), "plain");
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
