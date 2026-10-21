//! Renaming a thread in its lane label — contract R1–R8 (`doc/plans/THREAD-name.md`,
//! *Renaming a thread — decided 2026-10-18*; spec ch. 03, *Thread deletion and
//! the fallback lane*, and ch. 12).
//!
//! A double-click on a lane's label ([`LabelClicks`], over the label rects of
//! [`crate::canvas::lane_label_rect`]) opens [`LaneRename`] over it: the
//! editor's single-line field (as the new-thread prompt's,
//! `crate::new_thread`), holding the current name, selected. Enter answers
//! [`RenameStep::Save`], which the shell writes through
//! [`save_thread_rename`] — that is, through the one stamped thread-write path
//! ([`crate::thread_writes::ThreadWrites`], THREAD-sync S1) — then reloads.
//! Only lanes that show a live thread have a renamable label ([`lane_labels`]);
//! each is announced as a button ([`lane_label_nodes`]).
//!
//! The signatures are the test author's (stubs); the bodies are the
//! implementer's. Tests: `thread_rename_tests.rs`; list and readings:
//! `crates/sovereign-shell/TESTS-thread-rename.md`; manual checks:
//! `THREAD_CHECKLIST.md` §R.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Instant;

use accesskit::{Node, Role, TreeUpdate};
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_db::DbError;
use sovereign_editor::host::Clipboard;
use sovereign_editor::input::{
    Command, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent,
};
use sovereign_editor::{Editor, Fonts};
use vello::kurbo::{Affine, Point, Rect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::canvas::LANES;
use crate::new_thread::{clean_name, name_field};
use crate::theme::{editor_palette, pal};
use crate::thread_writes::ThreadWrites;
use crate::winit_input::{click_count, route_key};

/// The open field's accessible label (R4) — the new-thread prompt's.
pub(crate) const FIELD_LABEL: &str = crate::new_thread::FIELD_LABEL;

/// A lane label's accessible name is this followed by the thread's name (R4).
pub(crate) const LABEL_BUTTON_PREFIX: &str = "Rename thread ";

/// The field's smallest width (logical px): a short name still gets room to
/// be retyped.
const MIN_FIELD_W: f64 = 260.0;
/// Room the field keeps beyond the label's own width.
const EXTRA_W: f64 = 48.0;
/// The field's margin from the window's edges.
const EDGE: f64 = 8.0;

/// A lane label that can be renamed: the lane, the live thread its label
/// shows, and the name shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LaneLabel {
    pub(crate) lane: usize,
    pub(crate) thread_id: String,
    pub(crate) name: String,
}

/// The renamable lane labels for `threads` (the store's thread list, as the
/// canvas's workspace load reads it): one per lane whose label shows a live
/// thread — the same thread and name the label shows (several threads can
/// share a lane: the label shows one of them). No label for a deleted thread
/// or a marker (R6), nor for the fallback lane, which holds no thread.
///
/// Mirrors `canvas::load_workspace`'s lane names: thread *i* of the live
/// threads is drawn in lane *i* mod [`LANES`]; a later thread with a name
/// takes the label over, one with an empty name leaves it, one whose name is
/// still sealed (no key) shows a lock, which is not renamable.
pub(crate) fn lane_labels(threads: &[Thread]) -> Vec<LaneLabel> {
    let mut per_lane: Vec<Option<LaneLabel>> = vec![None; LANES];
    for (i, t) in threads.iter().filter(|t| t.deleted_at.is_none()).enumerate() {
        let lane = i % LANES;
        if t.name_nonce.is_none() && !t.name.is_empty() {
            per_lane[lane] =
                t.id_string().map(|thread_id| LaneLabel { lane, thread_id, name: t.name.clone() });
        } else if t.name_nonce.is_some() {
            per_lane[lane] = None;
        }
    }
    per_lane.into_iter().flatten().collect()
}

/// One AccessKit node per label, in order: a button named
/// `"Rename thread <name>"` (R4). No action is declared: the shell's
/// AccessKit actions are not wired yet (THREAD-name P3, BACKLOG §3).
pub(crate) fn lane_label_nodes(labels: &[LaneLabel]) -> Vec<Node> {
    labels
        .iter()
        .map(|l| {
            let mut n = Node::new(Role::Button);
            n.set_label(format!("{LABEL_BUTTON_PREFIX}{}", l.name));
            n
        })
        .collect()
}

/// The lane whose label rect contains `p` (shell coordinates), if any.
pub(crate) fn label_at(p: Point, rects: &[(usize, Rect)]) -> Option<usize> {
    rects.iter().find(|(_, r)| r.contains(p)).map(|(lane, _)| *lane)
}

/// Recognises a double-click on a lane label (R1), from the presses the shell
/// sees (`crate::winit_input::click_count` rules).
#[derive(Debug, Default)]
pub(crate) struct LabelClicks {
    last: Option<(Instant, f64, f64, u8, Option<usize>)>,
}

impl LabelClicks {
    /// A left press at `p` at `now`, on the label of lane `hit` (`None`: not on
    /// a label). Returns the lane whose label this press double-clicked.
    pub(crate) fn press(&mut self, hit: Option<usize>, p: Point, now: Instant) -> Option<usize> {
        // A press on another label (or off the labels) starts a new count.
        let prev = self.last.and_then(|(t, x, y, n, h)| (h == hit).then_some((t, x, y, n)));
        let n = click_count(prev, now, p.x, p.y);
        self.last = Some((now, p.x, p.y, n, hit));
        match hit {
            Some(lane) if n == 2 => Some(lane),
            _ => None,
        }
    }
}

/// What the rename field asks the shell to do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameStep {
    /// Keep the field open.
    Open,
    /// Rename thread `thread_id` (the one whose label was double-clicked) to
    /// `name` (trimmed, on one line), then close (R2).
    Save { thread_id: String, name: String },
    /// Close; change nothing (R2).
    Cancel,
}

/// The open rename field, over a lane's label.
pub(crate) struct LaneRename {
    thread_id: String,
    /// The name the field opened with (R8 compares with it).
    opened: String,
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

impl LaneRename {
    /// Opens the field for thread `thread_id`, holding `current_name`, all of
    /// it selected, focused (R1); laid out with `fonts`, copying and pasting
    /// through `clipboard` (R3).
    pub(crate) fn open(thread_id: &str, current_name: &str, fonts: Fonts, clipboard: Box<dyn Clipboard>) -> LaneRename {
        let (mut field, ime_area, field_h) = name_field(fonts, clipboard);
        if !current_name.is_empty() {
            field.handle_event(InputEvent::Text(current_name.to_string()));
        }
        field.command(Command::SelectAll);
        LaneRename {
            thread_id: thread_id.to_string(),
            opened: current_name.to_string(),
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

    /// The thread being renamed.
    pub(crate) fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// The field: the editor's single-line field (R3).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field(&self) -> &Editor {
        &self.field
    }

    /// The field, mutable (the tests place the caret through it).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field_mut(&mut self) -> &mut Editor {
        &mut self.field
    }

    /// The name as typed (committed text only, no input-method pre-edit).
    pub(crate) fn name(&self) -> String {
        self.field.field_text()
    }

    /// One winit key press, with the text winit gives it (`"\r"` for Enter):
    /// Enter saves — or cancels when the name is empty or, once trimmed, the
    /// one the field opened with (R2, R8) — Esc cancels, except while an
    /// input-method composition shows (N7); everything else goes to the field
    /// (R2, R3).
    pub(crate) fn key(&mut self, key: &WKey, text: Option<&str>, mods: Modifiers) -> RenameStep {
        let composing = self.field.is_composing();
        match key {
            WKey::Named(WNamed::Enter) => {
                if composing {
                    // The input method commits the composition; its commit
                    // arrives as its own event (N7, editor U35).
                    return RenameStep::Open;
                }
                let name = clean_name(&self.name());
                if name.is_empty() || name == self.opened {
                    RenameStep::Cancel
                } else {
                    RenameStep::Save { thread_id: self.thread_id.clone(), name }
                }
            }
            WKey::Named(WNamed::Escape) => {
                if composing {
                    // The field ends the composition, committing nothing.
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
                RenameStep::Open
            }
        }
    }

    /// Any other input for the field: text, input-method composition, pointer
    /// (field coordinates), focus (R3).
    pub(crate) fn event(&mut self, event: InputEvent) -> RenameStep {
        self.field.handle_event(event);
        RenameStep::Open
    }

    /// An editing command for the field (copy, cut, paste, select all, undo).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn command(&mut self, command: Command) {
        if matches!(command, Command::Save) {
            return; // the name is saved by Enter
        }
        self.field.command(command);
    }

    /// The field's rect over the label `label` (shell coordinates), in a
    /// `w` × `h` window; also gives the field its width (R1, R3).
    pub(crate) fn layout(&mut self, label: Rect, w: f64, h: f64) -> Rect {
        let avail_w = (w - 2.0 * EDGE).max(1.0);
        let fw = (label.width() + EXTRA_W).max(MIN_FIELD_W).min(avail_w);
        let x0 = label.x0.min(w - EDGE - fw).max(0.0);
        let fh = self.field_h.min((h - 2.0).max(1.0));
        let cy = (label.y0 + label.y1) * 0.5;
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

    /// A primary click at `p` (shell coordinates), after [`Self::layout`]: in
    /// the field, to the field; outside it, [`RenameStep::Cancel`] (R2).
    pub(crate) fn click(&mut self, p: Point) -> RenameStep {
        let Some(r) = self.last else { return RenameStep::Cancel };
        if !r.contains(p) {
            return RenameStep::Cancel;
        }
        let (x, y) = (p.x - r.x0, p.y - r.y0);
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Down {
            x,
            y,
            button: PointerButton::Primary,
            click_count: 1,
            modifiers: Modifiers::NONE,
        }));
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
        RenameStep::Open
    }

    /// The field is closed by any other path: [`RenameStep::Cancel`] (R2).
    pub(crate) fn close(&mut self) -> RenameStep {
        self.field.handle_event(InputEvent::Focus(false));
        RenameStep::Cancel
    }

    /// The field's accessibility tree: a text input labelled
    /// [`FIELD_LABEL`], holding the focus (R4).
    pub(crate) fn accessibility_tree(&mut self) -> TreeUpdate {
        let mut update = self.field.accessibility_tree();
        for (_, n) in update.nodes.iter_mut() {
            if n.role() == Role::TextInput {
                n.set_label(FIELD_LABEL);
            }
        }
        update
    }

    // ---- what only the shell uses (drawing, the pointer, the input method) ----

    /// The display scale the shell draws at (logical → physical).
    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// Whether `p` (shell coordinates) lies in the field, as last laid out.
    pub(crate) fn contains(&self, p: Point) -> bool {
        self.last.is_some_and(|r| r.contains(p))
    }

    /// A left press at `p` (shell coordinates). In the field it goes to the
    /// field (with its click count: a double-click selects a word) and a drag
    /// follows; returns whether it landed in the field.
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

    /// The input method is on while the field is open; its candidate window
    /// goes at the field's caret, in physical pixels (`total`: logical →
    /// physical).
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

    /// Paints the field over the label `label` in a `w` × `h` window.
    pub(crate) fn draw(&mut self, scene: &mut Scene, label: Rect, w: f64, h: f64) {
        let r = self.layout(label, w, h);
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
        scene.stroke(&Stroke::new(1.5), Affine::IDENTITY, p.accent, None, &r);
    }
}

/// What a rename's save did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RenameOutcome {
    /// Written through the stamped thread-write path.
    Written,
    /// Nothing to write: the name is empty once trimmed (R2), or the thread
    /// already has it (R8). No write reached the path.
    Unchanged,
    /// The thread was deleted (or is missing) on this device meanwhile: no
    /// write, the shell closes the field with a notice (R7).
    ThreadGone,
}

/// Renames thread `thread_id` to `name` — trimmed, each line break a space —
/// through the one stamped thread-write path (`writes`, S1), after reading
/// the thread in `db`: an empty name, or the name it already has, writes
/// nothing (R2, R8); a thread deleted or missing here writes nothing — a
/// rename never brings a thread back from this device (R7). A name used by
/// another thread is accepted (R5).
pub(crate) async fn save_thread_rename(
    db: &dyn GraphDB,
    writes: &dyn ThreadWrites,
    thread_id: &str,
    name: &str,
) -> Result<RenameOutcome, String> {
    let name = clean_name(name);
    if name.is_empty() {
        return Ok(RenameOutcome::Unchanged);
    }
    let thread = match db.get_thread(thread_id).await {
        Ok(t) => t,
        Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => return Ok(RenameOutcome::ThreadGone),
        Err(e) => return Err(e.to_string()),
    };
    if thread.deleted_at.is_some() {
        return Ok(RenameOutcome::ThreadGone);
    }
    if thread.name == name {
        return Ok(RenameOutcome::Unchanged);
    }
    writes.rename_thread(thread_id, &name).await?;
    Ok(RenameOutcome::Written)
}
