//! Minimal clipboard access (Batch 6c Phase 2b).
//!
//! Used by the pairing flow: copy the offer code on the source device, paste it
//! on the joiner. A fresh `arboard::Clipboard` per call — cheap enough for the
//! occasional copy/paste, and it avoids holding the OS clipboard open.

use sovereign_editor::host::{Clipboard, ClipboardContent, ClipboardPrivacy, ClipboardWrite};

/// Read the clipboard as text, or `None` if empty / unavailable.
pub(crate) fn get_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

/// Write `text` to the clipboard. Returns whether it succeeded.
pub(crate) fn set_text(text: &str) -> bool {
    arboard::Clipboard::new()
        .and_then(|mut c| c.set_text(text.to_string()))
        .is_ok()
}

// ---- the privacy flags of an editor copy (C-TEXT, *Clipboard*) ----------
//
// Moved from `app/editor_v2.rs`: the document editor and the new-thread
// prompt's name field mark every copy the same way.

/// Windows: all three exclusions — `ExcludeClipboardContentFromMonitorProcessing`,
/// `CanIncludeInClipboardHistory = 0`, `CanUploadToCloudClipboard = 0`.
#[cfg(windows)]
pub(crate) fn set_private(cb: &mut arboard::Clipboard, text: &str, p: ClipboardPrivacy) -> Result<(), arboard::Error> {
    use arboard::SetExtWindows;
    let mut s = cb.set();
    if p.exclude_from_monitoring {
        s = s.exclude_from_monitoring();
    }
    if p.exclude_from_cloud_sync {
        s = s.exclude_from_cloud();
    }
    if p.exclude_from_history {
        s = s.exclude_from_history();
    }
    s.text(text.to_string())
}

/// Linux / BSD: the `x-kde-passwordManagerHint` that KDE's Klipper and some
/// managers honour (arboard's `exclude_from_history`); other managers may
/// still keep the copy — a stated limit (ch. 12).
#[cfg(all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))]
pub(crate) fn set_private(cb: &mut arboard::Clipboard, text: &str, p: ClipboardPrivacy) -> Result<(), arboard::Error> {
    use arboard::SetExtLinux;
    let s = cb.set();
    let s = if p.exclude_from_history || p.exclude_from_cloud_sync || p.exclude_from_monitoring {
        s.exclude_from_history()
    } else {
        s
    };
    s.text(text.to_string())
}

/// Elsewhere (macOS): no exclusion is applied — a stated limit.
#[cfg(not(any(windows, all(unix, not(any(target_os = "macos", target_os = "android", target_os = "emscripten"))))))]
pub(crate) fn set_private(cb: &mut arboard::Clipboard, text: &str, _p: ClipboardPrivacy) -> Result<(), arboard::Error> {
    cb.set_text(text.to_string())
}

/// The clipboard of a plain text field built on the editor component (the
/// new-thread prompt's name field, every build): plain text in and out, each
/// copy marked as every editor copy is ([`set_private`]).
///
/// It never hands back a Sovereign payload, so a personal-data token copied
/// from a document pastes into the field as the text it showed (its label),
/// never as the token: such a field resolves no tokens, and a name is stored
/// as typed. Nor does it run a delayed clear: the field shows no revealed
/// value, so its copies carry no `clear_after`. One `arboard::Clipboard` is
/// kept while the field lives (on Linux, dropping it right after a write can
/// lose the data).
#[derive(Default)]
pub(crate) struct FieldClipboard {
    cb: Option<arboard::Clipboard>,
}

impl FieldClipboard {
    fn cb(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.cb.is_none() {
            self.cb = arboard::Clipboard::new().ok();
        }
        self.cb.as_mut()
    }
}

impl Clipboard for FieldClipboard {
    fn read(&mut self) -> Option<ClipboardContent> {
        let text = self.cb()?.get_text().ok()?;
        Some(ClipboardContent { text, sovereign: None })
    }
    fn write(&mut self, item: ClipboardWrite) {
        if let Some(cb) = self.cb() {
            let _ = set_private(cb, &item.text, item.privacy);
        }
    }
}
