//! Rich-text / HTML clipboard (PR4).
//!
//! Windows puts several flavors on the clipboard at once. We used to read
//! only `CF_TEXT`; this module handles the `HTML Format` flavor:
//! parsing Microsoft's byte-offset header, sanitizing the fragment before
//! it ever reaches the webview, and building the flavor back on copy.

/// Extract the HTML fragment from a CF_HTML document.
/// Offsets are BYTE offsets from the document start (not chars — multibyte
/// UTF-8 would slice wrong otherwise). Returns None when malformed.
pub fn parse_cf_html(data: &[u8]) -> Option<String> {
    // Header lines look like `StartHTML:00000105`. The header ends at the
    // first line starting with `<` (the `<html>` tag).
    let mut start_html: Option<usize> = None;
    let mut end_html: Option<usize> = None;
    let mut start_fragment: Option<usize> = None;
    let mut end_fragment: Option<usize> = None;
    for line in data.split(|&b| b == b'\n') {
        let line = strip_cr(line);
        if line.first() == Some(&b'<') {
            break;
        }
        let Ok(text) = std::str::from_utf8(line) else {
            continue;
        };
        // Unknown keys (Version, SourceURL, ...) are skipped — only a
        // malformed OFFSET line is fatal to nothing; missing offsets fail
        // later when the slice is built.
        let Some((key, val)) = text.split_once(':') else {
            continue;
        };
        let num: usize = match key.trim() {
            "StartHTML" | "EndHTML" | "StartFragment" | "EndFragment" => {
                match val.trim().parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        match key.trim() {
            "StartHTML" => start_html = Some(num),
            "EndHTML" => end_html = Some(num),
            "StartFragment" => start_fragment = Some(num),
            "EndFragment" => end_fragment = Some(num),
            _ => {}
        }
    }
    // Prefer the fragment slice; fall back to the whole HTML document.
    let (start, end) = match (start_fragment, end_fragment) {
        (Some(s), Some(e)) => (s, e),
        _ => (start_html?, end_html?),
    };
    if start > end || end > data.len() {
        return None;
    }
    std::str::from_utf8(&data[start..end]).ok().map(str::to_string)
}

fn strip_cr(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// Build a full CF_HTML document around `fragment`, with correct byte
/// offsets. Round-trips through [`parse_cf_html`].
pub fn build_cf_html(fragment: &str) -> String {
    // Digit width is fixed at 8, so the header length never depends on the
    // values — offsets computed against placeholders stay exact.
    let header_len = "Version:0.9\r\n".len()
        + "StartHTML:".len() + 8 + "\r\n".len()
        + "EndHTML:".len() + 8 + "\r\n".len()
        + "StartFragment:".len() + 8 + "\r\n".len()
        + "EndFragment:".len() + 8 + "\r\n".len();
    let pre = "<html><body>\r\n<!--StartFragment-->";
    let post = "<!--EndFragment-->\r\n</body>\r\n</html>";
    let start_html = header_len;
    let start_fragment = header_len + pre.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + post.len();
    format!(
        "Version:0.9\r\nStartHTML:{start_html:08}\r\nEndHTML:{end_html:08}\r\n\
         StartFragment:{start_fragment:08}\r\nEndFragment:{end_fragment:08}\r\n\
         {pre}{fragment}{post}"
    )
}

/// Sanitize an HTML fragment for webview preview.
/// Trust boundary: clipboard HTML is untrusted input (any app can put
/// `<script>` on the clipboard). ammonia strips scripts and handlers;
/// `style` is allowed because modern Chromium cannot execute JS from CSS
/// (no expression(), no bindings) — color/bold survive, scripts don't.
pub fn sanitize_fragment(html: &str) -> String {
    ammonia::Builder::default()
        .add_tags(["h1", "h2", "h3", "span", "font", "pre"])
        .add_generic_attributes(["style"])
        .clean(html)
        .to_string()
}

/// Heuristic: does this sanitized HTML fragment look like syntax-highlighted
/// code copied from an IDE or docs site? VS Code / JetBrains / browsers wrap
/// code in `<pre>` or a monospace `font-family`; chat and word-processor rich
/// text has colors but almost never monospace. Pure so it can be unit-tested
/// against captured fragments.
pub fn looks_like_code_html(fragment: &str) -> bool {
    let lower = fragment.to_lowercase();
    if lower.contains("<pre") {
        return true;
    }
    const MONO_FONTS: [&str; 9] = [
        "monospace",
        "consolas",
        "courier",
        "menlo",
        "monaco",
        "cascadia",
        "fira code",
        "jetbrains mono",
        "source code pro",
    ];
    MONO_FONTS.iter().any(|f| lower.contains(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_doc() -> Vec<u8> {
        // Built by the builder under test; the hand-computed-offset case
        // below pins the wire format independently.
        build_cf_html("<b>Markets</b> rally \u{2713}").into_bytes()
    }

    #[test]
    fn parses_fragment_with_byte_offsets() {
        let frag = parse_cf_html(&sample_doc()).unwrap();
        assert_eq!(frag, "<b>Markets</b> rally \u{2713}");
    }

    #[test]
    fn falls_back_to_html_offsets_without_fragment_markers() {
        // Header is 51 bytes; <html> runs 51..79.
        let doc = b"Version:0.9\r\nStartHTML:00000051\r\nEndHTML:00000079\r\n<html><body>hi</body></html>";
        assert_eq!(parse_cf_html(doc).unwrap(), "<html><body>hi</body></html>");
    }

    #[test]
    fn rejects_malformed_documents() {
        assert!(parse_cf_html(b"not html at all").is_none());
        assert!(parse_cf_html(b"Version:0.9\r\nStartHTML:99999999\r\nEndHTML:99999999\r\nshort").is_none());
        // Offsets landing on non-UTF8 bytes are rejected, not sliced mid-char.
        // Header is 51 bytes, so \xff\xfe sit at 51..53.
        assert!(parse_cf_html(b"Version:0.9\r\nStartHTML:00000051\r\nEndHTML:00000053\r\n\xff\xfe bad utf8 here..........").is_none());
    }

    #[test]
    fn build_round_trips_through_parse() {
        let frag = "<h1 style=\"color:red\">H\u{00e9}llo <b>w\u{00f6}rld</b> \u{2713}</h1>";
        let doc = build_cf_html(frag);
        assert_eq!(parse_cf_html(doc.as_bytes()).unwrap(), frag);
    }

    #[test]
    fn sanitize_strips_scripts_but_keeps_formatting() {
        let dirty = "<b>Hi</b><script>alert(1)</script><a href=\"https://x.com\" onclick=\"evil()\">x</a><span style=\"color:red\">red</span>";
        let clean = sanitize_fragment(dirty);
        assert!(clean.contains("<b>Hi</b>"));
        assert!(!clean.contains("script"));
        assert!(!clean.contains("onclick"));
        assert!(clean.contains("https://x.com"));
        assert!(clean.contains("color:red"));
    }

    #[test]
    fn detects_ide_code_fragments() {
        let vscode = "<div style=\"color:#d4d4d4;background-color:#1e1e1e;font-family: Consolas, 'Courier New', monospace;white-space:pre;\"><span style=\"color:#569cd6\">const</span> x = 1;</div>";
        assert!(looks_like_code_html(vscode));
        assert!(looks_like_code_html("<pre><b>hi</b></pre>"));
        assert!(looks_like_code_html("<div style=\"font-family: JetBrains Mono\">x</div>"));
    }

    #[test]
    fn rejects_prose_rich_text_as_code() {
        assert!(!looks_like_code_html(
            "<span style=\"color:red\">hello world</span>"
        ));
        assert!(!looks_like_code_html("<b>Markets</b> rally"));
        assert!(!looks_like_code_html(
            "<ul><li style=\"font-family: Arial\">item</li></ul>"
        ));
    }
}
