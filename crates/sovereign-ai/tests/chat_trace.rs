//! The trace of a chat turn that reached the round limit (spec ch. 08, *Chat agent loop*,
//! v2.0.35; Céline 2026-10-17): saved at once as a Sovereign document through the document
//! service — **never owned** (trust undetermined, fenced like external content, because it quotes
//! tool results), provenance *the assistant*, in a thread named "Assistant traces" created the
//! first time — and readable in a fixed order: the user's request; for each round, the tool, its
//! arguments, the result as the model saw it, and what came of it; then why the turn stopped.
//!
//! API under test (`sovereign_ai::chat_trace`): `ChatTrace::new(request)`, `push_round(TraceRound)`,
//! `stopped(reason)`, `render()`, `TraceStep`, `TRACE_THREAD`, and
//! `save_chat_trace(&DocumentService, &dyn GraphDB, &ChatTrace) -> anyhow::Result<SavedTrace>`
//! (`SavedTrace { doc_id, title }`: the new document's id and its exact title, which the turn's
//! outcome names — v2.0.39).

use std::sync::Arc;

use sovereign_ai::chat_trace::{
    save_chat_trace, ChatTrace, SavedTrace, TraceRound, TraceStep, TRACE_THREAD,
};
use sovereign_ai::docs_host::{build_document_service, testing};
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Provenance, Trust};

fn round(tool: &str, args: &str, seen: &str, step: TraceStep) -> TraceRound {
    TraceRound {
        tool: tool.to_string(),
        arguments: args.to_string(),
        result_seen: seen.to_string(),
        came_of_it: step,
    }
}

fn sample() -> ChatTrace {
    let mut t = ChatTrace::new("Tidy my research threads and summarise the open questions");
    t.push_round(round(
        "list_threads",
        "{}",
        "Research (owned), Inbox (external)",
        TraceStep::Executed,
    ));
    t.push_round(round(
        "search_documents",
        "{\"query\":\"open questions\"}",
        "Notes on [email] (owned): first lines of the note…",
        TraceStep::Executed,
    ));
    t.push_round(round(
        "move_document",
        "{\"title\":\"Notes\",\"thread\":\"Research\"}",
        "",
        TraceStep::Declined,
    ));
    t.stopped("stopped after 5 steps without a final answer");
    t
}

#[test]
fn the_trace_reads_in_order_request_rounds_then_why_it_stopped() {
    let text = sample().render();
    let at = |needle: &str| text.find(needle).unwrap_or_else(|| panic!("{needle:?} missing in:\n{text}"));
    let request = at("Tidy my research threads");
    let r1 = at("list_threads");
    let r2 = at("search_documents");
    let r3 = at("move_document");
    let why = at("stopped after 5 steps without a final answer");
    assert!(request < r1 && r1 < r2 && r2 < r3 && r3 < why, "order wrong:\n{text}");
}

#[test]
fn the_trace_keeps_what_the_model_saw_verbatim() {
    let text = sample().render();
    // Personal data stays a type label and the preview stays truncated, as the model saw it.
    assert!(text.contains("Notes on [email] (owned): first lines of the note…"), "{text}");
    assert!(text.contains("{\"query\":\"open questions\"}"), "arguments shown: {text}");
}

#[test]
fn each_kind_of_outcome_is_named() {
    let mut t = ChatTrace::new("r");
    for (i, step) in [
        TraceStep::Proposed,
        TraceStep::Approved,
        TraceStep::Declined,
        TraceStep::Executed,
        TraceStep::Failed("thread not found".into()),
    ]
    .into_iter()
    .enumerate()
    {
        t.push_round(round(&format!("tool_{i}"), "{}", "", step));
    }
    t.stopped("limit");
    let lower = t.render().to_lowercase();
    for word in ["proposed", "approved", "declined", "executed", "failed", "thread not found"] {
        assert!(lower.contains(word), "{word:?} missing:\n{lower}");
    }
}

async fn setup(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
    let store = testing::encrypted_memory_store(tag).await;
    let docs = build_document_service(store.clone(), Some(testing::account_key()), "desktop".into());
    (store, docs)
}

async fn trace_threads(store: &EncryptedGraphDB) -> Vec<String> {
    store
        .list_threads()
        .await
        .unwrap()
        .into_iter()
        .filter(|t| t.name == TRACE_THREAD)
        .filter_map(|t| t.id_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_saved_trace_is_never_owned_and_lands_in_the_traces_thread() {
    let (store, docs) = setup("chat-trace-save").await;
    assert!(trace_threads(&store).await.is_empty(), "no traces thread before the first trace");

    let SavedTrace { doc_id: id, title } =
        save_chat_trace(&docs, store.as_ref(), &sample()).await.expect("the trace is saved");

    let threads = trace_threads(&store).await;
    assert_eq!(threads.len(), 1, "the thread is created on first use");
    let view = docs.read(&id).await.expect("the trace reads back");
    assert_eq!(view.title, title, "the title returned is the document's exact title");
    assert!(title.starts_with("Assistant trace — ") && title.ends_with(" UTC"), "{title:?}");
    assert_eq!(view.thread_id, threads[0]);
    assert_eq!(view.trust, Trust::Undetermined, "never owned: it quotes tool results");
    assert_eq!(view.provenance, Provenance::Assistant);
    let text = view.plain_text();
    assert!(text.contains("Tidy my research threads"), "{text}");
    assert!(text.contains("search_documents"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn later_traces_reuse_the_same_thread() {
    let (store, docs) = setup("chat-trace-reuse").await;
    let a = save_chat_trace(&docs, store.as_ref(), &sample()).await.unwrap().doc_id;
    let b = save_chat_trace(&docs, store.as_ref(), &sample()).await.unwrap().doc_id;
    assert_ne!(a, b, "each turn gets its own document");
    assert_eq!(trace_threads(&store).await.len(), 1, "one traces thread, reused");
    assert_eq!(docs.read(&a).await.unwrap().thread_id, docs.read(&b).await.unwrap().thread_id);
}
