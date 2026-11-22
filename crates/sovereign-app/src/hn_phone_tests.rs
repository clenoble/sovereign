//! History's names on the phone (spec ch. 12, *History on both hosts — names, not ids*, HN1-HN5;
//! v0.0.13, the scope's C5 line « device names, not the technical id »). Written before the code
//! by its test author: whoever builds it makes these pass without editing this file; a test that
//! looks wrong is reported, not worked around.
//!
//! The desktop's side is held by `sovereign-shell`'s `history_names_desktop_tests`; this file holds
//! the phone's, through the layers under its commands:
//! - `documents::commits_listing_named`, what `list_commits` answers, each row with its
//!   `shown_message` and `local_time`;
//! - `documents::history_names_in`, the session's paired names and this device's id;
//! - `documents::restore_in`, what `restore_commit` answers, with HN4's sentence.
//!
//! Versions are written through the document service, as the phone writes them: an edit here,
//! then a sync from a device, which keeps the edit as « Before sync from {device} ».

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Local, Utc};
use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::schema::Thread;
use sovereign_db::traits::GraphDB;
use sovereign_docs::history_names::ALREADY_CURRENT;
use sovereign_docs::{DocumentService, Edit, EditContent, EditSession, Origin, RemoteDocument, Trust};

use crate::tauri_commands::documents::{commits_listing, commits_listing_named, restore_in, NamedCommitListDto};

const ELLIPSIS: char = '\u{2026}';
const PIXEL: &str = "Pixel of hers";

/// A libp2p peer id, as the node prints it (`12D3KooW…`).
fn peer_id(tag: &str) -> String {
    let mk = sovereign_crypto::master_key::MasterKey::generate();
    let key = sovereign_crypto::device_key::DeviceKey::derive(&mk, tag).unwrap();
    sovereign_crypto::peer_identity::peer_id(&key).unwrap()
}

fn last8(id: &str) -> &str {
    &id[id.len() - 8..]
}

/// History's format for a time: `YYYY-MM-DD HH:MM`, in local time.
fn local(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%Y-%m-%d %H:%M").to_string()
}

fn v2_text(text: &str) -> String {
    serde_json::json!({"format": "sovereign.doc", "v": 2,
        "blocks": [{"type": "text", "id": "00000000000000000000000000001001", "markup": "markdown", "text": text}]})
    .to_string()
}

struct S {
    docs: Arc<DocumentService>,
    thread: String,
}

async fn session(tag: &str) -> S {
    let store = testing::encrypted_memory_store(tag).await;
    let thread = store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap();
    let docs = Arc::new(build_document_service(store, Some(testing::account_key()), "phone".into()));
    S { docs, thread }
}

impl S {
    /// A document written here, then, for each of `from` in turn, edited here and synced from that
    /// device, so the sync keeps the edit as « Before sync from {device} ». Its id.
    async fn synced(&self, title: &str, from: &[&str]) -> String {
        let id = self.docs.create(new_user_document(title, &self.thread, "written here")).await.unwrap().id;
        for (i, dev) in from.iter().enumerate() {
            let base = self.docs.read(&id).await.unwrap().revision;
            let edit = Edit {
                base,
                title: None,
                content: Some(EditContent::PlainText(format!("edited here before sync {i}"))),
                origin: Origin::Editor { session: EditSession(41) },
                explicit: false,
                pasted_tokens: vec![],
            };
            self.docs.save(&id, edit).await.expect("an edit here");
            let remote = RemoteDocument {
                id: id.clone(),
                title: title.into(),
                content: v2_text(&format!("from device {i}")),
                thread_id: self.thread.clone(),
                trust: Some(Trust::Owned),
                provenance: None,
                modified_at: Utc::now() + chrono::Duration::minutes(i as i64 + 1),
                deleted_at: None,
                device: (*dev).into(),
                trust_decision: None,
            };
            self.docs.apply_remote(remote).await.expect("the remote state applies");
        }
        id
    }

    /// The document's versions, newest first: (commit id, time).
    async fn versions(&self, id: &str) -> Vec<(String, DateTime<Utc>)> {
        self.docs.versions(id).await.unwrap().into_iter().map(|v| (v.commit_id, v.timestamp)).collect()
    }

    async fn named(&self, id: &str, devices: &HashMap<String, String>, own: &str) -> NamedCommitListDto {
        commits_listing_named(Ok(self.docs.clone()), id, devices, own).await.expect("the session is open")
    }
}

fn rows(l: &NamedCommitListDto) -> String {
    l.commits.iter().map(|c| format!("{} | {}", c.shown_message, c.local_time)).collect::<Vec<_>>().join("\n")
}

// ── HN1 ───────────────────────────────────────────────────────────────────

/// HN1: a device is named by the name the paired list holds; « … » and its last 8 characters when
/// no longer paired; « this device » for its own id. Never the whole peer id.
#[tokio::test]
async fn hn1_a_device_is_named_by_its_name_never_its_id() {
    let s = session("hn-phone-hn1").await;
    let (own, paired, gone) = (peer_id("own"), peer_id("paired"), peer_id("gone"));
    let id = s.synced("Notes", &[&paired, &gone, &own]).await;
    let devices = HashMap::from([(paired.clone(), PIXEL.to_string())]);

    let l = s.named(&id, &devices, &own).await;
    let shown = rows(&l);
    let stored: Vec<&str> = l.commits.iter().map(|c| c.commit.message.as_str()).collect();
    for dev in [&paired, &gone, &own] {
        assert!(stored.iter().any(|m| m.contains(dev.as_str())), "precondition: a stored message names {dev}: {stored:?}");
    }
    assert!(shown.contains(&format!("Before sync from {PIXEL}")), "HN1: the paired device's name\n{shown}");
    assert!(shown.contains(&format!("Before sync from {ELLIPSIS}{}", last8(&gone))), "HN1: a device no longer paired\n{shown}");
    assert!(shown.contains("Before sync from this device"), "HN1: this device's own id\n{shown}");
    for dev in [&paired, &gone, &own] {
        assert!(!shown.contains(dev.as_str()), "HN1: never the whole peer id ({dev})\n{shown}");
    }
}

// ── HN2, HN3 ──────────────────────────────────────────────────────────────

/// HN2 and HN3: a restored version's row names the version restored by its date and time, in
/// local time, the same words as that version's own row; never a raw commit id. The stored
/// message is unchanged. Every row's own time is local, in History's format.
#[tokio::test]
async fn hn2_hn3_a_restore_names_the_version_by_its_local_time() {
    let s = session("hn-phone-hn2").await;
    let paired = peer_id("paired");
    let id = s.synced("Notes", &[&paired]).await;
    let before = s.versions(&id).await;
    assert!(before.len() >= 2, "precondition: an earlier version to restore: {before:?}");
    let (older, older_at) = before.last().unwrap().clone();
    let r = restore_in(Ok(s.docs.clone()), &id, &older).await.expect("the restore runs");
    assert!(r.already_current.is_none(), "precondition: the older version was not the current one");

    let devices = HashMap::from([(paired.clone(), PIXEL.to_string())]);
    let l = s.named(&id, &devices, "").await;
    let stored: Vec<&str> = l.commits.iter().map(|c| c.commit.message.as_str()).collect();
    assert!(stored.contains(&format!("Restored from {older}").as_str()), "HN3: the stored message is unchanged: {stored:?}");

    let shown = rows(&l);
    assert!(
        shown.contains(&format!("Restored from the version of {}", local(older_at))),
        "HN2: the version restored, by its date and time in local time\n{shown}"
    );
    assert!(!shown.contains("commit:"), "HN2: never a raw commit id\n{shown}");
    let key = older.trim_start_matches("commit:");
    assert!(!shown.contains(key), "HN2: not even its key ({key})\n{shown}");

    let versions = s.versions(&id).await;
    for (c, (_, at)) in l.commits.iter().zip(&versions) {
        assert_eq!(c.local_time, local(*at), "HN2: each row's own time, local, in History's format");
    }
    let older_row = l.commits.iter().find(|c| c.commit.id == older).expect("the older version's own row");
    assert!(
        shown.contains(&format!("Restored from the version of {}", older_row.local_time)),
        "HN2: the restored row and the version's own row say the same time\n{shown}"
    );
}

// ── HN4 ───────────────────────────────────────────────────────────────────

/// HN4: restoring the version that is already current writes nothing, adds no version, and says
/// « This is already the current version. Nothing was changed. » The version made current here is
/// the one a restore writes: its title and content are the document's.
#[tokio::test]
async fn hn4_restoring_the_current_version_says_so_and_writes_nothing() {
    let s = session("hn-phone-hn4").await;
    let id = s.synced("Notes", &[&peer_id("paired")]).await;
    let (older, _) = s.versions(&id).await.last().unwrap().clone();
    let first = restore_in(Ok(s.docs.clone()), &id, &older).await.expect("the restore runs");
    assert!(first.already_current.is_none(), "precondition: the older version was not the current one");
    let before = s.versions(&id).await;
    let current = s.docs.read(&id).await.unwrap();
    let (latest, _) = before.first().unwrap().clone();

    let again = restore_in(Ok(s.docs.clone()), &id, &latest).await.expect("the restore runs");
    assert_eq!(again.already_current.as_deref(), Some(ALREADY_CURRENT), "HN4: the host says so, in these words");
    assert_eq!(s.versions(&id).await.len(), before.len(), "HN4: no version is added");
    assert_eq!(s.docs.read(&id).await.unwrap().revision, current.revision, "HN4: nothing is written");
}

/// A restore that changes the document says nothing of HN4's.
#[tokio::test]
async fn hn4_a_restore_that_changes_the_document_does_not_say_it() {
    let s = session("hn-phone-hn4-changed").await;
    let id = s.synced("Notes", &[&peer_id("paired")]).await;
    let (older, _) = s.versions(&id).await.last().unwrap().clone();
    let before = s.versions(&id).await.len();
    let r = restore_in(Ok(s.docs.clone()), &id, &older).await.expect("the restore runs");
    assert!(r.already_current.is_none(), "a restore that changes the document is not « already current »");
    assert!(s.versions(&id).await.len() > before, "it wrote a version");
}

/// No document service: a refusal, as for the listing.
#[tokio::test]
async fn the_layers_refuse_without_a_session() {
    const LOCKED: &str = "The workspace is locked: log in to change documents.";
    let r = restore_in(Err(LOCKED.to_string()), "document:any", "commit:any").await;
    assert!(matches!(r, Err(e) if e == LOCKED), "restore_in refuses with the reason");
    let l = commits_listing_named(Err(LOCKED.to_string()), "document:any", &HashMap::new(), "").await;
    assert!(matches!(l, Err(e) if e == LOCKED), "commits_listing_named refuses with the reason");
}

// ── HN5 ───────────────────────────────────────────────────────────────────

/// HN5: the phone's words for a row are those the shared crate makes, so the desktop and the phone
/// say the same for the same row; the listing without names stays what C5 built.
#[tokio::test]
async fn hn5_the_phones_words_are_the_shared_crates() {
    let s = session("hn-phone-hn5").await;
    let (own, paired) = (peer_id("own"), peer_id("paired"));
    let id = s.synced("Notes", &[&paired, &own]).await;
    let devices = HashMap::from([(paired.clone(), PIXEL.to_string())]);
    let l = s.named(&id, &devices, &own).await;
    let versions = s.versions(&id).await;
    let pairs: Vec<(&str, DateTime<Utc>)> = versions.iter().map(|(c, t)| (c.as_str(), *t)).collect();
    for c in &l.commits {
        let want = sovereign_docs::history_names::shown_message(&c.commit.message, &devices, &own, &pairs);
        assert_eq!(c.shown_message, want, "HN5: the row's words are `history_names::shown_message`'s");
    }
    let plain = commits_listing(Ok(s.docs.clone()), &id).await.expect("the session is open");
    assert_eq!(
        plain.commits.iter().map(|c| (&c.id, &c.message, &c.timestamp)).collect::<Vec<_>>(),
        l.commits.iter().map(|c| (&c.commit.id, &c.commit.message, &c.commit.timestamp)).collect::<Vec<_>>(),
        "the stored message and the RFC 3339 time stay as C5 built them"
    );
}

// ── HN1: the session's names ──────────────────────────────────────────────

/// HN1 on the phone: the names History uses are the session's paired list and this device's own
/// peer id; before the node has an identity, nothing.
#[cfg(feature = "p2p")]
#[test]
fn hn1_the_sessions_names_are_its_paired_list_and_its_own_id() {
    use crate::lot1_onboarding_tests::{fresh_profile, launch};
    use crate::tauri_commands::documents::history_names_in;
    use sovereign_p2p::pairing::{PairedDevice, PairingManager};

    let (_g, dir) = fresh_profile("hn-phone-names");
    let st = launch(&dir);
    let (none, me) = tauri::async_runtime::block_on(history_names_in(&st));
    assert!(none.is_empty() && me.is_empty(), "no node yet: no names, no own id");

    let master = sovereign_crypto::master_key::MasterKey::generate();
    let identity = Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&master, "hn-phone").unwrap());
    let own = sovereign_crypto::peer_identity::peer_id(&identity).unwrap();
    let paired = peer_id("paired");
    let mut pm = PairingManager::new(dir.join("paired_devices.json"));
    pm.add_device(PairedDevice::with_key(paired.clone(), PIXEL.into(), [1u8; 32]));
    tauri::async_runtime::block_on(async {
        st.set_p2p_identity_key(identity.clone()).await;
        *st.pairing_manager.write().await = Some(pm);
    });
    let (names, me) = tauri::async_runtime::block_on(history_names_in(&st));
    assert_eq!(names.get(&paired).map(String::as_str), Some(PIXEL), "the paired list's name");
    assert_eq!(me, own, "this device's own peer id");
}

/// The shape on the wire: the version's fields as C5 sent them, and the two more, in one object.
#[test]
fn a_named_row_travels_as_one_object_with_two_more_fields() {
    use crate::tauri_commands::documents::NamedCommitDto;
    use crate::tauri_commands::CommitSummaryDto;
    let row = NamedCommitDto {
        commit: CommitSummaryDto {
            id: "commit:a".into(),
            message: "m".into(),
            timestamp: "t".into(),
            snapshot_title: String::new(),
            snapshot_preview: String::new(),
            could_not_read: None,
        },
        shown_message: "w".into(),
        local_time: "2026-11-19 01:00".into(),
    };
    assert_eq!(
        serde_json::to_value(&row).unwrap(),
        serde_json::json!({ "id": "commit:a", "message": "m", "timestamp": "t", "snapshot_title": "", "snapshot_preview": "",
            "could_not_read": null, "shown_message": "w", "local_time": "2026-11-19 01:00" })
    );
}
