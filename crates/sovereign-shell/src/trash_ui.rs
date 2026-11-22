//! Drawing and hit-testing for the trash and the delete notice (C-DOC adoption
//! step 5, S5-8 and S5-11; decided by Céline 2026-10-17: one-click delete with
//! Undo, the trash in Settings). The state and the service calls are in
//! `trash.rs` and `delete_notice.rs`; `App` owns the instances and routes
//! clicks here. Manual checks: `TRASH_CHECKLIST.md`.

use std::time::Instant;

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;
use winit::keyboard::{Key, NamedKey};

use crate::canvas::{AXIS_H, BOTTOM_CHROME};
use crate::delete_notice::DeleteNotice;
use crate::text::{draw_text, TextShaper};
use crate::theme::pal;
use crate::trash::TrashView;

// ---- Settings → Trash ------------------------------------------------------

/// What a click on a row of the Trash tab does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TrashRowAction {
    /// Restore one document: into its thread, or — its thread deleted — into
    /// a thread the user chooses.
    RestoreOne(String),
    /// "Restore all into a thread…" for the group of a deleted thread.
    RestoreGroup(String),
    /// "Empty trash": asks first.
    EmptyTrash,
}

/// The Trash tab's label, with its count.
pub(crate) fn tab_label(view: Option<&TrashView>) -> String {
    match view {
        Some(v) => format!("Trash ({})", v.count()),
        None => "Trash".into(),
    }
}

/// The Trash tab's rows (`(is_header, key, value)`, as every Settings tab)
/// and, row for row, what a click on it does. No document content: titles,
/// thread names, dates and days left only.
pub(crate) fn settings_rows(view: Option<&TrashView>) -> (Vec<(bool, String, String)>, Vec<Option<TrashRowAction>>) {
    let mut rows = vec![];
    let mut acts = vec![];
    let mut push = |row: (bool, String, String), act: Option<TrashRowAction>| {
        rows.push(row);
        acts.push(act);
    };
    let Some(view) = view else {
        push((true, "Trash".into(), String::new()), None);
        push((false, "Status".into(), "log in to see the trash".into()), None);
        return (rows, acts);
    };
    if view.lines().is_empty() {
        push((true, "Trash".into(), String::new()), None);
        push((false, "Empty".into(), "deleted documents stay here for 30 days".into()), None);
        return (rows, acts);
    }
    let groups = view.deleted_thread_groups();
    for g in &groups {
        push((true, g.label.clone(), String::new()), None);
        let n = g.ids.len();
        push(
            (false, "Restore all into a thread\u{2026}".into(), format!("{n} document{}", if n == 1 { "" } else { "s" })),
            Some(TrashRowAction::RestoreGroup(g.thread_id.clone())),
        );
        for id in &g.ids {
            if let Some(l) = view.lines().iter().find(|l| &l.id == id) {
                push(line_row(l, "restore into a thread\u{2026}"), Some(TrashRowAction::RestoreOne(l.id.clone())));
            }
        }
    }
    let rest: Vec<_> = view.lines().iter().filter(|l| !l.thread_deleted).collect();
    if !rest.is_empty() {
        push((true, "In the trash".into(), String::new()), None);
        for l in rest {
            push(line_row(l, "click to restore"), Some(TrashRowAction::RestoreOne(l.id.clone())));
        }
    }
    push((true, "Empty".into(), String::new()), None);
    push(
        (false, "Empty trash".into(), "purges every document now \u{2014} asks first".into()),
        Some(TrashRowAction::EmptyTrash),
    );
    (rows, acts)
}

fn line_row(l: &crate::trash::TrashLine, hint: &str) -> (bool, String, String) {
    let title: String = crate::canvas::display_title(l.title.trim()).chars().take(40).collect();
    let days = if l.days_left == 1 { "1 day left".to_string() } else { format!("{} days left", l.days_left) };
    (
        false,
        title,
        format!("{} \u{00b7} {} \u{00b7} {} \u{00b7} {hint}", l.thread_name, l.trashed_at.format("%Y-%m-%d"), days),
    )
}

// ---- the modals ------------------------------------------------------------

/// What a restore into a chosen thread applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RestoreTarget {
    One(String),
    Group(String),
}

/// A trash modal: choosing the thread to restore into, or confirming Empty
/// trash (Destruct, K7).
///
/// The thread chooser offers every live thread, in the lanes' order, and
/// scrolls (v0.0.12 item 9, T4; review of v0.0.11, R20): the wheel, and Up /
/// Down / Page Up / Page Down / Home / End, move the chosen thread and keep it
/// in view; Enter restores into it; Esc cancels. To a screen reader it is a
/// dialog holding a list box whose options are the threads, the chosen one
/// selected and focused (as `orphans_ui::ThreadChooser`).
pub(crate) enum TrashModal {
    ChooseThread {
        target: RestoreTarget,
        title: String,
        /// `(id, name)` of every live thread, in the lanes' order.
        threads: Vec<(String, String)>,
        /// Index into `threads` of the chosen row.
        selected: usize,
        /// Index into `threads` of the first row drawn.
        top: usize,
    },
    ConfirmEmpty { count: usize },
}

/// A click in a trash modal. `Thread(i)` is an index into the chooser's
/// `threads`, whatever the scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalHit {
    Thread(usize),
    Confirm,
    Cancel,
    Nothing,
}

/// What a key in a trash modal asks of the shell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalKey {
    /// Keep the modal open.
    Open,
    /// Restore into this thread (an index into `threads`).
    Choose(usize),
    /// Close; nothing done.
    Cancel,
}

const MW: f64 = 440.0;
const ROW_H: f64 = 32.0;
/// How many thread rows are drawn at once; the rest scroll.
const MAX_ROWS: usize = 8;
/// The line under a list that scrolls ("Threads 1-8 of 30").
const MORE_H: f64 = 22.0;

impl TrashModal {
    /// The thread chooser, on the first thread.
    pub(crate) fn choose_thread(target: RestoreTarget, title: String, threads: Vec<(String, String)>) -> TrashModal {
        TrashModal::ChooseThread { target, title, threads, selected: 0, top: 0 }
    }

    /// The ids of the threads whose rows are drawn now, top to bottom (empty
    /// for the Empty trash confirmation).
    #[cfg_attr(not(test), allow(dead_code))] // read by the tests (T4)
    pub(crate) fn visible_threads(&self) -> Vec<String> {
        match self {
            TrashModal::ChooseThread { threads, top, .. } => {
                threads.iter().skip(*top).take(MAX_ROWS).map(|(id, _)| id.clone()).collect()
            }
            TrashModal::ConfirmEmpty { .. } => Vec::new(),
        }
    }

    /// Chooses thread `i` (clamped) and scrolls to keep it in view.
    fn select(&mut self, i: usize) {
        if let TrashModal::ChooseThread { threads, selected, top, .. } = self {
            if threads.is_empty() {
                return;
            }
            *selected = i.min(threads.len() - 1);
            if *selected < *top {
                *top = *selected;
            } else if *selected >= *top + MAX_ROWS {
                *top = *selected + 1 - MAX_ROWS;
            }
        }
    }

    /// The wheel: scrolls the list by `lines` rows (positive: up), keeping the
    /// chosen thread in view.
    pub(crate) fn scroll(&mut self, lines: i64) {
        if let TrashModal::ChooseThread { threads, selected, top, .. } = self {
            let max_top = threads.len().saturating_sub(MAX_ROWS) as i64;
            *top = (*top as i64 - lines).clamp(0, max_top) as usize;
            if *selected < *top {
                *selected = *top;
            } else if *selected >= *top + MAX_ROWS {
                *selected = *top + MAX_ROWS - 1;
            }
        }
    }

    /// A key while the modal is open (it owns the keyboard). The Empty trash
    /// confirmation answers Esc only: purging is a click (K7).
    pub(crate) fn key(&mut self, key: &Key) -> ModalKey {
        if matches!(key, Key::Named(NamedKey::Escape)) {
            return ModalKey::Cancel;
        }
        let (selected, n) = match self {
            TrashModal::ChooseThread { threads, selected, .. } => (*selected, threads.len()),
            TrashModal::ConfirmEmpty { .. } => return ModalKey::Open,
        };
        match key {
            Key::Named(NamedKey::Enter) if selected < n => return ModalKey::Choose(selected),
            Key::Named(NamedKey::ArrowDown) => self.select(selected + 1),
            Key::Named(NamedKey::ArrowUp) => self.select(selected.saturating_sub(1)),
            Key::Named(NamedKey::Home) => self.select(0),
            Key::Named(NamedKey::End) => self.select(usize::MAX),
            // A page: the rows drawn, less one kept for context.
            Key::Named(NamedKey::PageDown) => self.select(selected.saturating_add(MAX_ROWS - 1)),
            Key::Named(NamedKey::PageUp) => self.select(selected.saturating_sub(MAX_ROWS - 1)),
            _ => {}
        }
        ModalKey::Open
    }

    /// Whether `p` is over the modal's panel (the wheel scrolls it there).
    pub(crate) fn contains(&self, w: f64, h: f64, p: Point) -> bool {
        modal_geom(self, w, h).0.contains(p)
    }

    /// The modal's accessibility tree: a modal dialog named by its title. The
    /// chooser holds a list box of every live thread (the chosen one selected,
    /// and focused) and a Cancel button; the confirmation, what it will do and
    /// its two buttons, the focus on Cancel. The ids are local; the shell moves
    /// them into its own range (`new_thread::inline_tree`).
    pub(crate) fn accessibility_tree(&self) -> TreeUpdate {
        const DIALOG: NodeId = NodeId(1);
        const LIST: NodeId = NodeId(2);
        const CANCEL: NodeId = NodeId(3);
        const HINT: NodeId = NodeId(4);
        const CONFIRM: NodeId = NodeId(5);
        let option = |i: usize| NodeId(16 + i as u64);
        let button = |label: &str| {
            let mut b = Node::new(Role::Button);
            b.set_label(label.to_string());
            b.add_action(Action::Click);
            b.add_action(Action::Focus);
            b
        };
        let (title, body) = modal_text(self);
        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(title);
        dialog.set_modal();
        // v0.0.12 rc2 (V0012-rc2-labels-spoken, L1): a `Label` is spoken by
        // its value, so the hint's text goes there.
        let mut hint = Node::new(Role::Label);
        let mut nodes = Vec::new();
        let focus = match self {
            TrashModal::ChooseThread { threads, selected, .. } => {
                hint.set_value(if threads.is_empty() {
                    format!("{body} Escape cancels.")
                } else {
                    format!("{body} Up and Down choose a thread, Enter restores into it, Escape cancels.")
                });
                dialog.set_children(vec![HINT, LIST, CANCEL]);
                let mut list = Node::new(Role::ListBox);
                list.set_label("Threads");
                list.set_children((0..threads.len()).map(option).collect::<Vec<_>>());
                nodes.push((LIST, list));
                for (i, (_, name)) in threads.iter().enumerate() {
                    let mut o = Node::new(Role::ListBoxOption);
                    o.set_label(name.clone());
                    o.set_selected(i == *selected);
                    o.add_action(Action::Click);
                    o.add_action(Action::Focus);
                    nodes.push((option(i), o));
                }
                if threads.is_empty() {
                    CANCEL
                } else {
                    option(*selected)
                }
            }
            TrashModal::ConfirmEmpty { .. } => {
                hint.set_value(body);
                dialog.set_children(vec![HINT, CONFIRM, CANCEL]);
                nodes.push((CONFIRM, button("Empty trash")));
                CANCEL
            }
        };
        nodes.push((CANCEL, button("Cancel")));
        nodes.insert(0, (HINT, hint));
        nodes.insert(0, (DIALOG, dialog));
        TreeUpdate { nodes, tree: Some(Tree::new(DIALOG)), tree_id: TreeId::ROOT, focus }
    }
}

/// The modal's title and body text.
fn modal_text(modal: &TrashModal) -> (String, String) {
    match modal {
        TrashModal::ChooseThread { title, threads, .. } => (
            title.clone(),
            if threads.is_empty() {
                "There is no thread to restore into. Create a thread first (\u{201c}New lane\u{201d}), then restore.".to_string()
            } else {
                "Choose the thread it goes back to. To bring a deleted thread back, create a new thread first.".to_string()
            },
        ),
        TrashModal::ConfirmEmpty { count } => (
            "Empty the trash?".to_string(),
            format!(
                "{count} document{} will be deleted for good, with their versions. This cannot be undone.",
                if *count == 1 { "" } else { "s" }
            ),
        ),
    }
}

/// (panel, thread rows drawn, each with its index into `threads`, confirm
/// button, cancel button).
fn modal_geom(modal: &TrashModal, w: f64, h: f64) -> (Rect, Vec<(usize, Rect)>, Option<Rect>, Rect) {
    let n_rows = match modal {
        TrashModal::ChooseThread { threads, .. } => threads.len().clamp(1, MAX_ROWS),
        TrashModal::ConfirmEmpty { .. } => 0,
    };
    let body_h = match modal {
        TrashModal::ChooseThread { threads, .. } => {
            70.0 + n_rows as f64 * (ROW_H + 4.0) + if threads.len() > MAX_ROWS { MORE_H } else { 0.0 }
        }
        TrashModal::ConfirmEmpty { .. } => 96.0,
    };
    let ph = body_h + 60.0;
    let x0 = ((w - MW) * 0.5).max(8.0);
    let y0 = ((h - BOTTOM_CHROME - AXIS_H - ph) * 0.5 + AXIS_H).max(AXIS_H + 8.0);
    let panel = Rect::new(x0, y0, x0 + MW, y0 + ph);
    let rows = match modal {
        TrashModal::ChooseThread { threads, top, .. } => (0..threads.len().saturating_sub(*top).min(MAX_ROWS))
            .map(|i| {
                let ry = y0 + 70.0 + i as f64 * (ROW_H + 4.0);
                (top + i, Rect::new(x0 + 16.0, ry, x0 + MW - 16.0, ry + ROW_H))
            })
            .collect(),
        TrashModal::ConfirmEmpty { .. } => vec![],
    };
    let by0 = panel.y1 - 16.0 - 34.0;
    let bw = 130.0;
    let cancel = Rect::new(panel.x1 - 16.0 - bw, by0, panel.x1 - 16.0, by0 + 34.0);
    let confirm = match modal {
        TrashModal::ConfirmEmpty { .. } => Some(Rect::new(cancel.x0 - 12.0 - bw, by0, cancel.x0 - 12.0, by0 + 34.0)),
        TrashModal::ChooseThread { .. } => None,
    };
    (panel, rows, confirm, cancel)
}

pub(crate) fn modal_hit(modal: &TrashModal, w: f64, h: f64, p: Point) -> ModalHit {
    let (_panel, rows, confirm, cancel) = modal_geom(modal, w, h);
    if let Some((i, _)) = rows.iter().find(|(_, r)| r.contains(p)) {
        return ModalHit::Thread(*i);
    }
    if confirm.is_some_and(|c| c.contains(p)) {
        return ModalHit::Confirm;
    }
    if cancel.contains(p) {
        return ModalHit::Cancel;
    }
    ModalHit::Nothing
}

pub(crate) fn draw_modal(scene: &mut Scene, shaper: &mut TextShaper, modal: &TrashModal, w: f64, h: f64, cursor: Point) {
    let (panel, rows, confirm, cancel) = modal_geom(modal, w, h);
    let danger = Color::from_rgb8(205, 92, 80);
    let accent = match modal {
        TrashModal::ConfirmEmpty { .. } => danger,
        TrashModal::ChooseThread { .. } => pal().accent,
    };
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.45), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(panel, 11.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, accent, None, &rr);

    let (title, body) = modal_text(modal);
    let tl = shaper.shape(&title, (MW - 32.0) as f32, 15.0);
    draw_text(scene, &tl, Affine::translate((panel.x0 + 16.0, panel.y0 + 16.0)), pal().text);
    let bl = shaper.shape(&body, (MW - 32.0) as f32, 12.5);
    draw_text(scene, &bl, Affine::translate((panel.x0 + 16.0, panel.y0 + 40.0)), pal().text_body);

    if let TrashModal::ChooseThread { threads, selected, top, .. } = modal {
        for (idx, r) in &rows {
            let Some((_, name)) = threads.get(*idx) else { continue };
            let bg = if r.contains(cursor) { pal().surface_hover } else { pal().surface };
            scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 6.0));
            if idx == selected {
                // The keyboard's row: a visible focus outline (WCAG 2.4.7).
                scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, crate::theme::chosen_row_outline(&pal()), None, &RoundedRect::from_rect(*r, 6.0));
            }
            let nl = shaper.shape(name, (r.width() - 20.0) as f32, 13.0);
            draw_text(scene, &nl, Affine::translate((r.x0 + 10.0, r.y0 + (r.height() - nl.height() as f64) * 0.5)), pal().text);
        }
        if threads.len() > MAX_ROWS {
            let last = (top + MAX_ROWS).min(threads.len());
            let more = format!("Threads {}\u{2013}{last} of {} (scroll for more)", top + 1, threads.len());
            let ml = shaper.shape(&more, (MW - 32.0) as f32, 11.5);
            let y = rows.last().map_or(panel.y0 + 70.0, |(_, r)| r.y1 + 4.0);
            draw_text(scene, &ml, Affine::translate((panel.x0 + 16.0, y)), pal().text_dim);
        }
    }
    if let Some(c) = confirm {
        let bg = if c.contains(cursor) { danger } else { danger.with_alpha(0.82) };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(c, 7.0));
        let l = shaper.shape("Empty trash", 200.0, 13.5);
        draw_text(scene, &l, Affine::translate(centered(c, &l)), crate::theme::text_on_fill(crate::theme::seen_over(bg, pal().modal)));
    }
    let bg = if cancel.contains(cursor) { pal().border } else { pal().surface_alt };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(cancel, 7.0));
    let l = shaper.shape("Cancel (Esc)", 200.0, 13.5);
    draw_text(scene, &l, Affine::translate(centered(cancel, &l)), pal().text);
}

fn centered(r: Rect, l: &parley::Layout<crate::text::Brush>) -> (f64, f64) {
    (r.x0 + (r.width() - l.width() as f64) * 0.5, r.y0 + (r.height() - l.height() as f64) * 0.5)
}

// ---- the delete notice -----------------------------------------------------

// Ch. 12, SQ7: the button says its key, so the toast is wider than before
// (470 px, an 84 px button) and its text keeps the same room (350 px).
const TOAST_W: f64 = 510.0;
const TOAST_H: f64 = 46.0;
const UNDO_W: f64 = 120.0;

/// The toast's button, drawn and spoken (ch. 12, SQ7: Ctrl+Z presses it when
/// a click on it would).
pub(crate) const UNDO_LABEL: &str = "Undo (Ctrl+Z)";

/// (toast, Undo button): bottom-centre of the canvas, above the taskbar.
pub(crate) fn toast_geom(w: f64, h: f64) -> (Rect, Rect) {
    let x0 = ((w - TOAST_W) * 0.5).max(8.0);
    let y1 = h - BOTTOM_CHROME - 20.0;
    let toast = Rect::new(x0, y1 - TOAST_H, x0 + TOAST_W, y1);
    let undo = Rect::new(toast.x1 - 10.0 - UNDO_W, toast.y0 + 8.0, toast.x1 - 10.0, toast.y1 - 8.0);
    (toast, undo)
}

/// The notice and its Undo, while it is shown.
pub(crate) fn draw_toast(scene: &mut Scene, shaper: &mut TextShaper, notice: &DeleteNotice, w: f64, h: f64, cursor: Point) {
    if !notice.shown(Instant::now()) {
        return;
    }
    let (toast, undo) = toast_geom(w, h);
    let rr = RoundedRect::from_rect(toast, 9.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().popup, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().border, None, &rr);
    let tl = shaper.shape(&notice.text(), (TOAST_W - UNDO_W - 40.0) as f32, 13.0);
    draw_text(scene, &tl, Affine::translate((toast.x0 + 14.0, toast.y0 + (TOAST_H - tl.height() as f64) * 0.5)), pal().text);
    let bg = if undo.contains(cursor) { pal().accent } else { pal().accent.with_alpha(0.82) };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(undo, 6.0));
    let ul = shaper.shape(UNDO_LABEL, (UNDO_W - 4.0) as f32, 13.5);
    draw_text(scene, &ul, Affine::translate(centered(undo, &ul)), crate::theme::text_on_fill(crate::theme::seen_over(bg, pal().popup)));
}
