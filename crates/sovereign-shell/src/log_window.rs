//! What remains readable of a quarantined conversation log, and what she can do
//! with it (spec ch. 08, C-LOG, DG3 and DG4; the Fable maille, by delegation of
//! Céline, 2026-11-18, on the window's place and words, and on where Export
//! writes).
//!
//! The window opens from a control the shell adds below the quarantine notice,
//! « Show what remains readable », never from a link in the model's words. It
//! shows the entries before the break as plain text, counts the rest, and offers
//! three buttons: « Keep it », « Export for an expert… », « Delete it… ».
//! Closing it keeps. It is not a document: it has no id, it is not in the graph,
//! and nothing of it reaches the assistant.
//!
//! Export writes a new folder in her Downloads folder, « Sovereign quarantined
//! log YYYY-MM-DD HHMM » in local time, never into a folder that exists; the
//! action gate names the whole path and asks before anything is written (a
//! Transmit action). Delete asks to confirm, the focus on « Cancel ».
//!
//! The gate's question is asked in the window itself, in the action gate's form
//! (its Transmit badge, « Approve » and « Reject (Esc) »): the orchestrator's
//! prompt answers a proposal of the model, and this choice is hers, made in
//! the shell. What was done is said in the chat, as the quarantine was.
//!
//! The Downloads folder is the one under her home folder, as the import finds
//! it; a Downloads folder moved elsewhere is not followed (the Fable maille's
//! decision, 2026-11-18: no Downloads folder found, nothing is exported).
//!
//! To a screen reader the window is a modal dialog named « What remains
//! readable »: its lines as labels, then its buttons, the focus on the button
//! the keyboard holds. The keyboard: Tab, Left and Right move between the
//! buttons, Enter or Space presses the focused one, Escape steps back (from a
//! question to the window, from the window to closed, which keeps).

use std::path::PathBuf;

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use sovereign_ai::log_diagnosis as dg;
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key, NamedKey};

use crate::app::App;
use crate::canvas::{AXIS_H, BOTTOM_CHROME};
use crate::text::{draw_text, TextShaper};
use crate::theme::pal;

/// The id range of the window's accessibility nodes in the shell's tree.
pub(crate) const LOG_WINDOW_NS: u64 = 1 << 51;
/// The id range of the « Show what remains readable » controls below the chat's
/// messages: this range plus the message's index.
pub(crate) const LOG_CONTROL_NS: u64 = 1 << 50;

/// The control's words (the Fable maille, by delegation of Céline, 2026-11-18).
pub(crate) const SHOW: &str = "Show what remains readable";

const TITLE: &str = "What remains readable";
const KEEP: &str = "Keep it";
const EXPORT: &str = "Export for an expert\u{2026}";
const DELETE_IT: &str = "Delete it\u{2026}";
const CANCEL: &str = "Cancel";
const DELETE: &str = "Delete";
const APPROVE: &str = "Approve";
const REJECT: &str = "Reject (Esc)";
const DELETE_QUESTION: &str = "Delete this quarantined log? It cannot be brought back.";
const DELETED: &str = "The quarantined log was deleted.";
const NAME_TAKEN: &str = "A folder of that name already exists in Downloads. Nothing was exported.";
const NO_DOWNLOADS: &str = "No Downloads folder was found. Nothing was exported.";
const NONE: &str = "No conversation log is quarantined.";

/// The gate's question for Export, naming the whole path.
fn export_question(folder: &std::path::Path) -> String {
    format!(
        "Export the quarantined log to {}? Its files leave as they are on disk. Sealed entries stay sealed; \
         any line written before encryption can be read by whoever has the folder.",
        folder.display()
    )
}

/// What is said once the export is written.
fn exported(folder: &std::path::Path) -> String {
    format!("The quarantined log was exported to {}. It is still kept here.", folder.display())
}

/// The count of the entries from the break on, in the decided words.
fn not_shown(n: u64) -> String {
    if n == 1 {
        "1 entry is not shown: from the break on, it no longer verifies.".to_string()
    } else {
        format!("{n} entries are not shown: from the break on, they no longer verify.")
    }
}

/// The folder Export makes, for the time `now`, in local time.
fn folder_name(now: chrono::DateTime<chrono::Local>) -> String {
    format!("Sovereign quarantined log {}", now.format("%Y-%m-%d %H%M"))
}

/// Her Downloads folder, found as the import finds it: `Downloads` under her
/// home folder, when it is a folder.
fn downloads_folder() -> Option<PathBuf> {
    let d = sovereign_core::home_dir().join("Downloads");
    d.is_dir().then_some(d)
}

/// A button of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Button {
    Keep,
    Export,
    DeleteIt,
    Cancel,
    Delete,
    Approve,
    Reject,
}

impl Button {
    fn label(self) -> &'static str {
        match self {
            Button::Keep => KEEP,
            Button::Export => EXPORT,
            Button::DeleteIt => DELETE_IT,
            Button::Cancel => CANCEL,
            Button::Delete => DELETE,
            Button::Approve => APPROVE,
            Button::Reject => REJECT,
        }
    }
}

/// Where the window is.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Stage {
    /// What remains readable, and the three choices.
    Shown,
    /// « Delete it… » pressed: the confirmation.
    ConfirmDelete,
    /// « Export for an expert… » pressed: the gate's question, for this folder.
    ConfirmExport(PathBuf),
}

/// The window, while it is open.
pub(crate) struct LogWindow {
    /// The quarantine's readable part; `None` when the persona has none.
    readable: Option<Shown>,
    stage: Stage,
    /// Index into [`Self::buttons`] of the button the keyboard holds.
    focus: usize,
    /// What the last choice said, when it did not close the window.
    status: Option<String>,
    /// How far the lines are scrolled, in pixels from their top; kept within
    /// the lines' height by [`LogWindow::clamp_scroll`].
    scroll: f64,
    /// Where the last frame drew the window, its lines (scrolled) and its
    /// buttons: the accessibility tree gives them as bounds, so a reader or a
    /// magnifier knows what is on screen. `None` before the first frame.
    laid: Option<Laid>,
}

/// The window's rectangles on screen, as the last frame drew them.
struct Laid {
    panel: Rect,
    lines: Vec<Rect>,
    buttons: Vec<Rect>,
}

fn a11y_rect(r: Rect) -> accesskit::Rect {
    accesskit::Rect { x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1 }
}

/// What the window shows of the quarantine.
struct Shown {
    entries: Vec<String>,
    after_the_break: u64,
    order_not_guaranteed: bool,
    earlier: Vec<String>,
}

impl LogWindow {
    fn buttons(&self) -> Vec<Button> {
        match &self.stage {
            Stage::Shown if self.readable.is_some() => vec![Button::Keep, Button::Export, Button::DeleteIt],
            Stage::Shown => vec![Button::Keep],
            Stage::ConfirmDelete => vec![Button::Cancel, Button::Delete],
            Stage::ConfirmExport(_) => vec![Button::Reject, Button::Approve],
        }
    }

    /// The window's lines, in order: the question in a confirmation; else the
    /// entries, the count, the older format's sentence, the earlier
    /// quarantines; and the last choice's words.
    fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        match (&self.stage, &self.readable) {
            (Stage::ConfirmDelete, _) => out.push(DELETE_QUESTION.to_string()),
            (Stage::ConfirmExport(folder), _) => out.push(export_question(folder)),
            (Stage::Shown, None) => out.push(NONE.to_string()),
            (Stage::Shown, Some(r)) => {
                if r.order_not_guaranteed {
                    out.push(dg::order_not_guaranteed_sentence().to_string());
                }
                out.extend(r.entries.iter().cloned());
                if r.after_the_break > 0 {
                    out.push(not_shown(r.after_the_break));
                }
                out.extend(r.earlier.iter().cloned());
            }
        }
        if let Some(s) = &self.status {
            out.push(s.clone());
        }
        out
    }

    fn set_stage(&mut self, stage: Stage) {
        // The focus goes to the safe button: « Cancel », « Reject », or « Keep it ».
        self.stage = stage;
        self.focus = 0;
        self.scroll = 0.0;
    }

    /// Keeps the scroll between the lines' top and the point where their last
    /// one ends at the bottom of the view.
    fn clamp_scroll(&mut self, w: f64, h: f64, shaper: &mut TextShaper) {
        let g = geom(self, w, h, shaper);
        let max = (g.content_h - g.text.height()).max(0.0);
        self.scroll = self.scroll.clamp(0.0, max);
    }

    /// A key while the window is open (it holds the keyboard).
    fn key(&mut self, key: &Key) -> Step {
        let n = self.buttons().len();
        match key {
            Key::Named(NamedKey::Escape) => {
                return match self.stage {
                    Stage::Shown => Step::Press(Button::Keep),
                    Stage::ConfirmDelete => Step::Press(Button::Cancel),
                    Stage::ConfirmExport(_) => Step::Press(Button::Reject),
                }
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => return self.pressed(self.focus),
            Key::Character(c) if c.as_str() == " " => return self.pressed(self.focus),
            Key::Named(NamedKey::Tab) | Key::Named(NamedKey::ArrowRight) => self.focus = (self.focus + 1) % n,
            Key::Named(NamedKey::ArrowLeft) => self.focus = (self.focus + n - 1) % n,
            // The lines scroll by the keyboard too; the caller keeps them in range.
            Key::Named(NamedKey::ArrowDown) => self.scroll += LINE_STEP,
            Key::Named(NamedKey::ArrowUp) => self.scroll -= LINE_STEP,
            Key::Named(NamedKey::PageDown) => self.scroll += PAGE_STEP,
            Key::Named(NamedKey::PageUp) => self.scroll -= PAGE_STEP,
            Key::Named(NamedKey::Home) => self.scroll = 0.0,
            Key::Named(NamedKey::End) => self.scroll = f64::MAX,
            _ => {}
        }
        Step::Open
    }

    fn pressed(&self, i: usize) -> Step {
        self.buttons().get(i).map_or(Step::Open, |b| Step::Press(*b))
    }

    /// What a click at `p` does: a button presses; anywhere else, nothing.
    fn click(&self, w: f64, h: f64, p: Point, shaper: &mut TextShaper) -> Step {
        let g = geom(self, w, h, shaper);
        match g.buttons.iter().position(|r| r.contains(p)) {
            Some(i) => self.pressed(i),
            None => Step::Open,
        }
    }

    /// The window's accessibility tree: a modal dialog, its lines as labels
    /// (spoken by their value), its buttons, the focus on the keyboard's.
    fn accessibility_tree(&self) -> TreeUpdate {
        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(TITLE);
        dialog.set_modal();
        let mut nodes = Vec::new();
        let mut children = Vec::new();
        for (i, line) in self.lines().into_iter().enumerate() {
            let mut l = Node::new(Role::Label);
            l.set_value(line);
            if let Some(r) = self.laid.as_ref().and_then(|g| g.lines.get(i)) {
                l.set_bounds(a11y_rect(*r));
            }
            let id = NodeId(A11Y_LINES + i as u64);
            nodes.push((id, l));
            children.push(id);
        }
        for (i, b) in self.buttons().into_iter().enumerate() {
            let mut n = Node::new(Role::Button);
            n.set_label(b.label());
            n.add_action(Action::Click);
            n.add_action(Action::Focus);
            if let Some(r) = self.laid.as_ref().and_then(|g| g.buttons.get(i)) {
                n.set_bounds(a11y_rect(*r));
            }
            let id = a11y_button(i);
            nodes.push((id, n));
            children.push(id);
        }
        dialog.set_children(children);
        if let Some(g) = &self.laid {
            dialog.set_bounds(a11y_rect(g.panel));
        }
        nodes.insert(0, (A11Y_DIALOG, dialog));
        TreeUpdate { nodes, tree: Some(Tree::new(A11Y_DIALOG)), tree_id: TreeId::ROOT, focus: a11y_button(self.focus) }
    }

    /// A screen reader's action on one of the window's nodes: Click presses a
    /// button, Focus gives it the keyboard.
    fn a11y_action(&mut self, local: NodeId, action: Action) -> Step {
        if local.0 < A11Y_BUTTONS || local.0 >= A11Y_LINES {
            return Step::Open;
        }
        let i = (local.0 - A11Y_BUTTONS) as usize;
        match action {
            Action::Click => self.pressed(i),
            Action::Focus => {
                if i < self.buttons().len() {
                    self.focus = i;
                }
                Step::Open
            }
            _ => Step::Open,
        }
    }
}

/// What a key, a click or a screen reader's action asks of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Open,
    Press(Button),
}

const A11Y_DIALOG: NodeId = NodeId(1);
const A11Y_BUTTONS: u64 = 16;
const A11Y_LINES: u64 = 64;

fn a11y_button(i: usize) -> NodeId {
    NodeId(A11Y_BUTTONS + i as u64)
}

// ---- drawing ---------------------------------------------------------------

const MW: f64 = 560.0;
const BUTTON_W: f64 = 170.0;
const BUTTON_H: f64 = 34.0;
/// What the arrow keys and one notch of the wheel scroll.
const LINE_STEP: f64 = 40.0;
/// What Page Up and Page Down scroll.
const PAGE_STEP: f64 = 240.0;

struct Geom {
    panel: Rect,
    buttons: Vec<Rect>,
    /// Where the lines are shown; they scroll inside it.
    text: Rect,
    /// The lines' whole height, wrapped to the window's width.
    content_h: f64,
}

/// The window's lines shaped, and its height: the lines are shaped to measure
/// them (an entry can be long), the panel at most most of the screen.
fn geom(win: &LogWindow, w: f64, h: f64, shaper: &mut TextShaper) -> Geom {
    let content_h: f64 = win
        .lines()
        .iter()
        .map(|l| shaper.shape(l, (MW - 32.0) as f32, 12.5).height() as f64 + 6.0)
        .sum();
    let max_h = (h - BOTTOM_CHROME - AXIS_H - 40.0).max(160.0);
    let ph = (16.0 + 22.0 + 10.0 + content_h + 16.0 + BUTTON_H + 16.0).min(max_h);
    let x0 = ((w - MW) * 0.5).max(8.0);
    let y0 = ((h - BOTTOM_CHROME - AXIS_H - ph) * 0.5 + AXIS_H).max(AXIS_H + 8.0);
    let panel = Rect::new(x0, y0, x0 + MW, y0 + ph);
    let by0 = panel.y1 - 16.0 - BUTTON_H;
    let n = win.buttons().len();
    let buttons = (0..n)
        .map(|i| {
            let x1 = panel.x1 - 16.0 - (n - 1 - i) as f64 * (BUTTON_W + 10.0);
            Rect::new(x1 - BUTTON_W, by0, x1, by0 + BUTTON_H)
        })
        .collect();
    let text = Rect::new(panel.x0, panel.y0 + 16.0 + 22.0 + 10.0, panel.x1, by0 - 12.0);
    Geom { panel, buttons, text, content_h }
}

/// Draws the window over the scene, the screen behind it dimmed.
fn draw(scene: &mut Scene, shaper: &mut TextShaper, win: &mut LogWindow, w: f64, h: f64, cursor: Point) {
    let g = geom(win, w, h, shaper);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.45), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(g.panel, 11.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().accent, None, &rr);

    let y = g.panel.y0 + 16.0;
    let mut title_x = g.panel.x0 + 16.0;
    if matches!(win.stage, Stage::ConfirmExport(_)) {
        // The gate's badge: a Transmit action, as the action gate shows it.
        let badge = shaper.shape(crate::app::level_label(sovereign_core::security::ActionLevel::Transmit), 200.0, 11.5);
        let accent = crate::app::level_color(sovereign_core::security::ActionLevel::Transmit);
        let r = Rect::new(title_x, y, title_x + badge.width() as f64 + 18.0, y + 20.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, accent, None, &RoundedRect::from_rect(r, 5.0));
        draw_text(scene, &badge, Affine::translate((r.x0 + 9.0, r.y0 + 4.0)), crate::theme::text_on_fill(accent));
        title_x = r.x1 + 10.0;
    }
    let tl = shaper.shape(TITLE, (MW - 32.0) as f32, 15.0);
    draw_text(scene, &tl, Affine::translate((title_x, y)), pal().text);

    // The lines, wrapped to the window's width, scrolled inside their view.
    let view = g.text;
    scene.push_layer(Fill::NonZero, vello::peniko::Mix::Normal, 1.0, Affine::IDENTITY, &view);
    let mut y = view.y0 - win.scroll;
    let mut line_rects = Vec::new();
    for line in win.lines() {
        let l = shaper.shape(&line, (MW - 32.0) as f32, 12.5);
        let lh = l.height() as f64 + 6.0;
        line_rects.push(Rect::new(g.panel.x0 + 16.0, y, g.panel.x1 - 16.0, y + l.height() as f64));
        if y + lh >= view.y0 && y <= view.y1 {
            draw_text(scene, &l, Affine::translate((g.panel.x0 + 16.0, y)), pal().text_body);
        }
        y += lh;
    }
    scene.pop_layer();
    if g.content_h > view.height() {
        // A scroll bar says there is more, and where she is in it.
        let track = Rect::new(view.x1 - 9.0, view.y0, view.x1 - 5.0, view.y1);
        let frac = view.height() / g.content_h;
        let max = g.content_h - view.height();
        let th = (track.height() * frac).max(24.0);
        let ty = track.y0 + (track.height() - th) * (win.scroll / max).clamp(0.0, 1.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().border, None, &RoundedRect::from_rect(track, 2.0));
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().text_body, None, &RoundedRect::from_rect(Rect::new(track.x0, ty, track.x1, ty + th), 2.0));
    }

    for (i, (b, r)) in win.buttons().into_iter().zip(&g.buttons).enumerate() {
        let danger = matches!(b, Button::Delete | Button::Approve);
        let hot = r.contains(cursor);
        let (bg, fg) = if danger {
            let bg = if hot { pal().accent } else { pal().accent.with_alpha(0.82) };
            (bg, crate::theme::text_on_fill(crate::theme::seen_over(bg, pal().modal)))
        } else {
            (if hot { pal().border } else { pal().surface_alt }, pal().text)
        };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 7.0));
        let l = shaper.shape(b.label(), (BUTTON_W - 12.0) as f32, 13.5);
        let at = (r.x0 + (r.width() - l.width() as f64) * 0.5, r.y0 + (r.height() - l.height() as f64) * 0.5);
        draw_text(scene, &l, Affine::translate(at), fg);
        if i == win.focus {
            // The keyboard's button: a visible outline (WCAG 2.4.7).
            scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, pal().text, None, &RoundedRect::from_rect(r.inflate(3.0, 3.0), 9.0));
        }
    }
    win.laid = Some(Laid { panel: g.panel, lines: line_rects, buttons: g.buttons });
}

// ---- the app ---------------------------------------------------------------

impl App {
    /// Where Export finds her Downloads folder from now on, instead of looking
    /// for it: `Some` a folder, or `None` for « no Downloads folder was found ».
    /// The tests give a temporary one.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn log_export_downloads(&mut self, downloads: Option<PathBuf>) {
        self.log_downloads = Some(downloads);
    }

    /// The open persona's log paths and log key; `None` without a session.
    fn log_paths_and_key(&self) -> Option<(sovereign_ai::OrchestratorStatePaths, [u8; 32])> {
        let ak = self.account_key.clone()?;
        Some((crate::crypto::orchestrator_state_paths(self.persona), ak.derive_session_log_key()))
    }

    /// Opens the window on the persona's most recent quarantine, read now.
    pub(crate) fn open_log_window(&mut self) {
        if self.locked {
            return;
        }
        let Some((paths, key)) = self.log_paths_and_key() else { return };
        let readable = match dg::readable_for_the_person(&paths, &key) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("sovereign-shell: what remains readable could not be read: {e}");
                None
            }
        };
        let earlier: Vec<String> = dg::diagnose(&paths, &key)
            .ok()
            .flatten()
            .map(|d| d.earlier.iter().map(dg::earlier_quarantine_sentence).collect())
            .unwrap_or_default();
        self.log_window = Some(LogWindow {
            readable: readable.map(|r| Shown {
                entries: r.entries,
                after_the_break: r.after_the_break,
                order_not_guaranteed: r.order_not_guaranteed,
                earlier,
            }),
            stage: Stage::Shown,
            focus: 0,
            status: None,
            scroll: 0.0,
            laid: None,
        });
    }

    /// Whether the window is shown (it holds clicks and keys).
    pub(crate) fn log_window_open(&self) -> bool {
        !self.locked && self.log_window.is_some()
    }

    fn log_window_step(&mut self, step: Step) {
        let Step::Press(b) = step else { return };
        let downloads = self.log_downloads.clone();
        let session = self.log_paths_and_key();
        let Some(win) = self.log_window.as_mut() else { return };
        match b {
            Button::Keep => self.log_window = None,
            Button::Cancel | Button::Reject => {
                win.set_stage(Stage::Shown);
            }
            Button::DeleteIt => win.set_stage(Stage::ConfirmDelete),
            Button::Export => {
                win.status = None;
                let base = match downloads {
                    Some(set) => set,
                    None => downloads_folder(),
                };
                match base.filter(|b| b.is_dir()) {
                    None => win.status = Some(NO_DOWNLOADS.to_string()),
                    Some(base) => {
                        let folder = base.join(folder_name(chrono::Local::now()));
                        if folder.exists() {
                            win.status = Some(NAME_TAKEN.to_string());
                        } else {
                            win.set_stage(Stage::ConfirmExport(folder));
                        }
                    }
                }
            }
            Button::Approve => {
                let Stage::ConfirmExport(folder) = win.stage.clone() else { return };
                win.set_stage(Stage::Shown);
                let said = self.export_quarantine(&folder);
                if let Some(win) = self.log_window.as_mut() {
                    win.status = Some(said.clone());
                }
                if said.starts_with("The quarantined log was exported") {
                    self.push_chat_msg(said);
                }
            }
            Button::Delete => {
                let Some((paths, _)) = session else { return };
                match dg::delete_the_quarantine(&paths) {
                    Ok(()) => {
                        self.log_window = None;
                        self.push_chat_msg(DELETED.to_string());
                    }
                    Err(e) => {
                        win.set_stage(Stage::Shown);
                        win.status = Some(format!("The quarantined log could not be deleted: {e}"));
                    }
                }
            }
        }
    }

    /// Writes the export into `folder`, which must not exist; what is said.
    fn export_quarantine(&mut self, folder: &std::path::Path) -> String {
        if folder.exists() {
            return NAME_TAKEN.to_string();
        }
        let Some((paths, key)) = self.log_paths_and_key() else { return "No session is open. Nothing was exported.".into() };
        let diagnosis = match dg::diagnose(&paths, &key) {
            Ok(Some(d)) => d,
            Ok(None) => return NONE.to_string(),
            Err(e) => return format!("The quarantined log could not be read: {e}. Nothing was exported."),
        };
        match dg::export_for_an_expert(&paths, &diagnosis, folder) {
            Ok(()) => exported(folder),
            Err(_) if folder.exists() => NAME_TAKEN.to_string(),
            Err(e) => format!("The export could not be written: {e}. Nothing was exported."),
        }
    }

    /// A key while the window is shown.
    pub(crate) fn log_window_key(&mut self, key: &Key) {
        if let Some(step) = self.log_window.as_mut().map(|w| w.key(key)) {
            self.log_window_step(step);
        }
        self.log_window_clamp();
    }

    /// The wheel while the window is shown: its lines scroll, nothing under it.
    pub(crate) fn log_window_wheel(&mut self, lines: f64) {
        if let Some(win) = self.log_window.as_mut() {
            win.scroll -= lines * LINE_STEP;
        }
        self.log_window_clamp();
    }

    fn log_window_clamp(&mut self) {
        let Some((w, h)) = self.win_size() else { return };
        if let Some(win) = self.log_window.as_mut() {
            win.clamp_scroll(w, h, &mut self.shaper);
        }
    }

    /// A click while the window is shown.
    pub(crate) fn log_window_click(&mut self, p: Point, w: f64, h: f64) {
        let step = match self.log_window.as_ref() {
            Some(win) => win.click(w, h, p, &mut self.shaper),
            None => return,
        };
        self.log_window_step(step);
    }

    /// Draws the window, when it is shown.
    pub(crate) fn draw_log_window(&mut self, w: f64, h: f64) {
        if self.locked {
            return;
        }
        let cursor = Point::new(self.cursor.0, self.cursor.1);
        if let Some(win) = self.log_window.as_mut() {
            draw(&mut self.scene, &mut self.shaper, win, w, h, cursor);
        }
    }

    /// The window's accessibility tree, when it is shown.
    pub(crate) fn log_window_tree(&self) -> Option<TreeUpdate> {
        if !self.log_window_open() {
            return None;
        }
        self.log_window.as_ref().map(|w| w.accessibility_tree())
    }

    /// A screen reader's action; whether it was on the window's nodes or on a
    /// chat's control.
    pub(crate) fn log_window_a11y_action(&mut self, target: NodeId, action: Action) -> bool {
        let high = target.0 & !((1u64 << 32) - 1);
        if high == LOG_CONTROL_NS {
            if action == Action::Click {
                self.open_log_window();
            }
            return true;
        }
        if high != LOG_WINDOW_NS {
            return false;
        }
        let local = NodeId(target.0 & ((1u64 << 32) - 1));
        if let Some(step) = self.log_window.as_mut().map(|w| w.a11y_action(local, action)) {
            self.log_window_step(step);
        }
        true
    }

    /// C-LOG DG3: the last message of the first chat offers the control (the
    /// quarantine notice, or the reply after the diagnosis).
    pub(crate) fn offer_readable_log_on_last(&mut self) {
        if let Some(chat) = self.first_chat_mut() {
            if let Some(last) = chat.msgs.len().checked_sub(1) {
                if !chat.readable_log_at.contains(&last) {
                    chat.readable_log_at.push(last);
                }
            }
        }
    }

    /// The controls below the first chat's messages, as buttons (a node each,
    /// keyed by its message).
    pub(crate) fn log_control_nodes(&self) -> Vec<(NodeId, Node)> {
        if self.locked {
            return Vec::new();
        }
        let Some(chat) = self.first_chat() else { return Vec::new() };
        chat.readable_log_at
            .iter()
            .map(|i| {
                let mut b = Node::new(Role::Button);
                b.set_label(SHOW);
                b.add_action(Action::Click);
                (NodeId(LOG_CONTROL_NS + *i as u64), b)
            })
            .collect()
    }
}
