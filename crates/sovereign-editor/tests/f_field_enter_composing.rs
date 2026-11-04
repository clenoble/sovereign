//! v0.0.12 item 21, K3b (`doc/plans/V0012-item21-composer.md`, amended
//! 2026-10-23 on the laptop's 0130): "a single-line field keeps an Enter
//! pressed while a pre-edit is showing (`Handled`, nothing else happens), and
//! the host sees only the next one."
//!
//! Written before the code, by another author than the implementer: whoever
//! builds K3b makes these pass without editing this file. The hosts' half (the
//! rename, the new-thread name, the composer) is in the shell's
//! `field_enter_tests.rs` and `chat_field_tests.rs`. Whether winit delivers
//! that Enter at all under a real input method is a manual check.

mod common;
use common::*;

use sovereign_editor::input::{ImeEvent, Modifiers, NamedKey};
use sovereign_editor::{Editor, EventResult, FieldKind};

/// A single-line field, focused, holding `text`, the caret at its end.
fn field_with(text: &str) -> (Editor, Fakes) {
    let (host, f) = host();
    let mut field = Editor::open_field(FieldKind::SingleLine, config(), host);
    field.set_viewport(600.0, 60.0, 1.0);
    field.handle_event(sovereign_editor::input::InputEvent::Focus(true));
    type_text(&mut field, text);
    (field, f)
}

// K3b: an Enter while a pre-edit shows is the field's (Handled); the text, the
// one line and the composition are as they were; nothing is asked of the host.
#[test]
fn k3b_enter_during_a_preedit_is_handled_and_changes_nothing() {
    let (mut field, f) = field_with("Hi ");
    ime(&mut field, ImeEvent::Enabled);
    preedit(&mut field, "にほ");
    assert!(field.is_composing(), "the pre-edit shows");
    let saves_before = save_count(&f);

    assert_eq!(key(&mut field, NamedKey::Enter, Modifiers::NONE), EventResult::Handled, "the field keeps the Enter");
    assert_eq!(field.field_text(), "Hi ", "no text is committed or lost by the Enter");
    assert_eq!(field.document().blocks().len(), 1, "no line break: still one line");
    assert!(field.is_composing(), "the composition is still the input method's to commit");
    assert_eq!(save_count(&f), saves_before, "a field asks for no save");
}

// K3b: "the host sees only the next one" — once the input method has
// committed, the next Enter is NotHandled (the host's), and the text is the
// committed one, inserted once.
#[test]
fn k3b_the_enter_after_the_commit_is_the_hosts() {
    let (mut field, _f) = field_with("Hi ");
    ime(&mut field, ImeEvent::Enabled);
    preedit(&mut field, "にほ");
    assert_eq!(key(&mut field, NamedKey::Enter, Modifiers::NONE), EventResult::Handled);
    // The input method ends its composition and commits.
    ime(&mut field, ImeEvent::Preedit { text: String::new(), cursor: None });
    ime(&mut field, ImeEvent::Commit("日本".into()));
    assert!(!field.is_composing());
    assert_eq!(key(&mut field, NamedKey::Enter, Modifiers::NONE), EventResult::NotHandled, "the next Enter goes to the host");
    assert_eq!(field.field_text(), "Hi 日本", "the commit is inserted once");
}

// K3b, the other end of the composition: a pre-edit the input method cleared
// without committing leaves no composition, so the next Enter is the host's.
#[test]
fn k3b_a_cleared_preedit_gives_enter_back_to_the_host() {
    let (mut field, _f) = field_with("Hi");
    preedit(&mut field, "にほ");
    ime(&mut field, ImeEvent::Preedit { text: String::new(), cursor: None });
    assert_eq!(key(&mut field, NamedKey::Enter, Modifiers::NONE), EventResult::NotHandled);
    assert_eq!(field.field_text(), "Hi");
}

// Unchanged by K3b: with no composition, Enter in a single-line field is the
// host's, as today (N2, R2 depend on it).
#[test]
fn k3b_without_a_composition_enter_is_the_hosts() {
    let (mut field, _f) = field_with("Hi");
    assert_eq!(key(&mut field, NamedKey::Enter, Modifiers::NONE), EventResult::NotHandled);
    assert_eq!(field.field_text(), "Hi");
}
