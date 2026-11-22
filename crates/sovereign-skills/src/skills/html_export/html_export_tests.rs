//! HTML Export escapes the document's text (spec ch. 11; v0.0.13, the review's
//! item 23, and the link addresses decided by the Fable maille, by delegation
//! of Céline, 2026-11-18).
//!
//! A synced or imported document can hold raw HTML or a link that runs a
//! script. The exported page shows what the document holds as text; it never
//! runs it. Markdown's own formatting is still rendered.

use super::*;
use crate::test_util::{dummy_ctx, make_doc_with_title};

/// The exported page for a document titled `title` whose body is `body`.
fn export(title: &str, body: &str) -> String {
    let doc = make_doc_with_title(title, body);
    match HtmlExportSkill.execute("export", &doc, "", &dummy_ctx()).expect("the export runs") {
        SkillOutput::File { data, .. } => String::from_utf8(data).expect("the page is UTF-8"),
        other => panic!("the export gives a file, not {other:?}"),
    }
}

/// The page's markup, lowercased, for a search that a tag's case cannot hide from.
fn lower(page: &str) -> String {
    page.to_ascii_lowercase()
}

/// A block of raw HTML is exported as text, shown as written.
#[test]
fn i23_a_block_of_raw_html_is_text() {
    let page = export("Notes", "Before\n\n<script>alert(1)</script>\n\nAfter");
    assert!(!lower(&page).contains("<script"), "no script element in the page:\n{page}");
    assert!(page.contains("&lt;script&gt;alert(1)&lt;/script&gt;"), "the raw HTML is shown as written:\n{page}");
    assert!(page.contains("After"), "the text after it is kept");
}

/// Raw HTML inline in a paragraph is exported as text, shown as written.
#[test]
fn i23_inline_raw_html_is_text() {
    let page = export("Notes", "Hello <img src=x onerror=alert(1)> world");
    assert!(!lower(&page).contains("<img src=x"), "no img element from the raw HTML, so its handler is no attribute:\n{page}");
    assert!(page.contains("&lt;img src=x onerror=alert(1)&gt;"), "the raw HTML is shown as written:\n{page}");
    assert!(page.contains("Hello") && page.contains("world"));
}

/// An iframe, and a tag whose case is mixed, are text too.
#[test]
fn i23_an_iframe_and_a_mixed_case_tag_are_text() {
    let page = export("Notes", "<iframe src=\"https://example.org\"></iframe>\n\nText <ScRiPt>x</ScRiPt> here");
    let l = lower(&page);
    assert!(!l.contains("<iframe"), "no iframe element:\n{page}");
    assert!(!l.contains("<script"), "no script element, whatever its case:\n{page}");
    assert!(page.contains("&lt;iframe"), "the iframe is shown as written:\n{page}");
}

/// The title is escaped in `<title>` and where the page shows it.
#[test]
fn i23_the_title_is_escaped_wherever_it_is_shown() {
    let page = export("<script>alert(1)</script> plan", "Body");
    assert!(!lower(&page).contains("<script"), "no script element from the title:\n{page}");
    assert!(page.contains("<title>&lt;script&gt;alert(1)&lt;/script&gt; plan</title>"), "in <title>:\n{page}");
    assert!(page.contains("<h1>&lt;script&gt;alert(1)&lt;/script&gt; plan</h1>"), "where the page shows it:\n{page}");
}

/// Markdown's own formatting is still rendered: headings, lists, tables,
/// emphasis, links, code, and images as they are.
#[test]
fn i23_markdown_formatting_is_still_rendered() {
    let body = "## Heading\n\n- one\n- two\n\n| a | b |\n|---|---|\n| 1 | 2 |\n\n*em* **strong** \
                [site](https://example.org) `code`\n\n![a picture](https://example.org/a.png)";
    let page = export("Notes", body);
    for part in [
        "<h2>Heading</h2>",
        "<li>one</li>",
        "<table>",
        "<em>em</em>",
        "<strong>strong</strong>",
        "<a href=\"https://example.org\">site</a>",
        "<code>code</code>",
        "<img src=\"https://example.org/a.png\" alt=\"a picture\"",
    ] {
        assert!(page.contains(part), "{part} is rendered:\n{page}");
    }
}

/// A link whose address is `http`, `https`, `mailto`, relative or a fragment
/// stays a link.
#[test]
fn i23_safe_link_addresses_stay_links() {
    for (addr, shown) in [
        ("http://example.org", "http://example.org"),
        ("https://example.org/a?b=c", "https://example.org/a?b=c"),
        ("mailto:ana@example.org", "mailto:ana@example.org"),
        ("notes/plan.html", "notes/plan.html"),
        ("../plan.html", "../plan.html"),
        ("#top", "#top"),
    ] {
        let page = export("Notes", &format!("[go]({addr})"));
        assert!(page.contains(&format!("<a href=\"{shown}\">go</a>")), "{addr} stays a link:\n{page}");
    }
}

/// A link whose address is anything else keeps its text, and its address is
/// shown as text, not as a link: a `javascript:` link would run a script that
/// can read the page and send it out.
#[test]
fn i23_other_link_addresses_are_shown_as_text() {
    for addr in [
        "javascript:alert(1)",
        "JaVaScRiPt:alert(1)",
        "data:text/html,hello",
        "vbscript:msgbox(1)",
        "file:///C:/Windows/notepad.exe",
    ] {
        let page = export("Notes", &format!("Read [this]({addr}) now"));
        let l = lower(&page);
        let scheme = addr.split(':').next().unwrap().to_ascii_lowercase();
        assert!(!l.contains(&format!("href=\"{scheme}:")), "{addr} is never a link's address:\n{page}");
        assert!(page.contains("this"), "the link's text is kept ({addr}):\n{page}");
        assert!(page.contains(addr), "its address is shown as text ({addr}):\n{page}");
    }
}

/// An autolink with such an address is shown as text too.
#[test]
fn i23_an_autolink_with_another_address_is_text() {
    let page = export("Notes", "See <javascript:alert(1)> here");
    assert!(!lower(&page).contains("href=\"javascript:"), "never a link:\n{page}");
    assert!(page.contains("javascript:alert(1)"), "shown as text:\n{page}");
}
