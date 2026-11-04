//! Test harness for the v0.0.12 assistant lot (items 6, 11, 13;
//! `doc/plans/V0012-lot-assistant.md`): an orchestrator over an encrypted
//! in-memory store and a real document service, with a **fake model** that
//! records every call (its input and its answer budget) and answers by script.
//!
//! Test-only: declared under `#[cfg(test)]` from `orchestrator.rs`. Uses the
//! test-only seam `AsyncLlmBackend::fake` (`llm/async_backend.rs`), which
//! answers `generate` without llama.cpp.

use super::*;
use crate::docs_host::testing::{account_key, encrypted_memory_store};
use crate::llm::async_backend::AsyncLlmBackend;
use sovereign_db::encrypted::EncryptedGraphDB;

/// One call the model received.
#[derive(Debug, Clone)]
pub(super) struct Call {
    pub prompt: String,
    pub max_tokens: u32,
}

/// What happened, in order: an event the orchestrator emitted, or a model call
/// (its index in `calls`). Events are pumped into the log at each model call,
/// so an event emitted before a call is logged before it.
#[derive(Debug, Clone)]
#[allow(dead_code)] // the call index is for reading a failing log
pub(super) enum Logged {
    Event(OrchestratorEvent),
    Call(usize),
}

/// Records events and model calls, interleaved.
pub(super) struct Recorder {
    rx: Mutex<std::sync::mpsc::Receiver<OrchestratorEvent>>,
    log: Mutex<Vec<Logged>>,
    calls: Mutex<Vec<Call>>,
    /// How much of `log` `take_events` has already returned.
    cursor: Mutex<usize>,
}

impl Recorder {
    fn new(rx: std::sync::mpsc::Receiver<OrchestratorEvent>) -> Self {
        Self { rx: Mutex::new(rx), log: Mutex::new(vec![]), calls: Mutex::new(vec![]), cursor: Mutex::new(0) }
    }

    fn pump(&self) {
        let rx = self.rx.lock().unwrap();
        let mut log = self.log.lock().unwrap();
        while let Ok(e) = rx.try_recv() {
            log.push(Logged::Event(e));
        }
    }

    fn record_call(&self, prompt: &str, max_tokens: u32) -> usize {
        self.pump();
        let mut calls = self.calls.lock().unwrap();
        calls.push(Call { prompt: prompt.to_string(), max_tokens });
        let i = calls.len() - 1;
        self.log.lock().unwrap().push(Logged::Call(i));
        i
    }

    /// The events emitted since the last `take_events`, in order.
    pub fn take_events(&self) -> Vec<OrchestratorEvent> {
        self.pump();
        let log = self.log.lock().unwrap();
        let mut cursor = self.cursor.lock().unwrap();
        let out = log[*cursor..]
            .iter()
            .filter_map(|l| match l {
                Logged::Event(e) => Some(e.clone()),
                Logged::Call(_) => None,
            })
            .collect();
        *cursor = log.len();
        out
    }

    /// Everything logged so far (events and calls), in order.
    pub fn log(&self) -> Vec<Logged> {
        self.pump();
        self.log.lock().unwrap().clone()
    }

    /// Every model call so far.
    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }
}

/// A scripted model: `(prompt, max_tokens, call index) -> answer`.
pub(super) type Script = Arc<dyn Fn(&str, u32, usize) -> String + Send + Sync>;

/// A script answering every call with `text`.
pub(super) fn answer(text: &str) -> Script {
    let t = text.to_string();
    Arc::new(move |_, _, _| t.clone())
}

/// The router's answer classifying a query as `action` on `target`, with a
/// confidence above the escalation threshold (no reasoning model is needed).
pub(super) fn intent_json(action: &str, target: Option<&str>) -> String {
    serde_json::json!({ "action": action, "target": target, "confidence": 0.95 }).to_string()
}

pub(super) struct World {
    pub store: Arc<EncryptedGraphDB>,
    pub docs: Arc<sovereign_docs::DocumentService>,
    pub orch: Orchestrator,
    pub rec: Arc<Recorder>,
    pub n_ctx: u32,
}

/// An orchestrator over a fresh encrypted store and a document service built
/// with the account key (so documents created through it are scanned for
/// personal data, PII-002), with a fake model of window `n_ctx` answering by
/// `script`. With `with_service == false` no document service is installed
/// (a locked workspace).
pub(super) async fn world_with(tag: &str, n_ctx: u32, script: Script, with_service: bool) -> World {
    let store = encrypted_memory_store(tag).await;
    let docs = Arc::new(crate::docs_host::build_document_service(store.clone(), Some(account_key()), "t".into()));
    let (tx, rx) = std::sync::mpsc::channel();
    let dir = std::env::temp_dir().join(format!("sovereign_test_assistant_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut orch = Orchestrator::for_test(store.clone() as Arc<dyn GraphDB>, tx, dir);
    let rec = Arc::new(Recorder::new(rx));
    let config = AiConfig { n_ctx, ..AiConfig::default() };
    let mut classifier = IntentClassifier::new(config);
    let r = rec.clone();
    classifier.router = AsyncLlmBackend::fake(
        n_ctx,
        Arc::new(move |prompt: &str, max_tokens: u32| {
            let i = r.record_call(prompt, max_tokens);
            Ok(script(prompt, max_tokens, i))
        }),
    );
    orch.classifier = tokio::sync::Mutex::new(classifier);
    if with_service {
        orch.set_document_service(docs.clone());
    }
    World { store, docs, orch, rec, n_ctx }
}

pub(super) async fn world(tag: &str, n_ctx: u32, script: Script) -> World {
    world_with(tag, n_ctx, script, true).await
}

impl World {
    /// Creates a live thread through the stamped path; returns its id.
    pub async fn thread(&self, name: &str) -> String {
        self.docs.thread_create(name, "").await.unwrap()
    }

    /// Creates the user's own document in `thread` (scanned); returns its id.
    pub async fn doc(&self, title: &str, thread: &str, text: &str) -> String {
        crate::docs_host::create(&self.docs, crate::docs_host::new_user_document(title, thread, text))
            .await
            .unwrap()
            .id
    }

    /// Creates `n` documents in `thread`, titled `<prefix> 1..n`.
    pub async fn docs_in(&self, thread: &str, prefix: &str, n: usize) -> Vec<String> {
        let mut ids = vec![];
        for i in 1..=n {
            ids.push(self.doc(&format!("{prefix} {i}"), thread, &format!("text {i}")).await);
        }
        ids
    }

    /// The thread record as stored (live or deleted).
    pub async fn thread_row(&self, id: &str) -> Thread {
        self.store.get_thread(id).await.unwrap()
    }

    pub async fn is_live(&self, thread_id: &str) -> bool {
        self.thread_row(thread_id).await.deleted_at.is_none()
    }

    /// Ids of the live threads named exactly `name`.
    pub async fn live_threads_named(&self, name: &str) -> Vec<String> {
        self.store
            .list_threads()
            .await
            .unwrap()
            .into_iter()
            .filter(|t| t.deleted_at.is_none() && t.name == name)
            .filter_map(|t| t.id_string())
            .collect()
    }

    pub async fn thread_of(&self, doc_id: &str) -> String {
        self.store.get_document(doc_id).await.unwrap().thread_id
    }

    pub async fn trashed(&self, doc_id: &str) -> bool {
        self.docs.read(doc_id).await.unwrap().trashed_at.is_some()
    }

    /// Runs `action` on `target` straight through `execute_action` (the gate
    /// is tested separately) and returns the events it emitted.
    pub async fn act(&self, action: &str, target: Option<&str>) -> Vec<OrchestratorEvent> {
        self.rec.take_events();
        self.orch.execute_action(action, target, &format!("{action} {}", target.unwrap_or(""))).await.unwrap();
        self.rec.take_events()
    }
}

/// The `success` of every `ActionExecuted` among `events`.
pub(super) fn executed(events: &[OrchestratorEvent]) -> Vec<bool> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionExecuted { success, .. } => Some(*success),
            _ => None,
        })
        .collect()
}

/// What the user is told about an action: every chat message, every refusal,
/// and the action's executed event as a whole (its outcome, once the event
/// carries one). Read through `Debug` so the text an event carries counts
/// wherever the implementation puts it, without naming a type that does not
/// exist yet. Events that only move the UI (a thread renamed, results listed)
/// are not part of the reply.
pub(super) fn reply(events: &[OrchestratorEvent]) -> String {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatResponse { text } => Some(text.clone()),
            OrchestratorEvent::ActionRejected { .. } | OrchestratorEvent::ActionExecuted { .. } => {
                Some(format!("{e:?}"))
            }
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The proposals among `events`.
pub(super) fn proposals(events: &[OrchestratorEvent]) -> Vec<ProposedAction> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionProposed { proposal } => Some(proposal.clone()),
            _ => None,
        })
        .collect()
}

/// A comparable picture of everything an assistant action could change in the
/// store: threads (name, live), documents (title, thread, owned, trashed,
/// text, number of versions) and milestones. No timestamps: a write that
/// changes nothing a user can see is not a change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Snapshot {
    pub threads: Vec<(String, String, bool)>,
    pub documents: Vec<(String, String, String, bool, bool, String, usize)>,
    pub milestones: Vec<(String, String, String)>,
}

pub(super) async fn snapshot(store: &EncryptedGraphDB) -> Snapshot {
    let mut threads: Vec<(String, String, bool)> = store
        .list_threads()
        .await
        .unwrap()
        .into_iter()
        .map(|t| (t.id_string().unwrap_or_default(), t.name.clone(), t.deleted_at.is_none()))
        .collect();
    threads.sort();
    let mut documents = vec![];
    for d in store.list_documents_including_deleted().await.unwrap() {
        let id = d.id_string().unwrap_or_default();
        let versions = store.list_document_commits(&id).await.map(|c| c.len()).unwrap_or(0);
        documents.push((id, d.title.clone(), d.thread_id.clone(), d.is_owned, d.deleted_at.is_some(), d.content.clone(), versions));
    }
    documents.sort();
    let mut milestones = vec![];
    for (tid, _, _) in &threads {
        for m in store.list_milestones(tid).await.unwrap_or_default() {
            milestones.push((m.id_string().unwrap_or_default(), m.title.clone(), tid.clone()));
        }
    }
    milestones.sort();
    Snapshot { threads, documents, milestones }
}

/// Says "Done" (the word, or the check mark the UIs use for it).
pub(super) fn says_done(s: &str) -> bool {
    s.contains("Done") || s.contains('✓')
}
