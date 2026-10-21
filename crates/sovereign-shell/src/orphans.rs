//! Documents whose thread was deleted, in the shell's Alerts window (S5-12,
//! contract W1–W6 in `doc/plans/C-DOC-adoption.md`; spec ch. 03, *Thread
//! deletion and the fallback lane*, decided by Céline 2026-10-17): a section
//! "Documents whose thread was deleted" beside the alerts on changes from
//! paired devices, each document with three ways out — move it to an existing
//! thread, create a new thread for it, put it in the trash — and, for a
//! deleted thread with several documents, the same for all of them at once.
//! The canvas status line shows how many are waiting (W5). The section is
//! read from the document service's computed condition; it never raises,
//! replaces or dismisses an alert (W6).
//!
//! This file holds the section's state, its service calls and its geometry;
//! it is drawn by `panels::draw_alerts`, and the thread chooser and the
//! new-thread name field are in `orphans_ui.rs`. Tests: `orphans_tests.rs`;
//! what is drawn is checked by hand (`TRASH_CHECKLIST.md`, section 7).

use std::collections::{HashMap, HashSet};

use sovereign_docs::{DocumentService, OrphanResolution};
use vello::kurbo::Rect;

/// The section's heading in the Alerts window (W5).
pub(crate) const HEADING: &str = "Documents whose thread was deleted";

/// What the keys do on a line of the section (said in the window and to a
/// screen reader).
pub(crate) const ORPHAN_KEYS: &str = "On a document whose thread was deleted: M moves it to a thread, N puts it in a \
                                      new thread, T puts it in the trash; with Shift, all the documents of its deleted \
                                      thread. Enter or O opens it.";

/// One of the three ways out (W4), as offered on each line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrphanAction {
    /// Move it to an existing thread (the user chooses which).
    MoveTo,
    /// Create a new thread (the user names it) and move it there.
    NewThread,
    /// Put it in the trash.
    Trash,
}

impl OrphanAction {
    /// The button's label on a line.
    pub(crate) fn label(self) -> &'static str {
        match self {
            OrphanAction::MoveTo => "Move to a thread\u{2026}",
            OrphanAction::NewThread => "New thread\u{2026}",
            OrphanAction::Trash => "Trash",
        }
    }

    /// The button's label on a group ("all of <old name>").
    pub(crate) fn group_label(self) -> &'static str {
        match self {
            OrphanAction::MoveTo => "Move all to a thread\u{2026}",
            OrphanAction::NewThread => "All in a new thread\u{2026}",
            OrphanAction::Trash => "Trash all",
        }
    }
}

/// The actions every line offers, in order.
pub(crate) const ACTIONS: [OrphanAction; 3] = [OrphanAction::MoveTo, OrphanAction::NewThread, OrphanAction::Trash];

/// One document whose thread was deleted — and no document text.
#[derive(Clone, Debug)]
pub(crate) struct OrphanLine {
    pub(crate) id: String,
    pub(crate) title: String,
    /// The deleted thread it is still in.
    pub(crate) thread_id: String,
    /// The deleted thread's old name (from its tombstone); empty when unknown.
    pub(crate) old_thread_name: String,
    /// The device's name when it is a paired device, else its id.
    pub(crate) device: String,
}

impl OrphanLine {
    /// The line's notice, in words: the document's title, and that its thread
    /// (named) was deleted. No document text. Wording: "<title> — its thread
    /// <old name> was deleted." (contract W5, **[choice]**).
    pub(crate) fn notice(&self) -> String {
        let title = crate::canvas::display_title(&self.title);
        if self.old_thread_name.is_empty() {
            format!("\u{201c}{title}\u{201d} \u{2014} its thread was deleted.")
        } else {
            format!("\u{201c}{title}\u{201d} \u{2014} its thread \u{201c}{}\u{201d} was deleted.", self.old_thread_name)
        }
    }

    /// Where its current state came from, for the line under the notice.
    pub(crate) fn origin(&self) -> String {
        format!("Last written on {}.", self.device)
    }
}

/// The documents of one deleted thread, for "all of <old name>".
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct OrphanGroup {
    /// The deleted thread.
    pub(crate) thread_id: String,
    /// What the group action says (names the deleted thread).
    pub(crate) label: String,
    pub(crate) ids: Vec<String>,
}

/// A row of the section, as drawn: its heading, a deleted thread's group
/// row, or one document.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SectionRow {
    Heading,
    /// Index into [`OrphansSection::groups`].
    Group(usize),
    /// Index into [`OrphansSection::lines`].
    Line(usize),
}

/// Heights of the section's rows.
pub(crate) const HEADING_H: f64 = 40.0;
/// A group row whose thread holds one document: its name only.
pub(crate) const GROUP_LABEL_H: f64 = 30.0;
/// A group row with its three "all" buttons.
pub(crate) const GROUP_H: f64 = 66.0;
pub(crate) const LINE_H: f64 = 86.0;

/// The section.
#[derive(Clone)]
pub(crate) struct OrphansSection {
    lines: Vec<OrphanLine>,
    /// The line the keyboard acts on, when the keyboard is in the section.
    pub(crate) selected: Option<usize>,
}

impl OrphansSection {
    /// A section with nothing in it.
    pub(crate) fn empty() -> Self {
        OrphansSection { lines: Vec::new(), selected: None }
    }

    /// The documents whose thread was deleted, read from the service; device
    /// names from `device_names` (peer id → name).
    pub(crate) async fn load(docs: &DocumentService, device_names: &HashMap<String, String>) -> Result<Self, String> {
        let list = docs
            .orphans_of_deleted_threads()
            .await
            .map_err(|e| format!("The documents whose thread was deleted couldn't be read ({e})."))?;
        let mut lines: Vec<OrphanLine> = list
            .into_iter()
            .map(|o| OrphanLine {
                device: device_names.get(&o.device).cloned().unwrap_or_else(|| o.device.clone()),
                id: o.id,
                title: o.title,
                thread_id: o.thread_id,
                old_thread_name: o.deleted_thread_name.unwrap_or_default(),
            })
            .collect();
        // One deleted thread's documents together, by its old name, then by title.
        lines.sort_by(|a, b| {
            (a.old_thread_name.to_lowercase(), &a.thread_id, a.title.to_lowercase(), &a.id).cmp(&(
                b.old_thread_name.to_lowercase(),
                &b.thread_id,
                b.title.to_lowercase(),
                &b.id,
            ))
        });
        Ok(OrphansSection { lines, selected: None })
    }

    /// The lines shown.
    pub(crate) fn lines(&self) -> &[OrphanLine] {
        &self.lines
    }

    /// How many documents are waiting (the status line shows it).
    pub(crate) fn count(&self) -> usize {
        self.lines.len()
    }

    /// One group per deleted thread, its documents in the order of `lines`.
    pub(crate) fn groups(&self) -> Vec<OrphanGroup> {
        let mut out: Vec<OrphanGroup> = Vec::new();
        for l in &self.lines {
            match out.iter_mut().find(|g| g.thread_id == l.thread_id) {
                Some(g) => g.ids.push(l.id.clone()),
                None => out.push(OrphanGroup {
                    thread_id: l.thread_id.clone(),
                    label: group_label(&l.old_thread_name),
                    ids: vec![l.id.clone()],
                }),
            }
        }
        out
    }

    /// One way out for one document (W4), through the service; the section
    /// drops it at once. Refused (deleted or unknown thread, empty name, a
    /// document no longer waiting): nothing changes and the error says why.
    pub(crate) async fn resolve(
        &mut self,
        docs: &DocumentService,
        id: &str,
        resolution: OrphanResolution,
    ) -> Result<(), String> {
        let r = docs.resolve_orphan(id, resolution).await;
        self.settle(docs, r.as_ref().ok().map(|d| d.documents.as_slice())).await;
        r.map(|_| ()).map_err(|e| e.to_string())
    }

    /// The same way out for every document of the deleted thread
    /// `deleted_thread_id` (W4, the group action); the section drops them at
    /// once. The number of documents resolved.
    pub(crate) async fn resolve_group(
        &mut self,
        docs: &DocumentService,
        deleted_thread_id: &str,
        resolution: OrphanResolution,
    ) -> Result<usize, String> {
        let r = docs.resolve_orphans_of_thread(deleted_thread_id, resolution).await;
        self.settle(docs, r.as_ref().ok().map(|d| d.documents.as_slice())).await;
        r.map(|d| d.documents.len()).map_err(|e| e.to_string())
    }

    /// After a resolution, succeeded or refused: drops what was resolved, and
    /// whatever is no longer waiting (resolved elsewhere meanwhile), so the
    /// section shows what is true now. Keeps the lines when the condition
    /// cannot be read.
    async fn settle(&mut self, docs: &DocumentService, resolved: Option<&[String]>) {
        if let Some(done) = resolved {
            self.lines.retain(|l| !done.contains(&l.id));
        }
        if let Ok(now) = docs.orphans_of_deleted_threads().await {
            let waiting: HashSet<String> = now.into_iter().map(|o| o.id).collect();
            self.lines.retain(|l| waiting.contains(&l.id));
        }
        self.selected = match (self.selected, self.lines.len()) {
            (_, 0) => None,
            (Some(i), n) => Some(i.min(n - 1)),
            (None, _) => None,
        };
    }

    /// Readable lines for the accessibility tree: the heading, each document's
    /// notice with its three actions, each group action.
    pub(crate) fn a11y_lines(&self) -> Vec<String> {
        if self.lines.is_empty() {
            return Vec::new();
        }
        let groups = self.groups();
        let mut out = vec![HEADING.to_string()];
        for row in self.rows() {
            match row {
                SectionRow::Heading => {}
                SectionRow::Group(g) => {
                    let g = &groups[g];
                    let n = g.ids.len();
                    if n > 1 {
                        out.push(format!(
                            "{} ({n} documents). Actions for all: {}, {}, {}.",
                            g.label,
                            action_words(OrphanAction::MoveTo, true),
                            action_words(OrphanAction::NewThread, true),
                            action_words(OrphanAction::Trash, true),
                        ));
                    }
                }
                SectionRow::Line(i) => {
                    let l = &self.lines[i];
                    let sel = if self.selected == Some(i) { " Selected." } else { "" };
                    out.push(format!(
                        "{} {}{sel} Actions: {}, {}, {}.",
                        l.notice(),
                        l.origin(),
                        action_words(OrphanAction::MoveTo, false),
                        action_words(OrphanAction::NewThread, false),
                        action_words(OrphanAction::Trash, false),
                    ));
                }
            }
        }
        out.push(ORPHAN_KEYS.to_string());
        out
    }

    // ---- geometry (drawn in the Alerts window, below the alerts) ----------

    /// The section's rows, in drawing order: the heading, then each deleted
    /// thread's group row followed by its documents.
    pub(crate) fn rows(&self) -> Vec<SectionRow> {
        if self.lines.is_empty() {
            return Vec::new();
        }
        let mut out = vec![SectionRow::Heading];
        for (gi, g) in self.groups().iter().enumerate() {
            out.push(SectionRow::Group(gi));
            for id in &g.ids {
                if let Some(i) = self.lines.iter().position(|l| &l.id == id) {
                    out.push(SectionRow::Line(i));
                }
            }
        }
        out
    }

    /// Each row with its top offset in the section and its height.
    pub(crate) fn layout(&self) -> Vec<(SectionRow, f64, f64)> {
        let groups = self.groups();
        let mut y = 0.0;
        self.rows()
            .into_iter()
            .map(|r| {
                let h = match r {
                    SectionRow::Heading => HEADING_H,
                    SectionRow::Group(g) if groups[g].ids.len() > 1 => GROUP_H,
                    SectionRow::Group(_) => GROUP_LABEL_H,
                    SectionRow::Line(_) => LINE_H,
                };
                let at = (r, y, h);
                y += h;
                at
            })
            .collect()
    }

    /// The section's height (0 when nothing waits).
    pub(crate) fn height(&self) -> f64 {
        self.layout().last().map_or(0.0, |(_, y, h)| y + h)
    }

    /// The top offset of line `i` in the section, for keeping it in view.
    pub(crate) fn line_top(&self, i: usize) -> Option<f64> {
        self.layout().into_iter().find(|(r, _, _)| *r == SectionRow::Line(i)).map(|(_, y, _)| y)
    }

    /// What a click at `(x, y)` lands on, the section's top at `top` and its
    /// rows spanning `x0..x1`.
    pub(crate) fn hit(&self, top: f64, x0: f64, x1: f64, p: vello::kurbo::Point) -> Option<OrphanHit> {
        for (row, y, h) in self.layout() {
            let r = Rect::new(x0, top + y, x1, top + y + h);
            if !r.contains(p) {
                continue;
            }
            return match row {
                SectionRow::Heading => None,
                SectionRow::Group(g) => {
                    let n = self.groups().get(g).map_or(0, |g| g.ids.len());
                    if n < 2 {
                        return None;
                    }
                    group_buttons(r).into_iter().find(|(b, _)| b.contains(p)).map(|(_, a)| OrphanHit::Group(g, a))
                }
                SectionRow::Line(i) => Some(
                    line_buttons(r)
                        .into_iter()
                        .find(|(b, _)| b.contains(p))
                        .map_or(OrphanHit::Select(i), |(_, a)| OrphanHit::Line(i, a)),
                ),
            };
        }
        None
    }

    /// Moves the keyboard's line; `false` when it would leave the section at
    /// the top (the keyboard goes back to the alerts).
    pub(crate) fn move_selection(&mut self, down: bool) -> bool {
        let order: Vec<usize> =
            self.rows().into_iter().filter_map(|r| if let SectionRow::Line(i) = r { Some(i) } else { None }).collect();
        if order.is_empty() {
            self.selected = None;
            return false;
        }
        let pos = self.selected.and_then(|s| order.iter().position(|&i| i == s));
        match (pos, down) {
            (None, _) => {
                self.selected = Some(order[0]);
                true
            }
            (Some(p), true) => {
                self.selected = Some(order[(p + 1).min(order.len() - 1)]);
                true
            }
            (Some(0), false) => {
                self.selected = None;
                false
            }
            (Some(p), false) => {
                self.selected = Some(order[p - 1]);
                true
            }
        }
    }

    /// The line the keyboard acts on.
    pub(crate) fn selected_line(&self) -> Option<&OrphanLine> {
        self.selected.and_then(|i| self.lines.get(i))
    }
}

/// What a click in the section lands on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OrphanHit {
    /// A line's button.
    Line(usize, OrphanAction),
    /// A line, outside its buttons: selects it.
    Select(usize),
    /// A group's button (index into `groups()`).
    Group(usize, OrphanAction),
}

/// The group action's label: names the deleted thread.
fn group_label(old_name: &str) -> String {
    if old_name.is_empty() {
        "All of the deleted thread".to_string()
    } else {
        format!("All of \u{201c}{old_name}\u{201d}")
    }
}

fn action_words(a: OrphanAction, all: bool) -> String {
    match (a, all) {
        (OrphanAction::MoveTo, false) => "Move to a thread".into(),
        (OrphanAction::NewThread, false) => "Put in a new thread".into(),
        (OrphanAction::Trash, false) => "Put in the trash".into(),
        (OrphanAction::MoveTo, true) => "Move all to a thread".into(),
        (OrphanAction::NewThread, true) => "Put all in one new thread".into(),
        (OrphanAction::Trash, true) => "Put all in the trash".into(),
    }
}

const BTN_H: f64 = 26.0;
const BTN_GAP: f64 = 8.0;

fn buttons(row: Rect, widths: [(f64, OrphanAction); 3]) -> Vec<(Rect, OrphanAction)> {
    let y0 = row.y1 - BTN_H - 10.0;
    let mut x = row.x0 + 12.0;
    widths
        .into_iter()
        .map(|(w, a)| {
            let r = Rect::new(x, y0, x + w, y0 + BTN_H);
            x += w + BTN_GAP;
            (r, a)
        })
        .collect()
}

/// A line's three buttons, bottom-left of the row.
pub(crate) fn line_buttons(row: Rect) -> Vec<(Rect, OrphanAction)> {
    buttons(row, [(150.0, OrphanAction::MoveTo), (120.0, OrphanAction::NewThread), (70.0, OrphanAction::Trash)])
}

/// A group row's three buttons (when its thread holds several documents).
pub(crate) fn group_buttons(row: Rect) -> Vec<(Rect, OrphanAction)> {
    buttons(row, [(170.0, OrphanAction::MoveTo), (160.0, OrphanAction::NewThread), (90.0, OrphanAction::Trash)])
}

/// What the canvas status line says about the documents waiting: `None` when
/// there are none, else a segment naming how many.
pub(crate) fn status_segment(count: usize) -> Option<String> {
    match count {
        0 => None,
        1 => Some("1 document waits: its thread was deleted".into()),
        n => Some(format!("{n} documents wait: their thread was deleted")),
    }
}
