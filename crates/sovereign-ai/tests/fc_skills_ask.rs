//! Two threads of the name sought, through the skills (spec ch. 03, *Two
//! threads of the name sought: asked once, remembered*, FC1 and FC2; v0.0.13,
//! item 9): the daily journal finds or creates its « Journal » thread through
//! the store access a host gives a skill (`ServiceSkillDb`).
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. When two live threads have the name and no choice holds, the
//! call creates nothing and its error is an `AskWhichThread` the host can
//! downcast, listing them; once she answered (`remember_thread_choice`), the
//! same call uses the thread chosen. The stores are real: an encrypted store
//! over an in-memory SurrealDB (`docs_host::testing`).

use std::sync::Arc;

use sovereign_ai::docs_host::{build_document_service, testing, AskWhichThread, ServiceSkillDb};
use sovereign_db::GraphDB;
use sovereign_docs::DocumentService;
use sovereign_skills::traits::{SkillContext, SkillDocument, SkillOutput};
use sovereign_skills::SkillDbAccess;

struct Fixture {
    store: Arc<sovereign_db::encrypted::EncryptedGraphDB>,
    docs: Arc<DocumentService>,
}

async fn fixture(tag: &str) -> Fixture {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = Arc::new(build_document_service(store.clone(), Some(testing::account_key()), "laptop".into()));
    Fixture { store, docs }
}

impl Fixture {
    async fn thread(&self, name: &str) -> String {
        self.docs.thread_create(name, "").await.unwrap()
    }

    fn skill_db(&self) -> ServiceSkillDb {
        ServiceSkillDb::new(sovereign_skills::wrap_db(self.store.clone()), self.docs.clone(), "daily-journal")
    }

    /// The live threads of `name`, by id, sorted.
    async fn named(&self, name: &str) -> Vec<String> {
        let mut v: Vec<String> = GraphDB::list_threads(self.store.as_ref())
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
        GraphDB::list_documents(self.store.as_ref(), None).await.unwrap().len()
    }

    /// The daily journal's « today », as a host runs it.
    fn journal_today(&self) -> anyhow::Result<SkillOutput> {
        let mut registry = sovereign_skills::SkillRegistry::new();
        registry.register(Box::new(sovereign_skills::skills::daily_journal::DailyJournalSkill));
        let db: Arc<dyn SkillDbAccess> = Arc::new(self.skill_db());
        let ctx = SkillContext {
            granted: [sovereign_skills::manifest::Capability::WriteAllDocuments].into_iter().collect(),
            db: Some(db),
            llm: None,
        };
        let doc = SkillDocument { id: String::new(), title: String::new(), content: Default::default() };
        registry.execute_skill("daily-journal", "today", &doc, "", &ctx)
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// FC1: two live threads of the name, no choice: the call is a question,
/// listing both, and creates nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_two_threads_of_the_name_ask_and_create_nothing() {
    let f = fixture("fc1-ask").await;
    let a = f.thread("Journal").await;
    let b = f.thread("Journal").await;
    let err = f.skill_db().find_or_create_thread("Journal", "Daily journal").expect_err("two threads: she is asked");
    let ask = err.downcast_ref::<AskWhichThread>().expect("the error is the question, for the host to ask");
    assert_eq!(ask.name, "Journal");
    assert_eq!(sorted(ask.threads.iter().filter_map(|t| t.id_string()).collect()), sorted(vec![a.clone(), b.clone()]));
    assert_eq!(f.named("Journal").await, sorted(vec![a, b]), "nothing is created while she has not answered");
}

/// FC2: once she answered, the same call uses the thread chosen, without
/// asking.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_once_answered_the_call_uses_the_thread_chosen() {
    let f = fixture("fc2-use").await;
    let _a = f.thread("Journal").await;
    let b = f.thread("Journal").await;
    sovereign_db::remember_thread_choice(f.store.as_ref(), "Journal", &b).await.expect("her answer is remembered");
    let got = f.skill_db().find_or_create_thread("Journal", "Daily journal").expect("a remembered choice is used");
    assert_eq!(got, b);
}

/// One live thread of the name is used, none is created: asking nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_thread_is_used_and_none_creates() {
    let f = fixture("fc1-one").await;
    let a = f.thread("Journal").await;
    assert_eq!(f.skill_db().find_or_create_thread("Journal", "").unwrap(), a);
    let g = fixture("fc1-none").await;
    let made = g.skill_db().find_or_create_thread("Journal", "").unwrap();
    assert_eq!(g.named("Journal").await, vec![made]);
}

/// FC1 through the skill: the daily journal's « today » with two « Journal »
/// threads is the question, from the skill's error, and writes no entry.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc1_the_journal_asks_and_writes_nothing() {
    let f = fixture("fc1-journal").await;
    let _a = f.thread("Journal").await;
    let _b = f.thread("Journal").await;
    let before = f.documents().await;
    let err = f.journal_today().expect_err("the journal cannot pick");
    assert!(err.downcast_ref::<AskWhichThread>().is_some(), "the skill's error is the question: {err:#}");
    assert_eq!(f.documents().await, before, "no entry is written before she answers");
}

/// FC2 through the skill: after her answer, the journal's entry goes to the
/// thread chosen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc2_the_journal_writes_into_the_thread_chosen() {
    let f = fixture("fc2-journal").await;
    let _a = f.thread("Journal").await;
    let b = f.thread("Journal").await;
    sovereign_db::remember_thread_choice(f.store.as_ref(), "Journal", &b).await.unwrap();
    let out = f.journal_today().expect("the journal runs");
    let SkillOutput::StructuredData { json, .. } = out else { panic!("the journal's entry") };
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["thread_id"].as_str(), Some(b.as_str()), "the entry is filed in the thread chosen");
}

/// FC6: a host that cannot ask shows the refusal's sentence (the error's
/// text), naming the name, the number of threads, and that none was picked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fc6_the_question_reads_as_the_refusal_where_nobody_can_answer() {
    let f = fixture("fc6-text").await;
    f.thread("Journal").await;
    f.thread("Journal").await;
    let err = f.skill_db().find_or_create_thread("Journal", "").unwrap_err();
    let said = err.to_string();
    assert!(said.contains("2 threads are named 'Journal'") && said.contains("none was picked"), "{said}");
}
