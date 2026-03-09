/// Event forwarding from backend channels to Tauri frontend.
///
/// The orchestrator emits `OrchestratorEvent` values on an mpsc channel.
/// This module drains that channel and re-emits them as Tauri events with
/// serializable payloads that the SvelteKit frontend can listen to.

use serde::Serialize;
use sovereign_core::interfaces::OrchestratorEvent;
use tauri::Emitter;

// ---------------------------------------------------------------------------
// Serializable event payloads
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct ChatResponsePayload {
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BubbleStatePayload {
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionProposedPayload {
    pub action: String,
    pub level: String,
    pub description: String,
    pub doc_id: Option<String>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionExecutedPayload {
    pub action: String,
    pub success: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ActionRejectedPayload {
    pub action: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultsPayload {
    pub query: String,
    pub doc_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SuggestionPayload {
    pub text: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillResultPayload {
    pub skill: String,
    pub action: String,
    pub kind: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentCreatedPayload {
    pub doc_id: String,
    pub title: String,
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GenericPayload {
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadRenamedPayload {
    pub thread_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadDeletedPayload {
    pub thread_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DocumentMovedPayload {
    pub doc_id: String,
    pub new_thread_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct NewMessagesPayload {
    pub channel: String,
    pub count: u32,
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactCreatedPayload {
    pub contact_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InjectionDetectedPayload {
    pub source: String,
    pub indicators: Vec<String>,
    pub severity: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct BrowserNavigatedPayload {
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReliabilityAssessedPayload {
    pub doc_id: String,
    pub classification: String,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadMergedPayload {
    pub target_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ThreadSplitPayload {
    pub new_thread_id: String,
    pub name: String,
    pub doc_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestedPayload {
    pub suggestion_id: String,
    pub from_doc_id: String,
    pub from_title: String,
    pub to_doc_id: String,
    pub to_title: String,
    pub relation_type: String,
    pub strength: f32,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkSuggestionResolvedPayload {
    pub suggestion_id: String,
    pub accepted: bool,
}

// ---------------------------------------------------------------------------
// Event forwarder
// ---------------------------------------------------------------------------

/// Spawn a background thread that forwards orchestrator events to the Tauri
/// frontend via `app_handle.emit()`.
#[cfg(feature = "tauri-ui")]
pub fn spawn_event_forwarder(
    app_handle: tauri::AppHandle,
    orch_rx: std::sync::mpsc::Receiver<OrchestratorEvent>,
) {
    std::thread::spawn(move || {
        while let Ok(event) = orch_rx.recv() {
            match event {
                OrchestratorEvent::ChatResponse { text } => {
                    let _ = app_handle.emit("chat-response", ChatResponsePayload { text });
                }

                OrchestratorEvent::BubbleState(state) => {
                    let state_str = format!("{:?}", state);
                    let _ = app_handle
                        .emit("bubble-state", BubbleStatePayload { state: state_str });
                }

                OrchestratorEvent::ActionProposed { proposal } => {
                    let _ = app_handle.emit(
                        "action-proposed",
                        ActionProposedPayload {
                            action: proposal.action,
                            level: format!("{:?}", proposal.level),
                            description: proposal.description,
                            doc_id: proposal.doc_id,
                            thread_id: proposal.thread_id,
                        },
                    );
                }

                OrchestratorEvent::ActionExecuted { action, success } => {
                    let _ = app_handle.emit(
                        "action-executed",
                        ActionExecutedPayload { action, success },
                    );
                }

                OrchestratorEvent::ActionRejected { action, reason } => {
                    let _ = app_handle.emit(
                        "action-rejected",
                        ActionRejectedPayload { action, reason },
                    );
                }

                OrchestratorEvent::SearchResults { query, doc_ids } => {
                    let _ = app_handle.emit(
                        "search-results",
                        SearchResultsPayload { query, doc_ids },
                    );
                }

                OrchestratorEvent::Suggestion { text, action } => {
                    let _ =
                        app_handle.emit("suggestion", SuggestionPayload { text, action });
                }

                OrchestratorEvent::SkillResult {
                    skill,
                    action,
                    kind,
                    data,
                } => {
                    let _ = app_handle.emit(
                        "skill-result",
                        SkillResultPayload {
                            skill,
                            action,
                            kind,
                            data,
                        },
                    );
                }

                OrchestratorEvent::DocumentCreated {
                    doc_id,
                    title,
                    thread_id,
                } => {
                    let _ = app_handle.emit(
                        "document-created",
                        DocumentCreatedPayload {
                            doc_id,
                            title,
                            thread_id,
                        },
                    );
                }

                OrchestratorEvent::DocumentOpened { doc_id } => {
                    let _ = app_handle.emit(
                        "document-opened",
                        GenericPayload {
                            message: doc_id,
                        },
                    );
                }

                OrchestratorEvent::ThreadCreated { thread_id, name } => {
                    let _ = app_handle.emit(
                        "thread-created",
                        GenericPayload {
                            message: format!("{thread_id}:{name}"),
                        },
                    );
                }

                OrchestratorEvent::InjectionDetected { source, indicators, severity, .. } => {
                    let _ = app_handle.emit(
                        "injection-detected",
                        InjectionDetectedPayload {
                            source,
                            indicators,
                            severity,
                        },
                    );
                }

                // Phase 3: Thread operations
                OrchestratorEvent::ThreadRenamed { thread_id, name } => {
                    let _ = app_handle.emit(
                        "thread-renamed",
                        ThreadRenamedPayload { thread_id, name },
                    );
                }

                OrchestratorEvent::ThreadDeleted { thread_id } => {
                    let _ = app_handle.emit(
                        "thread-deleted",
                        ThreadDeletedPayload { thread_id },
                    );
                }

                OrchestratorEvent::DocumentMoved { doc_id, new_thread_id } => {
                    let _ = app_handle.emit(
                        "document-moved",
                        DocumentMovedPayload { doc_id, new_thread_id },
                    );
                }

                OrchestratorEvent::ThreadMerged { target_id, source_id } => {
                    let _ = app_handle.emit(
                        "thread-merged",
                        ThreadMergedPayload { target_id, source_id },
                    );
                }

                OrchestratorEvent::ThreadSplit { new_thread_id, name, doc_ids } => {
                    let _ = app_handle.emit(
                        "thread-split",
                        ThreadSplitPayload { new_thread_id, name, doc_ids },
                    );
                }

                // Phase 3: Communications
                OrchestratorEvent::NewMessagesReceived { channel, count, conversation_id } => {
                    let _ = app_handle.emit(
                        "new-messages",
                        NewMessagesPayload { channel, count, conversation_id },
                    );
                }

                OrchestratorEvent::ContactCreated { contact_id, name } => {
                    let _ = app_handle.emit(
                        "contact-created",
                        ContactCreatedPayload { contact_id, name },
                    );
                }

                // Web browsing events
                OrchestratorEvent::BrowserNavigated { url, title } => {
                    let _ = app_handle.emit(
                        "browser-navigated",
                        BrowserNavigatedPayload { url, title },
                    );
                }

                OrchestratorEvent::BrowserContentExtracted { url, title, .. } => {
                    let _ = app_handle.emit(
                        "browser-content-extracted",
                        BrowserNavigatedPayload { url, title },
                    );
                }

                OrchestratorEvent::ReliabilityAssessed { doc_id, classification, score } => {
                    let _ = app_handle.emit(
                        "reliability-assessed",
                        ReliabilityAssessedPayload { doc_id, classification, score },
                    );
                }

                // Memory consolidation events
                OrchestratorEvent::LinkSuggested {
                    suggestion_id, from_doc_id, from_title,
                    to_doc_id, to_title, relation_type, strength, rationale,
                } => {
                    let _ = app_handle.emit(
                        "link-suggested",
                        LinkSuggestedPayload {
                            suggestion_id, from_doc_id, from_title,
                            to_doc_id, to_title, relation_type, strength, rationale,
                        },
                    );
                }

                OrchestratorEvent::LinkSuggestionResolved { suggestion_id, accepted } => {
                    let _ = app_handle.emit(
                        "link-suggestion-resolved",
                        LinkSuggestionResolvedPayload { suggestion_id, accepted },
                    );
                }

                // All other events: log but don't emit
                other => {
                    tracing::debug!("Unhandled orchestrator event: {:?}", other);
                }
            }
        }
        tracing::info!("Event forwarder stopped (channel closed)");
    });
}
