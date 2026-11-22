//! History names devices and versions, never raw ids (spec ch. 12, *History
//! on both hosts — names, not ids*, HN1, HN2, HN5): the one function both
//! hosts call.

use std::collections::HashMap;

use chrono::{DateTime, Local, TimeZone, Utc};

use crate::history_names::shown_message;

/// A peer id as libp2p writes them: 52 characters, all starting `12D3KooW`.
fn peer(tail: char) -> String {
    let mut id = String::from("12D3KooW");
    while id.len() < 52 {
        id.push(tail);
    }
    id
}

fn last8(id: &str) -> String {
    id.chars().skip(id.chars().count() - 8).collect()
}

/// History's format for a time: `YYYY-MM-DD HH:MM`, in local time.
fn local(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
}

struct Fixture {
    paired: String,
    own: String,
    gone: String,
    devices: HashMap<String, String>,
}

fn fixture() -> Fixture {
    let (paired, own, gone) = (peer('a'), peer('b'), peer('c'));
    let devices = HashMap::from([(paired.clone(), "Studio laptop".to_string())]);
    Fixture { paired, own, gone, devices }
}

/// HN1: every form that names a device names it: by its paired name, « this
/// device » for this device's own id, « … » and the last 8 characters for a
/// device no longer paired. « Before sync from » follows the rule like the others.
#[test]
fn hn1_every_row_that_names_a_device_names_it() {
    let f = fixture();
    for form in ["Synced from ", "Received from ", "Concurrent edit from ", "Before sync from "] {
        let shown = |id: &str| shown_message(&format!("{form}{id}"), &f.devices, &f.own, &[]);
        assert_eq!(shown(&f.paired), format!("{form}Studio laptop"));
        assert_eq!(shown(&f.own), format!("{form}this device"));
        assert_eq!(shown(&f.gone), format!("{form}\u{2026}{}", last8(&f.gone)));
        for id in [&f.paired, &f.own, &f.gone] {
            assert!(!shown(id).contains(id.as_str()), "HN1: never the whole peer id: {}", shown(id));
        }
    }
}

/// HN2: « Restored from {commit id} » names the version by its date and time,
/// in local time, as History writes them.
#[test]
fn hn2_a_restored_version_is_named_by_its_date_and_time() {
    let f = fixture();
    let t = Utc.with_ymd_and_hms(2026, 11, 17, 14, 2, 0).unwrap();
    let other = Utc.with_ymd_and_hms(2026, 11, 16, 9, 30, 0).unwrap();
    let versions = [("commit:aaa111", other), ("commit:bbb222", t)];
    let shown = shown_message("Restored from commit:bbb222", &f.devices, &f.own, &versions);
    assert_eq!(shown, format!("Restored from the version of {}", local(t)));
    assert!(!shown.contains("commit:"), "HN2: never a raw commit id: {shown}");
}

/// HN2: a version no longer in the document's history is « an earlier version ».
#[test]
fn hn2_a_version_no_longer_found_is_an_earlier_version() {
    let f = fixture();
    let versions = [("commit:aaa111", Utc.with_ymd_and_hms(2026, 11, 16, 9, 30, 0).unwrap())];
    let shown = shown_message("Restored from commit:gone999", &f.devices, &f.own, &versions);
    assert_eq!(shown, "Restored from an earlier version");
}

/// HN2: « Restored from backup » stays as it is; any other message as it is.
#[test]
fn hn2_restored_from_backup_and_other_messages_stay() {
    let f = fixture();
    assert_eq!(shown_message("Restored from backup", &f.devices, &f.own, &[]), "Restored from backup");
    for m in ["Edited", "Created", "Synced", "Saved from the editor"] {
        assert_eq!(shown_message(m, &f.devices, &f.own, &[]), m);
    }
}

/// HN2, « Never a raw commit id »: no row shows one, whatever its form. The
/// prior version a restore keeps is stored as « Before restoring {commit id} ».
#[test]
fn hn2_no_row_shows_a_raw_commit_id() {
    let f = fixture();
    let t = Utc.with_ymd_and_hms(2026, 11, 17, 14, 2, 0).unwrap();
    let versions = [("commit:bbb222", t)];
    for stored in ["Restored from commit:bbb222", "Before restoring commit:bbb222", "Restored from commit:gone999"] {
        let shown = shown_message(stored, &f.devices, &f.own, &versions);
        assert!(!shown.contains("commit:"), "HN2: never a raw commit id: {stored:?} is shown {shown:?}");
        assert!(!shown.contains("bbb222") && !shown.contains("gone999"), "HN2: not even its key: {shown:?}");
    }
}

/// « Before restoring {commit id} », the version kept before a restore, is
/// named as HN2 names a version (the Fable maille, by delegation of Céline,
/// 2026-11-18): « Before restoring the version of {YYYY-MM-DD HH:MM} », local
/// time, or « Before restoring an earlier version ».
#[test]
fn hn2_before_restoring_names_the_version_as_hn2_does() {
    let f = fixture();
    let t = Utc.with_ymd_and_hms(2026, 11, 17, 14, 2, 0).unwrap();
    let versions = [("commit:bbb222", t)];
    assert_eq!(
        shown_message("Before restoring commit:bbb222", &f.devices, &f.own, &versions),
        format!("Before restoring the version of {}", local(t))
    );
    assert_eq!(shown_message("Before restoring commit:gone999", &f.devices, &f.own, &versions), "Before restoring an earlier version");
}
