//! Guarantee 1 — "Nothing typed is lost. Every character entered is in the
//! in-memory model until the user removes it; a save writes exactly that model;
//! a failed save keeps it and says so." (spec ch. 12, C-TEXT)

mod common;
use common::*;

use sovereign_editor::host::RemoteChange;
use sovereign_editor::input::{Command, ImeEvent, InputEvent, NamedKey};
use sovereign_editor::{CloseRequest, NoticeKind, SaveStatus};

const MIXED: &str = "Hello wörld — 日本語 👩‍👩‍👧‍👦 🇫🇷 e\u{301} \u{1100}\u{1161}\u{11A8} مرحبا שלום\ttab";

// "Every character entered is in the in-memory model"
#[test]
fn g1_every_typed_character_is_in_the_model() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, MIXED);
    assert_eq!(text_of(&ed, 1), MIXED);
}

// "Every character entered is in the in-memory model" — a key press can carry
// several characters at once (a whole emoji, a dead-key sequence).
#[test]
fn g1_multi_character_text_event_is_inserted_whole() {
    let (mut ed, _f) = open_focused(&doc1("ab"));
    caret_at(&mut ed, 1, 1);
    ed.handle_event(InputEvent::Text("👩‍👩‍👧‍👦xyz".into()));
    assert_eq!(text_of(&ed, 1), "a👩‍👩‍👧‍👦xyzb");
}

// "Every character entered is in the in-memory model" — input-method commits
// are entered characters too.
#[test]
fn g1_ime_commit_is_in_the_model() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "にほんご");
    ime(&mut ed, ImeEvent::Commit("日本語".into()));
    assert_eq!(text_of(&ed, 1), "日本語");
}

// "Every character entered is in the in-memory model" — Enter inserts a line
// break in the markdown source (text model: markdown source).
#[test]
fn g1_enter_keeps_both_halves() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "first");
    press(&mut ed, NamedKey::Enter);
    type_text(&mut ed, "second");
    let all = all_text(ed.document());
    assert!(all.contains("first"), "{all:?}");
    assert!(all.contains("second"), "{all:?}");
    assert!(all.find("first").unwrap() < all.find("second").unwrap());
}

// "a save writes exactly that model"
#[test]
fn g1_ctrl_s_saves_exactly_the_model() {
    let (mut ed, f) = open_focused(&doc1("start "));
    caret_at(&mut ed, 1, 6);
    type_text(&mut ed, MIXED);
    let model = model_json(&ed);
    ctrl(&mut ed, "s");
    let req = last_save(&f);
    assert_eq!(req.content, model, "the saved bytes are the model's bytes");
    assert_eq!(texts_in(&req.content), vec![format!("start {MIXED}")]);
}

// "a save writes exactly that model" — typing while a save is in flight is
// neither lost nor attributed to the earlier save.
#[test]
fn g1_typing_during_an_inflight_save_is_kept_and_saved_next() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "one");
    cmd(&mut ed, Command::Save);
    let first = last_save(&f);
    assert_eq!(texts_in(&first.content), vec!["one"]);
    type_text(&mut ed, " two");
    finish(&mut ed, first.id, true);
    assert_eq!(text_of(&ed, 1), "one two");
    assert!(ed.has_unsaved_changes(), "\" two\" is not in the store yet");
    cmd(&mut ed, Command::Save);
    assert_eq!(texts_in(&last_save(&f).content), vec!["one two"]);
}

// "a failed save keeps it and says so"
#[test]
fn g1_failed_save_keeps_the_model_and_says_so() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "precious");
    ed.take_notices();
    cmd(&mut ed, Command::Save);
    let req = last_save(&f);
    finish(&mut ed, req.id, false);
    assert_eq!(text_of(&ed, 1), "precious", "the model is kept");
    assert!(ed.has_unsaved_changes());
    assert!(matches!(ed.save_status(), SaveStatus::Failed { .. }), "status says so: {:?}", ed.save_status());
    let notices = ed.take_notices();
    assert!(notices.iter().any(|n| n.kind == NoticeKind::SaveFailed), "a notice says so: {notices:?}");
}

// "a failed save keeps it and says so" — and a retry saves the same model.
#[test]
fn g1_failed_save_then_retry_saves_the_kept_model() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "retry me");
    cmd(&mut ed, Command::Save);
    let first = last_save(&f);
    finish(&mut ed, first.id, false);
    cmd(&mut ed, Command::Save);
    let second = last_save(&f);
    assert_ne!(first.id, second.id, "a retry is a new request");
    assert_eq!(second.content, first.content);
    finish(&mut ed, second.id, true);
    assert_eq!(ed.save_status(), SaveStatus::Saved);
    assert!(!ed.has_unsaved_changes());
}

// "a failed save keeps it and says so" — an older save succeeding after a newer
// one failed must not report the newer model as saved.
#[test]
fn g1_out_of_order_completion_does_not_hide_a_failure() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "a");
    cmd(&mut ed, Command::Save);
    let older = last_save(&f);
    type_text(&mut ed, "b");
    cmd(&mut ed, Command::Save);
    let newer = last_save(&f);
    finish(&mut ed, newer.id, false);
    finish(&mut ed, older.id, true);
    assert_eq!(text_of(&ed, 1), "ab");
    assert!(ed.has_unsaved_changes(), "\"b\" never reached the store");
    assert!(matches!(ed.save_status(), SaveStatus::Failed { .. }), "{:?}", ed.save_status());
}

// "a failed save keeps it and says so" + "The unsaved prompt remains for the
// case where a save fails" (Behaviours, Saving).
#[test]
fn g1_close_after_failed_save_asks_the_user() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "x");
    cmd(&mut ed, Command::Save);
    let req = last_save(&f);
    finish(&mut ed, req.id, false);
    assert_eq!(ed.request_close(), CloseRequest::ConfirmUnsaved);
    assert_eq!(text_of(&ed, 1), "x");
}

// "Every character entered is in the in-memory model until the user removes
// it; a save writes exactly that model" — closing with unsaved typing saves it
// (autosave makes the prompt only for failures).
#[test]
fn g1_close_with_unsaved_typing_saves_it_first() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "last words");
    let before = save_count(&f);
    let answer = ed.request_close();
    assert_eq!(answer, CloseRequest::Saving);
    assert_eq!(save_count(&f), before + 1);
    assert_eq!(texts_in(&last_save(&f).content), vec!["last words"]);
    let id = last_save(&f).id;
    finish(&mut ed, id, true);
    assert_eq!(ed.request_close(), CloseRequest::Ready);
}

// "Every character entered is in the in-memory model until the user removes
// it" — a remote change (sync) arriving mid-edit does not remove it.
#[test]
fn g1_remote_change_mid_edit_keeps_local_typing_in_the_model() {
    let (mut ed, _f) = open_focused(&doc1("shared "));
    caret_at(&mut ed, 1, 7);
    type_text(&mut ed, "LOCAL-UNSAVED");
    ed.remote_change(RemoteChange { content: doc1("shared REMOTE"), device: "phone".into() });
    let all = all_text(ed.document());
    assert!(all.contains("LOCAL-UNSAVED"), "local typing was dropped by a remote change: {all:?}");
}

// Same, while a save of the local text is in flight and then fails.
#[test]
fn g1_remote_change_during_failed_save_keeps_local_typing() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "MINE");
    cmd(&mut ed, Command::Save);
    let req = last_save(&f);
    ed.remote_change(RemoteChange { content: doc1("THEIRS"), device: "laptop".into() });
    finish(&mut ed, req.id, false);
    assert!(all_text(ed.document()).contains("MINE"));
    assert!(ed.has_unsaved_changes());
}

// Decided 2026-10-10: in any read-only state, "input never disappears without
// a notice" — editing input changes nothing and produces a ReadOnly notice.
#[test]
fn g1_input_in_a_read_only_state_is_never_dropped_silently() {
    let over_cap = doc1(&"y".repeat(sovereign_editor::MAX_EDITABLE_TEXT_BYTES + 1));
    let newer = format!(r#"{{"format":"sovereign.doc","v":3,"blocks":[{}]}}"#, text_block(1, "future"));
    let states: [(&str, String, sovereign_editor::OpenMode); 3] = [
        ("opened read-only", doc1("theirs"), sovereign_editor::OpenMode::ReadOnly),
        ("newer format", newer, sovereign_editor::OpenMode::Edit),
        ("over the cap", over_cap, sovereign_editor::OpenMode::Edit),
    ];
    for (state, content, mode) in states {
        let (mut ed, f) = open_with(&content, mode);
        ed.handle_event(InputEvent::Focus(true));
        assert!(ed.is_read_only(), "{state}");
        set_clip_text(&f, "pasted");
        let before = model_json(&ed);
        for (what, attempt) in editing_attempts() {
            ed.take_notices();
            attempt(&mut ed);
            let notices = ed.take_notices();
            assert!(
                notices.iter().any(|n| n.kind == NoticeKind::ReadOnly),
                "{state}: {what} disappeared without a notice ({notices:?})"
            );
            assert_eq!(model_json(&ed), before, "{state}: {what} changed a read-only document");
        }
        assert_eq!(save_count(&f), 0, "{state}");
    }
}

// "Every character entered is in the in-memory model" — losing focus is not
// removing.
#[test]
fn g1_focus_loss_keeps_typing() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "stay");
    ed.handle_event(InputEvent::Focus(false));
    ed.handle_event(InputEvent::Focus(true));
    assert_eq!(text_of(&ed, 1), "stay");
}

// "Every character entered is in the in-memory model" — a paste is entered text.
#[test]
fn g1_paste_inserts_the_whole_clipboard_text() {
    let (mut ed, f) = open_focused(&doc1("[]"));
    let pasted: String = "lorem ipsum ✓ ".repeat(14_000); // ~200 KB, under the cap
    set_clip_text(&f, pasted.clone());
    caret_at(&mut ed, 1, 1);
    ctrl(&mut ed, "v");
    assert_eq!(text_of(&ed, 1), format!("[{pasted}]"));
}

// "Every character entered is in the in-memory model" — CR LF typed or pasted
// is kept as entered; existing CRLF content is not rewritten.
#[test]
fn g1_crlf_content_is_kept_byte_for_byte() {
    let (mut ed, f) = open_focused("line one\r\nline two\r\n");
    let n = ed.document().blocks()[0].id();
    let len = ed.document().blocks()[0].text().unwrap().len();
    ed.set_selection(sovereign_editor::Selection::caret(sovereign_editor::Position { block: n, offset: len }));
    type_text(&mut ed, "three");
    ctrl(&mut ed, "s");
    assert_eq!(texts_in(&last_save(&f).content), vec!["line one\r\nline two\r\nthree"]);
}
