//! Pairing a device again (spec ch. 05, PR1 and PR2; v0.0.13, item C3).
//!
//! A device set up again comes back with a new peer id, and the pairing
//! carries only its name. So when a device joins under a name a paired device
//! already has, the source side asks whether it is that device, set up again:
//! « Replace it » or « Keep both ». « Replace it » removes the earlier pairing,
//! with its peer id out of the allow-list and the sealing map, as unpairing
//! does, and that device stops syncing with this one; « Keep both », or no
//! answer, keeps both. With several paired devices of that name, the question
//! lists them, and she picks one or keeps all. The window speaks of pairings,
//! not records: the store's record is how a pairing is kept, not what she sees.
//!
//! The event translator has already saved the new record when the app hears of
//! the pairing (`p2p::persist_paired_device`), so until she answers both
//! records are there: nothing is replaced without her answer.
//!
//! On the desktop the question is a modal window over everything but the
//! unsaved-edits prompt: what happened and what is asked, the list of the
//! devices of that name when there are several, and two buttons, « Replace it »
//! and « Keep both » (« Keep all » for several). To a screen reader it is a
//! modal dialog named by its title, its text, a list box of the devices when
//! there are several, and the two buttons, the focus on « Keep both ». The
//! keyboard: Tab, Left and Right move between the buttons, Up and Down choose
//! the device, Enter or Space presses the focused button, Escape closes the
//! window without an answer.

use accesskit::{Action, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use vello::kurbo::{Affine, Point, Rect, RoundedRect, Stroke};
use vello::peniko::Fill;
use vello::Scene;
use winit::keyboard::{Key, NamedKey};

use crate::app::App;
use crate::canvas::{AXIS_H, BOTTOM_CHROME};
use crate::text::{draw_text, TextShaper};
use crate::theme::pal;

/// The id range of the question's accessibility nodes in the shell's tree.
pub(crate) const SAME_NAME_NS: u64 = 1 << 52;

/// A paired device the question asks about.
#[derive(Clone, Debug)]
struct Candidate {
    peer_id: String,
    name: String,
    /// When it was paired (RFC 3339, as the store keeps it).
    paired_at: String,
}

impl Candidate {
    /// How it is told apart from the others of its name: the day it was
    /// paired, on this device's clock as History's rows are, and its peer id,
    /// shortened as the Devices window shows it. A time that does not parse is
    /// shown by its first ten characters, as it is stored.
    fn detail(&self) -> String {
        let day: String = match chrono::DateTime::parse_from_rfc3339(&self.paired_at) {
            Ok(t) => t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string(),
            Err(_) => self.paired_at.chars().take(10).collect(),
        };
        if day.is_empty() {
            format!("id {}", crate::panels::short_peer(&self.peer_id))
        } else {
            format!("paired {day}, id {}", crate::panels::short_peer(&self.peer_id))
        }
    }
}

/// Which of the two buttons has the keyboard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Replace,
    Keep,
}

/// The question, while it waits for her answer.
pub(crate) struct SameNameQuestion {
    new_peer: String,
    name: String,
    candidates: Vec<Candidate>,
    /// Index into `candidates` of the device « Replace it » replaces.
    selected: usize,
    /// Index into `candidates` of the first row drawn.
    top: usize,
    focus: Focus,
}

impl SameNameQuestion {
    /// The device that just joined: its peer id.
    pub(crate) fn new_peer(&self) -> &str {
        &self.new_peer
    }

    /// The paired devices of that name it asks about: (peer id, name), each
    /// one she can pick.
    #[cfg_attr(not(test), allow(dead_code))] // read by the tests
    pub(crate) fn candidates(&self) -> Vec<(String, String)> {
        self.candidates.iter().map(|c| (c.peer_id.clone(), c.name.clone())).collect()
    }

    fn several(&self) -> bool {
        self.candidates.len() > 1
    }

    /// The peer id « Replace it » would remove now.
    fn chosen(&self) -> Option<&str> {
        self.candidates.get(self.selected).map(|c| c.peer_id.as_str())
    }

    /// Chooses device `i` (clamped) and scrolls to keep it in view.
    fn select(&mut self, i: usize) {
        if self.candidates.is_empty() {
            return;
        }
        self.selected = i.min(self.candidates.len() - 1);
        if self.selected < self.top {
            self.top = self.selected;
        } else if self.selected >= self.top + MAX_ROWS {
            self.top = self.selected + 1 - MAX_ROWS;
        }
    }

    /// The wheel over the list: scrolls it by `lines` rows (positive: up).
    pub(crate) fn scroll(&mut self, lines: i64) {
        let max_top = self.candidates.len().saturating_sub(MAX_ROWS) as i64;
        self.top = (self.top as i64 - lines).clamp(0, max_top) as usize;
        if self.selected < self.top {
            self.selected = self.top;
        } else if self.selected >= self.top + MAX_ROWS {
            self.selected = self.top + MAX_ROWS - 1;
        }
    }

    fn keep_label(&self) -> &'static str {
        if self.several() {
            KEEP_ALL
        } else {
            KEEP_BOTH
        }
    }

    /// The window's title and text.
    fn text(&self) -> (String, String) {
        let name = &self.name;
        let title = format!("Is this {name} set up again?");
        let body = if let [only] = self.candidates.as_slice() {
            format!(
                "Paired with {name}. This device is already paired with a device named {name} ({}). \
                 If the new one is that device, set up again, replace the earlier pairing. If they are \
                 two devices, keep both: a device whose pairing is replaced stops syncing with this device.",
                only.detail()
            )
        } else {
            format!(
                "Paired with {name}. This device is already paired with {} devices named {name}. If the \
                 new one is one of them, set up again, choose it and replace its earlier pairing. Otherwise \
                 keep all: a device whose pairing is replaced stops syncing with this device.",
                self.candidates.len()
            )
        };
        (title, body)
    }

    /// A key while the question is open (it holds the keyboard).
    pub(crate) fn key(&mut self, key: &Key) -> QuestionKey {
        match key {
            Key::Named(NamedKey::Escape) => return QuestionKey::Dismiss,
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => return self.press(self.focus),
            Key::Named(NamedKey::Tab) | Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowRight) => {
                self.focus = match self.focus {
                    Focus::Replace => Focus::Keep,
                    Focus::Keep => Focus::Replace,
                };
            }
            Key::Named(NamedKey::ArrowDown) => self.select(self.selected + 1),
            Key::Named(NamedKey::ArrowUp) => self.select(self.selected.saturating_sub(1)),
            Key::Named(NamedKey::Home) => self.select(0),
            Key::Named(NamedKey::End) => self.select(usize::MAX),
            Key::Character(c) if c.as_str() == " " => return self.press(self.focus),
            _ => {}
        }
        QuestionKey::Open
    }

    fn press(&self, button: Focus) -> QuestionKey {
        match button {
            Focus::Keep => QuestionKey::Answer(SameNameAnswer::KeepBoth),
            Focus::Replace => match self.chosen() {
                Some(peer) => QuestionKey::Answer(SameNameAnswer::Replace(peer.to_string())),
                None => QuestionKey::Open,
            },
        }
    }

    /// Whether `p` is over the window (the wheel scrolls its list there).
    pub(crate) fn contains(&self, w: f64, h: f64, p: Point) -> bool {
        geom(self, w, h).panel.contains(p)
    }

    /// What a click at `p` does.
    pub(crate) fn click(&mut self, w: f64, h: f64, p: Point) -> QuestionKey {
        let g = geom(self, w, h);
        if let Some((i, _)) = g.rows.iter().find(|(_, r)| r.contains(p)) {
            self.select(*i);
            return QuestionKey::Open;
        }
        if g.replace.contains(p) {
            return self.press(Focus::Replace);
        }
        if g.keep.contains(p) {
            return self.press(Focus::Keep);
        }
        // Anywhere else: the question waits for her answer (Escape closes it).
        QuestionKey::Open
    }

    /// The question's accessibility tree: a modal dialog named by its title,
    /// its text, the list box of the devices when there are several (the
    /// chosen one selected), and the two buttons, the focus on the button the
    /// keyboard holds. The ids are local; the shell moves them into
    /// [`SAME_NAME_NS`] (`new_thread::inline_tree`).
    pub(crate) fn accessibility_tree(&self) -> TreeUpdate {
        let (title, body) = self.text();
        let mut dialog = Node::new(Role::Dialog);
        dialog.set_label(title);
        dialog.set_modal();
        // A `Label` is spoken by its value (v0.0.12 rc2, L1).
        let mut hint = Node::new(Role::Label);
        let keys = if self.several() {
            "Up and Down choose the device, Tab moves between the buttons, Escape closes without an answer, which keeps all."
        } else {
            "Tab moves between the buttons, Escape closes without an answer, which keeps both."
        };
        hint.set_value(format!("{body} {keys}"));
        let mut children = vec![A11Y_HINT];
        let mut nodes = Vec::new();
        if self.several() {
            let mut list = Node::new(Role::ListBox);
            list.set_label(format!("Devices named {}", self.name));
            list.set_children((0..self.candidates.len()).map(a11y_option).collect::<Vec<_>>());
            nodes.push((A11Y_LIST, list));
            for (i, c) in self.candidates.iter().enumerate() {
                let mut o = Node::new(Role::ListBoxOption);
                o.set_label(format!("{}, {}", c.name, c.detail()));
                o.set_selected(i == self.selected);
                o.add_action(Action::Click);
                o.add_action(Action::Focus);
                nodes.push((a11y_option(i), o));
            }
            children.push(A11Y_LIST);
        }
        let button = |label: &str| {
            let mut b = Node::new(Role::Button);
            b.set_label(label.to_string());
            b.add_action(Action::Click);
            b.add_action(Action::Focus);
            b
        };
        nodes.push((A11Y_REPLACE, button(REPLACE_IT)));
        nodes.push((A11Y_KEEP, button(self.keep_label())));
        children.push(A11Y_REPLACE);
        children.push(A11Y_KEEP);
        dialog.set_children(children);
        nodes.insert(0, (A11Y_HINT, hint));
        nodes.insert(0, (A11Y_DIALOG, dialog));
        let focus = match self.focus {
            Focus::Replace => A11Y_REPLACE,
            Focus::Keep => A11Y_KEEP,
        };
        TreeUpdate { nodes, tree: Some(Tree::new(A11Y_DIALOG)), tree_id: TreeId::ROOT, focus }
    }

    /// A screen reader's action on one of the question's nodes (`local` is the
    /// id within the question's range): Click on a button presses it, Focus
    /// moves the keyboard to it; Click or Focus on a device chooses it.
    pub(crate) fn a11y_action(&mut self, local: NodeId, action: Action) -> QuestionKey {
        let button = match local {
            A11Y_REPLACE => Some(Focus::Replace),
            A11Y_KEEP => Some(Focus::Keep),
            _ => None,
        };
        match (button, action) {
            (Some(b), Action::Click) => return self.press(b),
            (Some(b), Action::Focus) => self.focus = b,
            (None, Action::Click | Action::Focus) if local.0 >= A11Y_OPTIONS => {
                let i = (local.0 - A11Y_OPTIONS) as usize;
                if i < self.candidates.len() {
                    self.select(i);
                }
            }
            _ => {}
        }
        QuestionKey::Open
    }
}

/// What a key, a click or a screen reader's action asks of the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum QuestionKey {
    /// The question stays open.
    Open,
    /// Her answer.
    Answer(SameNameAnswer),
    /// Closed without an answer.
    Dismiss,
}

/// Her answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SameNameAnswer {
    /// « Replace it »: the paired device with this peer id is the one set up
    /// again; its earlier pairing goes.
    Replace(String),
    /// « Keep both »: every record stays.
    KeepBoth,
}

const REPLACE_IT: &str = "Replace it";
const KEEP_BOTH: &str = "Keep both";
const KEEP_ALL: &str = "Keep all";

const A11Y_DIALOG: NodeId = NodeId(1);
const A11Y_HINT: NodeId = NodeId(2);
const A11Y_LIST: NodeId = NodeId(3);
const A11Y_REPLACE: NodeId = NodeId(4);
const A11Y_KEEP: NodeId = NodeId(5);
const A11Y_OPTIONS: u64 = 16;

fn a11y_option(i: usize) -> NodeId {
    NodeId(A11Y_OPTIONS + i as u64)
}

// ---- drawing ---------------------------------------------------------------

const MW: f64 = 470.0;
const ROW_H: f64 = 32.0;
/// How many device rows are drawn at once; the rest scroll.
const MAX_ROWS: usize = 6;
const BODY_H: f64 = 96.0;
const BUTTON_W: f64 = 130.0;
const BUTTON_H: f64 = 34.0;

struct Geom {
    panel: Rect,
    /// The device rows drawn, each with its index into `candidates`.
    rows: Vec<(usize, Rect)>,
    replace: Rect,
    keep: Rect,
}

fn geom(q: &SameNameQuestion, w: f64, h: f64) -> Geom {
    let n_rows = if q.several() { q.candidates.len().min(MAX_ROWS) } else { 0 };
    let list_h = if n_rows > 0 { n_rows as f64 * (ROW_H + 4.0) + 8.0 } else { 0.0 };
    let ph = 40.0 + BODY_H + list_h + 16.0 + BUTTON_H + 16.0;
    let x0 = ((w - MW) * 0.5).max(8.0);
    let y0 = ((h - BOTTOM_CHROME - AXIS_H - ph) * 0.5 + AXIS_H).max(AXIS_H + 8.0);
    let panel = Rect::new(x0, y0, x0 + MW, y0 + ph);
    let list_y = y0 + 40.0 + BODY_H;
    let rows = (0..q.candidates.len().saturating_sub(q.top).min(n_rows))
        .map(|i| {
            let ry = list_y + i as f64 * (ROW_H + 4.0);
            (q.top + i, Rect::new(x0 + 16.0, ry, x0 + MW - 16.0, ry + ROW_H))
        })
        .collect();
    let by0 = panel.y1 - 16.0 - BUTTON_H;
    let keep = Rect::new(panel.x1 - 16.0 - BUTTON_W, by0, panel.x1 - 16.0, by0 + BUTTON_H);
    let replace = Rect::new(keep.x0 - 12.0 - BUTTON_W, by0, keep.x0 - 12.0, by0 + BUTTON_H);
    Geom { panel, rows, replace, keep }
}

/// Draws the question over the scene, the screen behind it dimmed.
pub(crate) fn draw(scene: &mut Scene, shaper: &mut TextShaper, q: &SameNameQuestion, w: f64, h: f64, cursor: Point) {
    let g = geom(q, w, h);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().scrim.with_alpha(0.45), None, &Rect::new(0.0, 0.0, w, h));
    let rr = RoundedRect::from_rect(g.panel, 11.0);
    scene.fill(Fill::NonZero, Affine::IDENTITY, pal().modal, None, &rr);
    scene.stroke(&Stroke::new(1.0), Affine::IDENTITY, pal().accent, None, &rr);

    let (title, body) = q.text();
    let tl = shaper.shape(&title, (MW - 32.0) as f32, 15.0);
    draw_text(scene, &tl, Affine::translate((g.panel.x0 + 16.0, g.panel.y0 + 16.0)), pal().text);
    let bl = shaper.shape(&body, (MW - 32.0) as f32, 12.5);
    draw_text(scene, &bl, Affine::translate((g.panel.x0 + 16.0, g.panel.y0 + 40.0)), pal().text_body);

    for (idx, r) in &g.rows {
        let Some(c) = q.candidates.get(*idx) else { continue };
        let bg = if r.contains(cursor) { pal().surface_hover } else { pal().surface };
        scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(*r, 6.0));
        if *idx == q.selected {
            // The chosen device: an outline, not a colour alone (WCAG 1.4.1).
            scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, crate::theme::chosen_row_outline(&pal()), None, &RoundedRect::from_rect(*r, 6.0));
        }
        let line = format!("{}  \u{00b7}  {}", c.name, c.detail());
        let nl = shaper.shape(&line, (r.width() - 20.0) as f32, 13.0);
        draw_text(scene, &nl, Affine::translate((r.x0 + 10.0, r.y0 + (r.height() - nl.height() as f64) * 0.5)), pal().text);
    }

    // « Replace it »: the accent; « Keep both »: the plain button. The
    // keyboard's button has a visible focus outline (WCAG 2.4.7).
    let bg = if g.replace.contains(cursor) { pal().accent } else { pal().accent.with_alpha(0.82) };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(g.replace, 7.0));
    let l = shaper.shape(REPLACE_IT, 200.0, 13.5);
    draw_text(scene, &l, Affine::translate(centered(g.replace, &l)), crate::theme::text_on_fill(crate::theme::seen_over(bg, pal().modal)));
    let bg = if g.keep.contains(cursor) { pal().border } else { pal().surface_alt };
    scene.fill(Fill::NonZero, Affine::IDENTITY, bg, None, &RoundedRect::from_rect(g.keep, 7.0));
    let l = shaper.shape(q.keep_label(), 200.0, 13.5);
    draw_text(scene, &l, Affine::translate(centered(g.keep, &l)), pal().text);
    let focused = match q.focus {
        Focus::Replace => g.replace,
        Focus::Keep => g.keep,
    };
    scene.stroke(&Stroke::new(2.0), Affine::IDENTITY, pal().text, None, &RoundedRect::from_rect(focused.inflate(3.0, 3.0), 9.0));
}

fn centered(r: Rect, l: &parley::Layout<crate::text::Brush>) -> (f64, f64) {
    (r.x0 + (r.width() - l.width() as f64) * 0.5, r.y0 + (r.height() - l.height() as f64) * 0.5)
}

// ---- the app ---------------------------------------------------------------

impl App {
    /// The question waiting for her answer, if any.
    pub(crate) fn same_name_question(&self) -> Option<&SameNameQuestion> {
        self.same_name.front()
    }

    /// Answers the question waiting, as her choice in its window does.
    pub(crate) fn answer_same_name(&mut self, answer: SameNameAnswer) {
        let Some(q) = self.same_name.pop_front() else { return };
        let name = q.name.clone();
        match answer {
            SameNameAnswer::Replace(peer) => {
                if !q.candidates.iter().any(|c| c.peer_id == peer) {
                    // Not a device it asked about: nothing is removed, and
                    // the question still waits.
                    self.same_name.push_front(q);
                    return;
                }
                // As unpairing does: out of the store and the saved list, and
                // the trimmed allow-list and key map pushed to the node.
                self.p2p_forget(&peer);
                // A question still waiting no longer asks about it.
                for later in self.same_name.iter_mut() {
                    later.candidates.retain(|c| c.peer_id != peer);
                    later.select(later.selected);
                }
                self.same_name.retain(|later| !later.candidates.is_empty());
                self.set_notice(
                    "Devices",
                    &format!("Paired with {name}. The earlier pairing of {name} is removed."),
                );
            }
            SameNameAnswer::KeepBoth => self.say_kept(&q),
        }
    }

    /// Closes the question without an answer, as Escape or its window's close
    /// does: no answer keeps both.
    pub(crate) fn dismiss_same_name(&mut self) {
        if let Some(q) = self.same_name.pop_front() {
            self.say_kept(&q);
        }
    }

    fn say_kept(&mut self, q: &SameNameQuestion) {
        let name = &q.name;
        let kept = if q.several() { "All are kept" } else { "Both are kept" };
        self.set_notice(
            "Devices",
            &format!("Paired with {name}. {kept}; you can forget one in Devices & Sync."),
        );
    }

    /// PR2: a device just paired as `peer` under `name`. When paired devices
    /// other than it hold that name, the question is asked (after any already
    /// waiting); whether it was. Nothing is removed here.
    pub(crate) fn ask_if_same_name(&mut self, peer: &str, name: &str) -> bool {
        let Some(h) = self.p2p.as_ref() else { return false };
        let pm = h.pairing_manager.clone();
        let candidates: Vec<Candidate> = self.rt.block_on(async {
            pm.read()
                .await
                .same_name(name, peer)
                .into_iter()
                .map(|d| Candidate {
                    peer_id: d.peer_id.clone(),
                    name: d.device_name.clone(),
                    paired_at: d.paired_at.clone(),
                })
                .collect()
        });
        if candidates.is_empty() {
            return false;
        }
        // The same device paired again while its question waits: one question.
        self.same_name.retain(|q| q.new_peer() != peer);
        self.same_name.push_back(SameNameQuestion {
            new_peer: peer.to_string(),
            name: name.to_string(),
            candidates,
            selected: 0,
            top: 0,
            focus: Focus::Keep,
        });
        true
    }

    /// Whether the question's window is shown (it holds clicks and keys).
    pub(crate) fn same_name_open(&self) -> bool {
        !self.locked && !self.same_name.is_empty()
    }

    /// Acts on what a key, a click or a screen reader asked of the question.
    pub(crate) fn same_name_step(&mut self, step: QuestionKey) {
        match step {
            QuestionKey::Open => {}
            QuestionKey::Answer(a) => self.answer_same_name(a),
            QuestionKey::Dismiss => self.dismiss_same_name(),
        }
    }

    /// A key while the question is shown.
    pub(crate) fn same_name_key(&mut self, key: &Key) {
        if let Some(step) = self.same_name.front_mut().map(|q| q.key(key)) {
            self.same_name_step(step);
        }
    }

    /// A click while the question is shown.
    pub(crate) fn same_name_click(&mut self, p: Point, w: f64, h: f64) {
        if let Some(step) = self.same_name.front_mut().map(|q| q.click(w, h, p)) {
            self.same_name_step(step);
        }
    }

    /// The wheel while the question is shown: its list scrolls under the cursor.
    pub(crate) fn same_name_wheel(&mut self, p: Point, w: f64, h: f64, lines: i64) {
        if let Some(q) = self.same_name.front_mut() {
            if q.contains(w, h, p) {
                q.scroll(lines);
            }
        }
    }

    /// A screen reader's action; whether it was on the question's nodes.
    pub(crate) fn same_name_a11y_action(&mut self, target: NodeId, action: Action) -> bool {
        if target.0 & !((1u64 << 32) - 1) != SAME_NAME_NS {
            return false;
        }
        let local = NodeId(target.0 & ((1u64 << 32) - 1));
        if let Some(step) = self.same_name.front_mut().map(|q| q.a11y_action(local, action)) {
            self.same_name_step(step);
        }
        true
    }
}
