//! The desktop's History names devices and versions, never raw ids (spec
//! ch. 12, *History on both hosts — names, not ids*, HN1-HN4; v0.0.13, the
//! review's item 19).
//!
//! Each test unlocks an `App` over an in-memory store, as the device-name
//! tests do, writes versions through the document service, and reads History
//! as the window shows it and gives it to a screen reader.

use chrono::{DateTime, Local, Utc};
use sovereign_ai::docs_host::new_user_document;
use sovereign_docs::history_names::ALREADY_CURRENT;
use sovereign_docs::{Edit, EditContent, EditSession, Origin};

use super::device_names_tests::{
    docs, history_lines, install_node, last8, peer_id, remote, stored_messages, this_name, unlocked, Unlocked,
    ELLIPSIS, LEARNED,
};
use super::App;

/// History's format for a time: `YYYY-MM-DD HH:MM`, in local time.
fn local(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
}

/// A document written here, then, for each of `from` in turn, edited here in
/// the editor (a save that makes no version) and synced from that device, so
/// the sync keeps the edit as « Before sync from {device} ». Returns its id.
fn synced(u: &mut Unlocked, title: &str, from: &[&str]) -> String {
    let d = docs(&u.app);
    let id = u.app.rt.block_on(d.create(new_user_document(title, &u.thread, "written here"))).unwrap().id;
    for (i, dev) in from.iter().enumerate() {
        let base = u.app.rt.block_on(d.read(&id)).unwrap().revision;
        let edit = Edit {
            base,
            title: None,
            content: Some(EditContent::PlainText(format!("edited here before sync {i}"))),
            origin: Origin::Editor { session: EditSession(41) },
            explicit: false,
            pasted_tokens: vec![],
        };
        u.app.rt.block_on(d.save(&id, edit)).expect("an edit here");
        let r = remote(&id, &u.thread, title, &format!("from device {i}"), dev, i as i64 + 1);
        u.app.rt.block_on(d.apply_remote(r)).expect("the remote state applies");
    }
    u.app.load_workspace_now();
    id
}

/// The document's versions, newest first: (commit id, time).
fn versions(app: &App, id: &str) -> Vec<(String, DateTime<Utc>)> {
    app.rt.block_on(docs(app).versions(id)).unwrap().into_iter().map(|v| (v.commit_id, v.timestamp)).collect()
}

/// HN1: « Before sync from {device} », written at every applied sync, names the
/// device as the other rows do: its paired name, or « … » and the last 8
/// characters of a device no longer paired. Never the whole peer id.
#[test]
fn hn1_before_sync_from_names_the_device() {
    let mut u = unlocked("hn1-before-sync");
    let dir = tempfile::tempdir().unwrap();
    let (own, paired, gone) = (peer_id(), peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&paired, LEARNED)], dir.path());

    let id = synced(&mut u, "Notes", &[&paired, &gone]);
    let stored = stored_messages(&u.app, &id);
    for dev in [&paired, &gone] {
        assert!(stored.contains(&format!("Before sync from {dev}")), "precondition: a version « Before sync from {dev} » is stored: {stored:?}");
    }

    let lines = history_lines(&mut u.app, &id).join("\n");
    assert!(lines.contains(&format!("Before sync from {LEARNED}")), "HN1: the paired device's name\n{lines}");
    assert!(lines.contains(&format!("Before sync from {ELLIPSIS}{}", last8(&gone))), "HN1: a device no longer paired\n{lines}");
    for dev in [&paired, &gone, &own] {
        assert!(!lines.contains(dev.as_str()), "HN1: never the whole peer id ({dev})\n{lines}");
    }
}

/// HN2 and HN3: a restored version's row names the version restored by its
/// date and time, never a raw commit id; the stored message is unchanged.
#[test]
fn hn2_hn3_a_restore_names_the_version_by_its_time() {
    let mut u = unlocked("hn2-restored");
    let dir = tempfile::tempdir().unwrap();
    let (own, paired) = (peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&paired, LEARNED)], dir.path());
    let id = synced(&mut u, "Notes", &[&paired]);

    let before = versions(&u.app, &id);
    assert!(before.len() >= 2, "precondition: an earlier version to restore: {before:?}");
    let (older, older_at) = before.last().unwrap().clone();
    u.app.restore_commit(&id, &older);
    let after = versions(&u.app, &id);
    assert!(
        after.len() > before.len(),
        "precondition: the restore wrote a version; notice: {:?}; versions: {before:?}; messages: {:?}",
        u.app.notice.as_ref().map(|n| n.said.clone()),
        stored_messages(&u.app, &id),
    );

    let stored = stored_messages(&u.app, &id);
    assert!(stored.contains(&format!("Restored from {older}")), "HN3: the stored message is unchanged: {stored:?}");

    let lines = history_lines(&mut u.app, &id).join("\n");
    assert!(
        lines.contains(&format!("Restored from the version of {}", local(older_at))),
        "HN2: the version restored, by its date and time in local time\n{lines}"
    );
    assert!(!lines.contains("commit:"), "HN2: never a raw commit id\n{lines}");
    let key = older.trim_start_matches("commit:");
    assert!(!lines.contains(key), "HN2: not even its key ({key})\n{lines}");
}

/// Every time a History row shows is local, the row's own included (the
/// Fable maille, by delegation of Céline, 2026-11-18, after AT11): each row's
/// time is its version's, in local time, and the restored row names the
/// version restored with the same words as that version's own row.
///
/// Where the local zone is UTC at that date, this cannot tell UTC from local
/// time: it holds there whatever the code does. It says so in its output.
#[test]
fn hn2_a_rows_own_time_is_local_and_matches_how_it_is_named() {
    let mut u = unlocked("hn2-row-time");
    let dir = tempfile::tempdir().unwrap();
    let (own, paired) = (peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&paired, LEARNED)], dir.path());
    let id = synced(&mut u, "Notes", &[&paired]);
    let (older, older_at) = versions(&u.app, &id).last().unwrap().clone();
    u.app.restore_commit(&id, &older);
    if older_at.with_timezone(&Local).offset().local_minus_utc() == 0 {
        println!("the local zone is UTC here: this test cannot tell UTC from local time");
    }

    u.app.windows.retain(|w| !matches!(&w.content, super::WindowContent::History(_)));
    u.app.open_history(&id, "Notes");
    let hp = u
        .app
        .windows
        .iter()
        .find_map(|w| match &w.content {
            super::WindowContent::History(h) if h.doc_id == id => Some(h),
            _ => None,
        })
        .expect("the History window opens");
    for (cid, t) in versions(&u.app, &id) {
        let row = hp.commits.iter().find(|c| c.commit_id == cid).unwrap_or_else(|| panic!("a row for {cid}"));
        assert_eq!(row.when, local(t), "a row's own time is local: {cid}");
    }
    let older_row = hp.commits.iter().find(|c| c.commit_id == older).unwrap();
    let named = format!("Restored from the version of {}", older_row.when);
    assert!(
        hp.commits.iter().any(|c| c.message == named),
        "the restored row names the version with its own row's words, « {named} »: {:?}",
        hp.commits.iter().map(|c| (&c.message, &c.when)).collect::<Vec<_>>()
    );
}

/// HN4: choosing the version that is already the current one writes nothing,
/// adds no version, and the desktop says so.
#[test]
fn hn4_restoring_the_current_version_says_so_and_writes_nothing() {
    let mut u = unlocked("hn4-current");
    let dir = tempfile::tempdir().unwrap();
    let (own, paired) = (peer_id(), peer_id());
    let this = this_name(&mut u.app);
    let _node = install_node(&mut u.app, &own, &this, &[(&paired, LEARNED)], dir.path());
    let id = synced(&mut u, "Notes", &[&paired]);

    let before = versions(&u.app, &id);
    let (current, _) = before.first().unwrap().clone();
    let doc_before = u.app.rt.block_on(docs(&u.app).read(&id)).unwrap();
    u.app.notice = None;
    u.app.restore_commit(&id, &current);

    assert_eq!(versions(&u.app, &id).len(), before.len(), "HN4: no version is added");
    let doc_after = u.app.rt.block_on(docs(&u.app).read(&id)).unwrap();
    assert_eq!(doc_after.revision, doc_before.revision, "HN4: nothing is written");
    let said = u.app.notice.as_ref().map(|n| n.said.clone()).expect("HN4: the desktop says so");
    assert!(said.0 == ALREADY_CURRENT || said.1 == ALREADY_CURRENT, "HN4: « {ALREADY_CURRENT} », said: {said:?}");
    assert_ne!(said.0, "Not restored", "HN4: it is not a failure: {said:?}");
}
