use super::*;

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
pub async fn get_status(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<AppStatus, String> {
    // IPC-002: row counts are workspace metadata — keep them off the embedded
    // browser webview (an external page must not be able to enumerate how many
    // documents/threads/contacts exist).
    crate::tauri_state::require_main_webview(&webview)?;
    let docs = state.db.list_documents(None).await.str_err()?;
    let threads = state.db.list_threads().await.str_err()?;
    let contacts = state.db.list_contacts().await.str_err()?;

    Ok(AppStatus {
        documents: docs.len(),
        threads: threads.len(),
        contacts: contacts.len(),
        orchestrator_available: state.orchestrator.is_some(),
    })
}

/// SIDECAR-001/002: hand the provisioned jiminy sidecar token to the trusted
/// webview so `vision.ts` can authenticate its cross-origin `/vision/*` fetches
/// (the sidecars now require the bearer token regardless of Origin). Returns
/// the process `JIMINY_TOKEN`, or `None` if none is provisioned (sidecars not
/// in use). The renderer is the app's own trusted frontend, and the token only
/// guards the loopback sidecars against OTHER local processes — exposing it to
/// the renderer doesn't widen the trust boundary.
#[tauri::command]
pub fn get_jiminy_token() -> Option<String> {
    std::env::var("JIMINY_TOKEN").ok().filter(|t| !t.is_empty())
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
pub async fn chat_message(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    message: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.handle_chat(&message)
        .await
        .str_err()
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// Search documents by title (client-side quick filter).
#[tauri::command]
pub async fn search_documents(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<SearchHit>, String> {
    state.require_unlocked(&webview).await?;
    let docs = state
        .db
        .search_documents_by_title(&query)
        .await
        .str_err()?;

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
pub async fn search_query(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    query: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.handle_query(&query)
        .await
        .str_err()
}

// ---------------------------------------------------------------------------
// Action gate (confirmation / rejection)
// ---------------------------------------------------------------------------

/// Approve a pending action proposed by the orchestrator.
#[tauri::command]
pub async fn approve_action(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state
        .decision_tx
        .send(ActionDecision::Approve)
        .await
        .str_err()
}

/// Reject a pending action proposed by the orchestrator.
#[tauri::command]
pub async fn reject_action(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    reason: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state
        .decision_tx
        .send(ActionDecision::Reject(reason))
        .await
        .str_err()
}

/// Accept a proactive suggestion.
#[tauri::command]
pub async fn accept_suggestion(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state
        .feedback_tx
        .send(FeedbackEvent::SuggestionAccepted { action })
        .await
        .str_err()
}

/// Dismiss a proactive suggestion.
#[tauri::command]
pub async fn dismiss_suggestion(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    state
        .feedback_tx
        .send(FeedbackEvent::SuggestionDismissed { action })
        .await
        .str_err()
}


// ---------------------------------------------------------------------------
// Model management
// ---------------------------------------------------------------------------

/// Scan the model directory for .gguf files.
#[tauri::command]
pub async fn scan_models(state: State<'_, AppState>) -> Result<Vec<ModelEntryDto>, String> {
    let model_dir = &state.config.ai.model_dir;
    let dir = std::path::Path::new(model_dir);
    if !dir.exists() {
        return Ok(vec![]);
    }
    let assignments = state.model_assignments.lock().str_err()?;
    let mut models = Vec::new();
    let entries = std::fs::read_dir(dir).str_err()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("gguf") {
            if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
                let size_mb = std::fs::metadata(&path)
                    .map(|m| m.len() as f64 / (1024.0 * 1024.0))
                    .unwrap_or(0.0);
                models.push(ModelEntryDto {
                    filename: filename.to_string(),
                    size_mb,
                    is_router: assignments.router == filename,
                    is_reasoning: assignments.reasoning == filename,
                });
            }
        }
    }
    models.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(models)
}

/// Assign a model to a role (router or reasoning).
#[tauri::command]
pub async fn assign_model_role(
    state: State<'_, AppState>,
    filename: String,
    role: String,
) -> Result<(), String> {
    {
        let mut assignments = state.model_assignments.lock().str_err()?;
        match role.as_str() {
            "router" => assignments.router = filename.clone(),
            "reasoning" => assignments.reasoning = filename.clone(),
            _ => return Err(format!("Unknown role: {role}")),
        }
    }
    // Model hot-swap via orchestrator uses the chat intent system:
    //   orchestrator.handle_query("switch to <model>")
    // The UI can trigger this through chatMessage if needed.
    Ok(())
}

/// Delete a model file (must not be currently assigned).
#[tauri::command]
pub async fn delete_model(
    state: State<'_, AppState>,
    filename: String,
) -> Result<(), String> {
    // Pre-auth command + user-controlled filename: require a bare .gguf name
    // so `..`/absolute paths can't delete files outside the model dir
    // (Path::join replaces the base entirely when handed an absolute path).
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err("Invalid model filename".into());
    }
    if !filename.ends_with(".gguf") {
        return Err("Not a model file".into());
    }

    let assignments = state.model_assignments.lock().str_err()?;
    if assignments.router == filename {
        return Err("Cannot delete the active router model".into());
    }
    if assignments.reasoning == filename {
        return Err("Cannot delete the active reasoning model".into());
    }
    drop(assignments);

    let model_dir = &state.config.ai.model_dir;
    let path = std::path::Path::new(model_dir).join(&filename);
    std::fs::remove_file(&path).str_err()
}


// ---------------------------------------------------------------------------
// Phase 5: Trust dashboard
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct TrustEntryDto {
    pub action: String,
    pub approval_count: u32,
    pub auto_approve: bool,
    pub last_rejected: Option<String>,
}

/// Return all trust entries for the dashboard.
#[tauri::command]
pub async fn get_trust_entries(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<Vec<TrustEntryDto>, String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    let entries = orch.trust_entries();
    Ok(entries
        .into_iter()
        .map(|e| TrustEntryDto {
            action: e.action,
            approval_count: e.approval_count,
            auto_approve: e.auto_approve,
            last_rejected: e.last_rejected,
        })
        .collect())
}

/// Reset trust for a specific action.
#[tauri::command]
pub async fn reset_trust_action(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.reset_trust_action(&action);
    Ok(())
}

/// Reset all trust entries.
#[tauri::command]
pub async fn reset_trust_all(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.require_unlocked(&webview).await?;
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.reset_trust_all();
    Ok(())
}

