use serde::Serialize;
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_core::security::ActionDecision;
use sovereign_db::GraphDB;
use tauri::State;

use crate::tauri_state::AppState;

// ---------------------------------------------------------------------------
// DTOs (serializable types returned to the frontend)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct AppStatus {
    pub documents: usize,
    pub threads: usize,
    pub contacts: usize,
    pub orchestrator_available: bool,
}

#[derive(Serialize)]
pub struct DocSummary {
    pub id: String,
    pub title: String,
    pub thread_id: String,
    pub is_owned: bool,
    pub modified_at: String,
}

#[derive(Serialize)]
pub struct ThreadSummary {
    pub id: String,
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct SearchHit {
    pub id: String,
    pub title: String,
    pub snippet: String,
}

// ---------------------------------------------------------------------------
// Health / status
// ---------------------------------------------------------------------------

/// Health check command — verifies the backend is reachable.
#[tauri::command]
pub async fn greet(name: String) -> String {
    format!("Hello from Sovereign GE, {}!", name)
}

/// Return summary stats about the loaded data.
#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<AppStatus, String> {
    let docs = state.db.list_documents(None).await.map_err(|e| e.to_string())?;
    let threads = state.db.list_threads().await.map_err(|e| e.to_string())?;
    let contacts = state.db.list_contacts().await.map_err(|e| e.to_string())?;

    Ok(AppStatus {
        documents: docs.len(),
        threads: threads.len(),
        contacts: contacts.len(),
        orchestrator_available: state.orchestrator.is_some(),
    })
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

/// Send a chat message to the AI orchestrator.
///
/// The response arrives asynchronously via Tauri events (chat-response,
/// bubble-state, action-proposed, etc.) — this command only kicks off
/// processing and returns immediately.
#[tauri::command]
pub async fn chat_message(state: State<'_, AppState>, message: String) -> Result<(), String> {
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.handle_chat(&message)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// Search documents by title (client-side quick filter).
#[tauri::command]
pub async fn search_documents(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchHit>, String> {
    let docs = state
        .db
        .search_documents_by_title(&query)
        .await
        .map_err(|e| e.to_string())?;

    let results: Vec<SearchHit> = docs
        .into_iter()
        .take(50)
        .map(|d| {
            let id = d
                .id
                .as_ref()
                .map(sovereign_db::schema::thing_to_raw)
                .unwrap_or_default();
            let snippet = if d.content.len() > 120 {
                format!("{}...", &d.content[..120])
            } else {
                d.content.clone()
            };
            SearchHit {
                id,
                title: d.title,
                snippet,
            }
        })
        .collect();

    Ok(results)
}

/// Full AI-powered search via the orchestrator.
#[tauri::command]
pub async fn search_query(state: State<'_, AppState>, query: String) -> Result<(), String> {
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.handle_query(&query)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Action gate (confirmation / rejection)
// ---------------------------------------------------------------------------

/// Approve a pending action proposed by the orchestrator.
#[tauri::command]
pub async fn approve_action(state: State<'_, AppState>) -> Result<(), String> {
    state
        .decision_tx
        .send(ActionDecision::Approve)
        .await
        .map_err(|e| e.to_string())
}

/// Reject a pending action proposed by the orchestrator.
#[tauri::command]
pub async fn reject_action(
    state: State<'_, AppState>,
    reason: String,
) -> Result<(), String> {
    state
        .decision_tx
        .send(ActionDecision::Reject(reason))
        .await
        .map_err(|e| e.to_string())
}

/// Accept a proactive suggestion.
#[tauri::command]
pub async fn accept_suggestion(
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    state
        .feedback_tx
        .send(FeedbackEvent::SuggestionAccepted { action })
        .await
        .map_err(|e| e.to_string())
}

/// Dismiss a proactive suggestion.
#[tauri::command]
pub async fn dismiss_suggestion(
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    state
        .feedback_tx
        .send(FeedbackEvent::SuggestionDismissed { action })
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// List all documents, optionally filtered by thread.
#[tauri::command]
pub async fn list_documents(
    state: State<'_, AppState>,
    thread_id: Option<String>,
) -> Result<Vec<DocSummary>, String> {
    let docs = state
        .db
        .list_documents(thread_id.as_deref())
        .await
        .map_err(|e| e.to_string())?;

    Ok(docs
        .into_iter()
        .map(|d| DocSummary {
            id: d
                .id
                .as_ref()
                .map(sovereign_db::schema::thing_to_raw)
                .unwrap_or_default(),
            title: d.title,
            thread_id: d.thread_id,
            is_owned: d.is_owned,
            modified_at: d.modified_at.to_rfc3339(),
        })
        .collect())
}

/// List all threads.
#[tauri::command]
pub async fn list_threads(state: State<'_, AppState>) -> Result<Vec<ThreadSummary>, String> {
    let threads = state.db.list_threads().await.map_err(|e| e.to_string())?;

    Ok(threads
        .into_iter()
        .map(|t| ThreadSummary {
            id: t
                .id
                .as_ref()
                .map(sovereign_db::schema::thing_to_raw)
                .unwrap_or_default(),
            name: t.name,
            description: t.description,
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------

/// Toggle the UI theme and return the new theme name.
#[tauri::command]
pub async fn toggle_theme(state: State<'_, AppState>) -> Result<String, String> {
    let current = state.theme.lock().map_err(|e| e.to_string())?;
    let next = if *current == "dark" { "light" } else { "dark" };
    drop(current);
    let mut theme = state.theme.lock().map_err(|e| e.to_string())?;
    *theme = next.to_string();
    Ok(next.to_string())
}

/// Get the current theme.
#[tauri::command]
pub async fn get_theme(state: State<'_, AppState>) -> Result<String, String> {
    let theme = state.theme.lock().map_err(|e| e.to_string())?;
    Ok(theme.clone())
}
