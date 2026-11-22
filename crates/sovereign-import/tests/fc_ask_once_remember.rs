//! Two threads of the name sought: asked once, remembered (spec ch. 03, FC1,
//! FC2, FC4, FC5; Céline, 2026-11-13, « Demander une fois, retenir »; v0.0.13,
//! item 9).
//!
//! Two devices that each created « Web » or « Journal » before their first
//! sync used to stop saving pages and journal entries for good. Now the first
//! time Sovereign asks which, and remembers the choice while the live threads
//! of that name stay the ones it was chosen among. The import asks too,
//! instead of refusing, before writing anything.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file.

use std::cell::RefCell;
use std::path::Path;
use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::schema::Thread;
use sovereign_db::{find_or_create_choice, remember_thread_choice, FindOrCreate, GraphDB};
use sovereign_docs::DocumentService;
use sovereign_import::{execute, execute_asking, plan, ImportOptions, ThreadChooser};

struct Db {
    store: Arc<EncryptedGraphDB>,
    docs: DocumentService,
}

async fn fresh(tag: &str) -> Db {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "test".into());
    Db { store, docs }
}

async fn choice(db: &Db, name: &str) -> FindOrCreate {
    find_or_create_choice(&*db.store, name).await.expect("the lookup runs")
}

fn ids(ts: &[Thread]) -> Vec<String> {
    let mut v: Vec<String> = ts.iter().filter_map(|t| t.id_string()).collect();
    v.sort();
    v
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn used(c: FindOrCreate) -> String {
    match c {
        FindOrCreate::Use(id) => id,
        other => panic!("expected a thread to use, got {other:?}"),
    }
}

fn asked(c: FindOrCreate) -> Vec<String> {
    match c {
        FindOrCreate::Ask(ts) => ids(&ts),
        other => panic!("expected to ask, got {other:?}"),
    }
}

/// Two live « Web » threads, as two devices' first sync leaves them.
async fn two_web(db: &Db) -> (String, String) {
    let a = db.docs.thread_create("Web", "").await.unwrap();
    let b = db.docs.thread_create("Web", "").await.unwrap();
    (a, b)
}

// ================================================================ FC1, FC2

/// FC1: no live thread of the name creates it; one is used; two or more ask.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_none_creates_one_is_used_two_ask() {
    let db = fresh("fc1-cases").await;
    assert!(matches!(choice(&db, "Web").await, FindOrCreate::Create), "none: the caller creates it");
    let a = db.docs.thread_create("Web", "").await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), a, "one: it is used");
    let b = db.docs.thread_create("Web", "").await.unwrap();
    assert_eq!(asked(choice(&db, "Web").await), sorted(vec![a, b]), "two: Sovereign asks which, listing both");
}

/// FC2: once chosen, the next calls for that name use it without asking, the
/// name compared as find-or-create compares it (case, surrounding spaces).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_the_choice_is_remembered() {
    let db = fresh("fc2-remembered").await;
    let (_a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.expect("the choice is kept");
    assert_eq!(used(choice(&db, "Web").await), b, "the thread chosen, without asking");
    assert_eq!(used(choice(&db, "  web ").await), b, "the name as find-or-create compares it");
}

// ===================================================================== FC4

/// FC4: the chosen thread deleted: the choice is forgotten, and the one live
/// thread left is used without asking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_the_chosen_thread_deleted_forgets_and_uses_the_other() {
    let db = fresh("fc4-deleted").await;
    let (a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    sovereign_docs::delete_thread_to_trash(&db.docs, &*db.store, &b).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), a);
}

/// FC4: the other thread deleted: the set changed, so the choice is
/// forgotten; one live thread is left, and it is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_the_other_thread_deleted_leaves_one_to_use() {
    let db = fresh("fc4-other-deleted").await;
    let (a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &a).await.unwrap();
    sovereign_docs::delete_thread_to_trash(&db.docs, &*db.store, &b).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), a);
}

/// FC4: the chosen thread merged away into another: no longer a live thread
/// of the name; the other is used.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_the_chosen_thread_merged_away_forgets() {
    let db = fresh("fc4-merged").await;
    let (a, b) = two_web(&db).await;
    let elsewhere = db.docs.thread_create("Reading", "").await.unwrap();
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    db.docs.thread_merge(&elsewhere, &b).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), a);
}

/// FC4: the chosen thread renamed out of the name: forgotten.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_the_chosen_thread_renamed_out_of_the_name_forgets() {
    let db = fresh("fc4-renamed").await;
    let (a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    db.docs.thread_update(&b, Some("Web, old"), None).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), a);
}

/// FC4: a rename that leaves the name as find-or-create compares it (its case
/// only) changes nothing: the choice holds.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_a_rename_that_keeps_the_compared_name_keeps_the_choice() {
    let db = fresh("fc4-case").await;
    let (_a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    db.docs.thread_update(&b, Some("WEB"), None).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), b);
}

/// FC4: a third thread of the name, made here: forgotten, and two or more ask
/// again, the new one listed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_a_third_thread_made_here_asks_again() {
    let db = fresh("fc4-third-here").await;
    let (a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    let c = db.docs.thread_create("Web", "").await.unwrap();
    assert_eq!(asked(choice(&db, "Web").await), sorted(vec![a, b, c]));
}

/// FC4: a third thread of the name received by sync (written into the store
/// as an arriving row is): forgotten, and it asks again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_a_third_thread_received_by_sync_asks_again() {
    let db = fresh("fc4-third-sync").await;
    let (a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    let c = db.store.create_thread(Thread::new("Web".into(), String::new())).await.unwrap().id_string().unwrap();
    assert_eq!(asked(choice(&db, "Web").await), sorted(vec![a, b, c]));
}

/// FC4 then FC2: asked again, a new answer is remembered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc4_after_forgetting_a_new_choice_is_remembered() {
    let db = fresh("fc4-again").await;
    let (_a, b) = two_web(&db).await;
    remember_thread_choice(&*db.store, "Web", &b).await.unwrap();
    let c = db.docs.thread_create("Web", "").await.unwrap();
    assert!(matches!(choice(&db, "Web").await, FindOrCreate::Ask(_)));
    remember_thread_choice(&*db.store, "Web", &c).await.unwrap();
    assert_eq!(used(choice(&db, "Web").await), c);
}

// ===================================================================== FC5

fn write(dir: &Path, rel: &str, contents: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, contents).unwrap();
}

/// Folders « Work » and « Home », one note each.
fn two_folders() -> (tempfile::TempDir, sovereign_import::Manifest) {
    let tmp = tempfile::tempdir().unwrap();
    write(tmp.path(), "Work/memo.md", b"# Memo\nthe weekly memo");
    write(tmp.path(), "Home/list.md", b"# List\nthe shopping list");
    let m = plan(tmp.path(), &ImportOptions::default()).unwrap();
    (tmp, m)
}

/// A chooser that answers from a table, and records what it was asked.
struct Answers {
    picks: Vec<(String, Option<String>)>,
    asked: RefCell<Vec<(String, Vec<String>)>>,
}

impl ThreadChooser for Answers {
    fn choose(&self, name: &str, threads: &[Thread]) -> Option<String> {
        self.asked.borrow_mut().push((name.to_string(), ids(threads)));
        self.picks.iter().find(|(n, _)| n == name).and_then(|(_, pick)| pick.clone())
    }
}

async fn document_count(db: &Db) -> usize {
    db.docs.list(None).await.unwrap().len()
}

async fn titles_in(db: &Db, thread: &str) -> Vec<String> {
    db.docs.list(Some(thread)).await.unwrap().into_iter().map(|d| d.title).collect()
}

/// FC5: the import asks, for each name two live threads share, and files the
/// notes where she answered; each answer is remembered.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc5_the_import_asks_and_remembers() {
    let db = fresh("fc5-asks").await;
    let w1 = db.docs.thread_create("Work", "").await.unwrap();
    let w2 = db.docs.thread_create("Work", "").await.unwrap();
    let h1 = db.docs.thread_create("Home", "").await.unwrap();
    let h2 = db.docs.thread_create("Home", "").await.unwrap();
    let (_tmp, m) = two_folders();
    let chooser = Answers { picks: vec![("Work".into(), Some(w2.clone())), ("Home".into(), Some(h1.clone()))], asked: RefCell::default() };

    execute_asking(&*db.store, &db.docs, &m, &chooser).await.expect("the import runs, asked");
    let asked = chooser.asked.borrow().clone();
    let mut names: Vec<String> = asked.iter().map(|(n, _)| n.clone()).collect();
    names.sort();
    assert_eq!(names, vec!["Home".to_string(), "Work".to_string()], "asked once for each shared name: {asked:?}");
    for (n, listed) in &asked {
        let want = if n == "Work" { sorted(vec![w1.clone(), w2.clone()]) } else { sorted(vec![h1.clone(), h2.clone()]) };
        assert_eq!(listed, &want, "the question lists the live threads of {n}");
    }
    assert_eq!(titles_in(&db, &w2).await.len(), 1, "the note lands in the Work chosen");
    assert!(titles_in(&db, &w1).await.is_empty());
    assert_eq!(titles_in(&db, &h1).await.len(), 1, "the note lands in the Home chosen");
    assert_eq!(used(choice(&db, "Work").await), w2, "the answer is remembered");
    assert_eq!(used(choice(&db, "Home").await), h1, "the answer is remembered");
}

/// FC5: a remembered choice is used without asking again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc5_a_remembered_choice_is_not_asked_again() {
    let db = fresh("fc5-remembered").await;
    let _w1 = db.docs.thread_create("Work", "").await.unwrap();
    let w2 = db.docs.thread_create("Work", "").await.unwrap();
    remember_thread_choice(&*db.store, "Work", &w2).await.unwrap();
    let (_tmp, m) = two_folders();
    let chooser = Answers { picks: vec![], asked: RefCell::default() };
    execute_asking(&*db.store, &db.docs, &m, &chooser).await.expect("the import runs");
    assert!(chooser.asked.borrow().is_empty(), "nothing is asked: {:?}", chooser.asked.borrow());
    assert_eq!(titles_in(&db, &w2).await.len(), 1);
}

/// FC5, « before writing anything »: no answer for one of the shared names,
/// and nothing is written, nothing created, nothing remembered, even for the
/// name she did answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc5_no_answer_writes_nothing_and_remembers_nothing() {
    let db = fresh("fc5-no-answer").await;
    let (w1, w2) = (db.docs.thread_create("Work", "").await.unwrap(), db.docs.thread_create("Work", "").await.unwrap());
    let (h1, h2) = (db.docs.thread_create("Home", "").await.unwrap(), db.docs.thread_create("Home", "").await.unwrap());
    let threads_before = db.store.list_threads().await.unwrap().len();
    let (_tmp, m) = two_folders();
    let chooser = Answers { picks: vec![("Work".into(), Some(w1.clone())), ("Home".into(), None)], asked: RefCell::default() };

    let r = execute_asking(&*db.store, &db.docs, &m, &chooser).await;
    assert!(r.is_err(), "no answer: the import does not run");
    assert_eq!(document_count(&db).await, 0, "nothing is written");
    assert_eq!(db.store.list_threads().await.unwrap().len(), threads_before, "no thread is created");
    assert_eq!(asked(choice(&db, "Work").await), sorted(vec![w1, w2]), "nothing is remembered");
    assert_eq!(asked(choice(&db, "Home").await), sorted(vec![h1, h2]));
}

/// FC5: where nobody can answer (`execute`, the bulk import run without a
/// terminal), the whole import is refused as before, and the refusal says why.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc5_where_nobody_can_answer_the_import_is_refused_and_said() {
    let db = fresh("fc5-nobody").await;
    two_web(&db).await;
    let _w = (db.docs.thread_create("Work", "").await.unwrap(), db.docs.thread_create("Work", "").await.unwrap());
    let (_tmp, m) = two_folders();
    let err = execute(&*db.store, &db.docs, &m).await.expect_err("refused").to_string();
    assert!(err.contains("Work"), "the refusal names the name: {err}");
    assert_eq!(document_count(&db).await, 0, "nothing is written");
}
