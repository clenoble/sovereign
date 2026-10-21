//! Threads across devices — `doc/plans/THREAD-sync.md` S1–S7 (decided by
//! Céline 2026-10-18: "Same ID = timestamp décide — la dernière action
//! gagne"; "ID oui"), over the real sealed, signed row envelopes, without the
//! network. Spec ch. 03, *Thread deletion and the fallback lane*; ch. 05
//! C-WIRE (stamps, new fields with defaults, older builds).
//!
//! Two devices from `wire_common` (each its own encrypted store, document
//! service, sync service, identity key and fake clock). Thread rows move the
//! way `node.rs` moves them for the initiator of a round —
//! `compute_all_row_diffs`, then `get_rows` / `apply_rows` — and a full sync
//! runs it with each device as initiator, then the document plan (`settle`),
//! all in bounded loops. Threads are written through the one thread-write path
//! (`DocumentService::thread_create` / `thread_update`, and
//! `delete_thread_to_trash` for a delete).
//!
//! **An older build** is simulated at the envelope: the rows a device sends
//! are unsealed with the pair key, cut down to the fields the thread row has
//! today (`OLD_THREAD_FIELDS`, so whatever the new field is called it is not
//! there), re-sealed and re-signed with the sender's identity key, its counter
//! as the sender minted it. The re-signing copies the canonical envelope
//! encoding of `sync_service.rs` (`row_sig_bytes`) and checks first that the
//! copy reproduces the sender's own signature, so a forged row can never be
//! dropped for a bad signature and make a "refused" assertion pass vacuously.
//! An older build's own writes are store writes with no stamp
//! (`edb.update_thread`), as before S1.
//!
//! Index and red reasons: `crates/sovereign-p2p/TESTS-thread-sync.md`.

mod wire_common;

use base64::Engine as _;
use chrono::{DateTime, Duration, Utc};
use sovereign_crypto::account_key::AccountKey;
use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_docs::{delete_thread_to_trash, list_trash_named, purge_tick, restore_to_thread, Stamp};
use sovereign_p2p::libp2p::identity::Keypair;
use sovereign_p2p::protocol::sync::{EncryptedRow, SyncTable};
use sovereign_p2p::sync_engine::compute_all_row_diffs;
use wire_common::*;

const SEED_A: u8 = 41;
const SEED_B: u8 = 42;

// ----------------------------------------------------------------- harness --

async fn two(tag: &str) -> (Device, Device) {
    let a = device(&format!("{tag}-a"), SEED_A).await;
    let b = device(&format!("{tag}-b"), SEED_B).await;
    pair(&a, &b);
    (a, b)
}

/// One exchange of thread rows with `a` as the initiator, as `node.rs` runs it.
async fn thread_rows(a: &Device, b: &Device) {
    let (ma, mb) = (manifest(a).await, manifest(b).await);
    let Some(diff) = compute_all_row_diffs(&ma, &mb).remove(&SyncTable::Thread) else { return };
    if !diff.need_from_remote.is_empty() {
        let rows = b.sync.get_rows(SyncTable::Thread, &diff.need_from_remote, &a.peer).await.expect("rows from b");
        a.sync.apply_rows(SyncTable::Thread, rows, &b.peer).await.expect("rows on a");
    }
    if !diff.push_to_remote.is_empty() {
        let rows = a.sync.get_rows(SyncTable::Thread, &diff.push_to_remote, &b.peer).await.expect("rows from a");
        b.sync.apply_rows(SyncTable::Thread, rows, &a.peer).await.expect("rows on b");
    }
}

/// Thread rows, each device initiating in turn (bounded).
async fn sync_threads(a: &Device, b: &Device) {
    for _ in 0..3 {
        thread_rows(a, b).await;
        thread_rows(b, a).await;
    }
}

/// Thread rows and documents until settled (bounded).
async fn full_sync(a: &Device, b: &Device) {
    for _ in 0..3 {
        sync_threads(a, b).await;
        settle(a, b, 4).await;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadState {
    name: String,
    deleted: bool,
}

fn live(name: &str) -> Option<ThreadState> {
    Some(ThreadState { name: name.into(), deleted: false })
}

fn deleted(name: &str) -> Option<ThreadState> {
    Some(ThreadState { name: name.into(), deleted: true })
}

/// The thread record `id` on `d`, live or deleted, as the store gives it.
async fn state(d: &Device, id: &str) -> Option<ThreadState> {
    d.edb
        .list_threads_including_deleted()
        .await
        .unwrap()
        .into_iter()
        .find(|t| t.id_string().as_deref() == Some(id))
        .map(|t| ThreadState { name: t.name, deleted: t.deleted_at.is_some() })
}

async fn stamp_of(d: &Device, id: &str) -> Option<Stamp> {
    d.docs.thread_stamp(id).await.expect("the thread's stamp")
}

async fn active_ids(d: &Device) -> Vec<String> {
    d.edb.list_threads().await.unwrap().into_iter().filter(|t| t.deleted_at.is_none()).filter_map(|t| t.id_string()).collect()
}

async fn create_in(d: &Device, thread_id: &str, title: &str, text: &str) -> String {
    let new = sovereign_docs::NewDocument {
        title: title.into(),
        thread_id: thread_id.into(),
        content: sovereign_docs::NewContent::Text(text.into()),
        trust: sovereign_docs::Trust::Owned,
        provenance: sovereign_docs::Provenance::Written,
        origin: sovereign_docs::Origin::Editor { session: sovereign_docs::EditSession(1) },
        created_at: None,
        modified_at: None,
    };
    d.docs.create(new).await.unwrap().id
}

async fn trashed(d: &Device, id: &str) -> bool {
    d.docs.read(id).await.expect("present").trashed_at.is_some()
}

async fn orphan_ids(d: &Device) -> Vec<String> {
    let mut v: Vec<String> =
        d.docs.orphans_of_deleted_threads().await.expect("the condition").into_iter().map(|o| o.id).collect();
    v.sort();
    v
}

/// Mints a counter for thread `id` on `d` (what a send does when the row
/// changed since its last stamp), without delivering the row anywhere.
async fn mint_counter(d: &Device, id: &str, to: &Device) {
    d.sync.get_rows(SyncTable::Thread, &[id.to_string()], &to.peer).await.expect("rows");
}

/// "Shared", created on `a` through the path and synced to `b`.
async fn shared(tag: &str) -> (Device, Device, String) {
    let (a, b) = two(tag).await;
    let id = a.docs.thread_create("Shared", "").await.expect("created on a");
    full_sync(&a, &b).await;
    assert_eq!(state(&b, &id).await, live("Shared"), "setup: the thread reached b");
    assert_eq!(stamp_of(&b, &id).await, stamp_of(&a, &id).await, "setup: b keeps a's stamp");
    (a, b, id)
}

// ------------------------------------------------------- an older build --

/// The fields of a thread row as an older build serialises it (the `Thread`
/// struct before THREAD-sync).
const OLD_THREAD_FIELDS: [&str; 9] = [
    "id",
    "name",
    "description",
    "created_at",
    "modified_at",
    "deleted_at",
    "name_nonce",
    "description_nonce",
    "name_token_hashes",
];

fn pair_key(x: &Device, y: &Device) -> [u8; 32] {
    AccountKey::from_bytes(ACCOUNT_SEED).derive_pair_key(&x.peer.to_string(), &y.peer.to_string())
}

/// Copy of `sync_service.rs`'s canonical row envelope encoding, for a thread
/// row (checked against a real signature in `as_sent_by_an_older_build`).
fn row_sig_bytes(row: &EncryptedRow) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"sovereign-row-sig:v1");
    for field in [
        SyncTable::Thread.as_str(),
        row.id.as_str(),
        row.ciphertext.as_str(),
        row.nonce.as_str(),
        row.modified_at.as_str(),
        row.deleted_at.as_deref().unwrap_or(""),
        row.version_device.as_str(),
    ] {
        out.extend_from_slice(&(field.len() as u32).to_le_bytes());
        out.extend_from_slice(field.as_bytes());
    }
    out.push(row.deleted_at.is_some() as u8);
    out.extend_from_slice(&row.version_counter.to_le_bytes());
    out
}

fn unseal(row: &EncryptedRow, key: &[u8; 32]) -> serde_json::Value {
    let b64 = base64::engine::general_purpose::STANDARD;
    let ct = b64.decode(&row.ciphertext).expect("ciphertext");
    let nonce: [u8; 24] = b64.decode(&row.nonce).expect("nonce").try_into().expect("24-byte nonce");
    let json = sovereign_crypto::aead::decrypt(&ct, &nonce, key).expect("unseals under the pair key");
    serde_json::from_slice(&json).expect("a thread row is JSON")
}

/// `row`, as the same device on an older build would have sent it: the body
/// cut to [`OLD_THREAD_FIELDS`], re-sealed, re-signed by the sender (`seed`).
/// With `modified_at`, the row's modification time (in the body and on the
/// envelope, as an older build sends both) is set to it.
fn as_sent_by_an_older_build(
    row: EncryptedRow,
    key: &[u8; 32],
    seed: u8,
    modified_at: Option<DateTime<Utc>>,
) -> EncryptedRow {
    let b64 = base64::engine::general_purpose::STANDARD;
    let kp = Keypair::ed25519_from_bytes([seed; 32]).unwrap();
    let original = b64.decode(&row.signature).expect("signed");
    assert!(
        kp.public().verify(&row_sig_bytes(&row), &original),
        "harness: the copied envelope encoding no longer matches sync_service.rs"
    );
    let body = unseal(&row, key);
    let old: serde_json::Map<String, serde_json::Value> = body
        .as_object()
        .expect("an object")
        .iter()
        .filter(|(k, _)| OLD_THREAD_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let mut old = old;
    if let Some(t) = modified_at {
        old.insert("modified_at".into(), serde_json::Value::String(t.to_rfc3339()));
    }
    let (ct, nonce) =
        sovereign_crypto::aead::encrypt(&serde_json::to_vec(&serde_json::Value::Object(old)).unwrap(), key).unwrap();
    let mut out = row;
    out.ciphertext = b64.encode(ct);
    out.nonce = b64.encode(nonce);
    if let Some(t) = modified_at {
        out.modified_at = t.to_rfc3339();
    }
    out.signature = b64.encode(kp.sign(&row_sig_bytes(&out)).unwrap());
    out
}

/// Sends thread `id` from `from` (an older build, identity `seed`) to `to`;
/// returns how many rows `to` wrote.
async fn old_build_sends(from: &Device, seed: u8, to: &Device, id: &str) -> u32 {
    old_build_sends_at(from, seed, to, id, None).await
}

/// [`old_build_sends`], with the row's modification time set to `modified_at`.
async fn old_build_sends_at(from: &Device, seed: u8, to: &Device, id: &str, modified_at: Option<DateTime<Utc>>) -> u32 {
    let key = pair_key(from, to);
    let rows = from.sync.get_rows(SyncTable::Thread, &[id.to_string()], &to.peer).await.expect("rows");
    assert_eq!(rows.len(), 1, "harness: the row exists on the sender");
    let rows = rows.into_iter().map(|r| as_sent_by_an_older_build(r, &key, seed, modified_at)).collect();
    to.sync.apply_rows(SyncTable::Thread, rows, &from.peer).await.expect("apply").written
}

/// A pause on the system clock, so two store writes get distinct
/// `modified_at` (an older build's thread time is the system clock's).
async fn tick_system_clock() {
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
}

/// A thread written by an older build on `b` ("Legacy", no stamp), received
/// by `a` from it.
async fn legacy_on_both(a: &Device, b: &Device) -> String {
    let id = b.edb.create_thread(Thread::new("Legacy".into(), String::new())).await.unwrap().id_string().unwrap();
    assert_eq!(old_build_sends(b, SEED_B, a, &id).await, 1, "setup: a receives the thread");
    assert_eq!(state(a, &id).await, live("Legacy"));
    id
}

// ---------------------------------------------------------------------- S1 --

/// S1: "the row carries it on the wire (a field added with a default, C-WIRE's
/// rule for new fields)" — inside the sealed, signed envelope (C-WIRE 16), and
/// the receiver keeps the sender's stamp.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s1_the_row_carries_the_stamp_inside_the_sealed_envelope() {
    let (a, b) = two("s1-wire").await;
    let id = a.docs.thread_create("On the wire", "").await.expect("created");
    let st = stamp_of(&a, &id).await.expect("stamped");
    let rows = a.sync.get_rows(SyncTable::Thread, &[id.clone()], &b.peer).await.expect("rows");
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let body = unseal(row, &pair_key(&a, &b)).to_string();
    let wall = st.wall_ms.to_string();
    assert!(body.contains(&wall), "the sealed body carries the stamp's time ({wall}): {body}");
    assert!(body.contains(&st.device), "the sealed body carries the stamp's device");
    for (what, clear) in [
        ("id", &row.id),
        ("modified_at", &row.modified_at),
        ("version_device", &row.version_device),
        ("nonce", &row.nonce),
        ("signature", &row.signature),
    ] {
        assert!(!clear.contains(&wall), "the stamp is in the clear field {what}");
    }
    assert!(!row.deleted_at.as_deref().unwrap_or("").contains(&wall), "the stamp is in the clear deleted_at");

    let report = b.sync.apply_rows(SyncTable::Thread, rows, &a.peer).await.expect("applied");
    assert_eq!(report.written, 1);
    assert_eq!(stamp_of(&b, &id).await, Some(st), "the receiver keeps the sender's stamp");
}

/// S1: "from the same device clock as the documents'" — a thread stamp
/// received from a device whose clock runs ahead moves this device's clock, so
/// the next document write here is stamped after it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s1_a_received_thread_stamp_advances_this_devices_clock() {
    let (a, b) = two("s1-observe").await;
    b.clock.advance(Duration::minutes(3));
    let id = b.docs.thread_create("From ahead", "").await.expect("created on b");
    sync_threads(&a, &b).await;
    let ts = stamp_of(&a, &id).await.expect("received with its stamp");
    let doc = create(&a, "Written after", "x").await;
    let ds = a.docs.stored_state(&doc).await.unwrap().meta.stamp.expect("stamped");
    assert!(ds > ts, "a document written after receiving the thread is stamped after it: {ds:?} vs {ts:?}");
}

// ---------------------------------------------------------------------- S2 --

/// S2, rename vs delete: the delete was the later action.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_a_delete_made_after_a_rename_on_the_other_device_wins() {
    let (a, b, id) = shared("s2-ren-del").await;
    a.clock.advance(Duration::minutes(1));
    a.docs.thread_update(&id, Some("Renamed on the laptop"), None).await.expect("renamed on a");
    b.clock.advance(Duration::minutes(2));
    delete_thread_to_trash(&b.docs, b.edb.as_ref(), &id).await.expect("deleted on b");
    let winner = stamp_of(&b, &id).await;
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, deleted("Shared"), "{}: deleted, the rename discarded", d.name);
        assert_eq!(stamp_of(d, &id).await, winner, "{}: holds the delete's stamp", d.name);
        assert!(!active_ids(d).await.contains(&id), "{}: not listed", d.name);
    }
}

/// S2, delete vs rename: the rename was the later action.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_a_rename_made_after_a_delete_on_the_other_device_wins() {
    let (a, b, id) = shared("s2-del-ren").await;
    a.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&a.docs, a.edb.as_ref(), &id).await.expect("deleted on a");
    b.clock.advance(Duration::minutes(2));
    b.docs.thread_update(&id, Some("Renamed on the phone"), None).await.expect("renamed on b");
    let winner = stamp_of(&b, &id).await;
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live("Renamed on the phone"), "{}: live again, under the newer name", d.name);
        assert_eq!(stamp_of(d, &id).await, winner, "{}", d.name);
        assert!(active_ids(d).await.contains(&id), "{}: listed again", d.name);
    }
}

/// S2, rename vs rename, in both orders (the later device is once `a`, once
/// `b`): the device never decides.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_rename_vs_rename_the_later_wins_whichever_device_made_it() {
    for later_is_b in [true, false] {
        let (a, b, id) = shared(&format!("s2-ren-ren-{later_is_b}")).await;
        let (first, second) = if later_is_b { (&a, &b) } else { (&b, &a) };
        first.clock.advance(Duration::minutes(1));
        first.docs.thread_update(&id, Some("Earlier name"), None).await.expect("first rename");
        second.clock.advance(Duration::minutes(2));
        second.docs.thread_update(&id, Some("Later name"), None).await.expect("second rename");
        full_sync(&a, &b).await;
        for d in [&a, &b] {
            assert_eq!(state(d, &id).await, live("Later name"), "later on b: {later_is_b}; on {}", d.name);
            assert_eq!(stamp_of(d, &id).await, stamp_of(second, &id).await, "{}", d.name);
        }
    }
}

/// S2: "the higher stamp wins whatever the actions are" — not the row
/// counter: the earlier device sends its row twice in between (two counters),
/// the later device once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_the_later_action_wins_whatever_the_row_counters_say() {
    let (a, b, id) = shared("s2-counters").await;
    a.clock.advance(Duration::minutes(1));
    a.docs.thread_update(&id, Some("Laptop, first"), None).await.unwrap();
    mint_counter(&a, &id, &b).await;
    a.clock.advance(Duration::seconds(10));
    a.docs.thread_update(&id, Some("Laptop, second"), None).await.unwrap();
    mint_counter(&a, &id, &b).await;
    b.clock.advance(Duration::minutes(5) - Duration::seconds(30));
    b.docs.thread_update(&id, Some("Phone, later"), None).await.unwrap();
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live("Phone, later"), "{}", d.name);
    }
}

/// S2: order is by stamp — not by which change happened last on the system
/// clock, nor by which row arrives first. `a`'s clock runs 4 minutes ahead;
/// `a` renames first in real time, `b` afterwards, unaware of it: `a`'s stamp
/// is the higher, and both devices end with `a`'s name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_both_devices_converge_on_the_higher_stamp_whatever_the_system_clock_order() {
    let (a, b, id) = shared("s2-skew").await;
    a.clock.advance(Duration::minutes(4));
    a.docs.thread_update(&id, Some("Higher stamp"), None).await.unwrap();
    b.clock.advance(Duration::minutes(1));
    b.docs.thread_update(&id, Some("Lower stamp"), None).await.unwrap();
    assert!(stamp_of(&a, &id).await > stamp_of(&b, &id).await, "setup: a's stamp is the higher");
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live("Higher stamp"), "{}", d.name);
        assert_eq!(stamp_of(d, &id).await, stamp_of(&a, &id).await, "{}", d.name);
    }
}

/// S1 + S2: a device that has received a stamp issues later ones even when its
/// own clock is behind — so its next action is the later one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_an_action_made_after_receiving_a_stamp_wins_even_with_a_clock_behind() {
    let (a, b, id) = shared("s2-after").await;
    a.clock.advance(Duration::minutes(3));
    a.docs.thread_update(&id, Some("From the laptop"), None).await.unwrap();
    full_sync(&a, &b).await;
    assert_eq!(state(&b, &id).await, live("From the laptop"));
    b.docs.thread_update(&id, Some("Then the phone"), None).await.unwrap();
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live("Then the phone"), "{}", d.name);
    }
}

/// S2: "Equal stamps cannot happen across devices (the device is part of the
/// stamp)" — two renames at the same instant on two devices have different
/// stamps, and both devices converge on the one with the greater stamp.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s2_two_renames_at_the_same_instant_converge_on_one() {
    let (a, b, id) = shared("s2-same").await;
    let t = a.clock.get().max(b.clock.get()) + Duration::minutes(1);
    a.clock.advance(t - a.clock.get());
    b.clock.advance(t - b.clock.get());
    a.docs.thread_update(&id, Some("Name from a"), None).await.unwrap();
    b.docs.thread_update(&id, Some("Name from b"), None).await.unwrap();
    let (sa, sb) = (stamp_of(&a, &id).await.unwrap(), stamp_of(&b, &id).await.unwrap());
    assert_ne!(sa, sb, "the device is part of the stamp");
    let expected = if sa > sb { "Name from a" } else { "Name from b" };
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live(expected), "{}", d.name);
        assert_eq!(stamp_of(d, &id).await, Some(sa.clone().max(sb.clone())), "{}", d.name);
    }
}

// ------------------------------------------------------------------ S3, S4 --

/// `a` (laptop) and `b` (phone) share "Shared" holding `doc` (synced); then,
/// apart, `b` writes `offline` in it.
struct Apart {
    a: Device,
    b: Device,
    lane: String,
    doc: String,
    offline: String,
}

async fn apart(tag: &str) -> Apart {
    let (a, b) = two(tag).await;
    let lane = a.docs.thread_create("Shared", "").await.expect("created");
    let doc = create_in(&a, &lane, "Shared doc", "both have it").await;
    full_sync(&a, &b).await;
    assert!(!trashed(&b, &doc).await, "setup: the document reached b");
    assert_eq!(state(&b, &lane).await, live("Shared"), "setup: the thread reached b");
    let offline = create_in(&b, &lane, "Written offline", "phone text").await;
    Apart { a, b, lane, doc, offline }
}

/// S3: "The thread is live again, under the newer name. Documents the
/// deletion sent to the trash stay in the trash (restorable into it, now shown
/// under its live name, not '- deleted'); live documents of it stop asking …
/// Nothing is moved automatically."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_a_rename_that_wins_over_a_delete_brings_the_thread_back_and_leaves_the_trash_as_it_is() {
    let w = apart("s3").await;
    w.a.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&w.a.docs, w.a.edb.as_ref(), &w.lane).await.expect("deleted on the laptop");
    assert!(trashed(&w.a, &w.doc).await);
    w.b.clock.advance(Duration::minutes(2));
    w.b.docs.thread_update(&w.lane, Some("Renamed on the phone"), None).await.expect("renamed on the phone");
    full_sync(&w.a, &w.b).await;

    for d in [&w.a, &w.b] {
        assert_eq!(state(d, &w.lane).await, live("Renamed on the phone"), "{}: live, newer name", d.name);
        assert!(trashed(d, &w.doc).await, "{}: the trashed document stays in the trash", d.name);
        assert_eq!(d.docs.read(&w.doc).await.unwrap().thread_id, w.lane, "{}: still of its thread", d.name);
        assert!(!trashed(d, &w.offline).await, "{}: the live document stays live", d.name);
        assert_eq!(d.docs.read(&w.offline).await.unwrap().thread_id, w.lane, "{}: nothing moved", d.name);
        assert!(orphan_ids(d).await.is_empty(), "{}: no document asks any more", d.name);
        let listing = list_trash_named(&d.docs, d.edb.as_ref(), d.clock.get()).await.unwrap();
        let line = listing.iter().find(|l| l.entry.id == w.doc).expect("in the trash listing");
        assert_eq!(line.thread_name.as_deref(), Some("Renamed on the phone"), "{}: under its live name", d.name);
        assert_eq!(line.deleted_thread_name, None, "{}: not '- deleted'", d.name);
    }
    restore_to_thread(&w.a.docs, w.a.edb.as_ref(), &w.doc, None).await.expect("restorable into its thread");
    assert!(!trashed(&w.a, &w.doc).await);
    assert_eq!(w.a.docs.read(&w.doc).await.unwrap().thread_id, w.lane);
}

/// S4: "The thread stays deleted; the rename is discarded; this device's live
/// documents of that thread ask (S5-12)."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s4_a_delete_that_wins_over_a_rename_discards_it_and_the_renaming_devices_documents_ask() {
    let w = apart("s4").await;
    w.a.clock.advance(Duration::minutes(1));
    w.a.docs.thread_update(&w.lane, Some("Renamed on the laptop"), None).await.expect("renamed on the laptop");
    let written_here = create_in(&w.a, &w.lane, "Laptop note", "laptop text").await;
    w.b.clock.advance(Duration::minutes(2));
    delete_thread_to_trash(&w.b.docs, w.b.edb.as_ref(), &w.lane).await.expect("deleted on the phone");
    full_sync(&w.a, &w.b).await;

    for d in [&w.a, &w.b] {
        assert_eq!(state(d, &w.lane).await, deleted("Shared"), "{}: deleted, the rename discarded", d.name);
        assert!(!active_ids(d).await.contains(&w.lane), "{}", d.name);
    }
    assert!(!trashed(&w.a, &written_here).await, "the laptop's own document is not trashed by the sync");
    assert_eq!(orphan_ids(&w.a).await, vec![written_here.clone()], "the laptop's live document of the thread asks");
    let o = w.a.docs.orphans_of_deleted_threads().await.unwrap().remove(0);
    assert_eq!(o.deleted_thread_name.as_deref(), Some("Shared"), "under the name the delete kept, not the discarded one");
}

// ---------------------------------------------------------------------- S5 --
//
// S5 as revised (settled after the test author's report, 2026-10-18): "A row
// without a stamp is compared as if stamped at its `modified_at` (physical
// time, logical counter 0, its device), within the 5-minute future bound; so
// an older device's later rename can still win." An older build's thread
// time is the system clock's (`edb.update_thread` sets `modified_at` to now);
// the devices' fake clocks start at the real time and stand still unless a
// test moves them.

/// S5: between two rows without a stamp, the later `modified_at` wins, not
/// the counter (the earlier device sent twice, the later once); a replayed
/// earlier row does not apply; and thread rows still flow with an older
/// build's manifest (the old-build gate holds back documents only, C-WIRE 12).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s5_between_rows_without_stamps_the_later_modification_wins_not_the_counter() {
    let (a, b) = two("s5-unstamped").await;
    let id = legacy_on_both(&a, &b).await;
    let replay = {
        let rows = b.sync.get_rows(SyncTable::Thread, &[id.clone()], &a.peer).await.unwrap();
        as_sent_by_an_older_build(rows.into_iter().next().unwrap(), &pair_key(&b, &a), SEED_B, None)
    };
    // The laptop's older build renames twice, sending each time (two counters).
    tick_system_clock().await;
    a.edb.update_thread(&id, Some("Laptop, older build, first"), None).await.unwrap();
    mint_counter(&a, &id, &b).await;
    tick_system_clock().await;
    a.edb.update_thread(&id, Some("Laptop, older build, second"), None).await.unwrap();
    mint_counter(&a, &id, &b).await;
    // Then the phone's older build renames once: later, with a lower counter.
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("Phone, older build, later"), None).await.unwrap();

    let old = as_old_build(manifest(&b).await);
    assert!(
        compute_all_row_diffs(&manifest(&a).await, &old).contains_key(&SyncTable::Thread),
        "thread rows are not held back from an older build"
    );
    assert_eq!(old_build_sends(&b, SEED_B, &a, &id).await, 1, "the later modification applies");
    assert_eq!(state(&a, &id).await, live("Phone, older build, later"));
    let r = a.sync.apply_rows(SyncTable::Thread, vec![replay], &b.peer).await.unwrap();
    assert_eq!(r.written, 0, "a replayed earlier row does not apply");
    assert_eq!(state(&a, &id).await, live("Phone, older build, later"));
}

/// S5: "an older device's later rename can still win" — over a stamped
/// rename made before it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s5_an_older_devices_later_rename_wins_over_an_earlier_stamped_one() {
    let (a, b, id) = shared("s5-older-later").await;
    a.docs.thread_update(&id, Some("Stamped on the laptop"), None).await.unwrap();
    let stamped = stamp_of(&a, &id).await.expect("stamped");
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("Older build, later"), None).await.unwrap();
    let t = b.edb.get_thread(&id).await.unwrap().modified_at;
    assert!(t.timestamp_millis() > stamped.wall_ms, "setup: the older build's rename is the later one");
    assert_eq!(old_build_sends(&b, SEED_B, &a, &id).await, 1, "the later rename applies");
    assert_eq!(state(&a, &id).await, live("Older build, later"));
}

/// S5: a stamped rename made after an older device's rename wins, whatever the
/// counters (the older device sent twice), and both devices converge on it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s5_a_stamped_rename_made_after_an_older_devices_rename_wins_whatever_the_counters() {
    let (a, b) = two("s5-stamped-later").await;
    let id = legacy_on_both(&a, &b).await;
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("Older build, first"), None).await.unwrap();
    mint_counter(&b, &id, &a).await;
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("Older build, second"), None).await.unwrap();
    // Two minutes later on the laptop's clock, an updated rename.
    a.clock.advance(Duration::minutes(2));
    a.docs.thread_update(&id, Some("Stamped later on the laptop"), None).await.unwrap();
    assert!(
        stamp_of(&a, &id).await.expect("stamped").wall_ms > b.edb.get_thread(&id).await.unwrap().modified_at.timestamp_millis(),
        "setup: the stamped rename is the later one"
    );
    assert_eq!(old_build_sends(&b, SEED_B, &a, &id).await, 0, "the older build's earlier rename is refused");
    assert_eq!(state(&a, &id).await, live("Stamped later on the laptop"));
    let rows = a.sync.get_rows(SyncTable::Thread, &[id.clone()], &b.peer).await.unwrap();
    assert_eq!(b.sync.apply_rows(SyncTable::Thread, rows, &a.peer).await.unwrap().written, 1);
    assert_eq!(state(&b, &id).await, live("Stamped later on the laptop"), "b converges on it");
}

/// S5: "within the 5-minute future bound" — a row without a stamp whose
/// `modified_at` is more than 5 minutes ahead of this device's clock is
/// refused; 2 minutes ahead applies.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s5_a_row_without_a_stamp_more_than_5_minutes_ahead_is_refused() {
    let (a, b) = two("s5-bound").await;
    let id = legacy_on_both(&a, &b).await;
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("From a clock ahead"), None).await.unwrap();
    let far = a.clock.get() + Duration::minutes(10);
    assert_eq!(old_build_sends_at(&b, SEED_B, &a, &id, Some(far)).await, 0, "10 minutes ahead: refused");
    assert_eq!(state(&a, &id).await, live("Legacy"));
    let near = a.clock.get() + Duration::minutes(2);
    assert_eq!(old_build_sends_at(&b, SEED_B, &a, &id, Some(near)).await, 1, "2 minutes ahead: applies");
    assert_eq!(state(&a, &id).await, live("From a clock ahead"));
}

// ---------------------------------------------------------------------- S6 --

/// "Old project" (with a description) created on `a` and synced to `b`; then
/// deleted on `a` with no document — the deletion **not** synced — and both
/// clocks moved 31 days on; `a` runs its purge tick.
async fn shrunk_on_a(tag: &str, before_shrink: impl AsyncFnOnce(&Device, &Device, &str)) -> (Device, Device, String, Stamp) {
    shrunk_on_a_deleted_after(tag, Duration::minutes(1), before_shrink).await
}

/// [`shrunk_on_a`], the deletion made `delete_after` past `a`'s clock at the
/// start (zero: at the real time the devices were made, before any later
/// system-clock write).
async fn shrunk_on_a_deleted_after(
    tag: &str,
    delete_after: Duration,
    before_shrink: impl AsyncFnOnce(&Device, &Device, &str),
) -> (Device, Device, String, Stamp) {
    let (a, b) = two(tag).await;
    let id = a.docs.thread_create("Old project", "notes about the old project").await.unwrap();
    full_sync(&a, &b).await;
    assert_eq!(state(&b, &id).await, live("Old project"), "setup");
    a.clock.advance(delete_after);
    delete_thread_to_trash(&a.docs, a.edb.as_ref(), &id).await.unwrap();
    let deletion = stamp_of(&a, &id).await.expect("the deletion is stamped");
    before_shrink(&a, &b, &id).await;
    a.clock.advance(Duration::days(31));
    b.clock.advance(Duration::days(31));
    purge_tick(&a.docs).await.unwrap();
    purge_tick(&a.docs).await.unwrap();
    assert_marker(&a, &id, &deletion).await;
    (a, b, id, deletion)
}

/// S6/S7: on `d`, thread `id` is a marker — the id and the deletion stamp
/// only, the name and description gone from the store, shown nowhere — and it
/// stays in the sync manifest.
async fn assert_marker(d: &Device, id: &str, deletion: &Stamp) {
    assert_eq!(stamp_of(d, id).await.as_ref(), Some(deletion), "{}: the marker keeps the deletion stamp", d.name);
    if let Some(s) = state(d, id).await {
        assert!(s.deleted && s.name.is_empty(), "{}: a marker has no name: {s:?}", d.name);
    }
    if let Ok(raw) = d.raw.get_thread(id).await {
        let text = serde_json::to_string(&raw).unwrap();
        assert!(!text.contains("Old project") && !text.contains("notes about"), "{}: plaintext on disk", d.name);
        assert!(raw.name_token_hashes.is_empty(), "{}: the name's blind index is gone", d.name);
    }
    assert!(d.edb.find_thread_by_name("Old project").await.unwrap().is_none(), "{}: not found by name", d.name);
    assert!(!active_ids(d).await.contains(&id.to_string()), "{}: in no list", d.name);
    let m = manifest(d).await;
    let entry = m.threads.iter().find(|t| t.thread_id == id);
    assert!(entry.is_some_and(|t| t.deleted_at.is_some()), "{}: the marker stays in the sync manifest", d.name);
}

/// S6: "a received row for that id with an **older** stamp is refused (no
/// resurrection)" — `b` still holds the thread live, from before the deletion.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s6_a_marker_refuses_an_older_live_row_and_the_name_does_not_come_back() {
    let (a, b, id, deletion) = shrunk_on_a("s6-older", async |_, _, _| {}).await;
    assert_eq!(state(&b, &id).await, live("Old project"), "setup: b never got the deletion");
    full_sync(&a, &b).await;
    assert_marker(&a, &id, &deletion).await;
    assert!(state(&b, &id).await.is_none_or(|s| s.deleted), "the deletion (newer) reaches b");
    assert!(!active_ids(&b).await.contains(&id));
}

/// S6 with S2: a device holding the deletion with the thread's name (the same
/// deletion stamp) does not put the name back on the device that shrank it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s6_a_tombstone_with_the_same_deletion_stamp_brings_no_name_back() {
    let (a, b, id, deletion) = shrunk_on_a("s6-same", async |a, b, id| {
        sync_threads(a, b).await;
        assert_eq!(state(b, id).await, deleted("Old project"), "setup: b holds the named tombstone");
    })
    .await;
    assert_eq!(stamp_of(&b, &id).await.as_ref(), Some(&deletion));
    full_sync(&a, &b).await;
    assert_marker(&a, &id, &deletion).await;
}

/// S6: "a **newer** one (a rename made after the deletion, by S2) wins and
/// re-creates the thread with the received content."
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s6_a_rename_made_after_the_deletion_recreates_the_thread() {
    let (a, b, id, _) = shrunk_on_a("s6-newer", async |_, b, id| {
        b.clock.advance(Duration::minutes(2));
        b.docs.thread_update(id, Some("Revived on the phone"), Some("phone notes")).await.unwrap();
    })
    .await;
    let renamed = stamp_of(&b, &id).await;
    full_sync(&a, &b).await;
    for d in [&a, &b] {
        assert_eq!(state(d, &id).await, live("Revived on the phone"), "{}", d.name);
        assert_eq!(stamp_of(d, &id).await, renamed, "{}", d.name);
    }
    let t = a.edb.get_thread(&id).await.unwrap();
    assert_eq!(t.description, "phone notes", "with the received content");
}

/// S6 with S5 (revised): a row from an older build whose modification time is
/// earlier than the deletion stamp never brings a marker back, whatever its
/// counter.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s6_an_older_builds_row_earlier_than_the_deletion_never_revives_a_marker() {
    let (a, b, id, deletion) = shrunk_on_a("s6-old-build-earlier", async |_, _, _| {}).await;
    b.edb.update_thread(&id, Some("Older build edit"), None).await.unwrap();
    mint_counter(&b, &id, &a).await;
    tick_system_clock().await;
    b.edb.update_thread(&id, Some("Older build edit, again"), None).await.unwrap();
    let t = b.edb.get_thread(&id).await.unwrap().modified_at;
    assert!(t.timestamp_millis() < deletion.wall_ms, "setup: the older build's edit is earlier than the deletion");
    assert_eq!(old_build_sends(&b, SEED_B, &a, &id).await, 0, "refused");
    assert_marker(&a, &id, &deletion).await;
}

/// S6 with S5 (revised): an older build's rename made after the deletion is
/// the newer action, and re-creates the thread from a marker.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s6_an_older_builds_rename_made_after_the_deletion_recreates_the_thread() {
    let (a, b, id, deletion) = shrunk_on_a_deleted_after("s6-old-build-later", Duration::zero(), async |_, b, id| {
        tick_system_clock().await;
        b.edb.update_thread(id, Some("Older build, after the deletion"), None).await.unwrap();
    })
    .await;
    let t = b.edb.get_thread(&id).await.unwrap().modified_at;
    assert!(t.timestamp_millis() > deletion.wall_ms, "setup: the older build's rename is later than the deletion");
    assert_eq!(old_build_sends(&b, SEED_B, &a, &id).await, 1, "the later rename applies");
    assert_eq!(state(&a, &id).await, live("Older build, after the deletion"));
    assert!(active_ids(&a).await.contains(&id));
}

// ---------------------------------------------------------------------- S8 --

/// S8: "The 5-minute bound of the documents' stamps (C-DOC G25) applies to
/// thread stamps." A rename and a new thread stamped 10 minutes ahead of this
/// device's clock are refused; once this clock is within 5 minutes of them,
/// they apply.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s8_a_thread_stamp_more_than_5_minutes_ahead_is_refused() {
    let (a, b, id) = shared("s8-bound").await;
    b.clock.advance(Duration::minutes(10));
    b.docs.thread_update(&id, Some("From 10 minutes ahead"), None).await.unwrap();
    let new = b.docs.thread_create("New, from 10 minutes ahead", "").await.unwrap();
    sync_threads(&a, &b).await;
    assert_eq!(state(&a, &id).await, live("Shared"), "the rename is refused");
    assert_eq!(state(&a, &new).await, None, "the new thread is refused");

    a.clock.advance(Duration::minutes(6));
    sync_threads(&a, &b).await;
    assert_eq!(state(&a, &id).await, live("From 10 minutes ahead"), "4 minutes ahead now: applies");
    assert_eq!(state(&a, &new).await, live("New, from 10 minutes ahead"));
}

// ---------------------------------------------------------------------- S9 --

/// S9: "The 30 days run from the deletion's stamp (its physical time), wherever
/// the deletion was made." `b` receives `a`'s deletion 20 days after it was
/// made: it keeps the name at 29 days from the deletion, and shrinks it at 31
/// days from the deletion (11 days after it arrived).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s9_a_deletion_received_late_shrinks_30_days_after_its_stamp() {
    let (a, b) = two("s9-late").await;
    let id = a.docs.thread_create("Old project", "notes about the old project").await.unwrap();
    full_sync(&a, &b).await;
    a.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&a.docs, a.edb.as_ref(), &id).await.unwrap();
    let deletion = stamp_of(&a, &id).await.expect("stamped");
    a.clock.advance(Duration::days(20));
    b.clock.advance(Duration::days(20));
    sync_threads(&a, &b).await;
    assert_eq!(state(&b, &id).await, deleted("Old project"), "the deletion arrives on day 20");
    assert_eq!(stamp_of(&b, &id).await.as_ref(), Some(&deletion));

    b.clock.advance(Duration::days(9));
    purge_tick(&b.docs).await.unwrap();
    assert_eq!(state(&b, &id).await, deleted("Old project"), "29 days from the deletion: kept with its name");

    b.clock.advance(Duration::days(2));
    purge_tick(&b.docs).await.unwrap();
    purge_tick(&b.docs).await.unwrap();
    assert_marker(&b, &id, &deletion).await;
}

// --------------------------------------------------------------------- S10 --

/// S10: "A marker and a named tombstone are the same row for the sync: deleted
/// threads compare on id, deleted state and stamp only (name and description
/// excluded from their hash)" — a marker here and the not-yet-shrunk tombstone
/// there hash equal, and the row diff has nothing to move.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s10_a_marker_and_a_named_tombstone_with_the_same_stamp_are_in_sync() {
    let (a, b, id, deletion) = shrunk_on_a("s10-same-row", async |a, b, id| {
        sync_threads(a, b).await;
        assert_eq!(state(b, id).await, deleted("Old project"), "setup: b holds the named tombstone");
    })
    .await;
    assert_eq!(stamp_of(&b, &id).await.as_ref(), Some(&deletion));
    let (ma, mb) = (manifest(&a).await, manifest(&b).await);
    let entry = |m: &sovereign_p2p::protocol::manifest::SyncManifest| {
        m.threads.iter().find(|t| t.thread_id == id).cloned().expect("listed in the manifest")
    };
    assert_eq!(entry(&ma).content_hash, entry(&mb).content_hash, "the same row for the sync");
    let diff = compute_all_row_diffs(&ma, &mb).remove(&SyncTable::Thread);
    assert!(
        diff.is_none_or(|d| !d.need_from_remote.contains(&id) && !d.push_to_remote.contains(&id)),
        "nothing to send for it"
    );
}

/// S10: a deleted thread's hash leaves out its name only — a live thread and
/// a deleted one, or two deletions with different stamps, still differ.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s10_deleted_state_and_stamp_still_tell_rows_apart() {
    let (a, b, id) = shared("s10-differ").await;
    a.clock.advance(Duration::minutes(1));
    delete_thread_to_trash(&a.docs, a.edb.as_ref(), &id).await.unwrap();
    let hash = |m: &sovereign_p2p::protocol::manifest::SyncManifest| {
        m.threads.iter().find(|t| t.thread_id == id).map(|t| t.content_hash.clone()).expect("listed")
    };
    assert_ne!(hash(&manifest(&a).await), hash(&manifest(&b).await), "deleted here, live there: different rows");
    // `b` deletes it too, later: two deletions, two stamps.
    b.clock.advance(Duration::minutes(2));
    delete_thread_to_trash(&b.docs, b.edb.as_ref(), &id).await.unwrap();
    assert_ne!(stamp_of(&a, &id).await, stamp_of(&b, &id).await);
    assert_ne!(hash(&manifest(&a).await), hash(&manifest(&b).await), "two deletion stamps: different rows");
    full_sync(&a, &b).await;
    assert_eq!(stamp_of(&a, &id).await, stamp_of(&b, &id).await, "converged on the later deletion");
    assert_eq!(hash(&manifest(&a).await), hash(&manifest(&b).await));
}

// --------------------------------------------------------------------- S11 --

/// S11: "A document arriving for a thread already shrunk to its id still asks
/// (S5-12) … The marker itself is still shown nowhere (S7); the document is."
/// The phone wrote a document in the thread before the deletion reached it; it
/// arrives on the laptop after the laptop shrank the thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s11_a_document_arriving_for_a_thread_shrunk_here_asks_without_a_name() {
    let (a, b, id, deletion) = shrunk_on_a("s11-arrives", async |_, b, id| {
        create_in(b, id, "Written offline", "phone text").await;
    })
    .await;
    full_sync(&a, &b).await;
    let asks = a.docs.orphans_of_deleted_threads().await.unwrap();
    assert_eq!(asks.len(), 1, "the arriving document asks: {asks:?}");
    assert_eq!(asks[0].title, "Written offline");
    assert_eq!(asks[0].thread_id, id);
    assert_eq!(asks[0].deleted_thread_name, None, "no name to give: the thread shrank to its id");
    assert!(!trashed(&a, &asks[0].id).await, "the document is live, shown");
    // The marker itself stays a marker, shown nowhere.
    assert_eq!(stamp_of(&a, &id).await.as_ref(), Some(&deletion));
    assert!(state(&a, &id).await.is_none_or(|s| s.deleted && s.name.is_empty()), "no name comes back");
    assert!(!active_ids(&a).await.contains(&id), "no lane for it");
}
