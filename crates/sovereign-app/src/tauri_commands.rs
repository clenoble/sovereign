use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use sovereign_core::content::ContentFields;
use sovereign_core::interfaces::FeedbackEvent;
use sovereign_core::security::ActionDecision;
use sovereign_db::GraphDB;
use sovereign_db::schema::{Document, MessageDirection, ReadStatus, RelationType, Thread};
use sovereign_skills::traits::{SkillContext, SkillDocument};
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

#[derive(Serialize)]
pub struct FullDocument {
    pub id: String,
    pub title: String,
    pub body: String,
    pub images: Vec<ContentImageDto>,
    pub videos: Vec<ContentVideoDto>,
    pub thread_id: String,
    pub is_owned: bool,
    pub created_at: String,
    pub modified_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContentImageDto {
    pub path: String,
    pub caption: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ContentVideoDto {
    pub path: String,
    pub caption: String,
    pub duration_secs: Option<f64>,
    pub thumbnail_path: Option<String>,
}

#[derive(Serialize)]
pub struct CommitSummaryDto {
    pub id: String,
    pub message: String,
    pub timestamp: String,
    pub snapshot_title: String,
    pub snapshot_preview: String,
}

#[derive(Serialize)]
pub struct SkillInfo {
    pub skill_name: String,
    pub actions: Vec<SkillActionInfo>,
}

#[derive(Serialize)]
pub struct SkillActionInfo {
    pub action_id: String,
    pub label: String,
}

#[derive(Serialize)]
pub struct SkillResultDto {
    pub kind: String,
    pub body: Option<String>,
    pub images: Option<Vec<ContentImageDto>>,
    pub videos: Option<Vec<ContentVideoDto>>,
    pub file_name: Option<String>,
    pub file_mime: Option<String>,
    pub file_data_base64: Option<String>,
    pub structured_kind: Option<String>,
    pub structured_json: Option<String>,
}

#[derive(Serialize)]
pub struct ModelEntryDto {
    pub filename: String,
    pub size_mb: f64,
    pub is_router: bool,
    pub is_reasoning: bool,
}

// -- Phase 3 DTOs --

#[derive(Serialize)]
pub struct CanvasMessageDto {
    pub id: String,
    pub conversation_id: String,
    pub thread_id: String,
    pub contact_id: String,
    pub subject: String,
    pub is_outbound: bool,
    pub sent_at: String,
}

#[derive(Serialize)]
pub struct CanvasData {
    pub documents: Vec<CanvasDocDto>,
    pub threads: Vec<ThreadDto>,
    pub relationships: Vec<RelationshipDto>,
    pub contacts: Vec<ContactSummaryDto>,
    pub milestones: Vec<MilestoneDto>,
    pub messages: Vec<CanvasMessageDto>,
}

#[derive(Serialize)]
pub struct CanvasDocDto {
    pub id: String,
    pub title: String,
    pub thread_id: String,
    pub is_owned: bool,
    pub spatial_x: f32,
    pub spatial_y: f32,
    pub created_at: String,
    pub modified_at: String,
}

#[derive(Serialize)]
pub struct ThreadDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub created_at: String,
}

#[derive(Serialize)]
pub struct RelationshipDto {
    pub id: String,
    pub from_doc_id: String,
    pub to_doc_id: String,
    pub relation_type: String,
    pub strength: f32,
}

#[derive(Serialize)]
pub struct ContactSummaryDto {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub unread_count: u32,
    pub channels: Vec<String>,
}

#[derive(Serialize)]
pub struct ContactDetailDto {
    pub id: String,
    pub name: String,
    pub avatar: Option<String>,
    pub notes: String,
    pub addresses: Vec<ChannelAddressDto>,
    pub conversations: Vec<ConversationDto>,
}

#[derive(Serialize)]
pub struct ChannelAddressDto {
    pub channel: String,
    pub address: String,
    pub display_name: Option<String>,
    pub is_primary: bool,
}

#[derive(Serialize)]
pub struct ConversationDto {
    pub id: String,
    pub title: String,
    pub channel: String,
    pub participant_ids: Vec<String>,
    pub unread_count: u32,
    pub last_message_at: Option<String>,
}

#[derive(Serialize)]
pub struct MessageDto {
    pub id: String,
    pub conversation_id: String,
    pub direction: String,
    pub from_contact_id: String,
    pub subject: Option<String>,
    pub body: String,
    pub sent_at: String,
    pub read_status: String,
}

#[derive(Serialize)]
pub struct MilestoneDto {
    pub id: String,
    pub title: String,
    pub timestamp: String,
    pub thread_id: String,
    pub description: String,
}

// -- Phase 4 DTOs --

#[derive(Serialize)]
pub struct AuthCheckResult {
    pub needs_onboarding: bool,
    pub needs_login: bool,
    pub crypto_enabled: bool,
}

#[derive(Serialize)]
pub struct PasswordValidationDto {
    pub valid: bool,
    pub errors: Vec<String>,
}

#[derive(Serialize)]
pub struct UserProfileDto {
    pub user_id: String,
    pub designation: String,
    pub nickname: Option<String>,
    pub bubble_style: String,
    pub display_name: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveProfileDto {
    pub nickname: Option<String>,
    pub bubble_style: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Serialize)]
pub struct AppConfigDto {
    pub ai_model_dir: String,
    pub ai_router_model: String,
    pub ai_reasoning_model: String,
    pub ai_n_gpu_layers: i32,
    pub ai_n_ctx: u32,
    pub ai_prompt_format: String,
    pub crypto_enabled: bool,
    pub crypto_keystroke_enabled: bool,
    pub crypto_max_login_attempts: u32,
    pub crypto_lockout_seconds: u32,
    pub ui_theme: String,
}

#[derive(Deserialize)]
#[allow(dead_code)] // crypto-gated fields only read with encryption feature
pub struct OnboardingData {
    pub nickname: Option<String>,
    pub bubble_style: Option<String>,
    pub seed_sample_data: bool,
    pub password: Option<String>,
    pub duress_password: Option<String>,
    pub canary_phrase: Option<String>,
    pub keystrokes: Vec<Vec<KeystrokeSampleDto>>,
}

#[derive(Deserialize, Clone)]
#[allow(dead_code)] // fields only read with encryption feature
pub struct KeystrokeSampleDto {
    pub key: String,
    pub press_ms: u64,
    pub release_ms: u64,
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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn to_full_document(doc: Document) -> FullDocument {
    let id = doc
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default();
    let fields = ContentFields::parse(&doc.content);
    FullDocument {
        id,
        title: doc.title,
        body: fields.body,
        images: fields
            .images
            .into_iter()
            .map(|i| ContentImageDto {
                path: i.path,
                caption: i.caption,
            })
            .collect(),
        videos: fields
            .videos
            .into_iter()
            .map(|v| ContentVideoDto {
                path: v.path,
                caption: v.caption,
                duration_secs: v.duration_secs,
                thumbnail_path: v.thumbnail_path,
            })
            .collect(),
        thread_id: doc.thread_id,
        is_owned: doc.is_owned,
        created_at: doc.created_at.to_rfc3339(),
        modified_at: doc.modified_at.to_rfc3339(),
    }
}

// ---------------------------------------------------------------------------
// Document CRUD
// ---------------------------------------------------------------------------

/// Get a full document by ID (with parsed body/images/videos).
#[tauri::command]
pub async fn get_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<FullDocument, String> {
    let doc = state.db.get_document(&id).await.map_err(|e| e.to_string())?;
    Ok(to_full_document(doc))
}

/// Save document content (title + body + images + videos).
#[tauri::command]
pub async fn save_document(
    state: State<'_, AppState>,
    id: String,
    title: String,
    body: String,
    images: Vec<ContentImageDto>,
    videos: Vec<ContentVideoDto>,
) -> Result<(), String> {
    let fields = ContentFields {
        body,
        images: images
            .into_iter()
            .map(|i| sovereign_core::content::ContentImage {
                path: i.path,
                caption: i.caption,
            })
            .collect(),
        videos: videos
            .into_iter()
            .map(|v| sovereign_core::content::ContentVideo {
                path: v.path,
                caption: v.caption,
                duration_secs: v.duration_secs,
                thumbnail_path: v.thumbnail_path,
            })
            .collect(),
    };
    let content_json = fields.serialize();
    state
        .db
        .update_document(&id, Some(&title), Some(&content_json))
        .await
        .map_err(|e| e.to_string())?;
    state.autocommit.lock().await.record_edit(&id);
    Ok(())
}

/// Create a new document and return its ID.
#[tauri::command]
pub async fn create_document(
    state: State<'_, AppState>,
    title: String,
    thread_id: String,
) -> Result<String, String> {
    let doc = Document::new(title, thread_id, true);
    let created = state
        .db
        .create_document(doc)
        .await
        .map_err(|e| e.to_string())?;
    Ok(created
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default())
}

/// Close a document (flush auto-commit).
#[tauri::command]
pub async fn close_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.autocommit.lock().await.commit_on_close(&id).await;
    Ok(())
}

// ---------------------------------------------------------------------------
// Version history
// ---------------------------------------------------------------------------

/// List commits for a document.
#[tauri::command]
pub async fn list_commits(
    state: State<'_, AppState>,
    doc_id: String,
) -> Result<Vec<CommitSummaryDto>, String> {
    let commits = state
        .db
        .list_document_commits(&doc_id)
        .await
        .map_err(|e| e.to_string())?;

    Ok(commits
        .into_iter()
        .map(|c| {
            let preview = if c.snapshot.content.len() > 200 {
                format!("{}...", &c.snapshot.content[..200])
            } else {
                c.snapshot.content.clone()
            };
            CommitSummaryDto {
                id: c
                    .id
                    .as_ref()
                    .map(sovereign_db::schema::thing_to_raw)
                    .unwrap_or_default(),
                message: c.message,
                timestamp: c.timestamp.to_rfc3339(),
                snapshot_title: c.snapshot.title,
                snapshot_preview: preview,
            }
        })
        .collect())
}

/// Restore a document to a specific commit.
#[tauri::command]
pub async fn restore_commit(
    state: State<'_, AppState>,
    doc_id: String,
    commit_id: String,
) -> Result<FullDocument, String> {
    let doc = state
        .db
        .restore_document(&doc_id, &commit_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(to_full_document(doc))
}

// ---------------------------------------------------------------------------
// Skills
// ---------------------------------------------------------------------------

/// List skills applicable to a document (based on file extension in title).
#[tauri::command]
pub async fn list_skills_for_doc(
    state: State<'_, AppState>,
    doc_title: String,
) -> Result<Vec<SkillInfo>, String> {
    let ext = doc_title
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_lowercase();
    let skills = state.skill_registry.skills_for_file_type(&ext);
    Ok(skills
        .into_iter()
        .map(|(name, actions)| SkillInfo {
            skill_name: name.to_string(),
            actions: actions
                .into_iter()
                .map(|(id, label)| SkillActionInfo {
                    action_id: id,
                    label,
                })
                .collect(),
        })
        .collect())
}

/// Execute a skill action on a document.
#[tauri::command]
pub async fn execute_skill(
    state: State<'_, AppState>,
    skill_name: String,
    action: String,
    doc_id: String,
    params: String,
) -> Result<SkillResultDto, String> {
    let doc = state.db.get_document(&doc_id).await.map_err(|e| e.to_string())?;
    let fields = ContentFields::parse(&doc.content);
    let skill_doc = SkillDocument {
        id: doc_id,
        title: doc.title,
        content: fields,
    };
    let ctx = SkillContext {
        granted: HashSet::new(),
        db: Some(state.skill_db.clone()),
    };
    let output = state
        .skill_registry
        .execute_skill(&skill_name, &action, &skill_doc, &params, &ctx)
        .map_err(|e| e.to_string())?;

    match output {
        sovereign_skills::traits::SkillOutput::ContentUpdate(cf) => Ok(SkillResultDto {
            kind: "content_update".into(),
            body: Some(cf.body),
            images: Some(
                cf.images
                    .into_iter()
                    .map(|i| ContentImageDto {
                        path: i.path,
                        caption: i.caption,
                    })
                    .collect(),
            ),
            videos: Some(
                cf.videos
                    .into_iter()
                    .map(|v| ContentVideoDto {
                        path: v.path,
                        caption: v.caption,
                        duration_secs: v.duration_secs,
                        thumbnail_path: v.thumbnail_path,
                    })
                    .collect(),
            ),
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: None,
            structured_json: None,
        }),
        sovereign_skills::traits::SkillOutput::File { name, mime_type, data } => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            Ok(SkillResultDto {
                kind: "file".into(),
                body: None,
                images: None,
                videos: None,
                file_name: Some(name),
                file_mime: Some(mime_type),
                file_data_base64: Some(b64),
                structured_kind: None,
                structured_json: None,
            })
        }
        sovereign_skills::traits::SkillOutput::StructuredData { kind, json } => Ok(SkillResultDto {
            kind: "structured_data".into(),
            body: None,
            images: None,
            videos: None,
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: Some(kind),
            structured_json: Some(json),
        }),
        sovereign_skills::traits::SkillOutput::None => Ok(SkillResultDto {
            kind: "none".into(),
            body: None,
            images: None,
            videos: None,
            file_name: None,
            file_mime: None,
            file_data_base64: None,
            structured_kind: None,
            structured_json: None,
        }),
    }
}

/// List all registered skills and their actions.
#[tauri::command]
pub async fn list_all_skills(state: State<'_, AppState>) -> Result<Vec<SkillInfo>, String> {
    let skills = state.skill_registry.all_skills();
    Ok(skills
        .iter()
        .map(|s| SkillInfo {
            skill_name: s.name().to_string(),
            actions: s
                .actions()
                .into_iter()
                .map(|(id, label)| SkillActionInfo {
                    action_id: id,
                    label,
                })
                .collect(),
        })
        .collect())
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
    let assignments = state.model_assignments.lock().map_err(|e| e.to_string())?;
    let mut models = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
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
        let mut assignments = state.model_assignments.lock().map_err(|e| e.to_string())?;
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
    let assignments = state.model_assignments.lock().map_err(|e| e.to_string())?;
    if assignments.router == filename {
        return Err("Cannot delete the active router model".into());
    }
    if assignments.reasoning == filename {
        return Err("Cannot delete the active reasoning model".into());
    }
    drop(assignments);

    let model_dir = &state.config.ai.model_dir;
    let path = std::path::Path::new(model_dir).join(&filename);
    std::fs::remove_file(&path).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Canvas (Phase 3)
// ---------------------------------------------------------------------------

/// Bulk-load all data needed for the spatial canvas.
#[tauri::command]
pub async fn canvas_load(state: State<'_, AppState>) -> Result<CanvasData, String> {
    tracing::info!("canvas_load: called from frontend");
    let docs = state.db.list_documents(None).await.map_err(|e| e.to_string())?;
    tracing::info!("canvas_load: got {} documents from DB", docs.len());
    let threads = state.db.list_threads().await.map_err(|e| e.to_string())?;
    let rels = state.db.list_all_relationships().await.map_err(|e| e.to_string())?;
    let contacts = state.db.list_contacts().await.map_err(|e| e.to_string())?;

    // Compute unread counts per contact from conversations
    let conversations = state.db.list_conversations(None).await.map_err(|e| e.to_string())?;
    let mut unread_by_contact: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut channels_by_contact: std::collections::HashMap<String, HashSet<String>> = std::collections::HashMap::new();
    for conv in &conversations {
        for pid in &conv.participant_contact_ids {
            *unread_by_contact.entry(pid.clone()).or_default() += conv.unread_count;
            channels_by_contact
                .entry(pid.clone())
                .or_default()
                .insert(conv.channel.to_string());
        }
    }

    // Collect milestones across all threads
    let mut all_milestones = Vec::new();
    for t in &threads {
        let tid = t.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
        if let Ok(ms) = state.db.list_milestones(&tid).await {
            all_milestones.extend(ms);
        }
    }

    // Build set of owned contact IDs so we can pick the distant contact
    let owned_contact_ids: HashSet<String> = contacts
        .iter()
        .filter(|c| c.is_owned)
        .filter_map(|c| c.id.as_ref().map(sovereign_db::schema::thing_to_raw))
        .collect();

    // Collect messages from conversations that are linked to threads
    let mut all_messages = Vec::new();
    for conv in &conversations {
        let conv_id = conv.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
        let thread_id = match &conv.linked_thread_id {
            Some(tid) => tid.clone(),
            None => continue, // skip conversations not linked to a thread
        };
        // Pick the first non-owned (distant) participant as the contact
        let contact_id = conv.participant_contact_ids
            .iter()
            .find(|pid| !owned_contact_ids.contains(*pid))
            .or_else(|| conv.participant_contact_ids.first())
            .cloned()
            .unwrap_or_default();
        if let Ok(msgs) = state.db.list_messages(&conv_id, None, 50).await {
            for m in msgs {
                let mid = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let subject = m.subject.unwrap_or_else(|| {
                    let body = m.body.chars().take(30).collect::<String>();
                    if m.body.len() > 30 { format!("{}...", body) } else { body }
                });
                all_messages.push(CanvasMessageDto {
                    id: mid,
                    conversation_id: conv_id.clone(),
                    thread_id: thread_id.clone(),
                    contact_id: contact_id.clone(),
                    subject,
                    is_outbound: matches!(m.direction, MessageDirection::Outbound),
                    sent_at: m.sent_at.to_rfc3339(),
                });
            }
        }
    }

    let result = Ok(CanvasData {
        documents: docs
            .into_iter()
            .map(|d| {
                let id = d.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                CanvasDocDto {
                    id,
                    title: d.title,
                    thread_id: d.thread_id,
                    is_owned: d.is_owned,
                    spatial_x: d.spatial_x,
                    spatial_y: d.spatial_y,
                    created_at: d.created_at.to_rfc3339(),
                    modified_at: d.modified_at.to_rfc3339(),
                }
            })
            .collect(),
        threads: threads
            .into_iter()
            .map(|t| {
                let id = t.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                ThreadDto {
                    id,
                    name: t.name,
                    description: t.description,
                    created_at: t.created_at.to_rfc3339(),
                }
            })
            .collect(),
        relationships: rels
            .into_iter()
            .map(|r| {
                let id = r.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let from = r.out.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let to = r.in_.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                RelationshipDto {
                    id,
                    from_doc_id: from,
                    to_doc_id: to,
                    relation_type: format!("{:?}", r.relation_type),
                    strength: r.strength,
                }
            })
            .collect(),
        contacts: contacts
            .into_iter()
            .filter(|c| !c.is_owned)
            .map(|c| {
                let id = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                let unread = unread_by_contact.get(&id).copied().unwrap_or(0);
                let channels: Vec<String> = channels_by_contact
                    .get(&id)
                    .map(|s| s.iter().cloned().collect())
                    .unwrap_or_default();
                ContactSummaryDto {
                    id,
                    name: c.name,
                    avatar: c.avatar,
                    unread_count: unread,
                    channels,
                }
            })
            .collect(),
        milestones: all_milestones
            .into_iter()
            .map(|m| {
                let id = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
                MilestoneDto {
                    id,
                    title: m.title,
                    timestamp: m.timestamp.to_rfc3339(),
                    thread_id: m.thread_id,
                    description: m.description,
                }
            })
            .collect(),
        messages: all_messages,
    });
    tracing::info!("canvas_load: returning {} docs, {} threads, {} rels, {} contacts, {} milestones, {} messages",
        result.as_ref().map(|r| r.documents.len()).unwrap_or(0),
        result.as_ref().map(|r| r.threads.len()).unwrap_or(0),
        result.as_ref().map(|r| r.relationships.len()).unwrap_or(0),
        result.as_ref().map(|r| r.contacts.len()).unwrap_or(0),
        result.as_ref().map(|r| r.milestones.len()).unwrap_or(0),
        result.as_ref().map(|r| r.messages.len()).unwrap_or(0),
    );
    result
}

/// Update a document's spatial canvas position.
#[tauri::command]
pub async fn update_document_position(
    state: State<'_, AppState>,
    id: String,
    x: f32,
    y: f32,
) -> Result<(), String> {
    state
        .db
        .update_document_position(&id, x, y)
        .await
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Thread CRUD (Phase 3)
// ---------------------------------------------------------------------------

/// Create a new thread.
#[tauri::command]
pub async fn create_thread(
    state: State<'_, AppState>,
    name: String,
    description: String,
) -> Result<ThreadDto, String> {
    let thread = Thread::new(name, description);
    let created = state.db.create_thread(thread).await.map_err(|e| e.to_string())?;
    let id = created.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ThreadDto {
        id,
        name: created.name,
        description: created.description,
        created_at: created.created_at.to_rfc3339(),
    })
}

/// Update a thread's name and/or description.
#[tauri::command]
pub async fn update_thread(
    state: State<'_, AppState>,
    id: String,
    name: Option<String>,
    description: Option<String>,
) -> Result<ThreadDto, String> {
    let updated = state
        .db
        .update_thread(&id, name.as_deref(), description.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let tid = updated.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ThreadDto {
        id: tid,
        name: updated.name,
        description: updated.description,
        created_at: updated.created_at.to_rfc3339(),
    })
}

/// Soft-delete a thread.
#[tauri::command]
pub async fn delete_thread(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state.db.soft_delete_thread(&id).await.map_err(|e| e.to_string())
}

/// Move a document to a different thread.
#[tauri::command]
pub async fn move_document_to_thread(
    state: State<'_, AppState>,
    doc_id: String,
    thread_id: String,
) -> Result<(), String> {
    state
        .db
        .move_document_to_thread(&doc_id, &thread_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Contacts & Messaging (Phase 3)
// ---------------------------------------------------------------------------

/// List all non-owned contacts with unread counts.
#[tauri::command]
pub async fn list_contacts(state: State<'_, AppState>) -> Result<Vec<ContactSummaryDto>, String> {
    let contacts = state.db.list_contacts().await.map_err(|e| e.to_string())?;
    let conversations = state.db.list_conversations(None).await.map_err(|e| e.to_string())?;

    let mut unread_by_contact: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    let mut channels_by_contact: std::collections::HashMap<String, HashSet<String>> = std::collections::HashMap::new();
    for conv in &conversations {
        for pid in &conv.participant_contact_ids {
            *unread_by_contact.entry(pid.clone()).or_default() += conv.unread_count;
            channels_by_contact
                .entry(pid.clone())
                .or_default()
                .insert(conv.channel.to_string());
        }
    }

    Ok(contacts
        .into_iter()
        .filter(|c| !c.is_owned)
        .map(|c| {
            let id = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            let unread = unread_by_contact.get(&id).copied().unwrap_or(0);
            let channels: Vec<String> = channels_by_contact
                .get(&id)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            ContactSummaryDto {
                id,
                name: c.name,
                avatar: c.avatar,
                unread_count: unread,
                channels,
            }
        })
        .collect())
}

/// Get full contact detail including conversations.
#[tauri::command]
pub async fn get_contact_detail(
    state: State<'_, AppState>,
    id: String,
) -> Result<ContactDetailDto, String> {
    let contact = state.db.get_contact(&id).await.map_err(|e| e.to_string())?;
    let all_convs = state.db.list_conversations(None).await.map_err(|e| e.to_string())?;

    let contact_convs: Vec<ConversationDto> = all_convs
        .into_iter()
        .filter(|c| c.participant_contact_ids.contains(&id))
        .map(|c| {
            let cid = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            ConversationDto {
                id: cid,
                title: c.title,
                channel: c.channel.to_string(),
                participant_ids: c.participant_contact_ids,
                unread_count: c.unread_count,
                last_message_at: c.last_message_at.map(|t| t.to_rfc3339()),
            }
        })
        .collect();

    let cid = contact.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
    Ok(ContactDetailDto {
        id: cid,
        name: contact.name,
        avatar: contact.avatar,
        notes: contact.notes,
        addresses: contact
            .addresses
            .into_iter()
            .map(|a| ChannelAddressDto {
                channel: a.channel.to_string(),
                address: a.address,
                display_name: a.display_name,
                is_primary: a.is_primary,
            })
            .collect(),
        conversations: contact_convs,
    })
}

/// List conversations, optionally filtered by contact participant.
#[tauri::command]
pub async fn list_conversations(
    state: State<'_, AppState>,
    contact_id: Option<String>,
) -> Result<Vec<ConversationDto>, String> {
    let convs = state.db.list_conversations(None).await.map_err(|e| e.to_string())?;

    Ok(convs
        .into_iter()
        .filter(|c| {
            contact_id
                .as_ref()
                .map(|cid| c.participant_contact_ids.contains(cid))
                .unwrap_or(true)
        })
        .map(|c| {
            let id = c.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            ConversationDto {
                id,
                title: c.title,
                channel: c.channel.to_string(),
                participant_ids: c.participant_contact_ids,
                unread_count: c.unread_count,
                last_message_at: c.last_message_at.map(|t| t.to_rfc3339()),
            }
        })
        .collect())
}

/// List messages in a conversation with cursor-based pagination.
#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    conversation_id: String,
    before: Option<String>,
    limit: u32,
) -> Result<Vec<MessageDto>, String> {
    let before_dt = before
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let msgs = state
        .db
        .list_messages(&conversation_id, before_dt, limit)
        .await
        .map_err(|e| e.to_string())?;

    Ok(msgs
        .into_iter()
        .map(|m| {
            let id = m.id.as_ref().map(sovereign_db::schema::thing_to_raw).unwrap_or_default();
            MessageDto {
                id,
                conversation_id: m.conversation_id,
                direction: format!("{:?}", m.direction),
                from_contact_id: m.from_contact_id,
                subject: m.subject,
                body: m.body,
                sent_at: m.sent_at.to_rfc3339(),
                read_status: format!("{:?}", m.read_status),
            }
        })
        .collect())
}

/// Mark a message as read.
#[tauri::command]
pub async fn mark_message_read(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .update_message_read_status(&id, ReadStatus::Read)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Create a relationship between two documents.
#[tauri::command]
pub async fn create_relationship(
    state: State<'_, AppState>,
    from_id: String,
    to_id: String,
    relation_type: String,
    strength: f32,
) -> Result<(), String> {
    let rel_type = match relation_type.to_lowercase().as_str() {
        "references" => RelationType::References,
        "derivedfrom" => RelationType::DerivedFrom,
        "continues" => RelationType::Continues,
        "contradicts" => RelationType::Contradicts,
        "supports" => RelationType::Supports,
        "branchesfrom" => RelationType::BranchesFrom,
        "contactof" => RelationType::ContactOf,
        "attachedto" => RelationType::AttachedTo,
        _ => return Err(format!("Unknown relation type: {relation_type}")),
    };
    state
        .db
        .create_relationship(&from_id, &to_id, rel_type, strength)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 4: Auth, Onboarding, Settings, Document deletion
// ---------------------------------------------------------------------------

/// Check whether the user needs onboarding or login.
#[tauri::command]
pub async fn check_auth_state(state: State<'_, AppState>) -> Result<AuthCheckResult, String> {
    let onboarding_done = state.profile_dir.join("onboarding_done").exists();

    #[cfg(feature = "encryption")]
    let needs_login = onboarding_done && state.profile_dir.join("crypto/auth.store").exists();

    #[cfg(not(feature = "encryption"))]
    let needs_login = false;

    let _ = &state; // suppress unused warning in non-encryption build

    Ok(AuthCheckResult {
        needs_onboarding: !onboarding_done,
        needs_login,
        crypto_enabled: cfg!(feature = "encryption"),
    })
}

/// Validate a password against the auth store. Returns persona ("primary" or "duress").
#[tauri::command]
pub async fn validate_password(
    state: State<'_, AppState>,
    password: String,
    keystrokes: Vec<KeystrokeSampleDto>,
) -> Result<String, String> {
    #[cfg(feature = "encryption")]
    {
        let _ = &keystrokes; // keystroke comparison deferred to future phase
        let auth_path = state.profile_dir.join("crypto/auth.store");
        let store = sovereign_crypto::auth::AuthStore::load(&auth_path)
            .map_err(|e| e.to_string())?;
        let result = store
            .authenticate(password.as_bytes())
            .map_err(|_| "Invalid password".to_string())?;
        match result.persona {
            sovereign_crypto::auth::PersonaKind::Primary => Ok("primary".into()),
            sovereign_crypto::auth::PersonaKind::Duress => Ok("duress".into()),
        }
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (&state, &password, &keystrokes);
        Ok("primary".into())
    }
}

/// Validate a password against the password policy (strength/complexity).
#[tauri::command]
pub async fn validate_password_policy(
    state: State<'_, AppState>,
    password: String,
) -> Result<PasswordValidationDto, String> {
    #[cfg(feature = "encryption")]
    {
        let _ = &state;
        let policy = sovereign_crypto::auth::PasswordPolicy::default_policy();
        let result = policy.validate(&password);
        Ok(PasswordValidationDto {
            valid: result.valid,
            errors: result.errors,
        })
    }
    #[cfg(not(feature = "encryption"))]
    {
        let _ = (&state, &password);
        Ok(PasswordValidationDto {
            valid: true,
            errors: vec![],
        })
    }
}

/// Complete the onboarding wizard (save profile, optional crypto setup, seed data).
#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    data: OnboardingData,
) -> Result<(), String> {
    let profile_dir = &state.profile_dir;

    // Save user profile
    let mut profile = sovereign_core::profile::UserProfile::load(profile_dir)
        .unwrap_or_else(|_| sovereign_core::profile::UserProfile::default_new());
    if let Some(ref nick) = data.nickname {
        profile.nickname = Some(nick.clone());
    }
    if let Some(ref style) = data.bubble_style {
        profile.bubble_style =
            serde_json::from_str(&format!("\"{style}\"")).unwrap_or_default();
    }
    profile.save(profile_dir).map_err(|e| e.to_string())?;

    // Create crypto stores if encryption enabled and password provided
    #[cfg(feature = "encryption")]
    if let Some(ref password) = data.password {
        let crypto_dir = profile_dir.join("crypto");
        std::fs::create_dir_all(&crypto_dir).map_err(|e| e.to_string())?;

        let salt: [u8; 32] = rand::random();
        let device_id = uuid::Uuid::new_v4().to_string();
        let duress = data
            .duress_password
            .as_deref()
            .unwrap_or("duress-fallback-unused");

        let auth_store = sovereign_crypto::auth::AuthStore::create(
            password.as_bytes(),
            duress.as_bytes(),
            &salt,
            &device_id,
        )
        .map_err(|e| e.to_string())?;
        auth_store
            .save(&crypto_dir.join("auth.store"))
            .map_err(|e| e.to_string())?;

        // Save canary phrase if provided
        if let Some(ref phrase) = data.canary_phrase {
            if let Ok(auth_result) = auth_store.authenticate(password.as_bytes()) {
                let canary =
                    sovereign_crypto::canary::CanaryStore::encrypt(phrase, &auth_result.kek);
                canary
                    .save(&crypto_dir.join("canary.store"))
                    .map_err(|e| e.to_string())?;
            }
        }

        // Save keystroke reference if enrollment data provided
        if !data.keystrokes.is_empty() {
            if let Ok(auth_result) = auth_store.authenticate(password.as_bytes()) {
                let profiles: Vec<sovereign_crypto::keystroke::TypingProfile> = data
                    .keystrokes
                    .iter()
                    .map(|samples| sovereign_crypto::keystroke::TypingProfile {
                        samples: samples
                            .iter()
                            .map(|s| sovereign_crypto::keystroke::KeystrokeSample {
                                key: s.key.clone(),
                                press_ms: s.press_ms,
                                release_ms: s.release_ms,
                            })
                            .collect(),
                    })
                    .collect();
                let reference =
                    sovereign_crypto::keystroke::KeystrokeReference::from_enrollments(&profiles);
                let encrypted = reference.encrypt(&auth_result.kek);
                let ks_json =
                    serde_json::to_string(&encrypted).map_err(|e| e.to_string())?;
                std::fs::write(crypto_dir.join("keystroke.store"), ks_json)
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // Seed sample data if requested
    if data.seed_sample_data {
        crate::seed::seed_if_empty(&state.db)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Write onboarding_done marker
    std::fs::create_dir_all(profile_dir).map_err(|e| e.to_string())?;
    std::fs::write(profile_dir.join("onboarding_done"), "1")
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Get the current user profile.
#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>) -> Result<UserProfileDto, String> {
    let profile = sovereign_core::profile::UserProfile::load(&state.profile_dir)
        .map_err(|e| e.to_string())?;
    Ok(UserProfileDto {
        user_id: profile.user_id,
        designation: profile.designation,
        nickname: profile.nickname,
        bubble_style: serde_json::to_string(&profile.bubble_style)
            .unwrap_or_else(|_| "\"icon\"".into())
            .trim_matches('"')
            .to_string(),
        display_name: profile.display_name,
    })
}

/// Update user profile fields.
#[tauri::command]
pub async fn save_profile(
    state: State<'_, AppState>,
    data: SaveProfileDto,
) -> Result<(), String> {
    let mut profile = sovereign_core::profile::UserProfile::load(&state.profile_dir)
        .map_err(|e| e.to_string())?;
    if let Some(ref nick) = data.nickname {
        profile.nickname = Some(nick.clone());
    }
    if let Some(ref style) = data.bubble_style {
        profile.bubble_style =
            serde_json::from_str(&format!("\"{style}\"")).unwrap_or_default();
    }
    if let Some(ref name) = data.display_name {
        profile.display_name = Some(name.clone());
    }
    profile
        .save(&state.profile_dir)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Get the flattened application configuration.
#[tauri::command]
pub async fn get_config(state: State<'_, AppState>) -> Result<AppConfigDto, String> {
    let config = &state.config;
    Ok(AppConfigDto {
        ai_model_dir: config.ai.model_dir.clone(),
        ai_router_model: config.ai.router_model.clone(),
        ai_reasoning_model: config.ai.reasoning_model.clone(),
        ai_n_gpu_layers: config.ai.n_gpu_layers,
        ai_n_ctx: config.ai.n_ctx,
        ai_prompt_format: config.ai.prompt_format.clone(),
        crypto_enabled: cfg!(feature = "encryption"),
        crypto_keystroke_enabled: config.crypto.keystroke_enabled,
        crypto_max_login_attempts: config.crypto.max_login_attempts,
        crypto_lockout_seconds: config.crypto.lockout_seconds,
        ui_theme: config.ui.theme.clone(),
    })
}

/// Soft-delete a document.
#[tauri::command]
pub async fn delete_document(
    state: State<'_, AppState>,
    id: String,
) -> Result<(), String> {
    state
        .db
        .soft_delete_document(&id)
        .await
        .map_err(|e| e.to_string())
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
pub async fn get_trust_entries(state: State<'_, AppState>) -> Result<Vec<TrustEntryDto>, String> {
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
    state: State<'_, AppState>,
    action: String,
) -> Result<(), String> {
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.reset_trust_action(&action);
    Ok(())
}

/// Reset all trust entries.
#[tauri::command]
pub async fn reset_trust_all(state: State<'_, AppState>) -> Result<(), String> {
    let orch = state
        .orchestrator
        .as_ref()
        .ok_or_else(|| "AI orchestrator not available".to_string())?;
    orch.reset_trust_all();
    Ok(())
}

// ---------------------------------------------------------------------------
// Phase 5: File import
// ---------------------------------------------------------------------------

/// Import a file from the local filesystem as a new document.
#[tauri::command]
pub async fn import_file(
    state: State<'_, AppState>,
    file_path: String,
    thread_id: Option<String>,
) -> Result<CanvasDocDto, String> {
    let path = std::path::Path::new(&file_path);
    if !path.exists() {
        return Err(format!("File not found: {file_path}"));
    }

    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Imported File")
        .to_string();

    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {e}"))?;

    let tid = thread_id.unwrap_or_else(|| "thread:default".to_string());
    let doc = Document::new(title, tid.clone(), true);
    let created = state.db.create_document(doc).await.map_err(|e| e.to_string())?;
    let id = created
        .id
        .as_ref()
        .map(sovereign_db::schema::thing_to_raw)
        .unwrap_or_default();

    // Save the content
    state
        .db
        .update_document(&id, None, Some(&content))
        .await
        .map_err(|e| e.to_string())?;

    Ok(CanvasDocDto {
        id,
        title: created.title,
        thread_id: tid,
        is_owned: true,
        spatial_x: created.spatial_x,
        spatial_y: created.spatial_y,
        created_at: created.created_at.to_rfc3339(),
        modified_at: created.modified_at.to_rfc3339(),
    })
}

// ---------------------------------------------------------------------------
// Phase 5: Comms configuration
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct CommsConfigDto {
    pub comms_available: bool,
    pub email_configured: bool,
    pub email_imap_host: String,
    pub email_imap_port: u16,
    pub email_smtp_host: String,
    pub email_smtp_port: u16,
    pub email_username: String,
    pub signal_configured: bool,
    pub signal_phone: String,
}

/// Return the current comms configuration.
#[tauri::command]
pub async fn get_comms_config(_state: State<'_, AppState>) -> Result<CommsConfigDto, String> {
    #[cfg(feature = "comms")]
    {
        // Load comms config from disk
        let config_path = sovereign_core::home_dir().join(".sovereign/comms.toml");
        if config_path.exists() {
            let data = std::fs::read_to_string(&config_path).map_err(|e| e.to_string())?;
            let cfg: sovereign_comms::config::CommsConfig =
                toml::from_str(&data).map_err(|e| e.to_string())?;
            let (email_configured, imap_host, imap_port, smtp_host, smtp_port, username) =
                if let Some(ref email) = cfg.email {
                    (
                        true,
                        email.imap_host.clone(),
                        email.imap_port,
                        email.smtp_host.clone(),
                        email.smtp_port,
                        email.username.clone(),
                    )
                } else {
                    (false, String::new(), 993, String::new(), 587, String::new())
                };
            let (signal_configured, signal_phone) = if let Some(ref signal) = cfg.signal {
                (true, signal.phone_number.clone())
            } else {
                (false, String::new())
            };
            return Ok(CommsConfigDto {
                comms_available: true,
                email_configured,
                email_imap_host: imap_host,
                email_imap_port: imap_port,
                email_smtp_host: smtp_host,
                email_smtp_port: smtp_port,
                email_username: username,
                signal_configured,
                signal_phone,
            });
        }
        return Ok(CommsConfigDto {
            comms_available: true,
            email_configured: false,
            email_imap_host: String::new(),
            email_imap_port: 993,
            email_smtp_host: String::new(),
            email_smtp_port: 587,
            email_username: String::new(),
            signal_configured: false,
            signal_phone: String::new(),
        });
    }
    #[cfg(not(feature = "comms"))]
    Ok(CommsConfigDto {
        comms_available: false,
        email_configured: false,
        email_imap_host: String::new(),
        email_imap_port: 993,
        email_smtp_host: String::new(),
        email_smtp_port: 587,
        email_username: String::new(),
        signal_configured: false,
        signal_phone: String::new(),
    })
}

#[derive(Deserialize)]
pub struct SaveCommsConfigDto {
    pub email_imap_host: Option<String>,
    pub email_imap_port: Option<u16>,
    pub email_smtp_host: Option<String>,
    pub email_smtp_port: Option<u16>,
    pub email_username: Option<String>,
    pub signal_phone: Option<String>,
}

/// Save comms configuration to disk.
#[tauri::command]
pub async fn save_comms_config(
    _state: State<'_, AppState>,
    data: SaveCommsConfigDto,
) -> Result<(), String> {
    let config_dir = sovereign_core::home_dir().join(".sovereign");
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;

    let mut lines = Vec::new();
    lines.push("enabled = true".to_string());
    lines.push(String::new());

    // Email section
    if let Some(ref host) = data.email_imap_host {
        if !host.is_empty() {
            lines.push("[email]".to_string());
            lines.push(format!("imap_host = \"{}\"", host));
            lines.push(format!(
                "imap_port = {}",
                data.email_imap_port.unwrap_or(993)
            ));
            if let Some(ref smtp) = data.email_smtp_host {
                lines.push(format!("smtp_host = \"{}\"", smtp));
            }
            lines.push(format!(
                "smtp_port = {}",
                data.email_smtp_port.unwrap_or(587)
            ));
            if let Some(ref user) = data.email_username {
                lines.push(format!("username = \"{}\"", user));
            }
            lines.push(String::new());
        }
    }

    // Signal section
    if let Some(ref phone) = data.signal_phone {
        if !phone.is_empty() {
            lines.push("[signal]".to_string());
            lines.push(format!("phone_number = \"{}\"", phone));
            lines.push(String::new());
        }
    }

    let config_path = config_dir.join("comms.toml");
    std::fs::write(&config_path, lines.join("\n")).map_err(|e| e.to_string())?;
    Ok(())
}
