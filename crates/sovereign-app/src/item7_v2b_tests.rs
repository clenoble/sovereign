//! v0.0.12 item 7 V2b, B2 — the app has the list
//! (`doc/plans/V0012-item7-v2b-app-list.md`). Written before the code, by
//! another author: whoever implements V2b makes these pass without editing
//! this file. **It does not compile until the seams below exist.** Run with
//! the shipped feature set (`encryption,rocksdb,encrypted-log,p2p`).
//!
//! Seams named here (the code author creates them), following
//! `doc_orphans.rs`: the layer is kept out of the `#[tauri::command]`
//! wrapper so it is tested against a real encrypted store without a webview
//! (a command takes a `tauri::Webview`, which a test cannot build).
//!
//! - module `crate::doc_unreadable` (`src/doc_unreadable.rs`, declared in
//!   `lib.rs`);
//! - `#[derive(Clone, Debug, PartialEq, Eq, Serialize)] pub struct
//!   UnreadableDocumentsDto { pub heading: String, pub meaning: String,
//!   pub lines: Vec<String> }` — `heading` and `meaning` are
//!   `sovereign_docs::UNREADABLE_HEADING` / `UNREADABLE_MEANING`, `lines` are
//!   `sovereign_docs::unreadable_line` of each document, in the order
//!   `unlock_migration_undecryptable` gives. **[choice of the test author]**
//!   the heading and the meaning travel with the lines so the frontend holds
//!   no copy of them (B1: one wording, used by both hosts);
//! - `pub async fn list_unreadable_documents(docs: Option<&DocumentService>)
//!   -> UnreadableDocumentsDto` — `None` (no document service) gives no lines;
//!   a list that cannot be read gives no lines and logs a warning (B2
//!   [choice], as the shell's `load_alerts_as`);
//! - `doc_unreadable::commands::list_unreadable_documents(webview, state) ->
//!   Result<UnreadableDocumentsDto, String>`, the Tauri command
//!   `list_unreadable_documents`, registered in `generate_handler!` and, being
//!   a data command, classified in `GATED_COMMANDS` (IPC-004): it calls
//!   `state.require_unlocked(&webview)` first. **Note for the contract**: B2
//!   says "with no document service (locked, no-auth) it returns an empty
//!   list"; a locked session is refused by the gate (an `Err`) before any
//!   list is read. The frontend reads a refusal as an empty list
//!   (`SyncAlerts.unreadable.svelte.test.ts`), so the person sees the same.
//!
//! Stores: a real encrypted store over an in-memory SurrealDB, built here as
//! `docs_host::testing::encrypted_memory_store` builds it but keeping the raw
//! store, to make a row undecryptable at rest (as the shell's
//! `item7_tests.rs` does). Nothing touches a real workspace.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::{Document, Thread};
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{
    run_unlock_migration, unlock_migration_undecryptable, unreadable_line, DocumentService, UndecryptableDocument,
    UNREADABLE_HEADING, UNREADABLE_MEANING,
};

use crate::doc_unreadable::{self, UnreadableDocumentsDto};

/// base64("not-a-real-ciphertext") and 24 zero bytes: valid base64, not a
/// ciphertext of anything (DB-M1's test).
const JUNK_CT: &str = "bm90LWEtcmVhbC1jaXBoZXJ0ZXh0";
const JUNK_NONCE: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// In the titles and bodies of the fixture's rows: never in what the command
/// returns.
const MARKER: &str = "zebra-v2b-app";

// ================================================================= fixtures

/// `testing::encrypted_memory_store`, keeping the raw store.
async fn store_with_raw(tag: &str) -> (Arc<SurrealGraphDB>, Arc<EncryptedGraphDB>) {
    use sovereign_crypto::device_key::DeviceKey;
    use sovereign_crypto::index_key::IndexKey;
    use sovereign_crypto::kek::Kek;
    use sovereign_crypto::key_db::KeyDatabase;
    use sovereign_crypto::master_key::MasterKey;
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.expect("memory store"));
    raw.connect().await.expect("connect");
    raw.init_schema().await.expect("schema");
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("item7-v2b-app-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let device_key = DeviceKey::derive(&mk, "test-device").expect("device key");
    let edb = Arc::new(EncryptedGraphDB::new(
        raw.clone() as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(device_key),
    ));
    (raw, edb)
}

/// The last modified date of the fixture's rows.
fn when() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2025, 3, 2, 12, 0, 0).unwrap()
}

/// A row as writers before the service left it, modified at [`when`].
async fn legacy_row(store: &EncryptedGraphDB, thread: &str, title: &str, content: &str) -> String {
    let mut d = Document::new(title.into(), thread.into(), true);
    d.content = content.into();
    d.created_at = when();
    d.modified_at = when();
    store.create_document(d).await.unwrap().id_string().unwrap()
}

/// Makes the row undecryptable at rest.
async fn make_undecryptable(raw: &SurrealGraphDB, store: &EncryptedGraphDB, id: &str) {
    raw.set_document_content_encryption(id, JUNK_CT, JUNK_NONCE).await.expect("corrupt at rest");
    assert!(store.get_document(id).await.is_err(), "fixture: the row no longer decrypts");
}

async fn new_thread(store: &EncryptedGraphDB) -> String {
    store.create_thread(Thread::new("Lane".into(), String::new())).await.unwrap().id_string().unwrap()
}

/// The phone's service over `store`, after the one-time conversion has run
/// (what an unlock does: `start_unlock_migration` runs exactly this).
async fn unlocked(store: Arc<EncryptedGraphDB>) -> DocumentService {
    let docs = build_document_service(store, Some(testing::account_key()), "phone".into());
    run_unlock_migration(&docs).await.expect("the conversion runs");
    docs
}

/// A store with one readable row and one undecryptable row; the service
/// after the conversion, and the undecryptable row's id.
async fn one_unreadable(tag: &str) -> (Arc<SurrealGraphDB>, DocumentService, String) {
    let (raw, store) = store_with_raw(tag).await;
    let t = new_thread(&store).await;
    legacy_row(&store, &t, "Readable plain", "# fine\n\nreadable").await;
    let bad = legacy_row(&store, &t, &format!("{MARKER} title"), &format!("{MARKER} body")).await;
    make_undecryptable(&raw, &store, &bad).await;
    let docs = unlocked(store).await;
    (raw, docs, bad)
}

fn the_two_texts(dto: &UnreadableDocumentsDto) {
    assert_eq!(dto.heading, UNREADABLE_HEADING, "the heading, from sovereign-docs (B1)");
    assert_eq!(dto.meaning, UNREADABLE_MEANING, "the meaning, from sovereign-docs (B1)");
}

// ======================================================================= B2

/// B2: "A Tauri command returns this persona's lines, already worded by B1":
/// on a store with one undecryptable row, its line — the very line
/// `sovereign_docs::unreadable_line` gives for that row (its id, its last
/// modified date, not in the trash) — and nothing for the readable row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_the_list_gives_the_line_of_the_undecryptable_document() {
    let (_raw, docs, bad) = one_unreadable("b2-one").await;
    let dto = doc_unreadable::list_unreadable_documents(Some(&docs)).await;
    let expected = UndecryptableDocument { id: bad.clone(), modified_at: when(), deleted: false };
    assert_eq!(dto.lines, vec![unreadable_line(&expected)], "its line, worded by B1");
    the_two_texts(&dto);
}

/// B2: what travels to the frontend — three fields, no title, no text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_the_list_travels_with_the_frontends_field_names_and_no_text() {
    let (_raw, docs, _bad) = one_unreadable("b2-json").await;
    let dto = doc_unreadable::list_unreadable_documents(Some(&docs)).await;
    let json = serde_json::to_value(&dto).unwrap();
    let mut keys: Vec<&str> = json.as_object().expect("an object").keys().map(String::as_str).collect();
    keys.sort();
    assert_eq!(keys, vec!["heading", "lines", "meaning"]);
    assert_eq!(json["lines"].as_array().map(Vec::len), Some(1));
    assert!(!json.to_string().contains(MARKER), "no title, no text: {json}");
}

/// B2: "in the order `unlock_migration_undecryptable` gives", one line per
/// document; a trashed one says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_the_lines_follow_the_conversions_order() {
    let (raw, store) = store_with_raw("b2-order").await;
    let t = new_thread(&store).await;
    let a = legacy_row(&store, &t, "A", &format!("{MARKER} a")).await;
    let b = legacy_row(&store, &t, "B", &format!("{MARKER} b")).await;
    let c = legacy_row(&store, &t, "C", &format!("{MARKER} c")).await;
    store.soft_delete_document(&b).await.expect("b in the trash");
    for id in [&a, &b, &c] {
        make_undecryptable(&raw, &store, id).await;
    }
    let docs = unlocked(store).await;

    let listed = unlock_migration_undecryptable(&docs).await.expect("the list");
    assert_eq!(listed.len(), 3, "fixture: the conversion lists the three rows");
    let dto = doc_unreadable::list_unreadable_documents(Some(&docs)).await;
    let want: Vec<String> = listed.iter().map(unreadable_line).collect();
    assert_eq!(dto.lines, want, "the lines, in the conversion's order");
    assert_eq!(dto.lines.iter().filter(|l| l.contains("(in the trash)")).count(), 1, "the trashed one says so");
}

/// B2: "With no document service (locked, no-auth) it returns an empty list."
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_no_document_service_gives_an_empty_list() {
    let dto = doc_unreadable::list_unreadable_documents(None).await;
    assert!(dto.lines.is_empty(), "{dto:?}");
    the_two_texts(&dto);
}

/// B2: a store with no undecryptable row gives no line.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_no_undecryptable_row_no_line() {
    let (_raw, store) = store_with_raw("b2-none").await;
    let t = new_thread(&store).await;
    legacy_row(&store, &t, "Fine", "# fine").await;
    let docs = unlocked(store).await;
    assert!(doc_unreadable::list_unreadable_documents(Some(&docs)).await.lines.is_empty());
}

/// B2 [choice]: "If the list cannot be read, it returns an empty list and
/// logs a warning, as the shell does." The completion record that holds the
/// list is made unreadable at rest: no line, no error, no panic. (The warning
/// is not read here.)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn b2_a_list_that_cannot_be_read_gives_an_empty_list() {
    let (raw, docs, _bad) = one_unreadable("b2-unreadable-record").await;
    assert_eq!(doc_unreadable::list_unreadable_documents(Some(&docs)).await.lines.len(), 1, "fixture: listed");
    raw.put_store_record_sealed(sovereign_docs::unlock_migration::RECORD_KEY, JUNK_CT, JUNK_NONCE)
        .await
        .expect("corrupt the record at rest");
    assert!(unlock_migration_undecryptable(&docs).await.is_err(), "fixture: the list can no longer be read");
    let dto = doc_unreadable::list_unreadable_documents(Some(&docs)).await;
    assert!(dto.lines.is_empty(), "{dto:?}");
    the_two_texts(&dto);
}

// ============================================================ source scans

fn src() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn read(p: &Path) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()))
}

/// The slice `const NAME: &[&str] = &[ … ];` of `lib.rs`.
fn slice_of<'a>(lib: &'a str, name: &str) -> &'a str {
    let start = lib.find(&format!("const {name}: &[&str] = &[")).unwrap_or_else(|| panic!("{name} in lib.rs"));
    let rest = &lib[start..];
    &rest[..rest.find("];").expect("the slice's end")]
}

/// B2: the command `list_unreadable_documents` exists, is registered, and
/// is gated like every data command (IPC-004: classified in
/// `GATED_COMMANDS`, `require_unlocked` called first).
#[test]
fn b2_the_command_is_registered_and_gated() {
    let lib = read(&src().join("lib.rs"));
    assert!(
        lib.contains("doc_unreadable::commands::list_unreadable_documents"),
        "registered in generate_handler!"
    );
    assert!(slice_of(&lib, "ALL_COMMANDS").contains("\"list_unreadable_documents\""), "mirrored in ALL_COMMANDS");
    assert!(slice_of(&lib, "GATED_COMMANDS").contains("\"list_unreadable_documents\""), "classified as gated");

    // The wrapper: the `fn list_unreadable_documents` whose arguments take the
    // webview (the layer's take the service).
    let layer = read(&src().join("doc_unreadable.rs"));
    let at = layer
        .match_indices("pub async fn list_unreadable_documents(")
        .map(|(i, _)| i)
        .find(|&i| layer[i..].split(')').next().is_some_and(|args| args.contains("webview")))
        .expect("the wrapper in doc_unreadable.rs, taking the webview");
    assert!(layer[..at].trim_end().ends_with("#[tauri::command]"), "it is a Tauri command");
    // Its body, by matching braces from the first `{` after the signature.
    let open = at + layer[at..].find('{').expect("its body");
    let mut depth = 0usize;
    let mut close = layer.len();
    for (i, ch) in layer[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let body = &layer[open..close];
    let gate = body.find("require_unlocked(&webview)").expect("it calls require_unlocked(&webview)");
    if let Some(list) = body.find("list_unreadable_documents(") {
        assert!(gate < list, "the gate comes before the list is read");
    }
    if let Some(docs) = body.find(".docs") {
        assert!(gate < docs, "the gate comes before the document service is read");
    }
}

/// B1 on the app's side: no copy of the heading or the meaning in the app's
/// code (they come from `sovereign-docs`).
#[test]
fn b1_the_app_keeps_no_copy_of_the_heading_or_the_meaning() {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs")
                && !p.file_name().unwrap().to_string_lossy().ends_with("tests.rs")
            {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&src(), &mut files);
    for p in files {
        let code: String =
            read(&p).lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");
        for text in ["\"Documents that could not be read", "kept as it was, untouched", "try to read it again after its next update"] {
            assert!(!code.contains(text), "{} holds a copy of `{text}`", p.display());
        }
    }
}
