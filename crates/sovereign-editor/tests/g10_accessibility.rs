//! Guarantee 10 — "Accessible: the focused text is exposed with its value,
//! caret and selection through AccessKit; everything is reachable by keyboard;
//! a password field never announces its content." (spec ch. 12, C-TEXT)
//!
//! The editor's AccessKit output is read with `accesskit_consumer`, the layer
//! AccessKit's platform adapters (UIA, AT-SPI, …) are built on — so "exposed"
//! means: what a platform adapter would hand a screen reader. Texts in these
//! tests avoid markdown syntax, so the displayed text equals the source.

mod common;
use common::*;

use sovereign_editor::input::{InputEvent, Key, KeyEvent, Modifiers, NamedKey};
use sovereign_editor::{EventResult, FieldKind, OpenMode};

const LINE: &str = "Hello wörld 👩‍👩‍👧‍👦 مرحبا end";

fn usv(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

// "the focused text is exposed with its value"
#[test]
fn g10_focused_text_value_is_exposed() {
    let (mut ed, _f) = open_focused(&doc1(LINE));
    let ft = focused_text(&a11y_tree(&mut ed));
    assert!(matches!(ft.role, accesskit::Role::MultilineTextInput | accesskit::Role::TextInput | accesskit::Role::Document), "{:?}", ft.role);
    assert_eq!(ft.text.trim_end_matches('\n'), LINE);
}

// "… with its … caret"
#[test]
fn g10_caret_is_exposed() {
    let (mut ed, _f) = open_focused(&doc1(LINE));
    for byte in grapheme_boundaries(LINE) {
        caret_at(&mut ed, 1, byte);
        let ft = focused_text(&a11y_tree(&mut ed));
        assert_eq!(ft.caret, Some(usv(LINE, byte)), "caret at byte {byte}");
        assert_eq!(ft.selected, None);
    }
}

// "… and selection"
#[test]
fn g10_selection_is_exposed() {
    let (mut ed, _f) = open_focused(&doc1(LINE));
    caret_at(&mut ed, 1, 6);
    for _ in 0..5 {
        key(&mut ed, NamedKey::ArrowRight, Modifiers::SHIFT);
    }
    let ft = focused_text(&a11y_tree(&mut ed));
    assert_eq!(ft.selected.as_deref(), Some("wörld"));
    assert_eq!(ft.caret, Some(usv(LINE, 6) + 5));
}

// "exposed … through AccessKit" — live: the updates sent to the sink, applied
// in order, give the same text and caret as a fresh snapshot after each edit.
#[test]
fn g10_incremental_updates_track_the_model() {
    let (mut ed, f) = open_focused(&doc1("ab"));
    let mut tree = accesskit_consumer::Tree::new(ed.accessibility_tree(), true);
    f.a11y_updates.borrow_mut().clear();
    caret_at(&mut ed, 1, 2);
    type_text(&mut ed, "cd");
    press(&mut ed, NamedKey::ArrowLeft);
    paint(&mut ed);
    let updates: Vec<_> = f.a11y_updates.borrow_mut().drain(..).collect();
    assert!(!updates.is_empty(), "the editor pushed updates to the sink");
    for u in updates {
        tree.update_and_process_changes(u, &mut NoopHandler);
    }
    let live = focused_text(&tree);
    let fresh = focused_text(&a11y_tree(&mut ed));
    assert_eq!(live.text, fresh.text);
    assert_eq!(live.caret, fresh.caret);
    assert_eq!(live.text.trim_end_matches('\n'), "abcd");
    assert_eq!(live.caret, Some(3));
}

// "the focused text is exposed" — focus is reported only while the editor has
// it.
#[test]
fn g10_focus_follows_the_editor_focus() {
    let (mut ed, _f) = open(&doc1(LINE));
    ed.handle_event(InputEvent::Focus(true));
    let tree = a11y_tree(&mut ed);
    assert!(tree.state().focus().map(|n| n.is_text_input() || n.role() == accesskit::Role::Document).unwrap_or(false));
}

// The accessible tree uses the tree id the host gave (so the shell can graft it).
#[test]
fn g10_updates_use_the_host_tree_id() {
    let (mut ed, f) = open_focused(&doc1("x"));
    assert_eq!(ed.accessibility_tree().tree_id, accesskit::TreeId::ROOT);
    type_text(&mut ed, "y");
    for u in f.a11y_updates.borrow().iter() {
        assert_eq!(u.tree_id, accesskit::TreeId::ROOT);
    }
}

// Read-only documents are announced as read-only.
#[test]
fn g10_read_only_is_exposed() {
    let (mut ed, _f) = open_with(&doc1(LINE), OpenMode::ReadOnly);
    ed.handle_event(InputEvent::Focus(true));
    let ft = focused_text(&a11y_tree(&mut ed));
    assert!(ft.read_only);
    assert_eq!(ft.text.trim_end_matches('\n'), LINE);
}

// "a password field never announces its content"
#[test]
fn g10_password_field_never_exposes_its_content() {
    let (host, f) = host();
    let mut field = sovereign_editor::Editor::open_field(FieldKind::Password, config(), host);
    field.handle_event(InputEvent::Focus(true));
    let secret = "S3cr3t-Pässwörd";
    type_text(&mut field, secret);
    assert_eq!(field.field_text(), secret, "the field holds what was typed");
    let snapshot = field.accessibility_tree();
    let role_ok = snapshot.nodes.iter().any(|(_, n)| n.role() == accesskit::Role::PasswordInput);
    assert!(role_ok, "the field is exposed as a password input");
    let mut all = all_exposed_strings(&snapshot);
    for u in f.a11y_updates.borrow().iter() {
        all.extend(all_exposed_strings(u));
    }
    for s in &all {
        for piece in ["S3cr", "cr3t", "Päss", "wörd"] {
            assert!(!s.contains(piece), "password content announced: {s:?}");
        }
    }
    // Nor through text runs a screen reader could walk.
    let tree = accesskit_consumer::Tree::new(field.accessibility_tree(), true);
    if let Some(n) = tree.state().focus() {
        if n.supports_text_ranges() {
            let t = n.document_range().text();
            assert!(!t.contains("S3cr") && !t.contains("wörd"), "{t:?}");
        }
    }
}

// "everything is reachable by keyboard" — each command has a keyboard path
// inside the editor with the same effect.
#[test]
fn g10_commands_are_reachable_by_keyboard() {
    // select all + copy
    let (mut ed, f) = open_focused(&doc1("alpha beta"));
    ctrl(&mut ed, "a");
    ctrl(&mut ed, "c");
    assert_eq!(clip_text(&f).as_deref(), Some("alpha beta"));
    // cut
    ctrl(&mut ed, "x");
    assert_eq!(text_of(&ed, 1), "");
    // paste
    ctrl(&mut ed, "v");
    assert_eq!(text_of(&ed, 1), "alpha beta");
    // undo / redo
    ctrl(&mut ed, "z");
    assert_eq!(text_of(&ed, 1), "");
    ctrl(&mut ed, "y");
    assert_eq!(text_of(&ed, 1), "alpha beta");
    // save
    let n = save_count(&f);
    ctrl(&mut ed, "s");
    assert_eq!(save_count(&f), n + 1);
    // selection by keyboard: Shift+Home / Shift+End
    caret_at(&mut ed, 1, 5);
    key(&mut ed, NamedKey::Home, Modifiers::SHIFT);
    assert_eq!((ed.selection().anchor.offset, ed.selection().focus.offset), (5, 0));
    key(&mut ed, NamedKey::End, Modifiers::SHIFT);
    assert_eq!(ed.selection().focus.offset, 10);
}

// "everything is reachable by keyboard" — the caret can reach every block,
// including unknown ones, from the keyboard alone.
#[test]
fn g10_every_block_is_reachable_by_keyboard() {
    let doc = v2(&[text_block(1, "a"), unknown_block(2), text_block(3, "b"), unknown_block(4), text_block(5, "c")]);
    let (mut ed, _f) = open_focused(&doc);
    caret_at(&mut ed, 1, 0);
    let mut visited = std::collections::HashSet::new();
    for _ in 0..40 {
        visited.insert(caret(&ed).block);
        press(&mut ed, NamedKey::ArrowDown);
        visited.insert(caret(&ed).block);
        press(&mut ed, NamedKey::ArrowRight);
    }
    for n in 1..=5 {
        assert!(visited.contains(&bid(n)), "block {n} never reached by keyboard");
    }
}

// "everything is reachable by keyboard" — the keyboard can also leave the
// editor (no keyboard trap, WCAG 2.1.2): at least one of the usual focus keys
// is left to the host and changes nothing.
#[test]
fn g10_keyboard_can_leave_the_editor() {
    let (mut ed, _f) = open_focused(&doc1("text"));
    let before = model_json(&ed);
    let candidates = [
        (NamedKey::Tab, Modifiers::CTRL),
        (NamedKey::Tab, Modifiers::SHIFT),
        (NamedKey::Escape, Modifiers::NONE),
        (NamedKey::F6, Modifiers::NONE),
        (NamedKey::Tab, Modifiers::NONE),
    ];
    let mut leaves = false;
    for (k, m) in candidates {
        let r = ed.handle_event(InputEvent::Key(KeyEvent { key: Key::Named(k), modifiers: m }));
        if r == EventResult::NotHandled && model_json(&ed) == before {
            leaves = true;
        }
    }
    assert!(leaves, "every focus key is swallowed: keyboard trap");
}

