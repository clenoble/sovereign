//! FC3 (spec ch. 03, *Two threads of the name sought: asked once,
//! remembered*; v0.0.13, item 9): where a remembered answer is kept. « In the
//! session's own store, sealed at rest as a thread's name is: the name as
//! find-or-create compares it, the thread chosen, and the live threads of that
//! name it was chosen among. It is this persona's and this device's. »
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. `EncryptedGraphDB` over a real in-memory `SurrealGraphDB`, as
//! `encrypted_over_surreal.rs` builds it; the plain store under it is read raw
//! (`list_thread_choices_raw`, named by the implementer, 2026-11-18). That it
//! never travels to the other devices is in
//! `sovereign-p2p/tests/fc3_choices_not_synced.rs`.
//!
//! Runs with `cargo test -p sovereign-db --features encryption --test
//! fc3_choices_at_rest`.

use std::sync::Arc;

use tokio::sync::RwLock;

use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;

/// A name nothing else in the store holds, so its absence at rest means
/// something.
const NAME: &str = "Zanzibar Quokka";

fn uniq(tag: &str) -> String {
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    format!("{}-{n}-{nanos}-{tag}", std::process::id())
}

/// A persona's keys: what one session's `EncryptedGraphDB` is built with.
#[derive(Clone)]
struct Keys {
    kdbs: [Arc<RwLock<KeyDatabase>>; 6],
    kek: Arc<Kek>,
    index: Arc<IndexKey>,
    device: Arc<DeviceKey>,
}

fn keys(tag: &str, passphrase: &[u8]) -> Keys {
    let u = uniq(tag);
    let kdb = |s: &str| Arc::new(RwLock::new(KeyDatabase::new(std::env::temp_dir().join(format!("sovereign-fc3-{u}-{s}-keys.db")))));
    let mk = MasterKey::from_passphrase(passphrase, b"salt").unwrap();
    Keys {
        kdbs: [kdb("doc"), kdb("msg"), kdb("thr"), kdb("conv"), kdb("con"), kdb("shr")],
        kek: Arc::new(Kek::generate()),
        index: Arc::new(IndexKey::generate()),
        device: Arc::new(DeviceKey::derive(&mk, "fc3-device").unwrap()),
    }
}

fn session(raw: &Arc<SurrealGraphDB>, k: &Keys) -> EncryptedGraphDB {
    let [d, m, t, c, n, s] = k.kdbs.clone();
    EncryptedGraphDB::new(raw.clone() as Arc<dyn GraphDB>, d, m, t, c, n, s, k.kek.clone(), k.index.clone(), k.device.clone())
}

async fn store() -> Arc<SurrealGraphDB> {
    let raw = Arc::new(SurrealGraphDB::new(StorageMode::Memory).await.unwrap());
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    raw
}

/// Two live threads named `NAME`, through the encrypted layer; their ids.
async fn two_threads(edb: &EncryptedGraphDB) -> (String, String) {
    let a = edb.create_thread(Thread::new(NAME.into(), String::new())).await.unwrap().id_string().unwrap();
    let b = edb.create_thread(Thread::new(NAME.into(), String::new())).await.unwrap().id_string().unwrap();
    (a, b)
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The id's key, without its table (`thread:abc` → `abc`).
fn key_of(id: &str) -> &str {
    id.split_once(':').map(|(_, k)| k).unwrap_or(id)
}

// FC3: what is kept: the name as compared, the thread chosen, the live threads of that name.
#[tokio::test]
async fn fc3_the_choice_keeps_the_name_the_thread_chosen_and_the_threads_it_was_chosen_among() {
    let raw = store().await;
    let k = keys("keeps", b"primary");
    let edb = session(&raw, &k);
    let (a, b) = two_threads(&edb).await;
    sovereign_db::remember_thread_choice(&edb, NAME, &b).await.expect("her answer is remembered");
    let kept = edb.thread_choice(&format!("  {}  ", NAME.to_uppercase())).await.unwrap().expect("found by the name as compared");
    assert_eq!(kept.name.trim().to_lowercase(), NAME.to_lowercase(), "the name as find-or-create compares it: {kept:?}");
    assert_eq!(kept.chosen, b, "the thread chosen");
    assert_eq!(sorted(kept.among.clone()), sorted(vec![a, b]), "the live threads it was chosen among");
}

// FC3: sealed at rest: the plain store holds neither the name, in any case, nor any of the threads' ids.
#[tokio::test]
async fn fc3_at_rest_the_store_holds_neither_the_name_nor_the_threads() {
    let raw = store().await;
    let k = keys("at-rest", b"primary");
    let edb = session(&raw, &k);
    let (a, b) = two_threads(&edb).await;
    sovereign_db::remember_thread_choice(&edb, NAME, &a).await.unwrap();
    let rows = raw.list_thread_choices_raw().await.expect("the plain store's rows");
    assert!(!rows.is_empty(), "fixture: the choice is a row of the store");
    for row in &rows {
        let text = row.to_string().to_lowercase();
        for word in NAME.to_lowercase().split(' ') {
            assert!(!text.contains(word), "the name is not in the clear at rest («{word}»): {row}");
        }
        for id in [&a, &b] {
            assert!(!text.contains(&key_of(id).to_lowercase()), "no thread id in the clear at rest ({id}): {row}");
        }
    }
}

// FC3: kept in the store: the next session over the same store and keys finds it (FC2 after a restart).
#[tokio::test]
async fn fc3_the_next_session_finds_it() {
    let raw = store().await;
    let k = keys("next", b"primary");
    let (_, b) = {
        let edb = session(&raw, &k);
        let ab = two_threads(&edb).await;
        sovereign_db::remember_thread_choice(&edb, NAME, &ab.1).await.unwrap();
        ab
    };
    let next = session(&raw, &k);
    assert_eq!(next.thread_choice(NAME).await.unwrap().map(|c| c.chosen), Some(b));
}

// FC3: « the other persona never sees it »: other keys over the same store find nothing for that name.
#[tokio::test]
async fn fc3_other_keys_find_nothing() {
    let raw = store().await;
    let edb = session(&raw, &keys("persona-a", b"primary"));
    let (a, _) = two_threads(&edb).await;
    sovereign_db::remember_thread_choice(&edb, NAME, &a).await.unwrap();
    let other = session(&raw, &keys("persona-b", b"other persona"));
    let seen = other.thread_choice(NAME).await;
    assert!(!matches!(seen, Ok(Some(_))), "another persona's keys never read the choice: {seen:?}");
}

// FC3, FC4: a choice forgotten is gone, at rest too.
#[tokio::test]
async fn fc3_a_forgotten_choice_is_gone() {
    let raw = store().await;
    let edb = session(&raw, &keys("forget", b"primary"));
    let (a, _) = two_threads(&edb).await;
    sovereign_db::remember_thread_choice(&edb, NAME, &a).await.unwrap();
    edb.forget_thread_choice(NAME).await.expect("forgotten");
    assert_eq!(edb.thread_choice(NAME).await.unwrap(), None);
    assert!(raw.list_thread_choices_raw().await.unwrap().is_empty(), "no row left at rest");
}
