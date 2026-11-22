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

/// R1: the text a History row shows and announces for a version's stored
/// `message`, with no versions to name: the document service's one function
/// (spec ch. 12, HN5), which the phone calls too. A History row passes its
/// document's versions through [`crate::panels::HistoryPanel::name_devices`].
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn shown_message(message: &str, names: &HashMap<String, String>, own_id: &str) -> String {
    sovereign_docs::history_names::shown_message(message, names, own_id, &[])
}

/// How a device is named on screen (R1, item 22 D5; spec ch. 12, HN1): the
/// document service's rule, the same on both hosts.
pub(crate) fn device_label(device: &str, names: &HashMap<String, String>, own_id: &str) -> String {
    sovereign_docs::history_names::device_label(device, names, own_id)
}

const ELLIPSIS: &str = "\u{2026}";

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
