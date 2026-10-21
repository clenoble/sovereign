//! Naming a new thread — contract N1–N7 (`doc/plans/THREAD-name.md`, spec ch. 12,
//! *New thread — named first, as in the Tauri desktop*).
//!
//! The new-thread gestures (the canvas menu's *New lane*, which the `+` button
//! opens) open this prompt: one single-line name field — the editor's own field
//! ([`Editor::open_field`] with [`sovereign_editor::FieldKind::SingleLine`]), so
//! caret, selection, clipboard, input methods, bidi and horizontal scroll follow
//! the title's rules (N4) — and a *Create* button. The prompt owns no store:
//! the thread is created by [`create_named_thread`] when the prompt answers
//! [`PromptStep::Create`], and only then (N1, N3).
//!
//! It exists in every build (the `+` and the canvas menu do), as the document
//! editor does: keys go through the shell's shared winit routing
//! (`crate::winit_input`), copies through [`crate::clip::FieldClipboard`].
//! Its AccessKit tree is a tree of its own, a dialog, which the shell inlines
//! in its tree under its own id range ([`inline_tree`], [`PROMPT_NS`]).
//!
//! Tests (written before the code): `thread_name_tests.rs`; list and readings:
//! `crates/sovereign-shell/TESTS-thread-name.md`; manual checks:
//! `THREAD_CHECKLIST.md`.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use accesskit::{Action, Node, NodeId, Role, TextPosition, TextSelection, Tree, TreeId, TreeUpdate};
use parley::Layout;
use sovereign_db::traits::GraphDB;
use sovereign_editor::host::{
    AccessibilitySink, Clipboard, Clock, DocumentService, Host, PiiDisplay, SaveRequest, TokenDisplay, WindowHost,
};
use sovereign_editor::input::{Command, ImeEvent, InputEvent, Key, KeyEvent, Modifiers, NamedKey, PointerButton, PointerEvent};
use sovereign_editor::{Editor, EditorConfig, FieldKind, Fonts};
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key as WKey, NamedKey as WNamed};

use crate::canvas::BOTTOM_CHROME;
use crate::text::{draw_text, Brush, TextShaper};
use crate::theme::{editor_palette, pal};
use crate::winit_input::{click_count, route_key};

/// The field's placeholder while it is empty (N1).
pub(crate) const PLACEHOLDER: &str = "Thread name\u{2026}";
/// The prompt's accessible name: it is an AccessKit dialog (N5).
pub(crate) const DIALOG_LABEL: &str = "New thread";
/// The field's accessible label (N5).
pub(crate) const FIELD_LABEL: &str = "Thread name";
/// The button's label and accessible name (N1, N5).
pub(crate) const CREATE_LABEL: &str = "Create";

/// What the prompt asks the shell to do after an input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PromptStep {
    /// Keep the prompt open; nothing to create.
    Open,
    /// Create a thread with this name (already trimmed and on one line), then
    /// close the prompt (N2).
    Create(String),
    /// Close the prompt; create nothing (N3).
    Cancel,
}

/// Where the prompt's parts are, in the shell's logical coordinates, for a
/// window of the given size. Shared by drawing and hit-testing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PromptLayout {
    /// The whole prompt; a click outside it cancels (N3).
    pub(crate) bounds: Rect,
    /// The name field.
    pub(crate) field: Rect,
    /// The *Create* button.
    pub(crate) create: Rect,
}

// ---- geometry ----------------------------------------------------------------

/// The prompt's width (logical px), as the Tauri popup: a small field and its
/// button, bottom centre, above the `+`.
const PROMPT_W: f64 = 380.0;
/// Inner margin of the prompt around the field and the button.
const INSET: f64 = 10.0;
const GAP: f64 = 8.0;
const BUTTON_W: f64 = 84.0;
/// Above the `+` button (its disc spans 20–64 px above the bottom chrome).
const ABOVE_CHROME: f64 = 76.0;
/// The field's height before it is measured (and if it cannot be).
const FALLBACK_FIELD_H: f64 = 44.0;
/// The editor's text size (`sovereign-editor`, view::FONT_SIZE), for the
/// placeholder drawn in the field while it is empty.
const FIELD_FONT_PX: f32 = 16.0;
/// The editor's inner margin in a field (view::PAD).
const FIELD_PAD: f64 = 12.0;

// ---- the field's host --------------------------------------------------------

/// A field never saves (`open_field` never asks the document service).
struct NoSaves;
impl DocumentService for NoSaves {
    fn save(&mut self, _request: SaveRequest) {}
}

/// The prompt reads the field's whole tree when the shell asks for it
/// ([`NewThreadPrompt::accessibility_tree`]); incremental updates are dropped.
struct NoUpdates;
impl AccessibilitySink for NoUpdates {
    fn update(&mut self, _update: TreeUpdate) {}
}

struct MonoClock;
impl Clock for MonoClock {
    fn now(&self) -> Duration {
        static ORIGIN: OnceLock<Instant> = OnceLock::new();
        ORIGIN.get_or_init(Instant::now).elapsed()
    }
}

/// A thread name resolves no personal-data tokens: it is stored as typed.
struct NoTokens;
impl PiiDisplay for NoTokens {
    fn display(&self, _record_id: &str) -> TokenDisplay {
        TokenDisplay::NotAToken
    }
    fn link_to_document(&mut self, _record_ids: &[String]) {}
}

/// Keeps the field's input-method caret area (field coordinates).
struct FieldWindow(Rc<Cell<Option<sovereign_editor::Rect>>>);
impl WindowHost for FieldWindow {
    fn set_ime_cursor_area(&mut self, area: sovereign_editor::Rect) {
        self.0.set(Some(area));
    }
    fn request_redraw(&mut self) {
        // The shell redraws every frame.
    }
}

// ---- the prompt --------------------------------------------------------------

/// Which part of the prompt has the keyboard. Tab moves between them (a
/// keyboard user reaches *Create*, THREAD_CHECKLIST §6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Part {
    Field,
    Create,
}

/// The open new-thread prompt.
pub(crate) struct NewThreadPrompt {
    field: Editor,
    ime_area: Rc<Cell<Option<sovereign_editor::Rect>>>,
    focus: Part,
    /// The field's height: its line plus the editor's margins.
    field_h: f64,
    /// The display scale the shell draws at (passed to the field's viewport).
    scale: f64,
    /// The last layout, and the field viewport set from it.
    last: Option<PromptLayout>,
    viewport: (f64, f64, f64),
    /// A press in the field, until its release (a drag selects).
    pointer_down: bool,
    last_press: Option<(Instant, f64, f64, u8)>,
    scene: Scene,
    placeholder: Option<Layout<Brush>>,
    create_label: Option<Layout<Brush>>,
}

impl NewThreadPrompt {
    /// Opens the prompt: the field empty and focused (it takes typing at once),
    /// laid out with `fonts`, copying and pasting through `clipboard` (N1, N4).
    pub(crate) fn open(fonts: Fonts, clipboard: Box<dyn Clipboard>) -> NewThreadPrompt {
        let (field, ime_area, field_h) = name_field(fonts, clipboard);
        NewThreadPrompt {
            field,
            ime_area,
            focus: Part::Field,
            field_h,
            scale: 1.0,
            last: None,
            viewport: (0.0, 0.0, 0.0),
            pointer_down: false,
            last_press: None,
            scene: Scene::new(),
            placeholder: None,
            create_label: None,
        }
    }

    /// The name field: the editor's single-line field (N4). (A test seam:
    /// the shell reaches the field through the prompt's own methods.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field(&self) -> &Editor {
        &self.field
    }

    /// The name field, mutable (the tests place the caret through it).
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn field_mut(&mut self) -> &mut Editor {
        &mut self.field
    }

    /// The name as typed (committed text only, no input-method pre-edit).
    pub(crate) fn name(&self) -> String {
        self.field.field_text()
    }

    /// Whether *Create* is active: the name is not empty once trimmed (N2).
    pub(crate) fn can_create(&self) -> bool {
        !clean_name(&self.name()).is_empty()
    }

    /// One winit key press, with the text winit gives it (`"\r"` for Enter).
    /// Enter creates (when the name allows it), Esc cancels — except while an
    /// input-method composition shows, when both only end it (N7); everything else
    /// goes to the field, its text at most once (N2, N3, N4).
    pub(crate) fn key(&mut self, key: &WKey, text: Option<&str>, mods: Modifiers) -> PromptStep {
        let composing = self.focus == Part::Field && self.field.is_composing();
        match key {
            WKey::Named(WNamed::Enter) => {
                if composing {
                    // The input method commits the composition; its commit
                    // arrives as its own event (N7, editor U35).
                    return PromptStep::Open;
                }
                self.press_create()
            }
            WKey::Named(WNamed::Escape) => {
                if composing {
                    // The field ends the composition, committing nothing.
                    self.field.handle_event(InputEvent::Key(KeyEvent {
                        key: Key::Named(NamedKey::Escape),
                        modifiers: mods,
                    }));
                    return PromptStep::Open;
                }
                PromptStep::Cancel
            }
            WKey::Named(WNamed::Tab) if !composing && !mods.ctrl && !mods.alt && !mods.meta => {
                // Two stops, so Tab and Shift+Tab both go to the other one.
                self.set_focus(if self.focus == Part::Field { Part::Create } else { Part::Field });
                PromptStep::Open
            }
            _ if self.focus == Part::Create => {
                // The button takes Space (Enter above); typing goes nowhere.
                if matches!(key, WKey::Named(WNamed::Space)) {
                    return self.press_create();
                }
                PromptStep::Open
            }
            _ => {
                route_key(&mut self.field, key, text, mods);
                PromptStep::Open
            }
        }
    }

    /// Any other input for the field: text, input-method composition, pointer
    /// (in the field's own coordinates), focus (N4).
    pub(crate) fn event(&mut self, event: InputEvent) -> PromptStep {
        match &event {
            InputEvent::Pointer(PointerEvent::Down { .. }) => self.set_focus(Part::Field),
            InputEvent::Text(_) | InputEvent::Ime(ImeEvent::Preedit { .. }) | InputEvent::Ime(ImeEvent::Commit(_))
                if self.focus != Part::Field =>
            {
                self.set_focus(Part::Field);
            }
            _ => {}
        }
        self.field.handle_event(event);
        PromptStep::Open
    }

    /// An editing command for the field (copy, cut, paste, select all, undo).
    /// (The shell sends the keyboard shortcuts as keys, through [`Self::key`];
    /// this is for a menu, and the tests.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn command(&mut self, command: Command) {
        if matches!(command, Command::Save) {
            return; // nothing to save: the name is created by Create
        }
        self.field.command(command);
    }

    /// The *Create* button was activated (a click or the accessibility action):
    /// [`PromptStep::Create`] when the name allows it, else nothing (N2).
    pub(crate) fn press_create(&mut self) -> PromptStep {
        let name = clean_name(&self.name());
        if name.is_empty() {
            PromptStep::Open
        } else {
            PromptStep::Create(name)
        }
    }

    /// Where the parts are in a `w` × `h` window. Also gives the field its own
    /// width, so a long name scrolls inside it (N4, title T9).
    pub(crate) fn layout(&mut self, w: f64, h: f64) -> PromptLayout {
        let pw = PROMPT_W.min((w - 16.0).max(0.0)).max(BUTTON_W + GAP + 2.0 * INSET + 40.0).min(w.max(1.0));
        let ph = self.field_h + 2.0 * INSET;
        let x0 = ((w - pw) * 0.5).max(0.0);
        let y1 = (h - BOTTOM_CHROME - ABOVE_CHROME).min(h - 8.0).max((ph + 8.0).min(h));
        let y0 = (y1 - ph).max(0.0);
        let bounds = Rect::new(x0, y0, x0 + pw, y1);
        let create = Rect::new(bounds.x1 - INSET - BUTTON_W, y0 + INSET, bounds.x1 - INSET, y1 - INSET);
        let field = Rect::new(bounds.x0 + INSET, y0 + INSET, (create.x0 - GAP).max(bounds.x0 + INSET + 1.0), y1 - INSET);
        let vp = (field.width(), field.height(), self.scale);
        if vp != self.viewport {
            self.viewport = vp;
            self.field.set_viewport(vp.0, vp.1, vp.2);
        }
        let l = PromptLayout { bounds, field, create };
        self.last = Some(l);
        l
    }

    /// A primary click at `p` (shell coordinates) in a `w` × `h` window: on
    /// *Create*, as [`Self::press_create`]; in the field, to the field; inside
    /// the prompt elsewhere, nothing; outside the prompt, [`PromptStep::Cancel`]
    /// (N2, N3).
    pub(crate) fn click(&mut self, p: Point, w: f64, h: f64) -> PromptStep {
        let l = self.layout(w, h);
        if l.create.contains(p) {
            return self.press_create();
        }
        if l.field.contains(p) {
            let (x, y) = (p.x - l.field.x0, p.y - l.field.y0);
            self.event(InputEvent::Pointer(PointerEvent::Down {
                x,
                y,
                button: PointerButton::Primary,
                click_count: 1,
                modifiers: Modifiers::NONE,
            }));
            self.event(InputEvent::Pointer(PointerEvent::Up { x, y, button: PointerButton::Primary }));
            return PromptStep::Open;
        }
        if l.bounds.contains(p) {
            return PromptStep::Open;
        }
        PromptStep::Cancel
    }

    /// The prompt is closed by any other path (the shell dismisses it):
    /// always [`PromptStep::Cancel`] (N3).
    pub(crate) fn close(&mut self) -> PromptStep {
        self.field.handle_event(InputEvent::Focus(false));
        PromptStep::Cancel
    }

    /// The prompt's whole accessibility tree (a tree of its own, which the
    /// shell inlines in its tree as it does the editor's): a dialog named
    /// [`DIALOG_LABEL`] holding the field — a text input labelled
    /// [`FIELD_LABEL`] with the placeholder, holding the focus — and the
    /// *Create* button, disabled while the name is empty; no Cancel button
    /// (N5).
    pub(crate) fn accessibility_tree(&mut self) -> TreeUpdate {
        let empty = self.name().is_empty();
        let can_create = self.can_create();
        let field = self.field.accessibility_tree();
        let field_root = field.tree.as_ref().map(|t| t.root);
        let field_focus = field.focus;
        let mut parts: Vec<NodeId> = vec![];
        let mut nodes: Vec<(NodeId, Node)> = vec![];
        for (id, mut n) in remap(field.nodes, field_id) {
            if Some(id) == field_root.map(field_id) {
                // The field's own container: its children hang in the dialog.
                parts.extend(n.children().iter().copied());
                continue;
            }
            if n.role() == Role::TextInput {
                n.set_label(FIELD_LABEL);
                if empty {
                    n.set_placeholder(PLACEHOLDER);
                }
            }
            nodes.push((id, n));
        }
        parts.push(CREATE_ID);

        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(DIALOG_LABEL);
        dialog.set_modal();
        dialog.set_children(parts);
        let mut create = Node::new(Role::Button);
        create.set_label(CREATE_LABEL);
        create.add_action(Action::Click);
        create.add_action(Action::Focus);
        if !can_create {
            create.set_disabled();
        }
        let mut all = Vec::with_capacity(nodes.len() + 2);
        all.push((DIALOG_ID, dialog));
        all.push((CREATE_ID, create));
        all.extend(nodes);
        let focus = match self.focus {
            Part::Create => CREATE_ID,
            Part::Field if Some(field_focus) != field_root => field_id(field_focus),
            Part::Field => DIALOG_ID,
        };
        TreeUpdate { nodes: all, tree: Some(Tree::new(DIALOG_ID)), tree_id: TreeId::ROOT, focus }
    }

    // ---- what only the shell uses (drawing, the pointer, the input method) ----

    /// The display scale the shell draws at (logical → physical).
    pub(crate) fn set_scale(&mut self, scale: f64) {
        self.scale = scale;
    }

    /// A left press at `p` (shell coordinates). In the field it goes to the
    /// field (with its click count: a double-click selects a word) and a
    /// drag follows; returns whether it landed in the field.
    pub(crate) fn press(&mut self, p: Point, w: f64, h: f64, mods: Modifiers) -> bool {
        let l = self.layout(w, h);
        if !l.field.contains(p) {
            return false;
        }
        let (x, y) = (p.x - l.field.x0, p.y - l.field.y0);
        let now = Instant::now();
        let n = click_count(self.last_press, now, x, y);
        self.last_press = Some((now, x, y, n));
        self.pointer_down = true;
        self.event(InputEvent::Pointer(PointerEvent::Down {
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
        let Some(l) = self.last else { return };
        if self.pointer_down {
            self.field.handle_event(InputEvent::Pointer(PointerEvent::Move { x: p.x - l.field.x0, y: p.y - l.field.y0 }));
        }
    }

    /// The left button was released at `p`; returns whether a press in the
    /// field was under way (that click is then the field's).
    pub(crate) fn release(&mut self, p: Point) -> bool {
        let Some(l) = self.last else { return false };
        if !std::mem::take(&mut self.pointer_down) {
            return false;
        }
        self.field.handle_event(InputEvent::Pointer(PointerEvent::Up {
            x: p.x - l.field.x0,
            y: p.y - l.field.y0,
            button: PointerButton::Primary,
        }));
        true
    }

    /// Whether the input method should be on (the field has the keyboard),
    /// and its candidate window's place: the field's caret, in physical
    /// pixels (`total`: logical → physical).
    pub(crate) fn ime_request(&self, total: f64) -> (bool, Option<(i32, i32, u32, u32)>) {
        if self.focus != Part::Field {
            return (false, None);
        }
        let area = match (self.last, self.ime_area.get()) {
            (Some(l), Some(a)) => {
                let x = ((l.field.x0 + a.x0) * total).round() as i32;
                let y = ((l.field.y0 + a.y0) * total).round() as i32;
                let w = (((a.x1 - a.x0) * total).round() as u32).max(1);
                let h = (((a.y1 - a.y0) * total).round() as u32).max(1);
                Some((x, y, w, h))
            }
            _ => None,
        };
        (true, area)
    }

    /// Paints the prompt over the shell (after everything it floats above).
    pub(crate) fn draw(&mut self, scene: &mut Scene, shaper: &mut TextShaper, w: f64, h: f64, cursor: Point) {
        let l = self.layout(w, h);
        let p = pal();
        // A soft shadow, then the popup.
        let shadow = RoundedRect::from_rect(l.bounds + vello::kurbo::Vec2::new(0.0, 3.0), 10.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, p.scrim.with_alpha(0.25), None, &shadow);
        let rr = RoundedRect::from_rect(l.bounds, 10.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, p.popup, None, &rr);
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, p.border, None, &rr);

        // The field: the editor paints its text, caret and selection.
        let mut ep = editor_palette(&p);
        ep.background = p.panel;
        self.field.set_palette(ep);
        self.scene.reset();
        self.field.paint(&mut self.scene);
        scene.fill(Fill::NonZero, Affine::IDENTITY, p.panel, None, &l.field);
        scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &l.field);
        scene.append(&self.scene, Some(Affine::translate((l.field.x0, l.field.y0))));
        if self.name().is_empty() && !self.field.is_composing() {
            let ph = self.placeholder.get_or_insert_with(|| shaper.shape(PLACEHOLDER, 4000.0, FIELD_FONT_PX));
            let y = l.field.y0 + (l.field.height() - ph.height() as f64) * 0.5;
            draw_text(scene, ph, Affine::translate((l.field.x0 + FIELD_PAD, y)), p.text_dim);
        }
        scene.pop_layer();
        let field_border = if self.focus == Part::Field { p.accent } else { p.input_border };
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, field_border, None, &l.field);

        // Create: inactive while the name is empty (N2).
        let active = self.can_create();
        let bg = match (active, l.create.contains(cursor)) {
            (true, true) => p.accent.with_alpha(0.9),
            (true, false) => p.accent,
            (false, _) => p.surface_alt,
        };
        let br = RoundedRect::from_rect(l.create, 7.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &br);
        if self.focus == Part::Create {
            let ring = RoundedRect::from_rect(l.create.inflate(2.5, 2.5), 9.0);
            scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, p.text, None, &ring);
        }
        let cl = self.create_label.get_or_insert_with(|| shaper.shape(CREATE_LABEL, 400.0, 13.5));
        let tx = l.create.x0 + (l.create.width() - cl.width() as f64) * 0.5;
        let ty = l.create.y0 + (l.create.height() - cl.height() as f64) * 0.5;
        draw_text(scene, cl, Affine::translate((tx, ty)), if active { p.on_accent } else { p.text_dim });
    }

    fn set_focus(&mut self, part: Part) {
        if self.focus != part {
            self.focus = part;
            self.field.handle_event(InputEvent::Focus(part == Part::Field));
        }
    }
}

/// A thread-name field: the editor's single-line field, empty and focused,
/// over a host that never saves and resolves no tokens; with the cell where
/// the field keeps its input-method caret area (field coordinates), and the
/// field's height (its line plus the editor's margins). Shared by the
/// new-thread prompt and the lane label's rename field (`crate::lane_rename`).
pub(crate) fn name_field(
    fonts: Fonts,
    clipboard: Box<dyn Clipboard>,
) -> (Editor, Rc<Cell<Option<sovereign_editor::Rect>>>, f64) {
    let ime_area = Rc::new(Cell::new(None));
    let host = Host {
        documents: Box::new(NoSaves),
        clipboard,
        accessibility: Box::new(NoUpdates),
        clock: Box::new(MonoClock),
        pii: Box::new(NoTokens),
        window: Box::new(FieldWindow(ime_area.clone())),
    };
    // The field's tree is a root tree of its own; the shell moves its ids
    // into its range when it inlines it.
    let config = EditorConfig { fonts, accessibility_tree: TreeId::ROOT };
    let mut field = Editor::open_field(FieldKind::SingleLine, config, host);
    field.handle_event(InputEvent::Focus(true));
    // Measure one line: the field is that line plus the editor's margin
    // above and below it.
    field.set_viewport(PROMPT_W, 200.0, 1.0);
    let field_h = match field.caret_rect() {
        Some(r) if r.y1 > r.y0 => (r.y1 + r.y0.max(0.0)).ceil().max(FALLBACK_FIELD_H - 10.0),
        _ => FALLBACK_FIELD_H,
    };
    (field, ime_area, field_h)
}

/// The name as created: on one line (each line break a space) and trimmed.
pub(crate) fn clean_name(name: &str) -> String {
    name.replace("\r\n", " ").replace(['\n', '\r'], " ").trim().to_string()
}

/// Where a named thread can be created. Threads are written only through the
/// document service's one thread-write path, stamped (THREAD-sync S1, decided
/// by Céline 2026-10-18): the service creates; a bare store refuses and
/// writes nothing ("a path that has no document service either obtains one
/// or does not write").
pub(crate) trait NamedThreadTarget: Sync {
    fn create<'a>(
        &'a self,
        name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>>;
}

impl NamedThreadTarget for sovereign_docs::DocumentService {
    fn create<'a>(
        &'a self,
        name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            self.thread_create(name, "").await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))
        })
    }
}

impl NamedThreadTarget for dyn GraphDB {
    fn create<'a>(
        &'a self,
        _name: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            Err("threads are created through the document service; the workspace is locked".to_string())
        })
    }
}

/// Creates a thread named `name` — trimmed, each line break turned into a
/// space — with an empty description, through the one thread-write path
/// (THREAD-sync S1). An empty or spaces-only name creates nothing:
/// `Ok(None)`. Otherwise the new thread's id. A name already used by another
/// thread is accepted (N2, N6).
pub(crate) async fn create_named_thread<T: NamedThreadTarget + ?Sized>(
    target: &T,
    name: &str,
) -> Result<Option<String>, String> {
    let name = clean_name(name);
    if name.is_empty() {
        return Ok(None);
    }
    target.create(&name).await.map(Some)
}

// ---- accessibility ids ---------------------------------------------------------

/// The dialog, the root of the prompt's tree.
const DIALOG_ID: NodeId = NodeId(1);
const CREATE_ID: NodeId = NodeId(2);
/// The field's own tree ids, moved into the prompt's tree.
const FIELD_NS: u64 = 1 << 40;

fn field_id(id: NodeId) -> NodeId {
    NodeId(FIELD_NS | id.0)
}

/// The id range of the prompt's nodes inside the shell's tree (the document editor
/// uses `1 << 62`, the shell's own items count from 1).
pub(crate) const PROMPT_NS: u64 = 1 << 61;

/// `nodes` with every node reference moved by `map` (ids, children, text
/// selection: the references the editor's and the prompt's trees use).
fn remap(nodes: Vec<(NodeId, Node)>, map: impl Fn(NodeId) -> NodeId) -> Vec<(NodeId, Node)> {
    nodes
        .into_iter()
        .map(|(id, mut n)| {
            let kids: Vec<_> = n.children().iter().copied().map(&map).collect();
            if !kids.is_empty() {
                n.set_children(kids);
            }
            if let Some(sel) = n.text_selection().copied() {
                n.set_text_selection(TextSelection {
                    anchor: TextPosition { node: map(sel.anchor.node), ..sel.anchor },
                    focus: TextPosition { node: map(sel.focus.node), ..sel.focus },
                });
            }
            (map(id), n)
        })
        .collect()
}

/// A whole tree (the prompt's) made part of another tree: its nodes with
/// their ids moved into `ns`, its root to hang there, and its focus. Inlined
/// rather than grafted as a subtree, as the document editor is (AccessKit 0.35 to
/// 0.37 re-applies a subtree's focus to the root tree when the OS window
/// gains or loses focus, and panics; see `editor_v2::v2_push_a11y`).
pub(crate) fn inline_tree(update: TreeUpdate, ns: u64) -> (NodeId, Vec<(NodeId, Node)>, NodeId) {
    let map = |id: NodeId| NodeId(ns | id.0);
    let root = update.tree.as_ref().map(|t| t.root).unwrap_or(update.focus);
    (map(root), remap(update.nodes, map), map(update.focus))
}
