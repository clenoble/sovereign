pub mod ai;
pub mod auth;
pub mod browser;
pub mod canvas;
pub mod contacts;
pub mod documents;
pub mod suggestions;
pub mod threads;

use std::collections::HashSet;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sovereign_core::content::ContentFields;
use sovereign_core::interfaces::{FeedbackEvent, OrchestratorEvent};
use sovereign_core::security::ActionDecision;
use sovereign_db::GraphDB;
use sovereign_db::schema::{Document, MessageDirection, ReadStatus, RelationType, Thread};
use sovereign_skills::traits::{SkillContext, SkillDocument};
use tauri::State;

use crate::err::ToStringErr;
use crate::tauri_state::AppState;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Aggregated conversation stats per contact.
struct ContactAggregates {
    unread_by_contact: std::collections::HashMap<String, u32>,
    channels_by_contact: std::collections::HashMap<String, HashSet<String>>,
}

/// Compute unread counts and channel sets per contact from all conversations.
async fn aggregate_conversations(db: &dyn GraphDB) -> Result<ContactAggregates, String> {
    let conversations = db.list_conversations(None).await.str_err()?;
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
    Ok(ContactAggregates { unread_by_contact, channels_by_contact })
}

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
    pub reliability_classification: Option<String>,
    pub reliability_score: Option<f32>,
    pub source_url: Option<String>,
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

