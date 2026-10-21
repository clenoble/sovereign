//! The decided behaviours of C-TEXT (spec ch. 12, *Behaviours (decided by
//! Céline, 2026-10-10)*), at the level a component can test.

mod common;
use common::*;

use std::time::Duration;

use sovereign_editor::host::ClipboardPrivacy;
use sovereign_editor::input::{Command, NamedKey};
use sovereign_editor::{NoticeKind, OpenMode, ReadOnlyReason, SaveStatus};

const ALL_EXCLUDED: ClipboardPrivacy =
    ClipboardPrivacy { exclude_from_history: true, exclude_from_cloud_sync: true, exclude_from_monitoring: true };

// ------------------------------------------------------------ one editing device

// "[Switch editing to this device] (the other device's editor turns read-only,
// after saving what it has)"
#[test]
fn b_device_switch_saves_what_it_has_then_turns_read_only() {
    let (mut ed, f) = open_focused(&doc1("shared "));
    caret_at(&mut ed, 1, 7);
    type_text(&mut ed, "unsaved here");
    let before = save_count(&f);
    ed.editing_moved_to("phone");
    assert_eq!(save_count(&f), before + 1, "it saves what it has");
    let req = last_save(&f);
    assert_eq!(texts_in(&req.content), vec!["shared unsaved here"]);
    finish(&mut ed, req.id, true);
    assert!(ed.is_read_only());
    assert_eq!(ed.read_only_reason(), Some(ReadOnlyReason::EditingElsewhere { device: "phone".into() }));
    let frozen = model_json(&ed);
    type_text(&mut ed, "after");
    press(&mut ed, NamedKey::Backspace);
    ctrl(&mut ed, "v");
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "s");
    advance(&f, Duration::from_secs(3600));
    ed.poll();
    assert_eq!(model_json(&ed), frozen, "read-only after the switch");
    assert_eq!(save_count(&f), before + 1, "no save after the switch");
}

// Decided 2026-10-10: on the device that lost editing, input "produces a
// visible notice (… 'this document is being edited on <device>') — not
// silently dropped", and the model is unchanged.
#[test]
fn b_input_after_the_switch_gives_an_editing_elsewhere_notice() {
    let (mut ed, f) = open_focused(&doc1("shared"));
    caret_at(&mut ed, 1, 6);
    type_text(&mut ed, "!");
    ed.editing_moved_to("phone");
    let req = last_save(&f);
    finish(&mut ed, req.id, true);
    set_clip_text(&f, "pasted");
    let frozen = model_json(&ed);
    for (what, attempt) in editing_attempts() {
        ed.take_notices();
        attempt(&mut ed);
        let notices = ed.take_notices();
        let n = notices
            .iter()
            .find(|n| n.kind == NoticeKind::EditingElsewhere)
            .unwrap_or_else(|| panic!("{what} after the switch: no notice ({notices:?})"));
        assert!(n.message.contains("phone"), "the notice names the device: {:?}", n.message);
        assert_eq!(model_json(&ed), frozen, "{what} changed the model");
    }
}

// Same, with nothing unsaved: it turns read-only (a save is allowed but must be
// the unchanged model).
#[test]
fn b_device_switch_with_nothing_unsaved() {
    let content = doc1("clean");
    let (mut ed, f) = open_focused(&content);
    ed.editing_moved_to("laptop");
    if let Some(r) = f.saves.borrow().last().cloned() {
        assert_eq!(r.content, content);
        finish(&mut ed, r.id, true);
    }
    assert!(ed.is_read_only());
}

// "after saving what it has" + guarantee 1 — if that save fails, the text is
// kept and the failure is said.
#[test]
fn b_device_switch_with_failed_save_keeps_the_text_and_says_so() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "must not vanish");
    ed.editing_moved_to("phone");
    let req = last_save(&f);
    finish(&mut ed, req.id, false);
    assert_eq!(text_of(&ed, 1), "must not vanish");
    assert!(matches!(ed.save_status(), SaveStatus::Failed { .. }));
    assert!(ed.has_unsaved_changes());
}

// "[Read only]" on the second device — the editor opened read-only never
// changes or saves the document.
#[test]
fn b_opened_read_only_never_changes_or_saves() {
    let content = v2(&[text_block(1, "theirs"), unknown_block(2)]);
    let (mut ed, f) = open_with(&content, OpenMode::ReadOnly);
    assert_eq!(ed.read_only_reason(), Some(ReadOnlyReason::OpenedReadOnly));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "mine");
    ctrl(&mut ed, "x");
    ctrl(&mut ed, "v");
    cmd(&mut ed, Command::RemoveBlock(bid(2)));
    ctrl(&mut ed, "s");
    advance(&f, Duration::from_secs(3600));
    ed.poll();
    assert_eq!(model_json(&ed), content);
    assert_eq!(save_count(&f), 0);
}

// ------------------------------------------------------------ clipboard

// "Sovereign marks every copy as excluded from clipboard history, from cloud
// clipboard sync and from clipboard monitoring"
#[test]
fn b_every_clipboard_write_is_marked_excluded() {
    let (mut ed, f) = open_focused(&doc1("copy me [pii:pii_record:3] and cut me"));
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    cmd(&mut ed, Command::Copy);
    caret_at(&mut ed, 1, 0);
    key(&mut ed, NamedKey::End, sovereign_editor::input::Modifiers::SHIFT);
    ctrl(&mut ed, "x");
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "a");
    cmd(&mut ed, Command::Cut);
    let writes = f.clipboard.borrow().writes.clone();
    assert!(writes.len() >= 4, "each copy and cut wrote the clipboard ({} writes)", writes.len());
    for w in writes {
        assert_eq!(w.privacy, ALL_EXCLUDED);
        assert_eq!(w.clear_after, None, "ordinary text and unrevealed labels are never cleared");
    }
}

// "A new copy replaces the previous one entirely — the clipboard holds one
// item." — each copy writes the whole selection, as one item.
#[test]
fn b_each_copy_writes_the_whole_selection_once() {
    let (mut ed, f) = open_focused(&doc1("first second"));
    caret_at(&mut ed, 1, 0);
    for _ in 0..5 {
        key(&mut ed, NamedKey::ArrowRight, sovereign_editor::input::Modifiers::SHIFT);
    }
    ctrl(&mut ed, "c");
    caret_at(&mut ed, 1, 6);
    key(&mut ed, NamedKey::End, sovereign_editor::input::Modifiers::SHIFT);
    ctrl(&mut ed, "c");
    let writes = f.clipboard.borrow().writes.clone();
    let texts: Vec<&str> = writes.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, vec!["first", "second"]);
}

// "No automatic clearing." — for ordinary text: no expiry is asked for, and
// time passing does not touch the clipboard. (A copy including a revealed
// personal-data value is cleared after 30 s — decided 2026-10-10, tested in
// g7_copy_including_a_revealed_value_asks_for_clearing_after_30s.)
#[test]
fn b_no_automatic_clipboard_clearing() {
    let (mut ed, f) = open_focused(&doc1("plain words"));
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(last_write(&f).clear_after, None);
    let writes = f.clipboard.borrow().writes.len();
    for _ in 0..20 {
        advance(&f, Duration::from_secs(30));
        ed.poll();
    }
    assert_eq!(f.clipboard.borrow().writes.len(), writes, "the clipboard was written again");
    assert_eq!(clip_text(&f).as_deref(), Some("plain words"));
}

// ------------------------------------------------------------ autosave

// "The editor saves by itself after a short pause" — polling at the deadlines
// the editor asks for, within a minute of the last keystroke, a save of the
// current model is requested.
#[test]
fn b_autosave_after_a_pause() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "autosaved");
    let start = f.clock.get();
    for _ in 0..20 {
        if save_count(&f) > 0 {
            break;
        }
        match ed.next_deadline() {
            Some(d) if d <= start + Duration::from_secs(60) => {
                f.clock.set(d.max(f.clock.get()));
                ed.poll();
            }
            _ => break,
        }
    }
    assert!(save_count(&f) > 0, "no autosave within a minute of the pause");
    assert_eq!(texts_in(&last_save(&f).content), vec!["autosaved"]);
    assert_eq!(last_save(&f).content, model_json(&ed));
}

// Same, driven by a host that only polls once a minute.
#[test]
fn b_autosave_with_a_coarse_host_timer() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "coarse");
    advance(&f, Duration::from_secs(60));
    ed.poll();
    assert!(save_count(&f) > 0);
    assert_eq!(texts_in(&last_save(&f).content), vec!["coarse"]);
}

// "Ctrl+S saves at once"
#[test]
fn b_ctrl_s_saves_at_once() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "now");
    ctrl(&mut ed, "s");
    assert_eq!(save_count(&f), 1, "a save is requested synchronously, without waiting for a pause");
    assert_eq!(texts_in(&last_save(&f).content), vec!["now"]);
}

// "saves become versions in the document's history" — each save is a request
// to the document service (the editor does not overwrite in place itself) and
// successive saves carry successive models.
#[test]
fn b_successive_saves_carry_successive_models() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    for w in ["one", " two", " three"] {
        type_text(&mut ed, w);
        ctrl(&mut ed, "s");
        let r = last_save(&f);
        finish(&mut ed, r.id, true);
    }
    let texts: Vec<Vec<String>> = f.saves.borrow().iter().map(|r| texts_in(&r.content)).collect();
    assert_eq!(texts, vec![vec!["one".to_string()], vec!["one two".to_string()], vec!["one two three".to_string()]]);
}

// The save status the editor reports follows the save lifecycle: Saved →
// Unsaved → Saving → Saved (guarantee 1: "says so" needs a truthful status).
#[test]
fn b_save_status_transitions() {
    let (mut ed, f) = open_focused(&doc1(""));
    assert_eq!(ed.save_status(), SaveStatus::Saved);
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "s");
    assert_eq!(ed.save_status(), SaveStatus::Unsaved);
    ctrl(&mut ed, "s");
    assert_eq!(ed.save_status(), SaveStatus::Saving);
    let r = last_save(&f);
    finish(&mut ed, r.id, true);
    assert_eq!(ed.save_status(), SaveStatus::Saved);
}

// ------------------------------------------------------------ markdown text model

// "Text model: markdown source in text blocks" — text typed into a new
// document lands in a `text` block with markup "markdown", source kept as typed.
#[test]
fn b_new_text_is_a_markdown_text_block() {
    let (mut ed, f) = open_focused(r#"{"format":"sovereign.doc","v":2,"blocks":[]}"#);
    let src = "# Title\n\n*emphasis* and [a link](https://example.org)\n- item";
    for line in src.split('\n') {
        type_text(&mut ed, line);
        press(&mut ed, NamedKey::Enter);
    }
    ctrl(&mut ed, "s");
    let v = parse(&last_save(&f).content);
    let blocks = v["blocks"].as_array().unwrap();
    assert!(!blocks.is_empty());
    let joined: String = blocks
        .iter()
        .filter(|b| b["type"] == "text")
        .map(|b| {
            assert_eq!(b["markup"], "markdown");
            b["text"].as_str().unwrap().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    for piece in ["# Title", "*emphasis*", "[a link](https://example.org)", "- item"] {
        assert!(joined.contains(piece), "markdown source kept: {piece} in {joined:?}");
    }
}

// "shown with light formatting … while the source stays editable" — the
// markdown markers are in the model and can be edited like any character.
#[test]
fn b_markdown_markers_stay_editable() {
    let (mut ed, _f) = open_focused(&doc1("**bold**"));
    caret_at(&mut ed, 1, 0);
    press(&mut ed, NamedKey::Delete);
    press(&mut ed, NamedKey::Delete);
    assert_eq!(text_of(&ed, 1), "bold**");
}
