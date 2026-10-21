//! Guarantee 2 — "Unknown blocks pass through unchanged in content and
//! position; unchanged blocks keep their ids and bytes (ch. 03, stable
//! serialisation)." (spec ch. 12, C-TEXT)
//!
//! And ch. 03: "Unknown block types are preserved exactly — same position, same
//! JSON value — by every reader and writer. The editor shows them as a
//! read-only placeholder … and removes one only by an explicit user action on
//! that block." / "Writing a document whose blocks did not change produces the
//! same bytes: key order and number formatting are preserved for unchanged
//! blocks."

mod common;
use common::*;

use sovereign_editor::input::{Command, NamedKey};
use sovereign_editor::{OpenMode, ReadOnlyReason};

fn three_block_doc() -> (String, String, String) {
    let u1 = unknown_block(1);
    let u3 = unknown_block(3);
    (v2(&[u1.clone(), text_block(2, "edit me"), u3.clone()]), u1, u3)
}

/// Asserts `needle` occurs verbatim in `hay`, and returns where.
fn verbatim(hay: &str, needle: &str) -> usize {
    hay.find(needle).unwrap_or_else(|| panic!("block bytes not preserved:\n  want {needle}\n  in   {hay}"))
}

// "unchanged blocks keep their ids and bytes"; "Unknown blocks pass through
// unchanged in content and position"
#[test]
fn g2_editing_one_block_keeps_unknown_neighbours_bytes_and_order() {
    let (doc, u1, u3) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 2, 7);
    type_text(&mut ed, " — done");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    let a = verbatim(&saved, &u1);
    let b = verbatim(&saved, &u3);
    assert!(a < b);
    assert_eq!(block_types(&saved), vec!["x-chart-v9", "text", "x-chart-v9"]);
    assert_eq!(texts_in(&saved), vec!["edit me — done"]);
}

// "Writing a document whose blocks did not change produces the same bytes"
#[test]
fn g2_saving_an_unchanged_document_writes_the_same_bytes() {
    let (doc, _, _) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    assert_eq!(model_json(&ed), doc);
    ctrl(&mut ed, "s");
    for req in f.saves.borrow().iter() {
        assert_eq!(req.content, doc, "an unchanged save must be byte-identical");
    }
}

// "editing a block keeps its id"
#[test]
fn g2_edited_block_keeps_its_id() {
    let (doc, _, _) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 2, 0);
    type_text(&mut ed, "x");
    press(&mut ed, NamedKey::Backspace);
    press(&mut ed, NamedKey::Delete);
    ctrl(&mut ed, "s");
    let v = parse(&last_save(&f).content);
    let text = v["blocks"].as_array().unwrap().iter().find(|b| b["type"] == "text").unwrap();
    assert_eq!(text["id"], id(2));
}

// "Fields a reader does not know are preserved" (ch. 03) — on a known block
// that is edited.
#[test]
fn g2_unknown_fields_of_an_edited_text_block_are_preserved() {
    let block = format!(
        r#"{{"type":"text","id":"{}","markup":"markdown","text":"abc","x-style":{{"weight":700,"tags":["a",1.25]}}}}"#,
        id(1)
    );
    let (mut ed, f) = open_focused(&v2(&[block]));
    caret_at(&mut ed, 1, 3);
    type_text(&mut ed, "d");
    ctrl(&mut ed, "s");
    let v = parse(&last_save(&f).content);
    assert_eq!(v["blocks"][0]["text"], "abcd");
    assert_eq!(v["blocks"][0]["x-style"], serde_json::json!({"weight":700,"tags":["a",1.25]}));
}

// "Unknown top-level fields are preserved on write." (ch. 03)
#[test]
fn g2_unknown_top_level_fields_are_preserved() {
    let doc = format!(
        r#"{{"format":"sovereign.doc","v":2,"x-future":{{"k":[1,2.50]}},"blocks":[{}]}}"#,
        text_block(1, "abc")
    );
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "z");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    let v = parse(&saved);
    assert_eq!(v["x-future"], serde_json::json!({"k":[1,2.5]}));
    assert!(saved.contains("2.50"), "number formatting of the unknown field: {saved}");
}

// Known blocks this editor does not edit (image, video, typed table) are
// unchanged blocks: they keep their bytes when a text block changes.
#[test]
fn g2_unchanged_media_and_table_blocks_keep_their_bytes() {
    let img = format!(
        r#"{{"id":"{}","type":"image","alt":"","decorative":true,"caption":"c","ref":{{"sha256":"00ff","path":"C:\\pics\\a.png"}},"x-focal":[0.50,0.25]}}"#,
        id(1)
    );
    let vid = format!(
        r#"{{"type":"video","id":"{}","ref":{{"path":"/v/a.mp4","sha256":"aa"}},"caption":"","duration_secs":12.0}}"#,
        id(3)
    );
    let table = format!(
        r#"{{"type":"table","id":"{}","columns":[{{"name":"n","kind":"number"}}],"rows":[[1.50],[2e3],[-0.0],[12345678901234567890123]]}}"#,
        id(4)
    );
    let doc = v2(&[img.clone(), text_block(2, "t"), vid.clone(), table.clone()]);
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 2, 1);
    type_text(&mut ed, "ext");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    for b in [&img, &vid, &table] {
        verbatim(&saved, b);
    }
}

// "removes one only by an explicit user action on that block" — select all and
// delete is an action on the selection, not on the unknown block.
#[test]
fn g2_select_all_and_delete_keeps_unknown_blocks() {
    let (doc, u1, u3) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    ctrl(&mut ed, "a");
    press(&mut ed, NamedKey::Delete);
    ctrl(&mut ed, "a");
    press(&mut ed, NamedKey::Backspace);
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert!(verbatim(&saved, &u1) < verbatim(&saved, &u3));
}

// Same, through cut: a cut of everything does not remove unknown blocks.
#[test]
fn g2_cut_all_keeps_unknown_blocks() {
    let (doc, u1, u3) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "x");
    cmd(&mut ed, Command::Save);
    let saved = last_save(&f).content;
    assert!(verbatim(&saved, &u1) < verbatim(&saved, &u3));
}

// "removes one only by an explicit user action on that block" — Backspace at
// the start of the following text block, and Delete at the end of the
// preceding one, do not remove or alter it.
#[test]
fn g2_backspace_and_delete_at_block_boundaries_keep_unknown_blocks() {
    let u2 = unknown_block(2);
    let doc = v2(&[text_block(1, "before"), u2.clone(), text_block(3, "after")]);
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 3, 0);
    press(&mut ed, NamedKey::Backspace);
    caret_at(&mut ed, 1, 6);
    press(&mut ed, NamedKey::Delete);
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    verbatim(&saved, &u2);
    let all = texts_in(&saved).join("|");
    assert!(all.contains("before") && all.contains("after"), "{all}");
}

// "The editor shows them as a read-only placeholder" — typing with the caret
// on an unknown block does not change it, and the typed text is not lost.
#[test]
fn g2_typing_on_an_unknown_block_does_not_alter_it() {
    let (doc, u1, u3) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "QQ");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    verbatim(&saved, &u1);
    verbatim(&saved, &u3);
}

// "removes one only by an explicit user action on that block" — the explicit
// action removes exactly that block; every other block keeps its bytes.
#[test]
fn g2_explicit_remove_removes_exactly_that_block() {
    let (doc, _u1, u3) = three_block_doc();
    let (mut ed, f) = open_focused(&doc);
    cmd(&mut ed, Command::RemoveBlock(bid(1)));
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert!(!saved.contains(&id(1)), "block 1 removed");
    verbatim(&saved, &u3);
    verbatim(&saved, &text_block(2, "edit me"));
}

// A document made only of unknown blocks: still usable, nothing lost.
#[test]
fn g2_document_of_only_unknown_blocks() {
    let u1 = unknown_block(1);
    let u2 = unknown_block(2);
    let doc = v2(&[u1.clone(), u2.clone()]);
    let (mut ed, f) = open_focused(&doc);
    assert_eq!(model_json(&ed), doc, "read then written unchanged");
    ctrl(&mut ed, "a");
    press(&mut ed, NamedKey::Delete);
    type_text(&mut ed, "hi");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert!(verbatim(&saved, &u1) < verbatim(&saved, &u2));
    assert!(texts_in(&saved).iter().any(|t| t.contains("hi")), "typed text is somewhere: {saved}");
}

// "An empty document has "blocks": []" — typing into it creates a text block.
#[test]
fn g2_empty_document_round_trips_and_accepts_typing() {
    let empty = r#"{"format":"sovereign.doc","v":2,"blocks":[]}"#;
    let (mut ed, f) = open_focused(empty);
    assert_eq!(model_json(&ed), empty);
    type_text(&mut ed, "first words");
    ctrl(&mut ed, "s");
    assert_eq!(texts_in(&last_save(&f).content), vec!["first words"]);
}

// "A reader that meets a v higher than it knows opens the document read-only
// and never rewrites it." (ch. 03)
#[test]
fn g2_newer_format_version_is_read_only_and_never_rewritten() {
    let doc = format!(r#"{{"format":"sovereign.doc","v":3,"blocks":[{}],"x":1}}"#, text_block(1, "future"));
    let (mut ed, f) = open_focused(&doc);
    assert!(ed.is_read_only());
    assert_eq!(ed.read_only_reason(), Some(ReadOnlyReason::NewerFormat { v: 3 }));
    caret_at(&mut ed, 1, 0);
    type_text(&mut ed, "no");
    press(&mut ed, NamedKey::Backspace);
    ctrl(&mut ed, "x");
    ctrl(&mut ed, "s");
    cmd(&mut ed, Command::RemoveBlock(bid(1)));
    advance(&f, std::time::Duration::from_secs(3600));
    ed.poll();
    ed.request_close();
    assert_eq!(save_count(&f), 0, "never rewritten");
    assert_eq!(model_json(&ed), doc);
}

// "Unknown blocks pass through" also when the document is opened read-only.
#[test]
fn g2_read_only_open_never_saves() {
    let (doc, _, _) = three_block_doc();
    let (mut ed, f) = open_with(&doc, OpenMode::ReadOnly);
    caret_at(&mut ed, 2, 0);
    type_text(&mut ed, "no");
    ctrl(&mut ed, "s");
    advance(&f, std::time::Duration::from_secs(3600));
    ed.poll();
    assert_eq!(save_count(&f), 0);
    assert_eq!(model_json(&ed), doc);
}

// Decided 2026-10-10 (v1 media migration): the editor opens a v1 envelope,
// saves it as v2 at the next write, and the migrated images are kept as
// valid image blocks with the description and the hash pending — the editor
// never reads the media files.
#[test]
fn g2_v1_document_saves_with_media_pending() {
    let v1 = r#"{"body":"hello","images":[{"path":"/a.png","caption":"A"}],"videos":[]}"#;
    let (mut ed, f) = open_focused(v1);
    let n = ed.document().blocks()[0].id();
    ed.set_selection(sovereign_editor::Selection::caret(sovereign_editor::Position { block: n, offset: 5 }));
    type_text(&mut ed, "!");
    ctrl(&mut ed, "s");
    let saved = last_save(&f).content;
    assert_eq!(block_types(&saved), vec!["text", "image"]);
    assert_eq!(texts_in(&saved), vec!["hello!"]);
    let (doc, _) = sovereign_editor::doc::DocumentV2::read(&saved).unwrap();
    let img = &doc.blocks()[1];
    assert!(img.is_known() && img.needs_description());
    assert_eq!(img.media_path(), Some("/a.png"));
    assert_eq!(img.media_sha256(), None);
}

// "a new block gets a new one" — "a random 128-bit identifier (hex), unique
// within the document".
#[test]
fn g2_new_block_ids_are_128_bit_hex_and_unique() {
    let (mut ed, f) = open_focused(r#"{"format":"sovereign.doc","v":2,"blocks":[]}"#);
    for i in 0..20 {
        type_text(&mut ed, &format!("paragraph {i}"));
        press(&mut ed, NamedKey::Enter);
        press(&mut ed, NamedKey::Enter);
    }
    ctrl(&mut ed, "s");
    let v = parse(&last_save(&f).content);
    let ids: Vec<String> =
        v["blocks"].as_array().unwrap().iter().map(|b| b["id"].as_str().expect("every block has an id").to_string()).collect();
    assert!(!ids.is_empty());
    for i in &ids {
        assert_eq!(i.len(), 32, "128 bits as hex: {i}");
        assert!(i.chars().all(|c| c.is_ascii_hexdigit()), "{i}");
    }
    let mut dedup = ids.clone();
    dedup.sort();
    dedup.dedup();
    assert_eq!(dedup.len(), ids.len(), "ids unique within the document");
}
