//! v0.0.12 item 21, K3b (`doc/plans/V0012-item21-composer.md`, amended
//! 2026-10-23 on the laptop's 0130): a single-line field keeps an Enter
//! pressed while a pre-edit is showing, and the host sees only the next one —
//! "one [test] through each of the rename and the new-thread seams, so that
//! the three hosts inherit it". The field's own test is in
//! `sovereign-editor/tests/f_field_enter_composing.rs`; the composer's is in
//! `chat_field_tests.rs`.
//!
//! Written before the code, by another author than the implementer: whoever
//! builds K3b makes these pass without editing this file.

use std::cell::RefCell;
use std::rc::Rc;

use sovereign_editor::host::{Clipboard, ClipboardContent, ClipboardWrite};
use sovereign_editor::input::{ImeEvent, InputEvent, Modifiers};
use sovereign_editor::{EventResult, Fonts};
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::lane_rename::{LaneRename, RenameStep};
use crate::new_thread::{name_field, NewThreadPrompt, PromptStep};
use crate::winit_input::route_key;

const TEST_FONT: &[u8] = include_bytes!("../../sovereign-editor/tests/fixtures/Cantarell-Regular.ttf");

#[derive(Clone, Default)]
struct MemClip(Rc<RefCell<Option<ClipboardContent>>>);
impl Clipboard for MemClip {
    fn read(&mut self) -> Option<ClipboardContent> {
        self.0.borrow().clone()
    }
    fn write(&mut self, item: ClipboardWrite) {
        *self.0.borrow_mut() = Some(ClipboardContent { text: item.text, sovereign: item.sovereign });
    }
}

fn fonts() -> Fonts {
    Fonts::Only(vec![TEST_FONT.to_vec()])
}

fn enter_key() -> WKey {
    WKey::Named(WNamed::Enter)
}

fn compose(event: &mut dyn FnMut(InputEvent)) {
    event(InputEvent::Ime(ImeEvent::Enabled));
    event(InputEvent::Ime(ImeEvent::Preedit { text: "にほ".into(), cursor: Some((6, 6)) }));
}

fn commit(event: &mut dyn FnMut(InputEvent)) {
    event(InputEvent::Ime(ImeEvent::Preedit { text: String::new(), cursor: None }));
    event(InputEvent::Ime(ImeEvent::Commit("日本".into())));
}

// K3b through the shell's shared winit routing (K2): winit's Enter, routed to
// a single-line field while a pre-edit shows, is used by the field; once the
// composition is committed, the next Enter is left to the host.
#[test]
fn k3b_routed_enter_during_a_preedit_stays_in_the_field() {
    let (mut field, _ime, _h) = name_field(fonts(), Box::new(MemClip::default()));
    field.handle_event(InputEvent::Text("Ideas ".into()));
    compose(&mut |e| {
        field.handle_event(e);
    });
    assert_eq!(
        route_key(&mut field, &enter_key(), Some("\r"), Modifiers::NONE),
        EventResult::Handled,
        "the field keeps an Enter typed during a composition"
    );
    assert_eq!(field.field_text(), "Ideas ", "and nothing else happens");
    commit(&mut |e| {
        field.handle_event(e);
    });
    assert_eq!(
        route_key(&mut field, &enter_key(), Some("\r"), Modifiers::NONE),
        EventResult::NotHandled,
        "the next Enter is the host's"
    );
    assert_eq!(field.field_text(), "Ideas 日本");
}

// K3b through the rename seam (R3): an Enter during a composition saves
// nothing; the next one saves the committed name.
#[test]
fn k3b_rename_enter_during_a_preedit_does_not_save() {
    let mut r = LaneRename::open("thread:t1", "", fonts(), Box::new(MemClip::default()));
    r.event(InputEvent::Text("Ideas ".into()));
    compose(&mut |e| {
        r.event(e);
    });
    assert_eq!(r.key(&enter_key(), Some("\r"), Modifiers::NONE), RenameStep::Open, "no save during the composition");
    assert_eq!(r.name(), "Ideas ");
    commit(&mut |e| {
        r.event(e);
    });
    assert_eq!(
        r.key(&enter_key(), Some("\r"), Modifiers::NONE),
        RenameStep::Save { thread_id: "thread:t1".into(), name: "Ideas 日本".into() },
        "the next Enter saves the committed name"
    );
}

// K3b through the new-thread seam (N4, N7): an Enter during a composition
// creates nothing; the next one creates the thread with the committed name.
#[test]
fn k3b_new_thread_enter_during_a_preedit_does_not_create() {
    let mut p = NewThreadPrompt::open(fonts(), Box::new(MemClip::default()));
    p.event(InputEvent::Text("Ideas ".into()));
    compose(&mut |e| {
        p.event(e);
    });
    assert_eq!(p.key(&enter_key(), Some("\r"), Modifiers::NONE), PromptStep::Open, "nothing created during the composition");
    assert_eq!(p.name(), "Ideas ");
    commit(&mut |e| {
        p.event(e);
    });
    assert_eq!(
        p.key(&enter_key(), Some("\r"), Modifiers::NONE),
        PromptStep::Create("Ideas 日本".into()),
        "the next Enter creates the thread"
    );
}
