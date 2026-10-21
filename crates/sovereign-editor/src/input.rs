//! Input events the host forwards to the editor (keys, text input, input-method
//! composition, pointer, focus) and the editor's commands.

use crate::doc::BlockId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers { shift: false, ctrl: false, alt: false, meta: false };
    pub const SHIFT: Modifiers = Modifiers { shift: true, ctrl: false, alt: false, meta: false };
    pub const CTRL: Modifiers = Modifiers { shift: false, ctrl: true, alt: false, meta: false };
    pub const CTRL_SHIFT: Modifiers = Modifiers { shift: true, ctrl: true, alt: false, meta: false };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NamedKey {
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Backspace,
    Delete,
    Enter,
    Tab,
    Escape,
    F6,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Key {
    Named(NamedKey),
    /// A character key, as the logical key (lowercase for letters); used for
    /// shortcuts such as Ctrl+Z. Text to insert arrives as [`InputEvent::Text`].
    Character(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyEvent {
    pub key: Key,
    pub modifiers: Modifiers,
}

/// Input-method composition (mirrors winit's `Ime`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImeEvent {
    Enabled,
    /// Composition text to show in place; empty text ends the composition
    /// without committing (cancel). `cursor` is a byte range inside `text`.
    Preedit { text: String, cursor: Option<(usize, usize)> },
    /// Text to insert, once.
    Commit(String),
    Disabled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerButton {
    Primary,
    Secondary,
}

/// Pointer positions are in the editor's coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointerEvent {
    Down { x: f64, y: f64, button: PointerButton, click_count: u8, modifiers: Modifiers },
    Move { x: f64, y: f64 },
    Up { x: f64, y: f64, button: PointerButton },
}

#[derive(Clone, Debug, PartialEq)]
pub enum InputEvent {
    Key(KeyEvent),
    /// Committed text from the keyboard (one key press may give several
    /// characters, e.g. a whole emoji).
    Text(String),
    Ime(ImeEvent),
    Pointer(PointerEvent),
    /// The editor gained (`true`) or lost (`false`) keyboard focus.
    Focus(bool),
}

/// Editor commands — what menus and the host's own shortcuts call. Each is also
/// reachable from the keyboard inside the editor (guarantee 10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    SelectAll,
    /// Save now (Ctrl+S).
    Save,
    /// The explicit user action that removes one block — the only way an
    /// unknown block is removed (ch. 03).
    RemoveBlock(BlockId),
}
