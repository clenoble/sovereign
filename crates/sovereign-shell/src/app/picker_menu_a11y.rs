//! Lot C of the shell's accessibility (spec ch. 12, SQ10 « C », with SQ5 and SQ8): the bubble-style
//! picker and the right-click menu in the accessibility tree, and a key does what a click does. The
//! particular cases are the Fable maille's, by delegation of Céline (2026-11-14), on 2026-11-19:
//! - the picker is a dialog; its styles are radio buttons, the current one checked and holding the
//!   focus when it opens; the styles are one group, one stop for Tab (Tab and Shift+Tab go between
//!   the group, entered on its checked style, and « Done »), the arrows moving inside it; Enter or
//!   Space picks the focused style and leaves the picker open, as a click does; Escape closes it (the
//!   group's rule amended the picker's first answer the same day, after the convention); the arrows
//!   are a radio group's: Right and Down to the next style, Left and Up to the previous, wrapping;
//! - the focused control is drawn with the shell's focus outline (WCAG 2.4.7), in the picker always,
//!   in the menu once a key opened or moved it;
//! - the menu is a menu; Shift+F10 and the Menu key open it on the lane's label that holds the focus;
//!   the focus is on its first row; Up, Down, Tab and Shift+Tab move; Enter or Space presses a row, as
//!   a click does; Escape closes it, and the focus goes back where it was; « Skills › » becomes its
//!   submenu, the focus on the submenu's first row.

use accesskit::{Action, Node, NodeId, Role as AkRole, Toggled};
use sovereign_core::profile::BubbleStyle;
use vello::kurbo::Point;
use winit::keyboard::{Key, NamedKey};

use super::{App, ContextMenu, CtxAction, CtxTarget};
use crate::canvas::LANE_H;

/// The picker's nodes: the dialog, then its controls by index (the styles in their order, « Done »
/// last).
const PICKER_NS: u64 = 1 << 48;
const PICKER_DIALOG: NodeId = NodeId(PICKER_NS | 1);
const PICKER_HINT: NodeId = NodeId(PICKER_NS | 2);
/// The menu's nodes: the menu, then its rows by index.
const MENU_NS: u64 = 1 << 47;
const MENU_NODE: NodeId = NodeId(MENU_NS | 1);

const PICKER_TITLE: &str = "Choose your bubble";
const PICKER_DONE: &str = "Done";

fn picker_control(i: usize) -> NodeId {
    NodeId(PICKER_NS | (0x100 + i as u64))
}

fn menu_row(i: usize) -> NodeId {
    NodeId(MENU_NS | (0x100 + i as u64))
}

fn local(id: NodeId, ns: u64) -> Option<usize> {
    (id.0 & !0xFFFF == ns && id.0 & 0xFFFF >= 0x100).then(|| (id.0 & 0xFFFF) as usize - 0x100)
}

impl App {
    // ---- the picker -------------------------------------------------------------------------

    fn picker_styles() -> &'static [BubbleStyle] {
        BubbleStyle::all()
    }

    /// The picker's focused control while it is shown: the current style when it opens.
    pub(super) fn picker_focus_now(&mut self) -> Option<usize> {
        if !self.bubble_picker || self.locked {
            self.picker_focus = None;
            return None;
        }
        let n = Self::picker_styles().len() + 1;
        let f = match self.picker_focus {
            Some(f) if f < n => f,
            _ => Self::picker_styles().iter().position(|s| *s == self.bubble_style).unwrap_or(0),
        };
        self.picker_focus = Some(f);
        Some(f)
    }

    /// Presses the picker's control `i`, as a click on it does: a style is picked and the picker stays
    /// open; « Done » closes it.
    fn picker_press(&mut self, i: usize) {
        match Self::picker_styles().get(i) {
            Some(style) => self.select_bubble_style(*style),
            None => {
                self.bubble_picker = false;
                self.picker_focus = None;
            }
        }
    }

    pub(super) fn picker_key(&mut self, key: &Key, mods: sovereign_editor::input::Modifiers) -> bool {
        let Some(f) = self.picker_focus_now() else { return false };
        let styles = Self::picker_styles().len();
        let checked = Self::picker_styles().iter().position(|s| *s == self.bubble_style).unwrap_or(0);
        let moved = |to: usize| Some(to);
        let _ = mods;
        let next = match key {
            // The styles are one group, one stop for Tab (ch. 12, SQ10): Tab and Shift+Tab go
            // between the group, entered on its checked style, and « Done ».
            Key::Named(NamedKey::Tab) => moved(if f < styles { styles } else { checked }),
            // The arrows are a radio group's (ch. 12, SQ10; the Fable maille, by delegation of
            // Céline, 2026-11-19): Right and Down to the next style, Left and Up to the previous,
            // in the tree's order, wrapping; from « Done » they come back to the current style.
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) if f < styles => moved((f + 1) % styles),
            Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) if f < styles => moved((f + styles - 1) % styles),
            Key::Named(NamedKey::ArrowRight | NamedKey::ArrowLeft | NamedKey::ArrowDown | NamedKey::ArrowUp) => moved(checked),
            Key::Named(NamedKey::Enter | NamedKey::Space) => {
                self.picker_press(f);
                None
            }
            Key::Named(NamedKey::Escape) => {
                self.bubble_picker = false;
                self.picker_focus = None;
                None
            }
            // The picker holds the keyboard while it is shown, as it always has.
            _ => None,
        };
        if let Some(to) = next {
            self.picker_focus = Some(to);
        }
        true
    }

    /// The picker's tree while it is shown: (nodes, its dialog, the focused control).
    pub(super) fn picker_a11y_nodes(&mut self) -> Option<(Vec<(NodeId, Node)>, NodeId, NodeId)> {
        let f = self.picker_focus_now()?;
        let mut nodes = Vec::new();
        let mut kids = vec![PICKER_HINT];
        let mut hint = Node::new(AkRole::Label);
        hint.set_value(
            "The arrow keys move between the styles; Tab and Shift+Tab go between the styles and Done; Enter or Space picks the focused style; Escape closes.",
        );
        nodes.push((PICKER_HINT, hint));
        for (i, style) in Self::picker_styles().iter().enumerate() {
            let mut r = Node::new(AkRole::RadioButton);
            r.set_label(style.label());
            r.set_toggled(if *style == self.bubble_style { Toggled::True } else { Toggled::False });
            r.add_action(Action::Click);
            r.add_action(Action::Focus);
            nodes.push((picker_control(i), r));
            kids.push(picker_control(i));
        }
        let done = Self::picker_styles().len();
        let mut b = Node::new(AkRole::Button);
        b.set_label(PICKER_DONE);
        b.add_action(Action::Click);
        b.add_action(Action::Focus);
        nodes.push((picker_control(done), b));
        kids.push(picker_control(done));
        let mut d = Node::new(AkRole::Dialog);
        d.set_label(PICKER_TITLE);
        d.set_modal();
        d.set_children(kids);
        nodes.insert(0, (PICKER_DIALOG, d));
        Some((nodes, PICKER_DIALOG, picker_control(f)))
    }

    /// A screen reader's action on the picker; whether it was the picker's.
    pub(super) fn picker_a11y_action(&mut self, target: NodeId, action: Action) -> bool {
        if self.picker_focus_now().is_none() {
            return false;
        }
        if let Some(i) = local(target, PICKER_NS).filter(|i| *i <= Self::picker_styles().len()) {
            match action {
                Action::Click => {
                    self.picker_focus = Some(i);
                    self.picker_press(i);
                }
                Action::Focus => self.picker_focus = Some(i),
                _ => {}
            }
        }
        // While it is shown, nothing under it is pressed.
        true
    }

    // ---- the right-click menu ----------------------------------------------------------------

    /// The menu's focused row while it is open: its first row when it opens.
    fn menu_focus_now(&mut self) -> Option<usize> {
        if self.ctx_menu.is_none() || self.locked {
            self.menu_focus = None;
            return None;
        }
        let n = self.ctx_menu_rows().len();
        if n == 0 {
            return None;
        }
        let f = match self.menu_focus {
            Some(f) if f < n => f,
            _ => 0,
        };
        self.menu_focus = Some(f);
        Some(f)
    }

    /// Presses the menu's row `i`, as a click on it does; a submenu starts on its first row.
    fn menu_press(&mut self, i: usize) {
        let Some((_, action, _)) = self.ctx_menu_rows().into_iter().nth(i) else { return };
        let submenu = matches!(action, CtxAction::OpenSkills);
        self.run_ctx_action(action);
        self.menu_focus = if submenu && self.ctx_menu.is_some() { Some(0) } else { None };
    }

    /// Opens the menu from the keyboard on what holds the focus: a lane's label a screen reader
    /// focused opens that lane's menu. Whether one opened.
    fn menu_open_on_focus(&mut self) -> bool {
        let Some(thread) = self.a11y_label_focus.clone() else { return false };
        let Some(lane) = self.lane_labels.iter().find(|l| l.thread_id == thread).map(|l| l.lane) else { return false };
        let y = self.cam.offset_y + (lane as f64 + 0.5) * LANE_H * self.cam.zoom;
        self.ctx_menu = Some(ContextMenu { target: CtxTarget::Canvas(lane), anchor: Point::new(24.0, y) });
        self.menu_focus = Some(0);
        true
    }

    pub(super) fn menu_key(&mut self, key: &Key, mods: sovereign_editor::input::Modifiers) -> bool {
        let opens = matches!(key, Key::Named(NamedKey::ContextMenu))
            || (mods.shift && matches!(key, Key::Named(NamedKey::F10)));
        let Some(f) = self.menu_focus_now() else {
            let opened = opens && !self.locked && self.menu_open_on_focus();
            self.menu_keyed |= opened;
            return opened;
        };
        let n = self.ctx_menu_rows().len();
        match key {
            Key::Named(NamedKey::ArrowDown) => self.menu_focus = Some((f + 1) % n),
            Key::Named(NamedKey::Tab) if !mods.shift => self.menu_focus = Some((f + 1) % n),
            Key::Named(NamedKey::ArrowUp) => self.menu_focus = Some((f + n - 1) % n),
            Key::Named(NamedKey::Tab) => self.menu_focus = Some((f + n - 1) % n),
            Key::Named(NamedKey::Enter | NamedKey::Space) => self.menu_press(f),
            Key::Named(NamedKey::Escape) => {
                // The focus goes back to what held it: the lane's label, if a screen reader had
                // focused one (it is still held), otherwise the shell's own.
                self.ctx_menu = None;
                self.menu_focus = None;
            }
            _ if opens => {}
            _ => return false,
        }
        // A key moved or pressed in the menu: its focused row is drawn with the outline.
        self.menu_keyed = self.ctx_menu.is_some();
        true
    }

    /// The menu's tree while it is open: (nodes, the menu, the focused row).
    pub(super) fn menu_a11y_nodes(&mut self) -> Option<(Vec<(NodeId, Node)>, NodeId, NodeId)> {
        let f = self.menu_focus_now()?;
        let rows = self.ctx_menu_rows();
        let mut nodes = Vec::new();
        let mut kids = Vec::new();
        for (i, (label, _, _)) in rows.iter().enumerate() {
            let mut r = Node::new(AkRole::MenuItem);
            r.set_label(label.clone());
            r.add_action(Action::Click);
            r.add_action(Action::Focus);
            nodes.push((menu_row(i), r));
            kids.push(menu_row(i));
        }
        let mut m = Node::new(AkRole::Menu);
        let on_card = matches!(self.ctx_menu.as_ref().map(|m| &m.target), Some(CtxTarget::Card(_)));
        m.set_label(if on_card { "Document menu" } else { "Lane menu" });
        m.set_children(kids);
        nodes.insert(0, (MENU_NODE, m));
        Some((nodes, MENU_NODE, menu_row(f)))
    }

    /// A screen reader's action on the menu; whether it was the menu's.
    pub(super) fn menu_a11y_action(&mut self, target: NodeId, action: Action) -> bool {
        if self.menu_focus_now().is_none() {
            return false;
        }
        let Some(i) = local(target, MENU_NS) else { return false };
        match action {
            Action::Click => {
                self.menu_focus = Some(i);
                self.menu_press(i);
            }
            Action::Focus => self.menu_focus = Some(i),
            _ => {}
        }
        true
    }
}
