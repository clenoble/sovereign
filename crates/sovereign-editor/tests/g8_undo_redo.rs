//! Guarantee 8 — "Undo / redo restore exactly the previous model, grouped by
//! typing bursts; a save does not clear them; they are not written to disk
//! (the commit history is the durable history)." (spec ch. 12, C-TEXT)
//!
//! "Exactly the previous model" is checked on the stored form: the bytes
//! `to_json_string` gives, which also catches lost ids, reordered blocks or
//! reformatted unknown blocks.

mod common;
use common::*;

use std::time::Duration;

use sovereign_editor::input::{Command, Modifiers, NamedKey};

// "grouped by typing bursts" — one uninterrupted burst is one undo step.
#[test]
fn g8_one_burst_is_one_undo_step() {
    let (mut ed, _f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    let before = model_json(&ed);
    type_text(&mut ed, "a quick burst of typing");
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), before);
}

// "grouped by typing bursts" — two bursts separated by a long pause are two
// steps (a minute is longer than any reading of "burst").
#[test]
fn g8_bursts_separated_by_a_pause_undo_separately() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    let empty = model_json(&ed);
    type_text(&mut ed, "first");
    advance(&f, Duration::from_secs(60));
    ed.poll();
    let after_first = model_json(&ed);
    advance(&f, Duration::from_millis(10));
    type_text(&mut ed, " second");
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), after_first);
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), empty);
}

// "restore exactly the previous model" — redo restores the state undone.
#[test]
fn g8_redo_restores_exactly() {
    let doc = v2(&[unknown_block(1), text_block(2, "abc"), unknown_block(3)]);
    let (mut ed, _f) = open_focused(&doc);
    caret_at(&mut ed, 2, 3);
    type_text(&mut ed, " def 👩‍👩‍👧‍👦");
    let done = model_json(&ed);
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), doc, "undo back to the opened bytes");
    ctrl(&mut ed, "y");
    assert_eq!(model_json(&ed), done, "Ctrl+Y redoes");
    ctrl(&mut ed, "z");
    ctrl_shift(&mut ed, "z");
    assert_eq!(model_json(&ed), done, "Ctrl+Shift+Z redoes");
}

// "restore exactly the previous model" — every kind of edit, undone in
// reverse, walks back through exactly the states it went through.
#[test]
fn g8_undo_walks_back_through_every_state() {
    let doc = v2(&[text_block(1, "one two three"), unknown_block(2), text_block(3, &format!("x {} y", token("3")))]);
    let (mut ed, f) = open_focused(&doc);
    set_clip_text(&f, "PASTED\r\nTEXT");
    let mut states = vec![model_json(&ed)];
    let mut step = |ed: &mut sovereign_editor::Editor, act: &dyn Fn(&mut sovereign_editor::Editor)| {
        // a pause between steps so each is its own group
        advance(&f, Duration::from_secs(60));
        ed.poll();
        act(ed);
        let s = model_json(ed);
        if states.last() != Some(&s) {
            states.push(s);
        }
    };
    step(&mut ed, &|e| {
        caret_at(e, 1, 3);
        type_text(e, "!!");
    });
    step(&mut ed, &|e| {
        caret_at(e, 1, 0);
        key(e, NamedKey::End, Modifiers::SHIFT);
        press(e, NamedKey::Delete);
    });
    step(&mut ed, &|e| {
        caret_at(e, 3, 2);
        press(e, NamedKey::Delete);
    });
    step(&mut ed, &|e| {
        caret_at(e, 3, 0);
        ctrl(e, "v");
    });
    step(&mut ed, &|e| {
        caret_at(e, 3, 0);
        press(e, NamedKey::Enter);
    });
    step(&mut ed, &|e| cmd(e, Command::RemoveBlock(bid(2))));
    step(&mut ed, &|e| {
        ctrl(e, "a");
        ctrl(e, "x");
    });
    assert!(states.len() >= 6, "the edits changed the model");
    let mut i = states.len() - 1;
    while i > 0 {
        ctrl(&mut ed, "z");
        i -= 1;
        assert_eq!(model_json(&ed), states[i], "undo step back to state {i}");
    }
    for j in 1..states.len() {
        ctrl(&mut ed, "y");
        assert_eq!(model_json(&ed), states[j], "redo step to state {j}");
    }
}

// "a save does not clear them"
#[test]
fn g8_save_does_not_clear_undo_or_redo() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    let empty = model_json(&ed);
    type_text(&mut ed, "saved text");
    let typed = model_json(&ed);
    ctrl(&mut ed, "s");
    let id = last_save(&f).id;
    finish(&mut ed, id, true);
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), empty, "undo across a save");
    cmd(&mut ed, Command::Save);
    let id = last_save(&f).id;
    finish(&mut ed, id, true);
    ctrl(&mut ed, "y");
    assert_eq!(model_json(&ed), typed, "redo across a save");
}

// "a save does not clear them" — nor does an autosave.
#[test]
fn g8_autosave_does_not_clear_undo() {
    let (mut ed, f) = open_focused(&doc1("base"));
    caret_at(&mut ed, 1, 4);
    let before = model_json(&ed);
    type_text(&mut ed, " more");
    for _ in 0..10 {
        advance(&f, Duration::from_secs(30));
        ed.poll();
        let last = f.saves.borrow().last().map(|r| r.id);
        if let Some(id) = last {
            finish(&mut ed, id, true);
        }
    }
    ctrl(&mut ed, "z");
    assert_eq!(model_json(&ed), before);
}

// After undo, a save writes the undone model (undo changes the model, not just
// the view).
#[test]
fn g8_save_after_undo_writes_the_undone_model() {
    let (mut ed, f) = open_focused(&doc1("v1"));
    caret_at(&mut ed, 1, 2);
    type_text(&mut ed, "-draft");
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "s");
    for r in f.saves.borrow().iter() {
        assert_eq!(texts_in(&r.content), vec!["v1"]);
    }
}

// "they are not written to disk (the commit history is the durable history)" —
// nor into the saved document: text typed then deleted, or undone, is absent
// from what is saved.
#[test]
fn g8_undo_history_is_not_in_the_saved_document() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "UNDONETEXT");
    ctrl(&mut ed, "z");
    type_text(&mut ed, "DELETEDTEXT");
    for _ in 0.."DELETEDTEXT".len() {
        press(&mut ed, NamedKey::Backspace);
    }
    type_text(&mut ed, "kept");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert!(!saved.contains("UNDONETEXT") && !saved.contains("DELETEDTEXT"), "{saved}");
    let v = parse(&saved);
    let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
    for k in keys {
        assert!(["format", "v", "blocks"].contains(&k.as_str()), "unexpected top-level field {k}");
    }
}

// Undo with nothing to undo, and redo with nothing to redo, change nothing.
#[test]
fn g8_undo_redo_on_empty_history_change_nothing() {
    let doc = v2(&[unknown_block(1), text_block(2, "x")]);
    let (mut ed, f) = open_focused(&doc);
    ctrl(&mut ed, "z");
    ctrl(&mut ed, "y");
    cmd(&mut ed, Command::Undo);
    cmd(&mut ed, Command::Redo);
    assert_eq!(model_json(&ed), doc);
    ctrl(&mut ed, "s");
    for r in f.saves.borrow().iter() {
        assert_eq!(r.content, doc);
    }
}

// With guarantee 1: after a remote change, one undo removes the last local
// burst, not the earlier local text.
#[test]
fn g8_undo_after_remote_change_removes_only_the_last_burst() {
    let (mut ed, f) = open_focused(&doc1(""));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "A");
    advance(&f, Duration::from_secs(60));
    ed.poll();
    type_text(&mut ed, "B");
    ed.remote_change(sovereign_editor::host::RemoteChange { content: doc1("R"), device: "phone".into() });
    ctrl(&mut ed, "z");
    assert!(all_text(ed.document()).contains('A'), "undo removed only the last burst");
}
