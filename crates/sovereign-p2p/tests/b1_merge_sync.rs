//! v0.0.13 lot L5, B1 — **C-MERGE MG4 and MG5 across devices** (spec ch. 03,
//! *Merging keeps the source thread — contract (C-MERGE)*, v2.0.109):
//! « The two fields travel with a stamp of their own. A received row changes
//! the merge only by that stamp. A row without it never changes the merge, so
//! a rename and a merge made on two devices combine », and « A rename made
//! there [on an older peer] no longer undoes the merge ».
//!
//! Two devices from `wire_common`, thread rows moved as `node.rs` moves them
//! (`compute_all_row_diffs`, `get_rows`, `apply_rows`), as in
//! `wire_thread_sync.rs`, whose older-build harness is copied here.
//!
//! **A v0.0.12 peer** is simulated at the envelope: its row is the sender's
//! row cut to the fields of the thread row of v0.0.12 — the fields before
//! stamps (`wire_thread_sync.rs`, `OLD_THREAD_FIELDS`) and `stamp` — then
//! re-sealed and re-signed. v0.0.12 decodes a received row into its `Thread`
//! (no `deny_unknown_fields`) and rebuilds it field by field before storing it
//! (`thread_apply_remote` at the tag `v0.0.12`), so the merge fields never
//! reach its store, and a rename there writes its whole stored row under a
//! newer stamp: the cut row is what it sends (the test author's reading of
//! the tag, 2026-11-17).
//!
//! Written before the code by the B1 test author; the implementer does not
//! edit this file. A test that looks wrong is reported, not worked around.

mod wire_common;

use base64::Engine as _;
use chrono::Duration;
use sovereign_crypto::account_key::AccountKey;
use sovereign_db::schema::Thread;
use sovereign_db::GraphDB;
use sovereign_p2p::libp2p::identity::Keypair;
use sovereign_p2p::protocol::sync::{EncryptedRow, SyncTable};
use sovereign_p2p::sync_engine::compute_all_row_diffs;
use wire_common::*;

const SEED_A: u8 = 51;
const SEED_B: u8 = 52;

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

async fn row(d: &Device, id: &str) -> Thread {
    d.edb.get_thread(id).await.expect("the thread record")
}

/// What a device holds of a thread's merge: name, live, the thread it was
/// merged into, its end date (ms) and its merge stamp.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Merge {
    name: String,
    live: bool,
    merged_into: Option<String>,
    ended_ms: Option<i64>,
    merge_stamp: Option<String>,
}

async fn merge_of(d: &Device, id: &str) -> Merge {
    let t = row(d, id).await;
    Merge {
        name: t.name,
        live: t.deleted_at.is_none(),
        merged_into: t.merged_into,
        ended_ms: t.ended_at.map(|e| e.timestamp_millis()),
        merge_stamp: t.merge_stamp,
    }
}

/// Garden and Kitchen, created on `a`, synced to `b`.
async fn garden_and_kitchen(tag: &str) -> (Device, Device, String, String) {
    let (a, b) = two(tag).await;
    let garden = a.docs.thread_create("Garden", "").await.expect("Garden");
    let kitchen = a.docs.thread_create("Kitchen", "").await.expect("Kitchen");
    sync_threads(&a, &b).await;
    assert_eq!(row(&b, &garden).await.name, "Garden", "setup: Garden reached b");
    assert_eq!(row(&b, &kitchen).await.name, "Kitchen", "setup: Kitchen reached b");
    (a, b, garden, kitchen)
}

/// Merges Garden into Kitchen on `d`, and checks that it is ended there.
async fn merge_on(d: &Device, garden: &str, kitchen: &str) -> Merge {
    d.docs.thread_merge(kitchen, garden).await.expect("merged");
    let m = merge_of(d, garden).await;
    assert_eq!(m.merged_into.as_deref(), Some(kitchen), "MG1: Garden is ended, merged into Kitchen, on {}", d.name);
    assert!(m.live && m.ended_ms.is_some() && m.merge_stamp.is_some(), "MG1: live, with its end date and merge stamp: {m:?}");
    m
}

// ------------------------------------------------------ a v0.0.12 peer --

/// The fields of the thread row as v0.0.12 serialises it: the fields before
/// stamps, and the stamp (`schema.rs` at the tag `v0.0.12`).
const V0012_THREAD_FIELDS: [&str; 10] = [
    "id",
    "name",
    "description",
    "created_at",
    "modified_at",
    "deleted_at",
    "name_nonce",
    "description_nonce",
    "name_token_hashes",
    "stamp",
];

fn pair_key(x: &Device, y: &Device) -> [u8; 32] {
    AccountKey::from_bytes(ACCOUNT_SEED).derive_pair_key(&x.peer.to_string(), &y.peer.to_string())
}

/// Copy of `sync_service.rs`'s canonical row envelope encoding, for a thread
/// row (checked against a real signature in `as_sent_by_v0012`).
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

/// `row` as the same device on v0.0.12 would have sent it: the body cut to
/// [`V0012_THREAD_FIELDS`], re-sealed, re-signed by the sender (`seed`).
fn as_sent_by_v0012(row: EncryptedRow, key: &[u8; 32], seed: u8) -> EncryptedRow {
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
        .filter(|(k, _)| V0012_THREAD_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    assert!(old.contains_key("stamp"), "harness: the row carries the thread's stamp, as v0.0.12's does");
    let (ct, nonce) =
        sovereign_crypto::aead::encrypt(&serde_json::to_vec(&serde_json::Value::Object(old)).unwrap(), key).unwrap();
    let mut out = row;
    out.ciphertext = b64.encode(ct);
    out.nonce = b64.encode(nonce);
    out.signature = b64.encode(kp.sign(&row_sig_bytes(&out)).unwrap());
    out
}

/// Sends thread `id` from `from` (as v0.0.12 would, identity `seed`) to `to`;
/// returns how many rows `to` wrote.
async fn v0012_sends(from: &Device, seed: u8, to: &Device, id: &str) -> u32 {
    let key = pair_key(from, to);
    let rows = from.sync.get_rows(SyncTable::Thread, &[id.to_string()], &to.peer).await.expect("rows");
    assert_eq!(rows.len(), 1, "harness: the row exists on the sender");
    let rows = rows.into_iter().map(|r| as_sent_by_v0012(r, &key, seed)).collect();
    to.sync.apply_rows(SyncTable::Thread, rows, &from.peer).await.expect("apply").written
}

// ------------------------------------------------------------------ MG4 --

/// MG4: the two fields and their stamp travel, and survive between two
/// v0.0.13 devices: b holds the merge as a made it — same end date, same
/// thread merged into, same merge stamp — and Garden stays live there.
///
/// Red today: the merge deletes Garden, and has no field to send.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg4_the_merge_and_its_stamp_travel_between_two_v0013_devices() {
    let (a, b, garden, kitchen) = garden_and_kitchen("b1-mg4-travel").await;
    a.clock.advance(Duration::seconds(1));
    let m = merge_on(&a, &garden, &kitchen).await;
    sync_threads(&a, &b).await;
    assert_eq!(merge_of(&b, &garden).await, m, "b holds the merge as a made it");
    assert_eq!(merge_of(&b, &kitchen).await.merged_into, None, "Kitchen records nothing");
}

/// MG4: « A row without it never changes the merge … A rename made there [on
/// v0.0.12] no longer undoes the merge. » The merge on a, synced to b; b
/// renames Garden; b's row, as v0.0.12 sends it (no merge fields, a newer
/// thread stamp), reaches a: the rename applies, the merge holds.
///
/// Red today: the merge deletes Garden; and once built as v0.0.12 applies
/// rows (the whole row by its stamp), the rename's row would clear the merge.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg4_a_rename_from_a_v0012_peer_never_undoes_the_merge() {
    let (a, b, garden, kitchen) = garden_and_kitchen("b1-mg4-old-rename").await;
    a.clock.advance(Duration::seconds(1));
    let m = merge_on(&a, &garden, &kitchen).await;
    sync_threads(&a, &b).await;
    b.clock.advance(Duration::minutes(2));
    b.docs.thread_update(&garden, Some("Garden 2025"), None).await.expect("renamed on b");
    assert_eq!(v0012_sends(&b, SEED_B, &a, &garden).await, 1, "a applies the rename (a newer thread stamp)");
    let now = merge_of(&a, &garden).await;
    assert_eq!(now.name, "Garden 2025", "the rename is applied");
    assert_eq!(
        (now.merged_into, now.ended_ms, now.merge_stamp, now.live),
        (m.merged_into, m.ended_ms, m.merge_stamp, true),
        "and the merge holds"
    );
}

/// MG4: « a rename and a merge made on two devices combine » — whichever is
/// later. Garden merged into Kitchen on a while b renames it, with no sync in
/// between; after a sync, both devices hold the new name and the merge. Both
/// orders: the rename after the merge, and the merge after the rename.
///
/// Red today: the merge deletes Garden.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg4_a_rename_and_a_merge_made_on_two_devices_combine() {
    for (i, rename_later) in [true, false].into_iter().enumerate() {
        let (a, b, garden, kitchen) = garden_and_kitchen(&format!("b1-mg4-combine-{i}")).await;
        let (merge_at, rename_at) =
            if rename_later { (Duration::seconds(1), Duration::minutes(2)) } else { (Duration::minutes(2), Duration::seconds(1)) };
        a.clock.advance(merge_at);
        let m = merge_on(&a, &garden, &kitchen).await;
        b.clock.advance(rename_at);
        b.docs.thread_update(&garden, Some("Garden 2025"), None).await.expect("renamed on b");
        sync_threads(&a, &b).await;
        for d in [&a, &b] {
            let now = merge_of(d, &garden).await;
            assert_eq!(now.name, "Garden 2025", "rename later={rename_later}: the rename holds on {}", d.name);
            assert_eq!(
                (now.merged_into, now.ended_ms, now.merge_stamp, now.live),
                (m.merged_into.clone(), m.ended_ms, m.merge_stamp.clone(), true),
                "rename later={rename_later}: the merge holds on {}",
                d.name
            );
        }
    }
}

// ------------------------------------------------------------------ MG5 --

/// MG5: « At any time and from any device, by naming the thread. » The merge
/// made on a, undone on b (the service's undo, which the assistant calls):
/// the undo travels back to a — the two fields cleared under b's new merge
/// stamp.
///
/// Red today: the merge deletes Garden; `thread_unmerge` is not built.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg5_an_undo_made_on_the_other_device_travels() {
    let (a, b, garden, kitchen) = garden_and_kitchen("b1-mg5-other").await;
    a.clock.advance(Duration::seconds(1));
    merge_on(&a, &garden, &kitchen).await;
    sync_threads(&a, &b).await;
    b.clock.advance(Duration::minutes(1));
    b.docs.thread_unmerge(&garden).await.expect("undone on b");
    let undone = merge_of(&b, &garden).await;
    assert_eq!((undone.merged_into.as_deref(), undone.ended_ms), (None, None), "undone on b");
    sync_threads(&a, &b).await;
    assert_eq!(merge_of(&a, &garden).await, undone, "a holds the undo, under b's merge stamp");
}

// ------------------------------------------- b1-tests-v3: the row hash --
//
// The shape agreed for MG4 (PANDA2's proposal 1, with WSL's shape and the
// laptop's reading, 2026-11-17): the row hash stays as it was (name,
// description, the thread's stamp), and the merge stamp travels in a manifest
// list of its own beside `thread_stamps`. So a v0.0.12 peer, which drops the
// merge fields, hashes a merged thread as a v0.0.13 device does, and the row
// is not offered again on every round.

/// The fields of the manifest as v0.0.12 serialises it (`manifest.rs` at the
/// tag `v0.0.12`); its wire version was 1.
const V0012_MANIFEST_FIELDS: [&str; 17] = [
    "device_id",
    "generated_at",
    "documents",
    "threads",
    "entities",
    "pii_records",
    "share_records",
    "contacts",
    "messages",
    "conversations",
    "milestones",
    "relationships",
    "suggested_links",
    "wire_version",
    "document_wire",
    "purge_markers",
    "thread_stamps",
];

/// `d`'s manifest as v0.0.12 would build and read it: the fields it knows,
/// wire version 1. (v0.0.12 decodes a manifest with serde and no
/// `deny_unknown_fields`, so it drops a field it does not know, as this does.)
async fn v0012_manifest(d: &Device) -> sovereign_p2p::protocol::manifest::SyncManifest {
    let m = serde_json::to_value(manifest(d).await).unwrap();
    let mut old: serde_json::Map<String, serde_json::Value> = m
        .as_object()
        .expect("an object")
        .iter()
        .filter(|(k, _)| V0012_MANIFEST_FIELDS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    old.insert("wire_version".into(), serde_json::json!(1));
    serde_json::from_value(serde_json::Value::Object(old)).expect("a manifest without the new fields still reads")
}

/// One exchange of thread rows between `a` (v0.0.13) and `old` (v0.0.12),
/// with `a` as the initiator when `a_starts`, else `old`: each side's manifest
/// as the other reads it, and every row that reaches `old` cut to v0.0.12's
/// fields, as its decode does (and every row `old` sends is the row it
/// stored). Returns the ids of the rows sent, either way.
async fn round_with_v0012(a: &Device, old: &Device, a_starts: bool) -> Vec<String> {
    let (ma, mo) = (manifest(a).await, v0012_manifest(old).await);
    let ma_seen_by_old = {
        let v = serde_json::to_value(&ma).unwrap();
        let kept: serde_json::Map<String, serde_json::Value> = v
            .as_object()
            .unwrap()
            .iter()
            .filter(|(k, _)| V0012_MANIFEST_FIELDS.contains(&k.as_str()))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        serde_json::from_value::<sovereign_p2p::protocol::manifest::SyncManifest>(serde_json::Value::Object(kept)).unwrap()
    };
    let (to_a, to_old) = if a_starts {
        let d = compute_all_row_diffs(&ma, &mo).remove(&SyncTable::Thread).unwrap_or_default();
        (d.need_from_remote, d.push_to_remote)
    } else {
        let d = compute_all_row_diffs(&mo, &ma_seen_by_old).remove(&SyncTable::Thread).unwrap_or_default();
        (d.push_to_remote, d.need_from_remote)
    };
    let mut sent = vec![];
    if !to_old.is_empty() {
        let key = pair_key(a, old);
        let rows = a.sync.get_rows(SyncTable::Thread, &to_old, &old.peer).await.expect("rows from a");
        let rows: Vec<EncryptedRow> = rows.into_iter().map(|r| as_sent_by_v0012(r, &key, SEED_A)).collect();
        old.sync.apply_rows(SyncTable::Thread, rows, &a.peer).await.expect("rows on the older peer");
        sent.extend(to_old);
    }
    if !to_a.is_empty() {
        let key = pair_key(old, a);
        let rows = old.sync.get_rows(SyncTable::Thread, &to_a, &a.peer).await.expect("rows from the older peer");
        let rows: Vec<EncryptedRow> = rows.into_iter().map(|r| as_sent_by_v0012(r, &key, SEED_B)).collect();
        a.sync.apply_rows(SyncTable::Thread, rows, &old.peer).await.expect("rows on a");
        sent.extend(to_a);
    }
    sent
}

/// MG4 with a v0.0.12 peer (b1-tests-v3; the shape agreed on 2026-11-17): a
/// merged thread is sent to the older peer at most once, and then the diff
/// asks for nothing — in either direction, on every later round. The merge
/// holds on a throughout.
///
/// Red today: the merge deletes Garden. Red on `v0013/b1` at `0424c912`: the
/// merge stamp is in the row hash, which the older peer cannot reproduce, so
/// Garden is offered again on every round.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg4_a_merged_thread_is_not_resent_to_a_v0012_peer_on_every_round() {
    let (a, b, garden, kitchen) = garden_and_kitchen("b1-mg4-hash-old").await;
    a.clock.advance(Duration::seconds(1));
    let m = merge_on(&a, &garden, &kitchen).await;
    let first = round_with_v0012(&a, &b, true).await;
    assert!(first.iter().filter(|id| **id == garden).count() <= 1, "Garden is sent at most once: {first:?}");
    assert!(row(&b, &garden).await.merged_into.is_none(), "harness: the older peer stored the row without the merge");
    for (n, a_starts) in [(2, true), (3, false), (4, true), (5, false)] {
        let sent = round_with_v0012(&a, &b, a_starts).await;
        assert!(sent.is_empty(), "round {n} ({} starts): nothing to exchange, got {sent:?}", if a_starts { "a" } else { "the older peer" });
    }
    assert_eq!(merge_of(&a, &garden).await, m, "the merge holds on a");
}

/// The row hash (b1-tests-v3; the shape agreed on 2026-11-17): a thread's hash
/// in the manifest is the same with or without its merge fields, so that an
/// older peer, which drops them, hashes the thread as this device does. The
/// fields are set on the stored row directly, nothing else changed.
///
/// Green against today's code (the hash does not know the fields). Red on
/// `v0013/b1` at `0424c912`: its hash adds the merge stamp.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mg4_the_row_hash_is_the_same_with_or_without_the_merge_fields() {
    let (a, _b) = two("b1-mg4-hash-same").await;
    let garden = a.docs.thread_create("Garden", "").await.expect("Garden");
    let kitchen = a.docs.thread_create("Kitchen", "").await.expect("Kitchen");
    let hash_of = |m: &sovereign_p2p::protocol::manifest::SyncManifest, id: &str| {
        m.threads.iter().find(|t| t.thread_id == id).map(|t| t.content_hash.clone()).expect("listed")
    };
    let before = hash_of(&manifest(&a).await, &garden);
    let mut t = row(&a, &garden).await;
    let stamp = t.stamp.clone().expect("stamped");
    let wall: i64 = stamp.split(':').next().unwrap().parse().unwrap();
    t.ended_at = chrono::DateTime::from_timestamp_millis(wall + 1000);
    t.merged_into = Some(kitchen.clone());
    t.merge_stamp = Some(format!("{}:0:{}", wall + 1000, a.peer));
    a.edb.put_thread(t).await.unwrap();
    let with = row(&a, &garden).await;
    assert_eq!(with.merged_into.as_deref(), Some(kitchen.as_str()), "harness: the fields are stored");
    assert_eq!(with.stamp.as_deref(), Some(stamp.as_str()), "harness: the thread's own stamp is unchanged");
    assert_eq!(hash_of(&manifest(&a).await, &garden), before, "the hash does not see the merge fields");
}
