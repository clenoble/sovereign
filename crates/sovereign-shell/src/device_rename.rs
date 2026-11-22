//! Renaming a device in Settings → Devices (v0.0.12 item 19,
//! `doc/plans/V0012-items14-19-devices.md`, N3, N5, N6).
//!
//! A Rename action opens [`DeviceRename`] over the name it renames: the
//! editor's single-line field (as the lane rename's, `crate::lane_rename`),
//! holding the current name, all of it selected. Enter saves — the name
//! cleaned by N2's rules ([`crate::device_name::clean`]) — and closes; Escape
//! cancels and closes; a blank entry is refused with a message, and the field
//! stays open. The shell does the save ([`RenameStep::Save`]): this device's
//! own name in its workspace file (N3), a paired device's label in this
//! device's paired list (N5).

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use accesskit::{Role, TreeUpdate};
use sovereign_editor::host::Clipboard;
use sovereign_editor::input::{Command, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent};
use sovereign_editor::{Editor, Fonts};
use vello::kurbo::{Affine, Point, Rect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::new_thread::name_field;
use crate::text::{draw_text, TextShaper};
use crate::theme::{editor_palette, pal};
use crate::winit_input::{click_count, route_key};

/// What a blank entry says (N3: "refused with a message").
pub(crate) const BLANK_REFUSED: &str = "Enter a name, or press Escape to keep the current one.";

/// The field's smallest width (logical px).
const MIN_FIELD_W: f64 = 260.0;
/// Room the field keeps beyond the name's own width.
const EXTRA_W: f64 = 48.0;
/// The field's margin from the window's edges.
const EDGE: f64 = 8.0;

/// What a Rename action renames.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameTarget {
    /// This device's own name (N3).
    ThisDevice,
    /// A paired device's label on this device (N5), by its peer id.
    Paired(String),
}

/// What the field asks the shell to do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameStep {
    /// Keep the field open.
    Open,
    /// Save `name` (cleaned by N2's rules) for `target`, then close.
    Save { target: RenameTarget, name: String },
    /// Close; change nothing.
    Cancel,
}

/// The open field: the editor's single-line field.
pub(crate) struct DeviceRename {
    target: RenameTarget,
    /// The field's accessible name (N6): "This device's name", or
    /// "Name of {current name}".
    label: String,
    field: Editor,
    ime_area: Rc<Cell<Option<sovereign_editor::Rect>>>,
    field_h: f64,
    scale: f64,
    /// The message a refused entry shows, until the next edit.
    message: Option<String>,
    last: Option<Rect>,
    viewport: (f64, f64, f64),
    pointer_down: bool,
    last_press: Option<(Instant, f64, f64, u8)>,
    scene: Scene,
}

impl DeviceRename {
    /// Opens the field for `target`, holding `current_name`, all of it
    /// selected, focused; laid out with `fonts`, copying and pasting through
    /// `clipboard`.
    pub(crate) fn open(target: RenameTarget, current_name: &str, fonts: Fonts, clipboard: Box<dyn Clipboard>) -> DeviceRename {
        let (mut field, ime_area, field_h) = name_field(fonts, clipboard);
        if !current_name.is_empty() {
            field.handle_event(InputEvent::Text(current_name.to_string()));
        }
        field.command(Command::SelectAll);
        let label = match &target {
            RenameTarget::ThisDevice => crate::device_name::FIELD_LABEL.to_string(),
            RenameTarget::Paired(_) => format!("Name of {current_name}"),
        };
        DeviceRename {
            target,
            label,
            field,
            ime_area,
            field_h,
            scale: 1.0,
            message: None,
            last: None,
            viewport: (0.0, 0.0, 0.0),
            pointer_down: false,
            last_press: None,
            scene: Scene::new(),
        }
    }

    /// What the field renames.
    pub(crate) fn target(&self) -> &RenameTarget {
        &self.target
    }

    /// The name as typed (committed text).
    pub(crate) fn name(&self) -> String {
        self.field.field_text()
    }

    /// The message a refused entry shows (a blank one), if any.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// One winit key press, with the text winit gives it (`"\r"` for Enter):
    /// Enter saves the cleaned name, or refuses a blank one with a message;
    /// Escape cancels; both only end an input-method composition first;
    /// everything else goes to the field.
    pub(crate) fn key(&mut self, key: &WKey, text: Option<&str>, mods: Modifiers) -> RenameStep {
        let composing = self.field.is_composing();
        match key {
            WKey::Named(WNamed::Enter) => {
                if composing {
                    return RenameStep::Open;
                }
                match crate::device_name::clean(&self.name()) {
                    Some(name) => RenameStep::Save { target: self.target.clone(), name },
                    None => {
                        self.message = Some(BLANK_REFUSED.to_string());
                        RenameStep::Open
                    }
                }
            }
            WKey::Named(WNamed::Escape) => {
                if composing {
                    self.field.handle_event(InputEvent::Key(KeyEvent {
                        key: Key::Named(NamedKey::Escape),
                        modifiers: mods,
                    }));
                    return RenameStep::Open;
                }
                RenameStep::Cancel
            }
            _ => {
                route_key(&mut self.field, key, text, mods);
                if !self.name().trim().is_empty() {
                    self.message = None;
                }
                RenameStep::Open
            }
        }
    }

    /// Any other input for the field: text, input-method composition, focus.
    pub(crate) fn event(&mut self, event: InputEvent) -> RenameStep {
        self.field.handle_event(event);
        RenameStep::Open
    }

    /// The field's rect over `anchor` (the name it renames, shell
    /// coordinates), in a `w` × `h` window; also gives the field its width.
    pub(crate) fn layout(&mut self, anchor: Rect, w: f64, h: f64) -> Rect {
        let avail_w = (w - 2.0 * EDGE).max(1.0);
        let fw = (anchor.width() + EXTRA_W).max(MIN_FIELD_W).min(avail_w);
        let x0 = anchor.x0.min(w - EDGE - fw).max(0.0);
        let fh = self.field_h.min((h - 2.0).max(1.0));
        let cy = (anchor.y0 + anchor.y1) * 0.5;
        let y0 = (cy - fh * 0.5).min(h - fh).max(0.0);
        let r = Rect::new(x0, y0, (x0 + fw).min(w), y0 + fh);
        let vp = (r.width(), r.height(), self.scale);
        if vp != self.viewport {
            self.viewport = vp;
            self.field.set_viewport(vp.0, vp.1, vp.2);
        }
        self.last = Some(r);
        r
    }

    /// The display scale the shell draws at (logical → physical).
    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Whether `p` (shell coordinates) lies in the field, as last laid out.
    pub(crate) fn contains(&self, p: Point) -> bool {
        self.last.is_some_and(|r| r.contains(p))
    }

    /// A left press at `p`: in the field it goes to the field (with its click
    /// count) and a drag follows; returns whether it landed in the field.
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

    /// The pointer moved to `p`: extends a drag begun in the field.
    pub(crate) fn drag(&mut self, p: Point) {
        let Some(r) = self.last else { return };
        if self.pointer_down {
            self.field.handle_event(InputEvent::Pointer(PointerEvent::Move { x: p.x - r.x0, y: p.y - r.y0 }));
        }
    }

    /// The left button was released at `p`.
    pub(crate) fn release(&mut self, p: Point) {
        let Some(r) = self.last else { return };
        if !std::mem::take(&mut self.pointer_down) {
            return;
        }
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Up {
            x: p.x - r.x0,
            y: p.y - r.y0,
            button: PointerButton::Primary,
        }));
    }

    /// The field is closed by any other path.
    pub(crate) fn close(&mut self) {
        self.field.handle_event(InputEvent::Focus(false));
    }

    /// The field's accessibility tree: a text input named by [`Self::label`]
    /// (N6), holding the focus; a refused entry's message as its description.
    pub(crate) fn accessibility_tree(&mut self) -> TreeUpdate {
        let mut update = self.field.accessibility_tree();
        for (_, n) in update.nodes.iter_mut() {
            if n.role() == Role::TextInput {
                n.set_label(self.label.clone());
                if let Some(m) = &self.message {
                    n.set_description(m.clone());
                }
            }
        }
        update
    }

    /// The input method is on while the field is open; its candidate window
    /// goes at the field's caret, in physical pixels.
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

    /// Paints the field over `anchor` in a `w` × `h` window, and a refused
    /// entry's message under it.
    pub(crate) fn draw(&mut self, scene: &mut Scene, shaper: &mut TextShaper, anchor: Rect, w: f64, h: f64) {
        let r = self.layout(anchor, w, h);
        let p = pal();
        let shadow = vello::kurbo::RoundedRect::from_rect(r + vello::kurbo::Vec2::new(0.0, 2.0), 6.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, p.scrim.with_alpha(0.25), None, &shadow);
        let mut ep = editor_palette(&p);
        ep.background = p.panel;
        self.field.set_palette(ep);
        self.scene.reset();
        self.field.paint(&mut self.scene);
        scene.fill(Fill::NonZero, Affine::IDENTITY, p.panel, None, &r);
        scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &r);
        scene.append(&self.scene, Some(Affine::translate((r.x0, r.y0))));
        scene.pop_layer();
        scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, crate::theme::focused_field_border(&p), None, &r);
        if let Some(m) = &self.message {
            let l = shaper.shape(m, r.width().max(120.0) as f32, 12.0);
            let box_r = Rect::new(r.x0, r.y1 + 2.0, r.x0 + l.width() as f64 + 16.0, r.y1 + 8.0 + l.height() as f64);
            scene.fill(Fill::NonZero, Affine::IDENTITY, p.panel, None, &box_r);
            draw_text(scene, &l, Affine::translate((r.x0 + 8.0, r.y1 + 5.0)), p.accent_warm);
        }
    }
}
