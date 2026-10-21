//! The thread chooser of the section "Documents whose thread was deleted"
//! (S5-12 W4, in the Alerts window): choosing the existing thread to move a
//! document (or all of a deleted thread's documents) into. State and service
//! calls are in `orphans.rs`; `App` owns the chooser and routes clicks, keys
//! and the wheel here. Naming a new thread is the shell's new-thread prompt
//! (`new_thread.rs`, THREAD-name N1–N7), opened for the document(s).
//! Manual checks: `TRASH_CHECKLIST.md`, section 7.
//!
//! Keyboard (WCAG 2.1.1): Up / Down (and Home / End, Page Up / Page Down)
//! move over every live thread, the list scrolling to keep the chosen one in
//! view; Enter moves into it; Esc cancels; 1 to 8 choose a visible row
//! directly. Accessibility: a modal dialog holding a list box whose options
//! are the threads, the chosen one selected and focused, and a Cancel button.

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key, NamedKey};

use crate::canvas::{AXIS_H, BOTTOM_CHROME};
use crate::text::{draw_text, TextShaper};
use crate::theme::pal;

/// What a choice applies to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum OrphanTarget {
    /// One document (its id).
    One(String),
    /// Every waiting document of a deleted thread (its id).
    Group(String),
}

/// The chooser: the live threads, the one the keyboard is on, and the first
/// row shown.
pub(crate) struct ThreadChooser {
    pub(crate) target: OrphanTarget,
    pub(crate) title: String,
    /// `(id, name)` of every live thread, in the store's order.
    pub(crate) threads: Vec<(String, String)>,
    /// Index into `threads` of the chosen row.
    pub(crate) selected: usize,
    /// Index into `threads` of the first visible row.
    pub(crate) top: usize,
}

/// What an input asks of the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChooserStep {
    /// Keep the chooser open.
    Open,
    /// Move into this thread (its id); the chooser closes.
    Choose(String),
    /// Close; nothing done.
    Cancel,
}

/// How many rows are shown at once.
pub(crate) const VISIBLE_ROWS: usize = 8;
const MW: f64 = 440.0;
const ROW_H: f64 = 32.0;

struct Geom {
    panel: Rect,
    rows: Vec<Rect>,
    cancel: Rect,
}

impl ThreadChooser {
    pub(crate) fn new(target: OrphanTarget, title: String, threads: Vec<(String, String)>) -> Self {
        ThreadChooser { target, title, threads, selected: 0, top: 0 }
    }

    fn visible(&self) -> usize {
        self.threads.len().min(VISIBLE_ROWS)
    }

    /// Selects row `i` (clamped) and scrolls to keep it in view.
    pub(crate) fn select(&mut self, i: usize) {
        if self.threads.is_empty() {
            return;
        }
        self.selected = i.min(self.threads.len() - 1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + VISIBLE_ROWS {
            self.top = self.selected + 1 - VISIBLE_ROWS;
        }
    }

    /// The wheel: scrolls the list by `lines` rows (positive: up), keeping the
    /// chosen row in view.
    pub(crate) fn scroll(&mut self, lines: i64) {
        let max_top = self.threads.len().saturating_sub(VISIBLE_ROWS) as i64;
        self.top = (self.top as i64 - lines).clamp(0, max_top) as usize;
        if self.selected < self.top {
            self.selected = self.top;
        } else if self.selected >= self.top + VISIBLE_ROWS {
            self.selected = self.top + VISIBLE_ROWS - 1;
        }
    }

    fn choose(&self, i: usize) -> ChooserStep {
        match self.threads.get(i) {
            Some((id, _)) => ChooserStep::Choose(id.clone()),
            None => ChooserStep::Open,
        }
    }

    /// A key, while the chooser is open (it owns the keyboard).
    pub(crate) fn key(&mut self, key: &Key) -> ChooserStep {
        let page = VISIBLE_ROWS;
        match key {
            Key::Named(NamedKey::Escape) => ChooserStep::Cancel,
            Key::Named(NamedKey::Enter) => self.choose(self.selected),
            Key::Named(NamedKey::ArrowDown) => {
                self.select(self.selected + 1);
                ChooserStep::Open
            }
            Key::Named(NamedKey::ArrowUp) => {
                self.select(self.selected.saturating_sub(1));
                ChooserStep::Open
            }
            Key::Named(NamedKey::Home) => {
                self.select(0);
                ChooserStep::Open
            }
            Key::Named(NamedKey::End) => {
                self.select(usize::MAX);
                ChooserStep::Open
            }
            Key::Named(NamedKey::PageDown) => {
                self.select(self.selected + page);
                ChooserStep::Open
            }
            Key::Named(NamedKey::PageUp) => {
                self.select(self.selected.saturating_sub(page));
                ChooserStep::Open
            }
            Key::Character(c) => match c.parse::<usize>() {
                Ok(n) if (1..=self.visible()).contains(&n) => self.choose(self.top + n - 1),
                _ => ChooserStep::Open,
            },
            _ => ChooserStep::Open,
        }
    }

    fn geom(&self, w: f64, h: f64) -> Geom {
        let body_h = 70.0 + self.visible().max(1) as f64 * (ROW_H + 4.0) + 22.0;
        let ph = body_h + 60.0;
        let x0 = ((w - MW) * 0.5).max(8.0);
        let y0 = ((h - BOTTOM_CHROME - AXIS_H - ph) * 0.5 + AXIS_H).max(AXIS_H + 8.0);
        let panel = Rect::new(x0, y0, x0 + MW, y0 + ph);
        let rows = (0..self.visible())
            .map(|i| {
                let ry = y0 + 70.0 + i as f64 * (ROW_H + 4.0);
                Rect::new(x0 + 16.0, ry, x0 + MW - 16.0, ry + ROW_H)
            })
            .collect();
        let by0 = panel.y1 - 16.0 - 34.0;
        let cancel = Rect::new(panel.x1 - 16.0 - 130.0, by0, panel.x1 - 16.0, by0 + 34.0);
        Geom { panel, rows, cancel }
    }

    /// A click: a row moves into its thread; Cancel or a click outside closes.
    pub(crate) fn click(&mut self, w: f64, h: f64, p: Point) -> ChooserStep {
        let g = self.geom(w, h);
        if let Some(i) = g.rows.iter().position(|r| r.contains(p)) {
            return self.choose(self.top + i);
        }
        if g.cancel.contains(p) || !g.panel.contains(p) {
            return ChooserStep::Cancel;
        }
        ChooserStep::Open
    }

    /// Whether `p` is over the chooser (the wheel scrolls it there).
    pub(crate) fn contains(&self, w: f64, h: f64, p: Point) -> bool {
        self.geom(w, h).panel.contains(p)
    }

    pub(crate) fn draw(&self, scene: &mut Scene, shaper: &mut TextShaper, w: f64, h: f64, cursor: Point) {
        let g = self.geom(w, h);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.45), None, &Rect::new(0.0, 0.0, w, h));
        let rr = RoundedRect::from_rect(g.panel, 11.0);
        scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
        scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().accent, None, &rr);

        let tl = shaper.shape(&self.title, (MW - 32.0) as f32, 15.0);
        draw_text(scene, &tl, Affine::translate((g.panel.x0 + 16.0, g.panel.y0 + 16.0)), pal().text);
        let body = if self.threads.is_empty() {
            "There is no thread to move into. Choose \u{201c}New thread\u{2026}\u{201d} instead.".to_string()
        } else {
            "Choose the thread it goes to: Up and Down, then Enter (or click). Esc cancels.".to_string()
        };
        let bl = shaper.shape(&body, (MW - 32.0) as f32, 12.5);
        draw_text(scene, &bl, Affine::translate((g.panel.x0 + 16.0, g.panel.y0 + 40.0)), pal().text_body);

        for (i, r) in g.rows.iter().enumerate() {
            let idx = self.top + i;
            let Some((_, name)) = self.threads.get(idx) else { continue };
            let bg = if r.contains(cursor) { pal().surface_hover } else { pal().surface };
            scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 6.0));
            if idx == self.selected {
                // The keyboard's row: a visible focus outline (WCAG 2.4.7).
                scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, pal().accent, None, &RoundedRect::from_rect(*r, 6.0));
            }
            let label = if i < 9 { format!("{}  {name}", i + 1) } else { name.clone() };
            let nl = shaper.shape(&label, (r.width() - 20.0) as f32, 13.0);
            draw_text(scene, &nl, Affine::translate((r.x0 + 10.0, r.y0 + (r.height() - nl.height() as f64) * 0.5)), pal().text);
        }
        if self.threads.len() > VISIBLE_ROWS {
            let last = (self.top + VISIBLE_ROWS).min(self.threads.len());
            let more = format!("Threads {}\u{2013}{last} of {} (scroll for more)", self.top + 1, self.threads.len());
            let ml = shaper.shape(&more, (MW - 32.0) as f32, 11.5);
            let y = g.rows.last().map_or(g.panel.y0 + 70.0, |r| r.y1 + 4.0);
            draw_text(scene, &ml, Affine::translate((g.panel.x0 + 16.0, y)), pal().text_dim);
        }
        let bg = if g.cancel.contains(cursor) { pal().border } else { pal().surface_alt };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(g.cancel, 7.0));
        let l = shaper.shape("Cancel (Esc)", 200.0, 13.5);
        draw_text(
            scene,
            &l,
            Affine::translate((
                g.cancel.x0 + (g.cancel.width() - l.width() as f64) * 0.5,
                g.cancel.y0 + (g.cancel.height() - l.height() as f64) * 0.5,
            )),
            pal().text,
        );
    }

    /// The chooser's accessibility tree: a modal dialog named by its title,
    /// holding a list box of every live thread (the chosen one selected, and
    /// focused) and a Cancel button. The ids are local; the shell moves them
    /// into its own range (`new_thread::inline_tree`).
    pub(crate) fn accessibility_tree(&self) -> TreeUpdate {
        const DIALOG: NodeId = NodeId(1);
        const LIST: NodeId = NodeId(2);
        const CANCEL: NodeId = NodeId(3);
        const HINT: NodeId = NodeId(4);
        let option = |i: usize| NodeId(16 + i as u64);

        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(self.title.clone());
        dialog.set_modal();
        dialog.set_children(vec![HINT, LIST, CANCEL]);
        let mut hint = Node::new(Role::Label);
        hint.set_label(if self.threads.is_empty() {
            "There is no thread to move into. Escape cancels; choose New thread instead.".to_string()
        } else {
            "Up and Down choose a thread, Enter moves into it, Escape cancels.".to_string()
        });
        let mut list = Node::new(Role::ListBox);
        list.set_label("Threads");
        list.set_children((0..self.threads.len()).map(option).collect::<Vec<_>>());
        let mut cancel = Node::new(Role::Button);
        cancel.set_label("Cancel");
        cancel.add_action(Action::Click);
        cancel.add_action(Action::Focus);

        let mut nodes = vec![(DIALOG, dialog), (HINT, hint), (LIST, list), (CANCEL, cancel)];
        for (i, (_, name)) in self.threads.iter().enumerate() {
            let mut o = Node::new(Role::ListBoxOption);
            o.set_label(name.clone());
            o.set_selected(i == self.selected);
            o.add_action(Action::Click);
            o.add_action(Action::Focus);
            nodes.push((option(i), o));
        }
        let focus = if self.threads.is_empty() { CANCEL } else { option(self.selected) };
        TreeUpdate { nodes, tree: Some(Tree::new(DIALOG)), tree_id: TreeId::ROOT, focus }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chooser(n: usize) -> ThreadChooser {
        let threads = (0..n).map(|i| (format!("thread:t{i}"), format!("Thread {i}"))).collect();
        ThreadChooser::new(OrphanTarget::One("document:a".into()), "Move".into(), threads)
    }

    #[test]
    fn the_keyboard_reaches_every_live_thread_and_enter_moves_into_it() {
        let mut c = chooser(20);
        for i in 1..20 {
            assert_eq!(c.key(&Key::Named(NamedKey::ArrowDown)), ChooserStep::Open);
            assert_eq!(c.selected, i);
            assert!(c.selected >= c.top && c.selected < c.top + VISIBLE_ROWS, "kept in view");
        }
        assert_eq!(c.key(&Key::Named(NamedKey::ArrowDown)), ChooserStep::Open);
        assert_eq!(c.selected, 19, "stays on the last");
        assert_eq!(c.key(&Key::Named(NamedKey::Enter)), ChooserStep::Choose("thread:t19".into()));
        c.key(&Key::Named(NamedKey::Home));
        assert_eq!((c.selected, c.top), (0, 0));
        c.key(&Key::Named(NamedKey::End));
        assert_eq!(c.selected, 19);
        c.key(&Key::Named(NamedKey::ArrowUp));
        assert_eq!(c.selected, 18);
        assert_eq!(c.key(&Key::Named(NamedKey::Escape)), ChooserStep::Cancel);
    }

    #[test]
    fn digits_choose_a_visible_row() {
        let mut c = chooser(12);
        c.key(&Key::Named(NamedKey::End));
        assert_eq!(c.top, 4);
        assert_eq!(c.key(&Key::Character("1".into())), ChooserStep::Choose("thread:t4".into()));
        assert_eq!(c.key(&Key::Character("9".into())), ChooserStep::Open, "only the visible rows");
    }

    #[test]
    fn the_accessibility_tree_lists_every_thread_and_focuses_the_chosen_one() {
        let mut c = chooser(12);
        c.select(10);
        let t = c.accessibility_tree();
        let options: Vec<_> = t.nodes.iter().filter(|(_, n)| n.role() == Role::ListBoxOption).collect();
        assert_eq!(options.len(), 12, "every live thread, not only the visible rows");
        let focused = t.nodes.iter().find(|(id, _)| *id == t.focus).unwrap();
        assert_eq!(focused.1.label(), Some("Thread 10"));
        assert_eq!(focused.1.is_selected(), Some(true));
        let empty = chooser(0).accessibility_tree();
        assert_eq!(empty.nodes.iter().find(|(id, _)| *id == empty.focus).unwrap().1.role(), Role::Button);
    }

    #[test]
    fn a_click_outside_cancels_and_a_row_chooses_its_thread() {
        let mut c = chooser(3);
        assert_eq!(c.click(1200.0, 800.0, Point::new(2.0, 2.0)), ChooserStep::Cancel);
        let g = c.geom(1200.0, 800.0);
        assert_eq!(c.click(1200.0, 800.0, g.rows[2].center()), ChooserStep::Choose("thread:t2".into()));
        assert_eq!(c.click(1200.0, 800.0, g.cancel.center()), ChooserStep::Cancel);
    }

    #[test]
    fn the_wheel_scrolls_and_keeps_the_chosen_row_in_view() {
        let mut c = chooser(20);
        c.scroll(-5);
        assert_eq!(c.top, 5);
        assert_eq!(c.selected, 5);
        c.scroll(-100);
        assert_eq!(c.top, 12);
        c.scroll(100);
        assert_eq!(c.top, 0);
        assert!(c.selected < VISIBLE_ROWS);
    }
}
