//! The shell's questions and notices, heard by a screen reader and answered
//! from the keyboard (spec ch. 12, SQ1 to SQ9; lot A of SQ10's work).
//!
//! - **What is on top** ([`OnTop`]): the one order of SQ11a, which the
//!   drawing, a click, a key and the tree all follow. Only the window on top
//!   is in the tree and takes keys (SQ2, SQ11b): the tree says what can be
//!   answered now.
//! - **The questions** (the action confirmation, the injection prompt, the
//!   unsaved-edits prompt) are modal dialogs: their text as labels, a line
//!   saying their keys, their buttons as drawn, each taking a screen reader's
//!   Click and Focus (SQ1). Each holds the focus on one of its buttons
//!   ([`DialogFocus`]), starting where a stray key does no harm (SQ3); Tab and
//!   Shift+Tab move it and stay inside, Enter or Space presses it, Escape keeps
//!   its meaning (SQ4).
//! - **Every hand goes through the same gate** (SQ5): a key, a screen reader's
//!   Click and a mouse click all end in `decide_action`, `decide_injection` or
//!   `resolve_unsaved`, which check the minimum time shown, the deadline and
//!   the request the answer is bound to.
//! - **A notice** is an alert dialog named by its title, holding the focus;
//!   Escape or a screen reader's Click closes it; « Restart sync » is a button
//!   inside it, reached by Tab and pressed by Enter or Space, the focus
//!   starting on the notice itself so a stray Enter presses nothing (SQ6).
//! - **The trash's toast** is a status and its « Undo (Ctrl+Z) » button; it
//!   never takes the focus, and Ctrl+Z presses it only when a click on it would
//!   (SQ7).
//!
//! Lots B and C add their windows to [`OnTop`] and to `App::shell_key`.

use accesskit::{Action, Live, Node, NodeId, Role};
use sovereign_core::security::{ActionDecision, InjectionDecision};
use sovereign_editor::input::Modifiers;
use winit::keyboard::{Key, NamedKey};

use super::{level_word, App, UnsavedChoice, SYNC_RESTART_GESTURE};

/// The id range of these dialogs' accessibility nodes in the shell's tree.
const SHELL_DIALOG_NS: u64 = 1 << 49;
/// Which element a node belongs to (bits 44 to 46 of its id).
const KIND_CONFIRMATION: u64 = 1;
const KIND_INJECTION: u64 = 2;
const KIND_UNSAVED: u64 = 3;
const KIND_NOTICE: u64 = 4;
const KIND_TOAST: u64 = 5;
/// Within an element: the dialog, its lines from 1, its buttons from 32.
const LOCAL_DIALOG: u64 = 0;
const LOCAL_LINES: u64 = 1;
const LOCAL_BUTTONS: u64 = 32;

/// SQ1: the action confirmation's name, read before its text.
pub(crate) const CONFIRMATION_NAME: &str = "The assistant asks to do this";
/// The answer « Reject (Esc) » sends, by a click, a key or a screen reader.
pub(crate) const DECLINED_BY_USER: &str = "Declined by user";

/// SQ4: the lines that say each dialog's keys (heard, not drawn, as the
/// same-name question's).
const CONFIRMATION_KEYS: &str =
    "Tab and Shift+Tab move between the buttons, Enter or Space presses the one in focus, Escape rejects.";
const INJECTION_KEYS: &str =
    "Tab and Shift+Tab move between the buttons, Enter or Space presses the one in focus, Escape redacts and continues.";
const UNSAVED_KEYS: &str =
    "Tab and Shift+Tab move between the buttons, Enter or Space presses the one in focus, Escape goes back to editing.";
const NOTICE_KEYS: &str = "Escape closes it.";
/// SQ6: a notice that offers « Restart sync ».
const NOTICE_RESTART_KEYS: &str =
    "Tab moves to Restart sync, Enter or Space presses it, Escape closes the notice.";

/// The confirmation's buttons, left to right as drawn.
pub(crate) const CONFIRMATION_REJECT: usize = 0;
pub(crate) const CONFIRMATION_APPROVE: usize = 1;
/// The unsaved-edits prompt's buttons, left to right as drawn.
pub(crate) const UNSAVED_KEEP_EDITING: usize = 0;
pub(crate) const UNSAVED_DISCARD: usize = 1;
pub(crate) const UNSAVED_SAVE: usize = 2;

/// What is on top: what the eye sees above the others, what a click and a key
/// reach and what is in the tree (spec ch. 12, SQ11a). Every window answers
/// only while on top.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OnTop {
    /// The setup, the login or the join form: none of these is shown.
    Locked,
    UnsavedEdits,
    SameName,
    LogWindow,
    Pairing,
    Guardian,
    Recovery,
    BubblePicker,
    Compose,
    EmailSetup,
    Injection,
    Confirmation,
    Notice,
    TrashModal,
    OrphanChooser,
    NewThread,
    LaneRename,
    DeviceRename,
    /// No question, notice or modal window: the canvas, its windows, the toast.
    Workspace,
}

/// Which button of a question has the keyboard, for one question shown. A new
/// question gets its own (with a new `seq`): its focus starts again where a
/// stray key does no harm, and its nodes are new, so a screen reader reads it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DialogFocus {
    seq: u64,
    at: usize,
    count: usize,
}

impl DialogFocus {
    /// `count` buttons, the focus on `start`.
    pub(crate) fn new(seq: u64, start: usize, count: usize) -> Self {
        let count = count.max(1);
        DialogFocus { seq, at: start.min(count - 1), count }
    }

    /// The index of the button with the keyboard (its focus outline is drawn).
    pub(crate) fn at(&self) -> usize {
        self.at
    }

    fn next(&mut self) {
        self.at = (self.at + 1) % self.count;
    }

    fn prev(&mut self) {
        self.at = (self.at + self.count - 1) % self.count;
    }

    fn set(&mut self, i: usize) {
        if i < self.count {
            self.at = i;
        }
    }
}

/// What a key asks of a question.
enum DialogKey {
    /// Press the button at this index.
    Press(usize),
    Escape,
    /// The focus moved (Tab, Shift+Tab).
    Moved,
    /// Not one of the question's keys.
    Other,
}

/// SQ4: Tab and Shift+Tab move the focus and stay inside; Enter or Space
/// presses the focused button; Escape keeps its meaning.
fn dialog_key(focus: &mut DialogFocus, key: &Key, mods: Modifiers) -> DialogKey {
    let plain = !mods.ctrl && !mods.alt && !mods.meta;
    match key {
        Key::Named(NamedKey::Tab) if plain => {
            if mods.shift {
                focus.prev();
            } else {
                focus.next();
            }
            DialogKey::Moved
        }
        Key::Named(NamedKey::Enter) => DialogKey::Press(focus.at),
        Key::Named(NamedKey::Space) if plain => DialogKey::Press(focus.at),
        Key::Character(c) if plain && c.as_str() == " " => DialogKey::Press(focus.at),
        Key::Named(NamedKey::Escape) => DialogKey::Escape,
        _ => DialogKey::Other,
    }
}

fn node_id(kind: u64, seq: u64, local: u64) -> NodeId {
    NodeId(SHELL_DIALOG_NS | kind << 44 | (seq & ((1 << 36) - 1)) << 8 | local)
}

fn is_ours(id: NodeId) -> bool {
    id.0 >> 49 == 1
}

/// The index of the button `target` is, among `count` buttons of the element
/// `kind` shown as `seq`.
fn button_index(kind: u64, seq: u64, count: usize, target: NodeId) -> Option<usize> {
    (0..count).find(|i| node_id(kind, seq, LOCAL_BUTTONS + *i as u64) == target)
}

/// A dialog's nodes: the dialog (role `role`, named `name`, modal), its lines
/// as labels spoken by their value (L1), then its buttons named as drawn.
/// Returns (the dialog's id, the nodes, the button in focus if any).
#[allow(clippy::too_many_arguments)]
fn dialog_nodes(
    kind: u64,
    seq: u64,
    role: Role,
    name: &str,
    lines: &[String],
    buttons: &[&str],
    focused: Option<usize>,
    buttons_take_focus: bool,
) -> (NodeId, Vec<(NodeId, Node)>, Option<NodeId>) {
    let dialog_id = node_id(kind, seq, LOCAL_DIALOG);
    let mut nodes = Vec::with_capacity(lines.len() + buttons.len() + 1);
    let mut children = Vec::with_capacity(lines.len() + buttons.len());
    for (i, line) in lines.iter().filter(|l| !l.trim().is_empty()).enumerate() {
        let id = node_id(kind, seq, LOCAL_LINES + i as u64);
        let mut n = Node::new(Role::Label);
        n.set_value(line.clone());
        nodes.push((id, n));
        children.push(id);
    }
    let mut focus = None;
    for (i, label) in buttons.iter().enumerate() {
        let id = node_id(kind, seq, LOCAL_BUTTONS + i as u64);
        let mut b = Node::new(Role::Button);
        b.set_label(label.to_string());
        b.add_action(Action::Click);
        if buttons_take_focus {
            b.add_action(Action::Focus);
        }
        if focused == Some(i) {
            focus = Some(id);
        }
        nodes.push((id, b));
        children.push(id);
    }
    let mut d = Node::new(role);
    d.set_label(name.to_string());
    d.set_modal();
    d.set_children(children);
    nodes.insert(0, (dialog_id, d));
    (dialog_id, nodes, focus)
}

impl App {
    /// What is on top (see [`OnTop`]), in the one order of spec ch. 12,
    /// SQ11a: the eye (`build_scene` draws them in the reverse order), a
    /// click (`handle_click`), a key (`key_pressed`) and the tree
    /// (`build_a11y_tree`) all follow it. First the window with a deadline
    /// (the action confirmation or the injection prompt, never both: the
    /// orchestrator asks one question at a time, ch. 08, AT13; were both
    /// shown, the injection prompt stays above, as SQ2 has it); then the
    /// three that arrive by themselves; then the windows the person opens,
    /// in the order the shell drew them before (Céline, 2026-11-19).
    pub(crate) fn on_top(&self) -> OnTop {
        if self.locked {
            OnTop::Locked
        } else if self.injection_prompt.is_some() {
            OnTop::Injection
        } else if self.pending_action.is_some() {
            OnTop::Confirmation
        } else if self.unsaved_prompt.is_some() {
            OnTop::UnsavedEdits
        } else if self.same_name_open() {
            OnTop::SameName
        } else if self.notice.is_some() {
            OnTop::Notice
        } else if self.bubble_picker {
            OnTop::BubblePicker
        } else if self.log_window_open() {
            OnTop::LogWindow
        } else if self.recovery_wizard.is_some() {
            OnTop::Recovery
        } else if self.guardian_modal.is_some() {
            OnTop::Guardian
        } else if self.pairing_modal.is_some() {
            OnTop::Pairing
        } else if self.compose_form.is_some() {
            OnTop::Compose
        } else if self.comms_form.is_some() {
            OnTop::EmailSetup
        } else if self.orphan_modal.is_some() {
            OnTop::OrphanChooser
        } else if self.trash_modal.is_some() {
            OnTop::TrashModal
        } else if self.device_rename.is_some() {
            OnTop::DeviceRename
        } else if self.lane_rename.is_some() {
            OnTop::LaneRename
        } else if self.new_thread_prompt.is_some() {
            OnTop::NewThread
        } else {
            OnTop::Workspace
        }
    }

    /// A new question or notice is shown: the number its nodes carry.
    pub(crate) fn next_dialog_seq(&mut self) -> u64 {
        self.dialog_seq += 1;
        self.dialog_seq
    }

    // ---- the keys (SQ4, SQ6, SQ7) ----------------------------------------

    /// Lot A's turn in `shell_key`: `None` when none of its elements is on
    /// top (the dispatcher tries the next lot), else whether it took the key.
    /// On the workspace it answers only when Ctrl+Z pressed the toast's Undo,
    /// so that other keys reach what lot C brings there.
    pub(crate) fn a11y_key_a(&mut self, key: &Key, text: Option<&str>, mods: Modifiers) -> Option<bool> {
        // No element of lot A takes text.
        let _ = text;
        match self.on_top() {
            OnTop::UnsavedEdits => Some(self.unsaved_key(key, mods)),
            OnTop::Injection => Some(self.injection_key(key, mods)),
            OnTop::Confirmation => Some(self.confirmation_key(key, mods)),
            OnTop::Notice => Some(self.notice_key(key, mods)),
            OnTop::Workspace => self.workspace_key(key, mods).then_some(true),
            _ => None,
        }
    }

    /// The action confirmation's keys. Keys it does not use are left to
    /// `key_pressed`, which passes only the zoom (SQ11d).
    fn confirmation_key(&mut self, key: &Key, mods: Modifiers) -> bool {
        let Some(p) = self.pending_action.as_mut() else { return false };
        match dialog_key(&mut p.focus, key, mods) {
            DialogKey::Press(i) => self.confirmation_press(i),
            DialogKey::Escape => self.confirmation_press(CONFIRMATION_REJECT),
            DialogKey::Moved => {}
            DialogKey::Other => return false,
        }
        true
    }

    /// The injection prompt holds the keyboard while it is on top, as before.
    /// A Ctrl chord it does not use is left to `key_pressed`, which passes
    /// only the zoom (SQ11d).
    fn injection_key(&mut self, key: &Key, mods: Modifiers) -> bool {
        let Some(p) = self.injection_prompt.as_mut() else { return false };
        match dialog_key(&mut p.focus, key, mods) {
            DialogKey::Press(i) => self.injection_press(i),
            // Escape keeps its meaning: the safe choice.
            DialogKey::Escape => self.decide_injection(InjectionDecision::Redact),
            DialogKey::Moved => {}
            DialogKey::Other => return !mods.ctrl,
        }
        true
    }

    /// The unsaved-edits prompt holds the keyboard while it is on top, as
    /// before. A Ctrl chord it does not use is left to `key_pressed`, which
    /// passes the zoom and Ctrl+S, which saves (SQ11d).
    fn unsaved_key(&mut self, key: &Key, mods: Modifiers) -> bool {
        let Some(p) = self.unsaved_prompt.as_mut() else { return false };
        match dialog_key(&mut p.focus, key, mods) {
            DialogKey::Press(i) => self.unsaved_press(i),
            // Escape keeps its meaning: back to editing, nothing lost.
            DialogKey::Escape => self.resolve_unsaved(UnsavedChoice::Cancel),
            DialogKey::Moved => {}
            DialogKey::Other => return !mods.ctrl,
        }
        true
    }

    /// A notice on top: Escape closes it (and an open menu with it, as
    /// before). When it offers « Restart sync » (SQ6), Tab and Shift+Tab give
    /// that button the keyboard and stay inside, and Enter or Space press it
    /// once it has the keyboard; on the notice itself an Enter presses
    /// nothing. Other keys are left to `key_pressed`, which passes only the
    /// zoom (SQ11d).
    fn notice_key(&mut self, key: &Key, mods: Modifiers) -> bool {
        let Some(n) = self.notice.as_mut() else { return false };
        if matches!(key, Key::Named(NamedKey::Escape)) {
            self.notice = None;
            self.ctx_menu = None;
            return true;
        }
        if !n.restart {
            return false;
        }
        // One button: the focus goes to it and stays there, inside the notice.
        let mut focus = DialogFocus::new(n.seq, 0, 1);
        match dialog_key(&mut focus, key, mods) {
            DialogKey::Moved => {
                n.restart_focused = true;
                true
            }
            DialogKey::Press(_) if n.restart_focused => {
                self.restart_sync();
                true
            }
            DialogKey::Press(_) | DialogKey::Escape | DialogKey::Other => false,
        }
    }

    /// SQ7: Ctrl+Z presses the toast's « Undo (Ctrl+Z) » only when a click on
    /// it would, and no text field holds the keyboard (there it is the
    /// field's).
    fn workspace_key(&mut self, key: &Key, mods: Modifiers) -> bool {
        let ctrl_z = matches!(key, Key::Character(c) if c.eq_ignore_ascii_case("z"))
            && mods.ctrl
            && !mods.alt
            && !mods.meta
            && !mods.shift;
        if ctrl_z && self.toast_clickable() && !self.text_field_has_keyboard() {
            self.undo_delete();
            return true;
        }
        false
    }

    // ---- what each button does: what a click on it does (SQ5) ------------

    fn confirmation_press(&mut self, i: usize) {
        match i {
            CONFIRMATION_REJECT => self.decide_action(ActionDecision::Reject(DECLINED_BY_USER.into())),
            CONFIRMATION_APPROVE => self.decide_action(ActionDecision::Approve),
            _ => {}
        }
    }

    fn injection_press(&mut self, i: usize) {
        let decision = match i {
            0 => InjectionDecision::Redact,
            1 => InjectionDecision::PassThrough,
            2 => InjectionDecision::Abort,
            _ => return,
        };
        self.decide_injection(decision);
    }

    fn unsaved_press(&mut self, i: usize) {
        let choice = match i {
            UNSAVED_KEEP_EDITING => UnsavedChoice::Cancel,
            UNSAVED_DISCARD => UnsavedChoice::Discard,
            UNSAVED_SAVE => UnsavedChoice::Save,
            _ => return,
        };
        self.resolve_unsaved(choice);
    }

    // ---- the toast (SQ7) --------------------------------------------------

    /// The toast is shown and a click on it would reach it: nothing that
    /// takes clicks first is open.
    fn toast_clickable(&self) -> bool {
        self.on_top() == OnTop::Workspace
            && self.delete_notice.as_ref().is_some_and(|n| n.shown(std::time::Instant::now()))
    }

    /// A text field holds the keyboard: the top window's composer or search
    /// query, a document's editor, or the browser's address bar.
    fn text_field_has_keyboard(&self) -> bool {
        self.top_field().is_some()
            || self.v2_focus_target().is_some()
            || matches!(self.top_kind(), Some(super::WindowKind::Browser))
    }

    /// The toast's nodes are numbered by its document, so a new toast is a new node.
    fn toast_seq(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        if let Some(n) = &self.delete_notice {
            n.doc_id().hash(&mut h);
        }
        h.finish()
    }

    // ---- the tree (SQ1, SQ2, SQ6, SQ7) ------------------------------------

    /// The question or notice on top, as a dialog: (its id, its nodes, the
    /// node holding the focus). `None` when none of them is on top.
    pub(crate) fn shell_dialog_tree(&self) -> Option<(NodeId, Vec<(NodeId, Node)>, NodeId)> {
        match self.on_top() {
            OnTop::Confirmation => {
                let p = self.pending_action.as_ref()?;
                let lines = [format!("{}: {}", level_word(p.level), p.said), CONFIRMATION_KEYS.to_string()];
                let (d, nodes, f) = dialog_nodes(
                    KIND_CONFIRMATION,
                    p.focus.seq,
                    Role::Dialog,
                    CONFIRMATION_NAME,
                    &lines,
                    &super::CONFIRMATION_BUTTONS,
                    Some(p.focus.at),
                    true,
                );
                Some((d, nodes, f.unwrap_or(d)))
            }
            OnTop::Injection => {
                let p = self.injection_prompt.as_ref()?;
                let mut lines = p.a11y_lines();
                lines.push(INJECTION_KEYS.to_string());
                let (d, nodes, f) = dialog_nodes(
                    KIND_INJECTION,
                    p.focus.seq,
                    Role::Dialog,
                    crate::panels::INJECTION_TITLE,
                    &lines,
                    &crate::panels::INJECTION_BUTTONS,
                    Some(p.focus.at),
                    true,
                );
                Some((d, nodes, f.unwrap_or(d)))
            }
            OnTop::UnsavedEdits => {
                let p = self.unsaved_prompt.as_ref()?;
                let lines = [p.said.1.clone(), UNSAVED_KEYS.to_string()];
                let (d, nodes, f) = dialog_nodes(
                    KIND_UNSAVED,
                    p.focus.seq,
                    Role::Dialog,
                    &p.said.0,
                    &lines,
                    &super::UNSAVED_BUTTONS,
                    Some(p.focus.at),
                    true,
                );
                Some((d, nodes, f.unwrap_or(d)))
            }
            OnTop::Notice => {
                let n = self.notice.as_ref()?;
                let keys = if n.restart { NOTICE_RESTART_KEYS } else { NOTICE_KEYS };
                let lines = [n.said.1.clone(), keys.to_string()];
                let restart: &[&str] = if n.restart { &[SYNC_RESTART_GESTURE] } else { &[] };
                // SQ6: « Restart sync » holds the focus once Tab reached it;
                // until then the notice itself does.
                let focused = (n.restart && n.restart_focused).then_some(0);
                let (d, mut nodes, f) = dialog_nodes(
                    KIND_NOTICE,
                    n.seq,
                    Role::AlertDialog,
                    &n.said.0,
                    &lines,
                    restart,
                    focused,
                    n.restart,
                );
                // It holds the focus, so it is announced when it opens; a
                // screen reader's Click closes it, as a click does.
                if let Some((_, dialog)) = nodes.first_mut() {
                    dialog.add_action(Action::Focus);
                    dialog.add_action(Action::Click);
                }
                Some((d, nodes, f.unwrap_or(d)))
            }
            _ => None,
        }
    }

    /// The toast while a click would reach it: its text as a status, and its
    /// button. Neither takes the focus.
    pub(crate) fn toast_nodes(&self) -> Vec<(NodeId, Node)> {
        if !self.toast_clickable() {
            return Vec::new();
        }
        let Some(n) = &self.delete_notice else { return Vec::new() };
        let seq = self.toast_seq();
        let mut status = Node::new(Role::Status);
        status.set_label(n.text());
        status.set_live(Live::Polite);
        let mut undo = Node::new(Role::Button);
        undo.set_label(crate::trash_ui::UNDO_LABEL);
        undo.add_action(Action::Click);
        vec![(node_id(KIND_TOAST, seq, LOCAL_LINES), status), (node_id(KIND_TOAST, seq, LOCAL_BUTTONS), undo)]
    }

    /// A screen reader's action on one of these nodes; whether it was one.
    /// It acts only on the element on top, through what a click does (SQ5):
    /// on a question covered by another, it does nothing.
    pub(crate) fn shell_a11y_action(&mut self, target: NodeId, action: Action) -> bool {
        if !is_ours(target) {
            return false;
        }
        match self.on_top() {
            OnTop::Confirmation => {
                let Some(p) = self.pending_action.as_mut() else { return true };
                if let Some(i) = button_index(KIND_CONFIRMATION, p.focus.seq, p.focus.count, target) {
                    match action {
                        Action::Click => self.confirmation_press(i),
                        Action::Focus => p.focus.set(i),
                        _ => {}
                    }
                }
            }
            OnTop::Injection => {
                let Some(p) = self.injection_prompt.as_mut() else { return true };
                if let Some(i) = button_index(KIND_INJECTION, p.focus.seq, p.focus.count, target) {
                    match action {
                        Action::Click => self.injection_press(i),
                        Action::Focus => p.focus.set(i),
                        _ => {}
                    }
                }
            }
            OnTop::UnsavedEdits => {
                let Some(p) = self.unsaved_prompt.as_mut() else { return true };
                if let Some(i) = button_index(KIND_UNSAVED, p.focus.seq, p.focus.count, target) {
                    match action {
                        Action::Click => self.unsaved_press(i),
                        Action::Focus => p.focus.set(i),
                        _ => {}
                    }
                }
            }
            OnTop::Notice => {
                let Some(n) = self.notice.as_mut() else { return true };
                let (seq, restart) = (n.seq, n.restart);
                let on_button = restart && target == node_id(KIND_NOTICE, seq, LOCAL_BUTTONS);
                match action {
                    Action::Click if target == node_id(KIND_NOTICE, seq, LOCAL_DIALOG) => self.notice = None,
                    Action::Click if on_button => self.restart_sync(),
                    // SQ6: a screen reader's Focus gives « Restart sync » the keyboard.
                    Action::Focus if on_button => n.restart_focused = true,
                    _ => {}
                }
            }
            OnTop::Workspace => {
                if action == Action::Click
                    && self.toast_clickable()
                    && target == node_id(KIND_TOAST, self.toast_seq(), LOCAL_BUTTONS)
                {
                    self.undo_delete();
                }
            }
            _ => {}
        }
        true
    }
}
