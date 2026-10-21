//! # sovereign-editor — the document editor component
//!
//! The test suite in `tests/` is the executable form of the contract and was
//! written first (spec ch. 12, *The document editor — contract (C-TEXT)*); the
//! editor was then built to make it pass, by a different author, who did not
//! modify the tests. See `TESTS.md`.
//!
//! The editor edits one document in the v2 block format (spec ch. 03) **in
//! memory**. It owns no storage, opens no connection and writes no file. The
//! host gives it the document and a save function, input events, a clipboard,
//! notice of remote changes, an accessibility sink — and, so tests are
//! deterministic, a clock and the fonts to lay out with.
//!
//! ## Which item serves which guarantee
//!
//! | Guarantee (C-TEXT) | Items |
//! |---|---|
//! | 1 Nothing typed is lost | [`Editor::document`], [`Editor::save_status`], [`Editor::has_unsaved_changes`], [`Editor::take_notices`], [`host::DocumentService`], [`Editor::save_finished`], [`Editor::request_close`] |
//! | 2 Unknown blocks pass through | [`doc`] (the v2 model of `sovereign-docformat`: [`doc::DocumentV2::read`], [`doc::DocumentV2::to_json_string`]), [`input::Command::RemoveBlock`] |
//! | 3 No plaintext leaves memory except through save | no IO in this crate; `Debug` impls of [`Editor`], [`doc::DocumentV2`], [`doc::Block`] redact text |
//! | 4 No network | the dependency list in `Cargo.toml` (checked by `tests/g4_no_network.rs`) |
//! | 5 Caret and selection by grapheme cluster, bidi | [`Editor::selection`], [`Editor::set_selection`], [`Editor::caret_rect`], [`input::InputEvent`] |
//! | 6 Input methods | [`input::ImeEvent`], [`host::WindowHost::set_ime_cursor_area`] |
//! | 7 Personal-data tokens atomic | [`host::PiiDisplay`], [`Editor::pii_display_changed`], [`host::ClipboardContent::sovereign`] |
//! | 8 Undo / redo | [`input::Command::Undo`], [`input::Command::Redo`] |
//! | 9 Large documents | [`MAX_EDITABLE_TEXT_BYTES`], [`Editor::read_only_reason`], [`ReadOnlyReason::TooLarge`], [`Editor::paint`] |
//! | 10 Accessible | [`host::AccessibilitySink`], [`Editor::accessibility_tree`], [`EventResult`], [`Editor::open_field`] ([`FieldKind::Password`]) |
//! | One editing device | [`Editor::editing_moved_to`], [`OpenMode::ReadOnly`], [`ReadOnlyReason::EditingElsewhere`] |
//! | Clipboard | [`host::Clipboard`], [`host::ClipboardWrite`] (privacy flags, `clear_after`), [`host::ClipboardContent`] (Sovereign payload keeps tokens) |
//! | Autosave with versions | [`host::Clock`], [`Editor::poll`], [`Editor::next_deadline`], [`input::Command::Save`] |
//! | Markdown text model | [`doc::Block::markup`]; drawn lightly by [`Editor::paint`] (headings, emphasis, lists, links) without changing the source |
//! | Theme (ch. 12; WCAG 2.1 AA) | [`Palette`] ([`Palette::DARK`], [`Palette::LIGHT`]), [`Editor::set_palette`], [`contrast_ratio`] |
//! | Scrolling (wheel) | [`Editor::scroll_by`], [`Editor::scroll_offset`] |
//! | Remote change (sync) | [`Editor::remote_change`], [`host::RemoteChange`] |
//! | Title field (title contract T1–T7, spec ch. 12 *Title*; tests `tests/t_title.rs`) | [`EditorPart`], [`Editor::title`], [`Editor::title_font`] ([`TitleFont`]), [`Editor::part`], [`Editor::focus_title`], [`Editor::title_selection`], [`Editor::set_title_selection`], [`host::SaveRequest::title`] |

pub mod doc;
pub mod host;
pub mod input;

mod a11y;
mod clip;
mod editor;
mod ids;
mod md;
/// The JSON scanner of the v2 model (`sovereign-docformat`), shared.
mod json {
    pub(crate) use sovereign_docformat::json::*;
}
mod text;
mod view;

pub use editor::*;
pub use view::{contrast_ratio, Palette};

/// A rectangle in the editor's own coordinate space (logical pixels, origin at
/// the editor's top-left, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}
