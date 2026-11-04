//! The chat composer and the search query — contract K1–K8
//! (`doc/plans/V0012-item21-composer.md`, v0.0.12 item 21).
//!
//! Each is the editor's single-line field ([`Editor::open_field`] with
//! [`sovereign_editor::FieldKind::SingleLine`], built by
//! [`crate::new_thread::name_field`]), as the new-thread name and the lane
//! rename already are: caret, selection, Delete, word and line movement, undo,
//! clipboard, input methods, bidi and horizontal scroll follow the title's
//! rules (K1). Keys go through the shell's shared winit routing
//! (`crate::winit_input`), copies through [`crate::clip::FieldClipboard`]
//! (K2). The field answers each input with a [`FieldStep`], which `App` only
//! routes: send, close, or refresh the search results.
//!
//! Tests (written before the code): `chat_field_tests.rs`; K3b on the field
//! itself: `sovereign-editor/tests/f_field_enter_composing.rs`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use accesskit::{Role, TreeUpdate};
use sovereign_editor::host::Clipboard;
use sovereign_editor::input::{
    Command, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent,
};
use sovereign_editor::{Editor, EventResult, Fonts};
use vello::kurbo::{Affine, Point, Rect};
use vello::peniko::{Color, Fill};
use vello::Scene;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::new_thread::name_field;
use crate::theme::{editor_palette, pal};
use crate::winit_input::{click_count, route_key};

/// The composer's visible placeholder, unchanged (K5).
pub(crate) const MESSAGE_PLACEHOLDER: &str = "Type a message\u{2026}   (Enter to send)";
/// The search field's visible placeholder, unchanged (K5).
pub(crate) const SEARCH_PLACEHOLDER: &str = "Search documents by title\u{2026}  (Enter to ask the AI)";

/// Which window the field belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldMode {
    Message,
    Search,
}

impl FieldMode {
    /// The field's accessible name (K5).
    fn label(self) -> &'static str {
        match self {
            FieldMode::Message => "Message",
            FieldMode::Search => "Search",
        }
    }

    fn placeholder(self) -> &'static str {
        match self {
            FieldMode::Message => MESSAGE_PLACEHOLDER,
            FieldMode::Search => SEARCH_PLACEHOLDER,
        }
    }
}

/// What the field asks `App` to do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FieldStep {
    /// Nothing for `App` to do.
    None,
    /// Message: send this text (trimmed). Search: open a chat with this
    /// query (trimmed) sent (K3).
    Send(String),
    /// Close the window (K3: Escape; a blank search query on Enter).
    Close,
    /// Search only: the query changed to this text; refresh the results (K4).
    QueryChanged(String),
}

/// The composer, or the search query: the editor's single-line field.
pub(crate) struct ChatField {
    mode: FieldMode,
    field: Editor,
    ime_area: Rc<Cell<Option<sovereign_editor::Rect>>>,
    /// The field's height: its line plus the editor's margins.
    field_h: f64,
    /// The display scale the shell draws at (passed to the field's viewport).
    scale: f64,
    /// The last layout (shell coordinates), and the viewport set from it.
    last: Option<Rect>,
    viewport: (f64, f64, f64),
    /// A press in the field, until its release (a drag selects).
    pointer_down: bool,
    last_press: Option<(Instant, f64, f64, u8)>,
    scene: Scene,
}

impl ChatField {
    /// Opens the field, empty and focused, laid out with `fonts`, copying and
    /// pasting through `clipboard` (K1, K2).
    pub(crate) fn open(mode: FieldMode, fonts: Fonts, clipboard: Box<dyn Clipboard>) -> ChatField {
        let (field, ime_area, field_h) = name_field(fonts, clipboard);
        ChatField {
            mode,
            field,
            ime_area,
            field_h,
            scale: 1.0,
            last: None,
            viewport: (0.0, 0.0, 0.0),
            pointer_down: false,
            last_press: None,
            scene: Scene::new(),
        }
    }

    /// The field's committed text (no input-method pre-edit).
    pub(crate) fn text(&self) -> String {
        self.field.field_text()
    }

    /// The field: the editor's single-line field (K1).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field(&self) -> &Editor {
        &self.field
    }

    /// Whether the field shows its placeholder: no text and no composition.
    pub(crate) fn shows_placeholder(&self) -> bool {
        self.text().is_empty() && !self.field.is_composing()
    }

    /// One winit key press, with the text winit gives it (`"\r"` for Enter),
    /// through `crate::winit_input` (K2). Enter sends (K3) — except during a
    /// composition, when the field keeps it (K3b); Esc closes, except during
    /// a composition, which it only ends (N7); a search edit refreshes the
    /// results (K4).
    pub(crate) fn key(&mut self, key: &WKey, text: Option<&str>, mods: Modifiers) -> FieldStep {
        let before = self.text();
        let used = route_key(&mut self.field, key, text, mods);
        match key {
            // K3b: an Enter during a composition stayed in the field.
            WKey::Named(WNamed::Enter) if used == EventResult::Handled => FieldStep::None,
            WKey::Named(WNamed::Enter) => self.enter(),
            // N7: the field ended the composition, committing nothing.
            WKey::Named(WNamed::Escape) if used == EventResult::Handled => FieldStep::None,
            WKey::Named(WNamed::Escape) => FieldStep::Close,
            _ => self.changed(&before),
        }
    }

    /// Any other input for the field: text, input method, pointer (field
    /// coordinates), focus (K2, K4).
    pub(crate) fn event(&mut self, event: InputEvent) -> FieldStep {
        let before = self.text();
        self.field.handle_event(event);
        self.changed(&before)
    }

    /// The field's AccessKit tree: a text input named "Message" or "Search",
    /// holding the focus, with its value and selection (K5); the visible
    /// placeholder as its placeholder while it is empty.
    pub(crate) fn accessibility_tree(&mut self) -> TreeUpdate {
        let empty = self.text().is_empty();
        let mut update = self.field.accessibility_tree();
        for (_, n) in update.nodes.iter_mut() {
            if n.role() == Role::TextInput {
                n.set_label(self.mode.label());
                if empty {
                    n.set_placeholder(self.mode.placeholder());
                }
            }
        }
        update
    }

    /// Enter with no composition (K3): the composer sends its trimmed text
    /// and empties; the search opens a chat with its trimmed query, or only
    /// closes when it is blank.
    fn enter(&mut self) -> FieldStep {
        let text = self.text().trim().to_string();
        match self.mode {
            FieldMode::Message if text.is_empty() => FieldStep::None,
            FieldMode::Message => {
                self.clear();
                FieldStep::Send(text)
            }
            FieldMode::Search if text.is_empty() => FieldStep::Close,
            FieldMode::Search => FieldStep::Send(text),
        }
    }

    /// Search only: `QueryChanged` when the text is no longer `before` (K4).
    /// The composer's edits ask `App` for nothing (K7).
    fn changed(&self, before: &str) -> FieldStep {
        match self.mode {
            FieldMode::Search => {
                let now = self.text();
                if now != before {
                    FieldStep::QueryChanged(now)
                } else {
                    FieldStep::None
                }
            }
            FieldMode::Message => FieldStep::None,
        }
    }

    /// Empties the field, through the field's own editing (its undo history
    /// stays in memory, dropped with the field, K7).
    fn clear(&mut self) {
        self.field.command(Command::SelectAll);
        self.field.handle_event(InputEvent::Key(KeyEvent {
            key: Key::Named(NamedKey::Backspace),
            modifiers: Modifiers::NONE,
        }));
    }

    // ---- what only the shell uses (drawing, the pointer, the input method) ----

    /// The display scale the shell draws at (logical → physical).
    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// The field's rect inside the input box `bx` (shell coordinates): the
    /// box's width, its line centred in it; also gives the field its viewport.
    fn layout(&mut self, bx: Rect) -> Rect {
        let fh = self.field_h.min(bx.height()).max(1.0);
        let y0 = bx.y0 + (bx.height() - fh) * 0.5;
        let r = Rect::new(bx.x0, y0, bx.x1.max(bx.x0 + 1.0), y0 + fh);
        let vp = (r.width(), r.height(), self.scale);
        if vp != self.viewport {
            self.viewport = vp;
            self.field.set_viewport(vp.0, vp.1, vp.2);
        }
        self.last = Some(r);
        r
    }

    /// Paints the field in the input box `bx` (the box's fill, `background`,
    /// and its border are the panel's): its text, caret, selection and
    /// pre-edit, clipped to the box (K2).
    pub(crate) fn draw(&mut self, scene: &mut Scene, bx: Rect, background: Color) {
        let r = self.layout(bx);
        let mut ep = editor_palette(&pal());
        ep.background = background;
        self.field.set_palette(ep);
        self.scene.reset();
        self.field.paint(&mut self.scene);
        scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &bx);
        scene.append(&self.scene, Some(Affine::translate((r.x0, r.y0))));
        scene.pop_layer();
    }

    /// A left press at `p` (shell coordinates), after a draw. In the field it
    /// goes to the field (with its click count: a double-click selects a
    /// word) and a drag follows; returns whether it landed in the field.
    pub(crate) fn press(&mut self, p: Point, mods: Modifiers) -> bool {
        let Some(r) = self.last else { return false };
        if !r.contains(p) {
            return false;
        }
        let (x, y) = (p.x - r.x0, p.y - r.y0);
        let now = Instant::now();
        let n = click_count(self.last_press, now, x, y);
        self.last_press = Some((now, x, y, n));
        self.pointer_down = true;
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: n,
            modifiers: mods,
        }));
        true
    }

    /// The pointer moved to `p` (shell coordinates): extends a drag begun in
    /// the field.
    pub(crate) fn drag(&mut self, p: Point) {
        let Some(r) = self.last else { return };
        if self.pointer_down {
            self.field.handle_event(InputEvent::Pointer(PointerEvent::Move { x: p.x - r.x0, y: p.y - r.y0 }));
        }
    }

    /// The left button was released at `p`; returns whether a press in the
    /// field was under way.
    pub(crate) fn release(&mut self, p: Point) -> bool {
        let Some(r) = self.last else { return false };
        if !std::mem::take(&mut self.pointer_down) {
            return false;
        }
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Up {
            x: p.x - r.x0,
            y: p.y - r.y0,
            button: PointerButton::Primary,
        }));
        true
    }

    /// The input method is on while the field's window is on top; its
    /// candidate window goes at the field's caret, in physical pixels
    /// (`total`: logical → physical) (K2).
    pub(crate) fn ime_request(&self, total: f64) -> (bool, Option<(i32, i32, u32, u32)>) {
        let area = match (self.last, self.ime_area.get()) {
            (Some(r), Some(a)) => {
                let x = ((r.x0 + a.x0) * total).round() as i32;
                let y = ((r.y0 + a.y0) * total).round() as i32;
                let w = (((a.x1 - a.x0) * total).round() as u32).max(1);
                let h = (((a.y1 - a.y0) * total).round() as u32).max(1);
                Some((x, y, w, h))
            }
            _ => None,
        };
        (true, area)
    }
}
