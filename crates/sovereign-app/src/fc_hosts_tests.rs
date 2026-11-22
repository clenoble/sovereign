//! Two threads of the name sought, in the app (spec ch. 03, *Two threads of the
//! name sought: asked once, remembered*, FC1, FC2 and FC6; v0.0.13, item 9):
//! the browser's « Web » thread and the daily journal's « Journal », through
//! the layer under the commands (`crate::thread_choice`).
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. Stores: a real encrypted store over an in-memory SurrealDB
//! (`docs_host::testing`), with its document service. The screens' side (the
//! window that asks) is tested by vitest.

use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, new_user_document, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::GraphDB;
use sovereign_docs::DocumentService;

use crate::thread_choice::{execute_skill_on, remember_choice, thread_for_name, web_thread, ThreadForNameDto};

struct Fixture {
    store: Arc<EncryptedGraphDB>,
    docs: Arc<DocumentService>,
}

async fn fixture(tag: &str) -> Fixture {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = Arc::new(build_document_service(store.clone(), Some(testing::account_key()), "phone".into()));
    Fixture { store, docs }
}

impl Fixture {
    async fn thread(&self, name: &str) -> String {
        self.docs.thread_create(name, "").await.unwrap()
    }

    async fn doc_in(&self, thread_id: &str, title: &str) -> String {
        self.docs.create(new_user_document(title, thread_id, format!("{title} text"))).await.unwrap().id
    }

    async fn named(&self, name: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .store
            .list_threads()
            .await
            .unwrap()
            .into_iter()
            .filter(|t| t.deleted_at.is_none() && t.name == name)
            .filter_map(|t| t.id_string())
            .collect();
        v.sort();
        v
    }

    async fn documents(&self) -> usize {
        self.store.list_documents(None).await.unwrap().len()
    }

    async fn journal_today(&self, on_doc: &str) -> Result<crate::tauri_commands::SkillResultDto, String> {
        let mut registry = sovereign_skills::SkillRegistry::new();
        registry.register(Box::new(sovereign_skills::skills::daily_journal::DailyJournalSkill));
        let inner = sovereign_skills::wrap_db(self.store.clone());
        execute_skill_on(self.docs.clone(), &registry, inner, None, "daily-journal", "today", on_doc, "").await
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// FC1: no live thread of the name creates; one is used, asking nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_none_creates_and_one_is_used() {
    let f = fixture("fc-app-none-one").await;
    assert!(matches!(thread_for_name(&f.docs, "Web").await.unwrap(), ThreadForNameDto::Create));
    let web = f.thread("Web").await;
    match thread_for_name(&f.docs, "Web").await.unwrap() {
        ThreadForNameDto::Use { id } => assert_eq!(id, web),
        other => panic!("one live « Web »: it is used, got {other:?}"),
    }
}

/// FC1: two live threads of the name ask, listing each with its number of
/// documents and its creation time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_two_threads_ask_listing_each_with_its_documents_and_creation() {
    let f = fixture("fc-app-ask").await;
    let a = f.thread("Web").await;
    let b = f.thread("Web").await;
    f.doc_in(&a, "One").await;
    f.doc_in(&a, "Two").await;
    let ThreadForNameDto::Ask { name, threads } = thread_for_name(&f.docs, "Web").await.unwrap() else {
        panic!("two live « Web »: she is asked");
    };
    assert_eq!(name, "Web");
    assert_eq!(sorted(threads.iter().map(|t| t.id.clone()).collect()), sorted(vec![a.clone(), b.clone()]));
    for t in &threads {
        let want_docs = if t.id == a { 2 } else { 0 };
        assert_eq!(t.documents, want_docs, "the number of documents of {}", t.id);
        assert_eq!(t.name, "Web");
        let stored = f.store.get_thread(&t.id).await.unwrap();
        let shown = chrono::DateTime::parse_from_rfc3339(&t.created_at).expect("an RFC 3339 time").with_timezone(&chrono::Utc);
        assert_eq!(shown.timestamp(), stored.created_at.timestamp(), "its creation time");
    }
}

/// FC2: once she answered, the name is used without asking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_a_remembered_answer_is_used() {
    let f = fixture("fc-app-remember").await;
    let _a = f.thread("Web").await;
    let b = f.thread("Web").await;
    remember_choice(&f.docs, "Web", &b).await.expect("her answer is remembered");
    match thread_for_name(&f.docs, "Web").await.unwrap() {
        ThreadForNameDto::Use { id } => assert_eq!(id, b),
        other => panic!("a remembered choice is used, got {other:?}"),
    }
}

/// The browser: a thread the screen gave is used as it is.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_a_thread_given_is_used() {
    let f = fixture("fc-web-given").await;
    let t = f.thread("Reading").await;
    assert_eq!(web_thread(&f.docs, Some(t.clone())).await.unwrap(), t);
    assert!(f.named("Web").await.is_empty(), "no « Web » is made for a page filed elsewhere");
}

/// The browser: no « Web » creates one, and the next page goes to it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn web_none_creates_one_then_uses_it() {
    let f = fixture("fc-web-create").await;
    let first = web_thread(&f.docs, None).await.unwrap();
    let second = web_thread(&f.docs, None).await.unwrap();
    assert_eq!(first, second);
    assert_eq!(f.named("Web").await, vec![first]);
}

/// FC6: two « Web » and no answer: a save nobody asked about is refused with
/// the sentence, and creates nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc6_web_two_threads_and_no_answer_refuse_and_create_nothing() {
    let f = fixture("fc-web-refuse").await;
    let a = f.thread("Web").await;
    let b = f.thread("Web").await;
    let err = web_thread(&f.docs, None).await.expect_err("no thread is picked for her");
    assert!(err.contains("2 threads are named 'Web'") && err.contains("none was picked"), "{err}");
    assert_eq!(f.named("Web").await, sorted(vec![a, b]), "nothing is created");
}

/// FC2: after her answer, pages go to the « Web » chosen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_web_pages_go_to_the_thread_chosen() {
    let f = fixture("fc-web-chosen").await;
    let a = f.thread("Web").await;
    let _b = f.thread("Web").await;
    remember_choice(&f.docs, "Web", &a).await.unwrap();
    assert_eq!(web_thread(&f.docs, None).await.unwrap(), a);
}

/// FC1 through a skill: the journal with two « Journal » answers the question
/// (`ask_thread`, the question as JSON) and writes nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_the_journal_answers_the_question_and_writes_nothing() {
    let f = fixture("fc-journal-ask").await;
    let notes = f.thread("Notes").await;
    let on = f.doc_in(&notes, "Open").await;
    let a = f.thread("Journal").await;
    let b = f.thread("Journal").await;
    let before = f.documents().await;
    let out = f.journal_today(&on).await.expect("the question is an answer, not an error");
    assert_eq!(out.kind, "ask_thread");
    let q: serde_json::Value = serde_json::from_str(out.structured_json.as_deref().expect("the question")).unwrap();
    assert_eq!(q["kind"], "ask");
    assert_eq!(q["name"], "Journal");
    let ids: Vec<String> = q["threads"].as_array().unwrap().iter().map(|t| t["id"].as_str().unwrap().to_string()).collect();
    assert_eq!(sorted(ids), sorted(vec![a, b]));
    assert_eq!(f.documents().await, before, "no entry is written before she answers");
}

/// FC2 through a skill: once she answered, the journal runs again and files
/// its entry in the thread chosen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_the_journal_runs_again_into_the_thread_chosen() {
    let f = fixture("fc-journal-chosen").await;
    let notes = f.thread("Notes").await;
    let on = f.doc_in(&notes, "Open").await;
    let _a = f.thread("Journal").await;
    let b = f.thread("Journal").await;
    remember_choice(&f.docs, "Journal", &b).await.unwrap();
    let out = f.journal_today(&on).await.expect("the journal runs");
    assert_eq!(out.kind, "structured_data");
    let v: serde_json::Value = serde_json::from_str(out.structured_json.as_deref().unwrap()).unwrap();
    assert_eq!(v["thread_id"].as_str(), Some(b.as_str()));
}
