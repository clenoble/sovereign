//! The trace of a chat turn that reached the round limit (spec ch. 08, *Chat agent loop*,
//! v2.0.35; Céline 2026-10-17).
//!
//! The chat loop records every round it runs; when the turn stops at the round limit, the
//! trace is saved at once as a Sovereign document through the document service, so it is
//! encrypted at rest and personal-data ingest runs on it. Because it quotes tool results,
//! which can carry external content, it is **never owned** (trust undetermined) and its
//! provenance is the assistant. It goes to the thread [`TRACE_THREAD`], created the first
//! time. No trace is saved for a turn that ends any other way.

use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Origin, Provenance, Trust};

/// The thread every trace goes to, created the first time a trace is saved.
pub const TRACE_THREAD: &str = "Assistant traces";

/// What came of one round's tool call.
#[derive(Debug, Clone, PartialEq)]
pub enum TraceStep {
    /// Proposed to the user, with no decision recorded.
    Proposed,
    /// Proposed, approved by the user, and executed.
    Approved,
    /// Proposed and declined by the user (or not confirmed in time).
    Declined,
    /// Executed without a proposal (a read tool, or a write run without confirmation).
    Executed,
    /// Executed and failed, with the reason.
    Failed(String),
}

impl TraceStep {
    fn describe(&self) -> String {
        match self {
            TraceStep::Proposed => "Proposed to you; no decision was recorded.".to_string(),
            TraceStep::Approved => "Proposed to you, approved, and executed.".to_string(),
            TraceStep::Declined => "Proposed to you and declined.".to_string(),
            TraceStep::Executed => "Executed.".to_string(),
            TraceStep::Failed(reason) => format!("Failed: {reason}"),
        }
    }
}

/// One round of the turn: the tool called, its arguments, the result as the model saw it,
/// and what came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct TraceRound {
    pub tool: String,
    pub arguments: String,
    /// The tool result exactly as it was fed back to the model (fenced, personal data as
    /// type labels, previews truncated). Kept verbatim.
    pub result_seen: String,
    pub came_of_it: TraceStep,
}

/// The trace of one chat turn: the user's request, its rounds, and why it stopped.
#[derive(Debug, Clone, PartialEq)]
pub struct ChatTrace {
    request: String,
    rounds: Vec<TraceRound>,
    stop_reason: Option<String>,
}

impl ChatTrace {
    pub fn new(request: &str) -> Self {
        Self { request: request.to_string(), rounds: Vec::new(), stop_reason: None }
    }

    pub fn push_round(&mut self, round: TraceRound) {
        self.rounds.push(round);
    }

    /// Records why the turn stopped.
    pub fn stopped(&mut self, reason: &str) {
        self.stop_reason = Some(reason.to_string());
    }

    /// Markdown, in this order: the request; each round (tool, arguments, result as the
    /// model saw it, what came of it); why the turn stopped.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("## Your request\n\n");
        out.push_str(&self.request);
        out.push_str("\n\n");
        if self.rounds.is_empty() {
            out.push_str("## Steps\n\nNo tool was called.\n\n");
        }
        for (i, r) in self.rounds.iter().enumerate() {
            out.push_str(&format!("## Step {}: {}\n\n", i + 1, r.tool));
            out.push_str("Arguments:\n\n```\n");
            out.push_str(&r.arguments);
            out.push_str("\n```\n\n");
            out.push_str("Result, as the assistant saw it:\n\n```\n");
            out.push_str(&r.result_seen);
            out.push_str("\n```\n\n");
            out.push_str("What came of it: ");
            out.push_str(&r.came_of_it.describe());
            out.push_str("\n\n");
        }
        out.push_str("## Why the turn stopped\n\n");
        out.push_str(self.stop_reason.as_deref().unwrap_or("No reason was recorded."));
        out.push('\n');
        out
    }
}

/// A saved trace: the new document's id and its exact title, which the turn's outcome
/// names (spec ch. 08, *Chat agent loop*, v2.0.39).
#[derive(Debug, Clone, PartialEq)]
pub struct SavedTrace {
    pub doc_id: String,
    pub title: String,
}

/// Saves `trace` as a never-owned document in the [`TRACE_THREAD`] thread (created if
/// absent) and returns the new document's id and title.
pub async fn save_chat_trace(
    docs: &DocumentService,
    store: &dyn GraphDB,
    trace: &ChatTrace,
) -> anyhow::Result<SavedTrace> {
    let thread_id = trace_thread_id(docs, store).await?;
    let title = format!("Assistant trace — {} UTC", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"));
    let new = crate::docs_host::new_document(
        title.clone(),
        thread_id,
        trace.render(),
        Trust::Undetermined,
        Provenance::Assistant,
        Origin::Assistant,
    );
    let out = crate::docs_host::create(docs, new)
        .await
        .map_err(|e| anyhow::anyhow!("{}", crate::docs_host::doc_error_message(&e)))?;
    Ok(SavedTrace { doc_id: out.id, title })
}

/// The id of the active thread named exactly [`TRACE_THREAD`], created if there is none
/// (through the document service's one thread-write path, stamped: THREAD-sync S1).
async fn trace_thread_id(docs: &DocumentService, store: &dyn GraphDB) -> anyhow::Result<String> {
    let existing = store
        .list_threads()
        .await
        .map_err(|e| anyhow::anyhow!("could not list threads: {e}"))?
        .into_iter()
        .find(|t| t.deleted_at.is_none() && t.name == TRACE_THREAD);
    match existing {
        Some(t) => t.id_string().ok_or_else(|| anyhow::anyhow!("the traces thread has no id")),
        None => docs
            .thread_create(TRACE_THREAD, "")
            .await
            .map_err(|e| anyhow::anyhow!("could not create the traces thread: {e}")),
    }
}
