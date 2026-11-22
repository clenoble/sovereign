//! How History names a row, on both hosts (spec ch. 12, *History on both
//! hosts — names, not ids*, HN1-HN5).
//!
//! The stored message of a version stays as it was written (HN3); the names
//! are given where the row is shown, by this one function, which the desktop
//! and the phone both call (HN5), so they show the same words for the same row.

use std::collections::HashMap;

use chrono::{DateTime, Local, Utc};

/// The text a History row shows, and gives to a screen reader, for a version
/// whose stored message is `message`:
///
/// - **HN1:** a device named in the message (« Synced from {id} », « Received
///   from {id} », « Concurrent edit from {id} », « Before sync from {id} ») is
///   named by its name in `devices` (peer id → the name this device's paired
///   list holds); « this device » for `own_id`; for a device no longer paired,
///   « … » and the id's last 8 characters. Never the whole peer id.
/// - **HN2:** « Restored from {commit id} » is « Restored from the version of
///   {YYYY-MM-DD HH:MM} », in local time, the time of that version as
///   `versions` (the same document's history: commit id, time) gives it; « an
///   earlier version » when it is not there. Never a raw commit id. « Restored
///   from backup » stays as it is.
/// - Any other message as it is.
///
/// « Before restoring {commit id} », the version a restore keeps of what it
/// replaces, follows HN2's rule: « Before restoring the version of
/// {YYYY-MM-DD HH:MM} », or « an earlier version ».
pub fn shown_message(
    message: &str,
    devices: &HashMap<String, String>,
    own_id: &str,
    versions: &[(&str, DateTime<Utc>)],
) -> String {
    for form in DEVICE_FORMS {
        let Some(id) = message.strip_prefix(form) else { continue };
        let id = id.trim();
        if id.is_empty() {
            return message.to_string();
        }
        return format!("{form}{}", device_label(id, devices, own_id));
    }
    if message == RESTORED_FROM_BACKUP {
        return message.to_string();
    }
    for form in VERSION_FORMS {
        let Some(commit_id) = message.strip_prefix(form) else { continue };
        let commit_id = commit_id.trim();
        if commit_id.is_empty() {
            return message.to_string();
        }
        return match versions.iter().find(|(id, _)| *id == commit_id) {
            Some((_, at)) => format!("{form}the version of {}", local_time(*at)),
            None => format!("{form}an earlier version"),
        };
    }
    message.to_string()
}

/// How a device is named on screen, from a stamp or a peer's id (HN1, as item
/// 14 and item 22 built it on the desktop): « this device » for `own_id`; the
/// name `devices` holds for it; a device string that is not a libp2p peer id
/// as it is (a display name a stamp carried before item 22); and « … » with
/// the last 8 characters for a peer id no longer paired.
pub fn device_label(device: &str, devices: &HashMap<String, String>, own_id: &str) -> String {
    if !own_id.is_empty() && device == own_id {
        THIS_DEVICE.to_string()
    } else if let Some(name) = devices.get(device).filter(|n| !n.trim().is_empty()) {
        name.clone()
    } else if !sovereign_crypto::peer_identity::is_peer_id(device) {
        device.to_string()
    } else {
        short_id(device)
    }
}

/// History's format for a time: `YYYY-MM-DD HH:MM`, in local time.
pub fn local_time(at: DateTime<Utc>) -> String {
    at.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
}

/// The stored forms that name a device.
const DEVICE_FORMS: [&str; 4] = ["Synced from ", "Received from ", "Concurrent edit from ", "Before sync from "];

/// The stored forms that name a version by its commit id.
const VERSION_FORMS: [&str; 2] = ["Restored from ", "Before restoring "];

/// A restore from a backup names no version, and stays as it is.
const RESTORED_FROM_BACKUP: &str = "Restored from backup";

/// What a row says for this device's own id.
const THIS_DEVICE: &str = "this device";

/// How many characters of a forgotten device's id are shown: its **last**
/// ones, since every libp2p id starts with `12D3KooW`.
const SHORT_ID: usize = 8;

/// « … » and the id's last [`SHORT_ID`] characters (the whole id when shorter).
fn short_id(id: &str) -> String {
    let n = id.chars().count();
    if n <= SHORT_ID {
        return id.to_string();
    }
    let tail: String = id.chars().skip(n - SHORT_ID).collect();
    format!("\u{2026}{tail}")
}

/// HN4: what a host says when the version chosen is already the current one.
pub const ALREADY_CURRENT: &str = "This is already the current version. Nothing was changed.";
