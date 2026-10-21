//! Guarantee 6 — "Input methods work: composition text is shown in place,
//! committed once, cancelled cleanly; the caret area is reported to the host
//! for the candidate window." (spec ch. 12, C-TEXT)

mod common;
use common::*;

use sovereign_editor::input::{Command, ImeEvent, InputEvent, NamedKey};

fn composing(ed: &mut sovereign_editor::Editor, steps: &[&str]) {
    ime(ed, ImeEvent::Enabled);
    for s in steps {
        preedit(ed, s);
    }
}

// "committed once"
#[test]
fn g6_commit_inserts_exactly_once_at_the_caret() {
    let (mut ed, _f) = open_focused(&doc1("[]"));
    caret_at(&mut ed, 1, 1);
    composing(&mut ed, &["k", "か", "かん", "漢"]);
    ime(&mut ed, ImeEvent::Preedit { text: String::new(), cursor: None });
    ime(&mut ed, ImeEvent::Commit("漢".into()));
    ime(&mut ed, ImeEvent::Disabled);
    assert_eq!(text_of(&ed, 1), "[漢]");
    assert_eq!(caret(&ed).offset, "[漢".len(), "caret after the committed text");
}

// "committed once" — winit order without the empty pre-edit before the commit.
#[test]
fn g6_commit_while_preedit_is_showing_replaces_it() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    composing(&mut ed, &["にほん", "日本"]);
    ime(&mut ed, ImeEvent::Commit("日本".into()));
    assert_eq!(text_of(&ed, 1), "日本", "neither pre-edit left behind nor doubled");
}

// "committed once" — two compositions in a row, each committed once.
#[test]
fn g6_consecutive_compositions() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    for (pre, commit) in [("한", "한"), ("구", "국"), ("어", "어")] {
        preedit(&mut ed, pre);
        ime(&mut ed, ImeEvent::Commit(commit.into()));
    }
    assert_eq!(text_of(&ed, 1), "한국어");
}

// "cancelled cleanly"
#[test]
fn g6_cancel_leaves_the_model_exactly_as_before() {
    let (mut ed, _f) = open_focused(&doc1("keep this"));
    caret_at(&mut ed, 1, 4);
    let before = model_json(&ed);
    composing(&mut ed, &["ㅎ", "하", "한"]);
    ime(&mut ed, ImeEvent::Preedit { text: String::new(), cursor: None });
    ime(&mut ed, ImeEvent::Disabled);
    assert_eq!(model_json(&ed), before);
    assert_eq!(caret(&ed).offset, 4);
    type_text(&mut ed, "!");
    assert_eq!(text_of(&ed, 1), "keep! this", "typing works after a cancel");
}

// "cancelled cleanly" — Escape during composition is the IME's to handle; the
// model ends up as before or with the composition committed once, never with
// a fragment.
#[test]
fn g6_escape_or_disable_mid_composition_leaves_no_fragment() {
    let (mut ed, _f) = open_focused(&doc1("ab"));
    caret_at(&mut ed, 1, 1);
    composing(&mut ed, &["ni", "に"]);
    press(&mut ed, NamedKey::Escape);
    ime(&mut ed, ImeEvent::Disabled);
    let t = text_of(&ed, 1);
    assert!(t == "ab" || t == "aにb", "{t:?}");
    assert!(!t.contains("ni"));
}

// The model holds committed text only: while composing, the in-memory model
// (what a save would write) does not contain the pre-edit.
#[test]
fn g6_preedit_is_not_in_the_model_and_not_saved() {
    let (mut ed, f) = open_focused(&doc1("x"));
    caret_at(&mut ed, 1, 1);
    let before = model_json(&ed);
    composing(&mut ed, &["UNCOMMITTED"]);
    assert_eq!(model_json(&ed), before, "pre-edit is not committed text");
    ctrl(&mut ed, "s");
    cmd(&mut ed, Command::Save);
    advance(&f, std::time::Duration::from_secs(120));
    ed.poll();
    for r in f.saves.borrow().iter() {
        assert!(!r.content.contains("UNCOMMITTED"), "a save wrote uncommitted pre-edit");
    }
}

// "composition text is shown in place" — the pre-edit is laid out at the
// caret: with the pre-edit cursor at its end, the caret is drawn further right
// than before composing.
#[test]
fn g6_preedit_is_shown_in_place() {
    let (mut ed, _f) = open_focused(&doc1("ab"));
    caret_at(&mut ed, 1, 1);
    let before = ed.caret_rect().expect("caret").x0;
    composing(&mut ed, &["wwwwww"]);
    let during = ed.caret_rect().expect("caret").x0;
    assert!(during > before, "pre-edit takes space at the caret ({before} -> {during})");
    ime(&mut ed, ImeEvent::Preedit { text: String::new(), cursor: None });
    let after = ed.caret_rect().expect("caret").x0;
    assert!((after - before).abs() < 0.5, "cancel restores the caret position");
}

// "the caret area is reported to the host for the candidate window"
#[test]
fn g6_caret_area_is_reported_and_follows_the_caret() {
    let (mut ed, f) = open_focused(&doc1("wwwwwwwwwwwwwwwwwwww"));
    caret_at(&mut ed, 1, 0);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "か");
    paint(&mut ed);
    let first = *f.ime_areas.borrow().last().expect("a caret area is reported while composing");
    ime(&mut ed, ImeEvent::Commit("か".into()));
    let end = text_of(&ed, 1).len();
    caret_at(&mut ed, 1, end);
    preedit(&mut ed, "か");
    paint(&mut ed);
    let second = *f.ime_areas.borrow().last().unwrap();
    for a in [first, second] {
        assert!(a.x0.is_finite() && a.y0.is_finite() && a.x1 >= a.x0 && a.y1 >= a.y0, "{a:?}");
    }
    assert!(second.x0 > first.x0, "the reported area follows the caret ({first:?} -> {second:?})");
}

// Focus loss mid-composition: the composition is either committed once or
// dropped, never duplicated or left as a fragment; editing works afterwards.
#[test]
fn g6_focus_loss_mid_composition() {
    let (mut ed, _f) = open_focused(&doc1("ab"));
    caret_at(&mut ed, 1, 1);
    composing(&mut ed, &["に", "日"]);
    ed.handle_event(InputEvent::Focus(false));
    ime(&mut ed, ImeEvent::Disabled);
    let t = text_of(&ed, 1);
    assert!(t == "ab" || t == "a日b", "{t:?}");
    ed.handle_event(InputEvent::Focus(true));
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "x");
    ime(&mut ed, ImeEvent::Commit("x".into()));
    assert_eq!(text_of(&ed, 1).matches('x').count(), 1);
    assert_eq!(text_of(&ed, 1).matches('日').count(), t.matches('日').count());
}

// A commit with no pre-edit before it (dictation, emoji pickers) is inserted
// once.
#[test]
fn g6_commit_without_preedit() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    ime(&mut ed, ImeEvent::Commit("🙂 dictated".into()));
    assert_eq!(text_of(&ed, 1), "🙂 dictated");
}

// Undo after a composition: the commit is undone as entered text; the
// pre-edit steps leave nothing to undo.
#[test]
fn g6_undo_after_commit_restores_the_previous_model() {
    let (mut ed, _f) = open_focused(&doc1("base"));
    caret_at(&mut ed, 1, 4);
    let before = model_json(&ed);
    composing(&mut ed, &["に", "にほ", "日本"]);
    ime(&mut ed, ImeEvent::Commit("日本".into()));
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), before);
}

// Composing in a read-only document changes nothing.
#[test]
fn g6_composition_in_read_only_document_changes_nothing() {
    let (mut ed, f) = open_with(&doc1("ro"), sovereign_editor::OpenMode::ReadOnly);
    ed.handle_event(InputEvent::Focus(true));
    let before = model_json(&ed);
    composing(&mut ed, &["に"]);
    ime(&mut ed, ImeEvent::Commit("日".into()));
    assert_eq!(model_json(&ed), before);
    assert_eq!(save_count(&f), 0);
}
