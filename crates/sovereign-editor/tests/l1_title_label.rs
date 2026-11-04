//! v0.0.12 item 8, L1 (`doc/plans/V0012-item8-clipboard-labels.md`): "The
//! editor's accessible label is the title as it is drawn: a personal-data
//! token shows as its label, or as its value only while revealed, as in the
//! title's own node. Never the token's stored form `[pii:…]`."
//!
//! Written before the code, by another author: whoever builds item 8 makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around. Uses only what exists today.
//!
//! The editor's label is the accessible name of its text node (the body's
//! `MultilineTextInput`, or the read-only body). The title's own node carries
//! the title as shown (its value, or its label when read-only, T7): the
//! editor's label must say the same.

mod common;
use common::*;

use sovereign_editor::input::InputEvent;
use sovereign_editor::{Editor, OpenDocument, OpenMode, Trust};

const LABEL: &str = "\u{2039}email\u{203A}";
const VALUE: &str = "alice@example.com";
const BODY: &str = "Body text here";

fn titled(title: &str, mode: OpenMode, revealed: bool) -> (Editor, Fakes) {
    let (host, f) = host();
    add_pii(&f, "3", LABEL, VALUE, revealed);
    let doc = OpenDocument { title: title.into(), content: doc1(BODY), trust: Trust::Owned, mode };
    let mut ed = Editor::open(doc, config(), host).expect("open");
    ed.set_viewport(2000.0, 1000.0, 1.0);
    ed.handle_event(InputEvent::Focus(true));
    ed.pii_display_changed();
    (ed, f)
}

/// The editor's label: the name of its text node (the body).
fn editor_label(ed: &mut Editor) -> Option<String> {
    let update = ed.accessibility_tree();
    update
        .nodes
        .iter()
        .find(|(_, n)| n.role() == accesskit::Role::MultilineTextInput)
        .and_then(|(_, n)| n.label().map(str::to_string))
}

fn no_raw_token(ed: &mut Editor) {
    let update = ed.accessibility_tree();
    let raw: Vec<String> = all_exposed_strings(&update).into_iter().filter(|s| s.contains("[pii:")).collect();
    assert!(raw.is_empty(), "the editor's tree exposes a raw token: {raw:?}");
}

// L1: a title holding a token is labelled with the token's label — the
// editable title (the body's name), and nothing in the tree is `[pii:`.
#[test]
fn l1_a_titles_token_is_labelled_with_its_label() {
    let title = format!("Mail {} today", token("3"));
    let (mut ed, _f) = titled(&title, OpenMode::Edit, false);
    let label = editor_label(&mut ed).expect("the editor is labelled with its title");
    assert_eq!(label, format!("Mail {LABEL} today"), "the title as shown");
    assert!(!label.contains(VALUE), "an unrevealed value is never in the label");
    no_raw_token(&mut ed);
}

// L1: "or as its value only while revealed, as in the title's own node".
#[test]
fn l1_a_revealed_token_is_labelled_with_its_value_only_while_revealed() {
    let title = format!("Mail {} today", token("3"));
    let (mut ed, f) = titled(&title, OpenMode::Edit, true);
    assert_eq!(editor_label(&mut ed).as_deref(), Some(format!("Mail {VALUE} today").as_str()), "revealed: the value");
    no_raw_token(&mut ed);
    reveal(&f, "3", false);
    ed.pii_display_changed();
    assert_eq!(editor_label(&mut ed).as_deref(), Some(format!("Mail {LABEL} today").as_str()), "hidden again: the label");
}

// L1: a read-only document's editor too (T7: its title is text, a heading).
#[test]
fn l1_a_read_only_editors_label_shows_the_tokens_label() {
    let title = format!("Contract {}", token("3"));
    let (mut ed, _f) = titled(&title, OpenMode::ReadOnly, false);
    no_raw_token(&mut ed);
    assert_eq!(editor_label(&mut ed).as_deref(), Some(format!("Contract {LABEL}").as_str()), "the title as shown");
}

// L1: the label is the title *as shown* — token-form text that is not a
// token in this document is shown as typed, so the label says it as typed,
// as the title's own node does.
#[test]
fn l1_token_form_that_is_not_a_token_here_is_labelled_as_typed() {
    let title = format!("Note {}", token("9"));
    let (mut ed, _f) = titled(&title, OpenMode::Edit, false);
    assert_eq!(editor_label(&mut ed).as_deref(), Some(title.as_str()));
}
