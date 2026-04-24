//! Shared markdown helpers used by multiple skills.
//!
//! These were originally inlined in individual skill files; extracted here
//! once two-or-more skills started depending on the same logic. Pure
//! functions, no I/O, safe to call freely.

/// ATX-style markdown heading.
///
/// `level` is 1..=6. `text` is the heading body with the leading `#`s and
/// any trailing `#`s stripped, and surrounding whitespace trimmed.
pub type Heading = (u8, String);

/// Scan `body` for ATX headings (`#`, `##`, ...) and return them in
/// document order. Skips lines inside fenced code blocks (both ``` and
/// ~~~ fences). Setext headings (`Title\n====`) are not detected.
pub fn scan_headings(body: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut in_fence = false;
    let mut fence_marker: Option<&str> = None;

    for line in body.lines() {
        let trimmed = line.trim_start();

        if let Some(marker) = fence_marker {
            if trimmed.starts_with(marker) {
                in_fence = false;
                fence_marker = None;
            }
            continue;
        }
        if trimmed.starts_with("```") {
            in_fence = true;
            fence_marker = Some("```");
            continue;
        }
        if trimmed.starts_with("~~~") {
            in_fence = true;
            fence_marker = Some("~~~");
            continue;
        }

        if !in_fence {
            if let Some(h) = parse_atx_heading(trimmed) {
                out.push(h);
            }
        }
    }
    out
}

/// Parse a single line as an ATX heading. Returns `None` if the line is
/// not a heading. CommonMark requires a space after the `#`s; `#foo` is
/// not a heading.
pub fn parse_atx_heading(line: &str) -> Option<Heading> {
    let bytes = line.as_bytes();
    let mut level = 0u8;
    while level < 6 && bytes.get(level as usize) == Some(&b'#') {
        level += 1;
    }
    if level == 0 {
        return None;
    }
    let rest = &line[level as usize..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().trim_end_matches('#').trim().to_string()))
}

/// GitHub-style slug: lowercase, spaces / hyphens / underscores collapse
/// to a single `-`, everything else dropped except alphanumerics. Leading
/// and trailing hyphens trimmed.
pub fn slugify(text: &str) -> String {
    let mut buf = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() {
            buf.extend(c.to_lowercase());
        } else if c == ' ' || c == '-' || c == '_' {
            buf.push('-');
        }
    }
    let mut collapsed = String::with_capacity(buf.len());
    let mut prev_hyphen = false;
    for c in buf.chars() {
        if c == '-' {
            if !prev_hyphen {
                collapsed.push(c);
            }
            prev_hyphen = true;
        } else {
            collapsed.push(c);
            prev_hyphen = false;
        }
    }
    collapsed.trim_matches('-').to_string()
}

/// Filesystem-safe filename derived from a document title. Keeps
/// alphanumerics, dashes, dots, underscores, and spaces; replaces
/// everything else with `_`. Trims leading/trailing dots and whitespace.
/// Falls back to `"document"` if the result would be empty.
pub fn sanitize_filename(title: &str) -> String {
    let s: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '.' | '_' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim().trim_matches('.');
    if trimmed.is_empty() {
        "document".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Strip the heaviest markdown noise so word/syllable/token counts
/// reflect prose content rather than syntax. Drops fenced code blocks
/// entirely; drops inline code spans; keeps the visible text of
/// `[text](url)` links and discards the URL. Headings, emphasis, and
/// list markers are non-alphabetic so they don't perturb counts and are
/// left as-is.
pub fn strip_markdown_lite(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        out.push_str(&strip_inline(line));
        out.push('\n');
    }
    out
}

/// Strip inline-only markdown noise from a single line: inline code
/// spans (between backticks) and link target syntax (keep the visible
/// text, drop the URL). Used internally by `strip_markdown_lite`;
/// exposed for callers that handle their own line iteration.
pub fn strip_inline(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '`' => {
                for c2 in chars.by_ref() {
                    if c2 == '`' {
                        break;
                    }
                }
            }
            '[' => {
                let mut text = String::new();
                let mut closed = false;
                for c2 in chars.by_ref() {
                    if c2 == ']' {
                        closed = true;
                        break;
                    }
                    text.push(c2);
                }
                out.push_str(&text);
                if closed && chars.peek() == Some(&'(') {
                    chars.next();
                    for c2 in chars.by_ref() {
                        if c2 == ')' {
                            break;
                        }
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_headings_extracts_atx_headings_in_order() {
        let h = scan_headings("# A\nbody\n## B\n## C\n");
        assert_eq!(h, vec![(1, "A".into()), (2, "B".into()), (2, "C".into())]);
    }

    #[test]
    fn scan_headings_skips_inside_backtick_fences() {
        let h = scan_headings("# Real\n```\n# Fake\n```\n## Also Real\n");
        assert_eq!(h, vec![(1, "Real".into()), (2, "Also Real".into())]);
    }

    #[test]
    fn scan_headings_skips_inside_tilde_fences() {
        let h = scan_headings("# Real\n~~~\n# Fake\n~~~\n## Also Real\n");
        assert_eq!(h, vec![(1, "Real".into()), (2, "Also Real".into())]);
    }

    #[test]
    fn parse_atx_strips_trailing_hashes() {
        assert_eq!(parse_atx_heading("## Section ##"), Some((2, "Section".into())));
    }

    #[test]
    fn parse_atx_rejects_no_space() {
        assert_eq!(parse_atx_heading("#foo"), None);
    }

    #[test]
    fn slugify_lowers_and_collapses_separators() {
        assert_eq!(slugify("Hello, World!"), "hello-world");
        assert_eq!(slugify("snake_case_thing"), "snake-case-thing");
        assert_eq!(slugify("Section 1.2"), "section-12");
    }

    #[test]
    fn sanitize_filename_keeps_safe_chars() {
        assert_eq!(sanitize_filename("foo/bar.txt"), "foo_bar.txt");
        assert_eq!(sanitize_filename("a:b?c*"), "a_b_c_");
        assert_eq!(sanitize_filename(""), "document");
        assert_eq!(sanitize_filename("..."), "document");
    }

    #[test]
    fn strip_markdown_drops_fences_and_inline_code() {
        let out = strip_markdown_lite("text\n```\ncode\n```\nmore `inline` text\n");
        assert!(!out.contains("code"));
        assert!(!out.contains("inline"));
        assert!(out.contains("text"));
        assert!(out.contains("more"));
    }

    #[test]
    fn strip_inline_keeps_link_text_drops_url() {
        let out = strip_inline("See [the docs](https://example.com) please.");
        assert_eq!(out, "See the docs please.");
    }
}
