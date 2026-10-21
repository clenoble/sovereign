//! winit input → the editor's input events, shared by everything that
//! hosts a `sovereign-editor` component in the shell: the document editor
//! (`app/editor_v2.rs`) and the new-thread prompt's name field
//! (every build, contract N4: "the field follows the editor's single-line
//! field rules"). Moved here from `app/editor_v2.rs` so both route keys,
//! input methods and clicks through the same code.

use std::time::{Duration, Instant};

use sovereign_editor::input::{ImeEvent, InputEvent, Key, KeyEvent, Modifiers, NamedKey};
use sovereign_editor::{Editor, EventResult};
use winit::keyboard::{Key as WKey, ModifiersState, NamedKey as WNamed};

/// Clicks closer than this in time and space count as one multi-click.
pub(crate) const MULTI_CLICK: Duration = Duration::from_millis(500);
pub(crate) const MULTI_CLICK_SLOP: f64 = 4.0;

/// winit's modifier state as the editor's.
pub(crate) fn editor_modifiers(m: ModifiersState) -> Modifiers {
    Modifiers { shift: m.shift_key(), ctrl: m.control_key(), alt: m.alt_key(), meta: m.super_key() }
}

/// Where key events go (the editor; a recorder in tests).
pub(crate) trait EventSink {
    fn send(&mut self, event: InputEvent) -> EventResult;
}
impl EventSink for Editor {
    fn send(&mut self, event: InputEvent) -> EventResult {
        self.handle_event(event)
    }
}

/// winit's named keys the editor knows.
pub(crate) fn map_named(k: &WNamed) -> Option<NamedKey> {
    Some(match k {
        WNamed::ArrowLeft => NamedKey::ArrowLeft,
        WNamed::ArrowRight => NamedKey::ArrowRight,
        WNamed::ArrowUp => NamedKey::ArrowUp,
        WNamed::ArrowDown => NamedKey::ArrowDown,
        WNamed::Home => NamedKey::Home,
        WNamed::End => NamedKey::End,
        WNamed::PageUp => NamedKey::PageUp,
        WNamed::PageDown => NamedKey::PageDown,
        WNamed::Backspace => NamedKey::Backspace,
        WNamed::Delete => NamedKey::Delete,
        WNamed::Enter => NamedKey::Enter,
        WNamed::Tab => NamedKey::Tab,
        WNamed::Escape => NamedKey::Escape,
        WNamed::F6 => NamedKey::F6,
        _ => return None,
    })
}

/// One winit key press → the editor, **sending its text at most once**.
///
/// winit gives a key press both a logical key and the text it types (`"\r"`
/// for Enter, `"\t"` for Tab, `"\u{1a}"` for Ctrl+Z, `"é"` after a dead key).
/// The editor acts on keys it knows and inserts what arrives as text, so
/// forwarding both would type Enter twice (`"\n"` + `"\r"`) and insert a tab
/// the editor declined. The rule:
/// - a key the editor knows by name (arrows, Enter, Tab, Backspace, …) is
///   sent as a key and its text is **never** sent, handled or not;
/// - a character key is sent as a key (shortcuts: Ctrl+Z, Ctrl+V, …); if the
///   editor handled it, its text is dropped; if not, the text is sent;
/// - any other key (Space, a dead key, an unidentified key) sends its text only;
/// - text is never sent while Ctrl (without Alt — AltGr is Ctrl+Alt) or the
///   logo key is held, and control characters are dropped.
///
/// Returns whether the editor used the press; a key it left (Esc, Tab, F6) is
/// the shell's.
pub(crate) fn route_key<S: EventSink>(sink: &mut S, key: &WKey, text: Option<&str>, mods: Modifiers) -> EventResult {
    let shortcut_held = (mods.ctrl && !mods.alt) || mods.meta;
    let send_text = |sink: &mut S| -> EventResult {
        if shortcut_held {
            return EventResult::NotHandled;
        }
        let t: String = text.unwrap_or("").chars().filter(|c| !c.is_control()).collect();
        if t.is_empty() {
            EventResult::NotHandled
        } else {
            sink.send(InputEvent::Text(t))
        }
    };
    match key {
        WKey::Named(n) => match map_named(n) {
            Some(n) => sink.send(InputEvent::Key(KeyEvent { key: Key::Named(n), modifiers: mods })),
            None => send_text(sink),
        },
        WKey::Character(c) => {
            let r = sink.send(InputEvent::Key(KeyEvent { key: Key::Character(c.to_string()), modifiers: mods }));
            if r == EventResult::Handled {
                r
            } else {
                send_text(sink)
            }
        }
        _ => send_text(sink),
    }
}

/// winit's input-method event → the editor's.
pub(crate) fn map_ime(ime: winit::event::Ime) -> ImeEvent {
    match ime {
        winit::event::Ime::Enabled => ImeEvent::Enabled,
        winit::event::Ime::Preedit(text, cursor) => ImeEvent::Preedit { text, cursor },
        winit::event::Ime::Commit(text) => ImeEvent::Commit(text),
        winit::event::Ime::Disabled => ImeEvent::Disabled,
    }
}

/// The click count for a press at `(x, y)` at `now`, given the previous press.
pub(crate) fn click_count(prev: Option<(Instant, f64, f64, u8)>, now: Instant, x: f64, y: f64) -> u8 {
    match prev {
        Some((t, px, py, n))
            if now.duration_since(t) <= MULTI_CLICK && (x - px).abs() <= MULTI_CLICK_SLOP && (y - py).abs() <= MULTI_CLICK_SLOP =>
        {
            n.saturating_add(1).min(3)
        }
        _ => 1,
    }
}
