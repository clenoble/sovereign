//! History names the device (v0.0.12 item 14,
//! `doc/plans/V0012-items14-19-devices.md`, R1 and R2).
//!
//! - **R1** — a version received from a peer is stored as "Synced from {id}"
//!   or "Received from {id}"; History shows and announces it with the
//!   device's name in place of the id. The stored message is unchanged: this
//!   is the display only.
//! - **R2** — a row's message line is one line, cut with "…" at the row's
//!   width, so it never runs over the time line under it.

use std::collections::HashMap;

use unicode_segmentation::UnicodeSegmentation;

use crate::text::TextShaper;

/// The stored forms that name a device: R1's two (the sending peer's id) and,
/// since item 22 (D5), a concurrent edit kept as a version, whose device is
/// the stamp's author (`sovereign_docs` `keep_as_version`).
const FORMS: [&str; 3] = ["Synced from ", "Received from ", "Concurrent edit from "];

/// What a row says for this device's own id (R1).
const THIS_DEVICE: &str = "this device";

const ELLIPSIS: &str = "\u{2026}";

/// How many characters of a forgotten device's id are shown: its **last**
/// ones, since every libp2p id starts with `12D3KooW` (R1, laptop 0139).
const SHORT_ID: usize = 8;

/// R1: the text a History row shows and announces for a version's stored
/// `message`: "Synced from {id}" / "Received from {id}" / "Concurrent edit
/// from {author}" with the device named as [`device_label`] names it; any
/// other message as it is.
pub(crate) fn shown_message(message: &str, names: &HashMap<String, String>, own_id: &str) -> String {
    for form in FORMS {
        let Some(id) = message.strip_prefix(form) else { continue };
        let id = id.trim();
        if id.is_empty() {
            return message.to_string();
        }
        return format!("{form}{}", device_label(id, names, own_id));
    }
    message.to_string()
}

/// How a device is named on screen, from a stamp or a peer's id (R1, item 22
/// D5 — one identity space for display): "this device" for `own_id` (this
/// persona's peer id); the device's name from `names` (peer id → name, as
/// `App::device_names` gives them, renamed or learned); a device string that
/// is not a libp2p peer id as it is (a display name a stamp carried before
/// item 22); and "…" with the last 8 characters for a peer id no longer
/// paired.
pub(crate) fn device_label(device: &str, names: &HashMap<String, String>, own_id: &str) -> String {
    if !own_id.is_empty() && device == own_id {
        THIS_DEVICE.to_string()
    } else if let Some(name) = names.get(device).filter(|n| !n.trim().is_empty()) {
        name.clone()
    } else if !sovereign_crypto::peer_identity::is_peer_id(device) {
        device.to_string()
    } else {
        short_id(device)
    }
}

/// "…" and the id's last [`SHORT_ID`] characters (the whole id when shorter).
fn short_id(id: &str) -> String {
    let n = id.chars().count();
    if n <= SHORT_ID {
        return id.to_string();
    }
    let tail: String = id.chars().skip(n - SHORT_ID).collect();
    format!("{ELLIPSIS}{tail}")
}

/// The shaped width of `text` on one line.
fn one_line_width(shaper: &mut TextShaper, text: &str, font_px: f32) -> f64 {
    shaper.shape(text, 1.0e6, font_px).width() as f64
}

/// R2: `text` on one line no wider than `width` (logical px) when shaped by
/// `shaper` at `font_px`: itself when it fits, else a prefix of it followed
/// by "…". The prefix ends on a grapheme cluster's boundary.
pub(crate) fn cut_to_width(shaper: &mut TextShaper, text: &str, width: f64, font_px: f32) -> String {
    // One line: a line break is drawn as a space (a stored message is one
    // line; this keeps a stray one from making two).
    let flat: String = if text.contains(['\n', '\r']) {
        text.replace("\r\n", " ").replace(['\n', '\r'], " ")
    } else {
        text.to_string()
    };
    if one_line_width(shaper, &flat, font_px) <= width {
        return flat;
    }
    // The cut points: every grapheme boundary of the text.
    let bounds: Vec<usize> = flat.grapheme_indices(true).map(|(i, _)| i).skip(1).collect();
    let candidate = |end: usize| -> String {
        let kept = flat[..end].trim_end();
        format!("{kept}{ELLIPSIS}")
    };
    // The longest prefix that fits with its ellipsis (widths grow with the
    // prefix, so a binary search over the boundaries).
    let (mut lo, mut hi) = (0usize, bounds.len());
    let mut best = ELLIPSIS.to_string();
    while lo < hi {
        let mid = (lo + hi) / 2;
        let c = candidate(bounds[mid]);
        if one_line_width(shaper, &c, font_px) <= width {
            best = c;
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_short_id_is_kept_whole() {
        let names = HashMap::new();
        assert_eq!(shown_message("Synced from abc", &names, "own"), "Synced from abc");
        assert_eq!(shown_message("Synced from ", &names, "own"), "Synced from ");
        assert_eq!(shown_message("Synced from x", &names, ""), "Synced from x");
    }
}
