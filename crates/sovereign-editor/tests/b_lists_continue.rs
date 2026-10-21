//! Behaviour "Lists continue on Enter" (spec ch. 12, C-TEXT behaviours,
//! decided by Céline 2026-10-14, spec v2.0.17):
//!
//! "Enter at the end of, or inside, a non-empty list item starts the next item
//! with the same marker (`-`, `*`, `+`), the next number for an ordered list
//! (`1.` → `2.`, keeping `.` or `)`), the same indentation, and an unchecked box
//! for a task item (`- [ ]`). Enter on an empty item ends the list: its marker
//! is removed and the line becomes empty. Shift+Enter inserts a plain line break
//! inside the item. The continuation is ordinary text in the source (it is
//! undone in one step with the Enter that made it)."
//!
//! The text is observed as the markdown source of the text blocks, joined by
//! "\n" (`all_text`), so the tests hold whether Enter keeps one block or splits
//! it. Every test opens a document with the text already in it and presses
//! Enter alone, so no typing burst can merge with the Enter in undo.

mod common;
use common::*;

use sovereign_editor::input::{ImeEvent, InputEvent, Modifiers, NamedKey};

/// Opens a one-block document holding `text`, puts the caret at byte `at`,
/// presses Enter, and returns the source.
fn enter_at(text: &str, at: usize) -> String {
    let (mut ed, _f) = open_focused(&doc1(text));
    caret_at(&mut ed, 1, at);
    press(&mut ed, NamedKey::Enter);
    all_text(ed.document())
}

fn enter_at_end(text: &str) -> String {
    enter_at(text, text.len())
}

// ------------------------------------------------------------ continuation

// "starts the next item with the same marker (`-`, `*`, `+`)"
#[test]
fn b_lists_bullet_markers_continue_with_the_same_marker() {
    for m in ["-", "*", "+"] {
        let line = format!("{m} item");
        assert_eq!(enter_at_end(&line), format!("{line}\n{m} "), "marker {m:?}");
    }
}

// "the next number for an ordered list (`1.` → `2.`, keeping `.` or `)`)"
#[test]
fn b_lists_ordered_items_continue_with_the_next_number_and_delimiter() {
    for (line, next) in [("1. one", "2. "), ("1) one", "2) "), ("4. four", "5. "), ("7) seven", "8) ")] {
        assert_eq!(enter_at_end(line), format!("{line}\n{next}"), "{line:?}");
    }
}

// "the next number" — across a digit boundary.
#[test]
fn b_lists_ordered_numbers_carry_over_digits() {
    for (line, next) in [("9. nine", "10. "), ("99) n", "100) "), ("123. x", "124. ")] {
        assert_eq!(enter_at_end(line), format!("{line}\n{next}"), "{line:?}");
    }
}

// "the same indentation" — spaces and tabs, bullets and numbers.
#[test]
fn b_lists_indentation_is_preserved() {
    for (line, next) in [
        ("  - sub", "  - "),
        ("    * deep", "    * "),
        ("\t- tab", "\t- "),
        ("\t\t+ two tabs", "\t\t+ "),
        ("   3. three", "   4. "),
        ("\t9) nine", "\t10) "),
    ] {
        assert_eq!(enter_at_end(line), format!("{line}\n{next}"), "{line:?}");
    }
}

// "an unchecked box for a task item (`- [ ]`)" — a checked item continues
// unchecked too; the item's own marker and indentation are kept.
#[test]
fn b_lists_task_items_continue_unchecked() {
    for (line, next) in [
        ("- [ ] todo", "- [ ] "),
        ("- [x] done", "- [ ] "),
        ("* [x] done", "* [ ] "),
        ("  - [x] nested", "  - [ ] "),
        ("1. [x] first", "2. [ ] "),
    ] {
        assert_eq!(enter_at_end(line), format!("{line}\n{next}"), "{line:?}");
    }
}

// "Enter … inside a non-empty list item starts the next item": the item is
// split, and the tail goes after the new marker.
#[test]
fn b_lists_enter_inside_an_item_splits_it() {
    for (line, at, want) in [
        ("- alphabeta", 7, "- alpha\n- beta"),
        ("* alphabeta", 7, "* alpha\n* beta"),
        ("1. onetwo", 6, "1. one\n2. two"),
        ("  3) xy", 6, "  3) x\n  4) y"),
        ("- [x] ab", 7, "- [x] a\n- [ ] b"),
    ] {
        assert_eq!(enter_at(line, at), want, "{line:?} at {at}");
    }
}

// The caret lands after the new marker: typing goes into the new item.
#[test]
fn b_lists_caret_lands_after_the_new_marker() {
    let (mut ed, _f) = open_focused(&doc1("1. first"));
    caret_at(&mut ed, 1, 8);
    press(&mut ed, NamedKey::Enter);
    type_text(&mut ed, "second");
    assert_eq!(all_text(ed.document()), "1. first\n2. second");
}

// Continuing an item in the middle of a list inserts one new item after it.
// (Whether later numbers are renumbered is not decided; only the lines before
// and the new one are checked.)
#[test]
fn b_lists_enter_in_the_middle_of_a_list() {
    let got = enter_at("- a\n- b", 3);
    assert_eq!(got, "- a\n- \n- b");
    let got = enter_at("1. a\n2. b", 4);
    let lines: Vec<&str> = got.split('\n').collect();
    assert_eq!(&lines[..2], &["1. a", "2. "], "{got:?}");
    assert!(lines.len() == 3 && lines[2].ends_with(". b"), "{got:?}");
}

// ------------------------------------------------------------ ending the list

// "Enter on an empty item ends the list: its marker is removed and the line
// becomes empty."
#[test]
fn b_lists_enter_on_an_empty_item_ends_the_list() {
    for (text, want) in [
        ("- a\n- ", "- a\n"),
        ("* a\n* ", "* a\n"),
        ("1. a\n2. ", "1. a\n"),
        ("1) a\n2) ", "1) a\n"),
        ("- [ ] a\n- [ ] ", "- [ ] a\n"),
        ("- a\n  - ", "- a\n"),
        ("- a\n\t- ", "- a\n"),
    ] {
        assert_eq!(enter_at_end(text), want, "{text:?}");
    }
}

// "its marker is removed" — only that one: the items around it are untouched.
#[test]
fn b_lists_enter_on_an_empty_item_removes_only_its_marker() {
    let text = "- a\n- \n- c";
    assert_eq!(enter_at(text, 6), "- a\n\n- c");
    let text = "1. a\n2. \n3. c";
    assert_eq!(enter_at(text, 8), "1. a\n\n3. c");
}

// Two Enters at the end of an item: the first continues, the second ends.
#[test]
fn b_lists_second_enter_ends_the_list() {
    let (mut ed, _f) = open_focused(&doc1("- a"));
    caret_at(&mut ed, 1, 3);
    press(&mut ed, NamedKey::Enter);
    assert_eq!(all_text(ed.document()), "- a\n- ");
    press(&mut ed, NamedKey::Enter);
    assert_eq!(all_text(ed.document()), "- a\n");
    type_text(&mut ed, "after");
    assert_eq!(all_text(ed.document()), "- a\nafter", "the caret is on the emptied line");
}

// ------------------------------------------------------------ Shift+Enter

// "Shift+Enter inserts a plain line break inside the item" — no marker.
#[test]
fn b_lists_shift_enter_adds_no_marker() {
    for line in ["- item", "1. item", "- [ ] item", "  * item"] {
        let (mut ed, _f) = open_focused(&doc1(line));
        caret_at(&mut ed, 1, line.len());
        key(&mut ed, NamedKey::Enter, Modifiers::SHIFT);
        let got = all_text(ed.document());
        let rest = got.strip_prefix(line).unwrap_or_else(|| panic!("{line:?} changed: {got:?}"));
        assert!(rest.starts_with('\n'), "{line:?}: no line break ({got:?})");
        assert!(rest.trim().is_empty(), "{line:?}: Shift+Enter added a marker: {got:?}");
    }
}

// Shift+Enter on an empty item does not end the list either.
#[test]
fn b_lists_shift_enter_on_an_empty_item_keeps_its_marker() {
    let (mut ed, _f) = open_focused(&doc1("- a\n- "));
    caret_at(&mut ed, 1, 6);
    key(&mut ed, NamedKey::Enter, Modifiers::SHIFT);
    assert!(all_text(ed.document()).starts_with("- a\n- \n"), "{:?}", all_text(ed.document()));
}

// ------------------------------------------------------------ not a list item

// A line that is not a list item does not continue: Enter is a plain line break.
#[test]
fn b_lists_non_list_lines_do_not_continue() {
    for line in [
        "-word",
        "*emphasis* at the start",
        "**bold** start",
        "+5 points",
        "1.5 litres",
        "1.foo",
        "10 items",
        "# Heading",
        "## - not an item",
        "plain text",
        "a - b",
        "[ ] box without marker",
    ] {
        assert_eq!(enter_at_end(line), format!("{line}\n"), "{line:?}");
    }
}

// Inside a fenced code block, nothing continues.
#[test]
fn b_lists_nothing_continues_inside_a_fenced_code_block() {
    for text in ["```\n- item", "```rust\n1. one", "~~~\n* item", "```\n- a\n- b", "  ```\n- [ ] x"] {
        assert_eq!(enter_at_end(text), format!("{text}\n"), "{text:?}");
    }
    // nor does an empty item end anything there
    assert_eq!(enter_at_end("```\n- "), "```\n- \n");
}

// After the fence closes, lists continue again.
#[test]
fn b_lists_continue_again_after_a_closed_fence() {
    let text = "```\ncode\n```\n- item";
    assert_eq!(enter_at_end(text), format!("{text}\n- "));
}

// ------------------------------------------------------------ undo, save

// "(it is undone in one step with the Enter that made it)" — and redo restores
// both.
#[test]
fn b_lists_one_undo_removes_the_enter_and_its_continuation() {
    for (text, at) in [("- item", 6), ("1. one", 6), ("- [x] t", 7), ("- alphabeta", 7), ("- a\n- ", 6)] {
        let (mut ed, _f) = open_focused(&doc1(text));
        let before = model_json(&ed);
        caret_at(&mut ed, 1, at);
        press(&mut ed, NamedKey::Enter);
        let after = model_json(&ed);
        assert_ne!(after, before, "{text:?}: Enter changed nothing");
        ctrl(&mut ed, "z");
        assert_eq!(model_json(&ed), before, "{text:?}: one undo restores the model exactly");
        ctrl(&mut ed, "y");
        assert_eq!(model_json(&ed), after, "{text:?}: redo restores the Enter and its continuation");
    }
}

// "The continuation is ordinary text in the source" — saved exactly.
#[test]
fn b_lists_saved_source_holds_exactly_the_continued_text() {
    let (mut ed, f) = open_focused(&doc1("- [x] a\n9. b"));
    caret_at(&mut ed, 1, 7);
    press(&mut ed, NamedKey::Enter);
    type_text(&mut ed, "c");
    let last = ed.document().blocks().last().unwrap().id();
    let last_len = ed.document().blocks().last().unwrap().text().unwrap().len();
    ed.set_selection(sovereign_editor::Selection::caret(sovereign_editor::Position { block: last, offset: last_len }));
    press(&mut ed, NamedKey::Enter);
    type_text(&mut ed, "d");
    ctrl(&mut ed, "s");
    let saved = texts_in(&last_save(&f).content).join("\n");
    assert_eq!(saved, "- [x] a\n- [ ] c\n9. b\n10. d");
}

// ------------------------------------------------------------ IME, tokens

// Reading (TESTS.md, Uncertain): the Enter that commits a composition belongs
// to the input method. If the host forwards that Enter key while a pre-edit is
// showing, it must not also continue the list; the commit is inserted once.
#[test]
fn b_lists_enter_committing_a_composition_does_not_continue() {
    let (mut ed, _f) = open_focused(&doc1("- item "));
    caret_at(&mut ed, 1, 7);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "にほん");
    press(&mut ed, NamedKey::Enter);
    ime(&mut ed, ImeEvent::Commit("日本".into()));
    ime(&mut ed, ImeEvent::Disabled);
    let got = all_text(ed.document());
    assert_eq!(got.matches("日本").count(), 1, "{got:?}");
    assert!(!got.contains("\n- "), "the committing Enter continued the list: {got:?}");
    assert!(got.starts_with("- item 日本"), "{got:?}");
}

// Without composition, a commit followed by a real Enter does continue.
#[test]
fn b_lists_enter_after_a_finished_composition_continues() {
    let (mut ed, _f) = open_focused(&doc1("- item "));
    caret_at(&mut ed, 1, 7);
    ime(&mut ed, ImeEvent::Enabled);
    preedit(&mut ed, "にほん");
    ime(&mut ed, ImeEvent::Commit("日本".into()));
    ime(&mut ed, ImeEvent::Disabled);
    ed.handle_event(InputEvent::Focus(true));
    press(&mut ed, NamedKey::Enter);
    assert_eq!(all_text(ed.document()), "- item 日本\n- ");
}

// A personal-data token at the end of an item is not split: Enter after it
// continues the list; Enter right before it moves the whole token to the new
// item.
#[test]
fn b_lists_a_token_at_the_end_of_an_item_is_not_split() {
    let t = token("3");
    let line = format!("- mail {t}");
    let (mut ed, f) = open_focused(&doc1(&line));
    add_pii(&f, "3", "\u{2039}email\u{203A}", "a@example.com", false);
    ed.pii_display_changed();
    caret_at(&mut ed, 1, line.len());
    press(&mut ed, NamedKey::Enter);
    assert_eq!(all_text(ed.document()), format!("{line}\n- "));

    let line = format!("- mail{t}");
    let (mut ed, f) = open_focused(&doc1(&line));
    add_pii(&f, "3", "\u{2039}email\u{203A}", "a@example.com", false);
    ed.pii_display_changed();
    caret_at(&mut ed, 1, "- mail".len());
    press(&mut ed, NamedKey::Enter);
    assert_eq!(all_text(ed.document()), format!("- mail\n- {t}"));
}
