//! Minimal clipboard access (Batch 6c Phase 2b).
//!
//! Used by the pairing flow: copy the offer code on the source device, paste it
//! on the joiner. A fresh `arboard::Clipboard` per call — cheap enough for the
//! occasional copy/paste, and it avoids holding the OS clipboard open.

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
