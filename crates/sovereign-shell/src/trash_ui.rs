//! Drawing and hit-testing for the trash and the delete notice (C-DOC adoption
//! step 5, S5-8 and S5-11; decided by Céline 2026-10-17: one-click delete with
//! Undo, the trash in Settings). The state and the service calls are in
//! `trash.rs` and `delete_notice.rs`; `App` owns the instances and routes
//! clicks here. Manual checks: `TRASH_CHECKLIST.md`.

use std::time::Instant;

use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::{Color, Fill};
use vello::Scene;

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
pub(crate) enum TrashModal {
    ChooseThread { target: RestoreTarget, title: String, threads: Vec<(String, String)> },
    ConfirmEmpty { count: usize },
}

/// A click in a trash modal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModalHit {
    Thread(usize),
    Confirm,
    Cancel,
    Nothing,
}

const MW: f64 = 440.0;
const ROW_H: f64 = 32.0;
const MAX_ROWS: usize = 8;

/// (panel, thread rows, confirm button, cancel button).
fn modal_geom(modal: &TrashModal, w: f64, h: f64) -> (Rect, Vec<Rect>, Option<Rect>, Rect) {
    let n_rows = match modal {
        TrashModal::ChooseThread { threads, .. } => threads.len().clamp(1, MAX_ROWS),
        TrashModal::ConfirmEmpty { .. } => 0,
    };
    let body_h = match modal {
        TrashModal::ChooseThread { .. } => 70.0 + n_rows as f64 * (ROW_H + 4.0),
        TrashModal::ConfirmEmpty { .. } => 96.0,
    };
    let ph = body_h + 60.0;
    let x0 = ((w - MW) * 0.5).max(8.0);
    let y0 = ((h - BOTTOM_CHROME - AXIS_H - ph) * 0.5 + AXIS_H).max(AXIS_H + 8.0);
    let panel = Rect::new(x0, y0, x0 + MW, y0 + ph);
    let rows = match modal {
        TrashModal::ChooseThread { threads, .. } => (0..threads.len().min(MAX_ROWS))
            .map(|i| {
                let ry = y0 + 70.0 + i as f64 * (ROW_H + 4.0);
                Rect::new(x0 + 16.0, ry, x0 + MW - 16.0, ry + ROW_H)
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
    if let Some(i) = rows.iter().position(|r| r.contains(p)) {
        return ModalHit::Thread(i);
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

    let (title, body) = match modal {
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
    };
    let tl = shaper.shape(&title, (MW - 32.0) as f32, 15.0);
    draw_text(scene, &tl, Affine::translate((panel.x0 + 16.0, panel.y0 + 16.0)), pal().text);
    let bl = shaper.shape(&body, (MW - 32.0) as f32, 12.5);
    draw_text(scene, &bl, Affine::translate((panel.x0 + 16.0, panel.y0 + 40.0)), pal().text_body);

    if let TrashModal::ChooseThread { threads, .. } = modal {
        for (r, (_, name)) in rows.iter().zip(threads.iter()) {
            let bg = if r.contains(cursor) { pal().surface_hover } else { pal().surface };
            scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 6.0));
            let nl = shaper.shape(name, (r.width() - 20.0) as f32, 13.0);
            draw_text(scene, &nl, Affine::translate((r.x0 + 10.0, r.y0 + (r.height() - nl.height() as f64) * 0.5)), pal().text);
        }
    }
    if let Some(c) = confirm {
        let bg = if c.contains(cursor) { danger } else { danger.with_alpha(0.82) };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(c, 7.0));
        let l = shaper.shape("Empty trash", 200.0, 13.5);
        draw_text(scene, &l, Affine::translate(centered(c, &l)), pal().on_accent);
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

const TOAST_W: f64 = 470.0;
const TOAST_H: f64 = 46.0;

/// (toast, Undo button): bottom-centre of the canvas, above the taskbar.
pub(crate) fn toast_geom(w: f64, h: f64) -> (Rect, Rect) {
    let x0 = ((w - TOAST_W) * 0.5).max(8.0);
    let y1 = h - BOTTOM_CHROME - 20.0;
    let toast = Rect::new(x0, y1 - TOAST_H, x0 + TOAST_W, y1);
    let undo = Rect::new(toast.x1 - 10.0 - 84.0, toast.y0 + 8.0, toast.x1 - 10.0, toast.y1 - 8.0);
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
    let tl = shaper.shape(&notice.text(), (TOAST_W - 120.0) as f32, 13.0);
    draw_text(scene, &tl, Affine::translate((toast.x0 + 14.0, toast.y0 + (TOAST_H - tl.height() as f64) * 0.5)), pal().text);
    let bg = if undo.contains(cursor) { pal().accent } else { pal().accent.with_alpha(0.82) };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(undo, 6.0));
    let ul = shaper.shape("Undo", 100.0, 13.5);
    draw_text(scene, &ul, Affine::translate(centered(undo, &ul)), pal().on_accent);
}
