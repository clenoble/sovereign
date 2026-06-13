/// Mobile-specific Tauri commands:
///   - voice_transcribe_buffer  Web Audio API PCM → Whisper STT
///   - receive_shared_content   Save content arriving from OS share sheet

use serde::Deserialize;
use sovereign_db::GraphDB;
use tauri::State;

use crate::err::ToStringErr;
use crate::tauri_state::{require_main_webview, AppState};

// ---------------------------------------------------------------------------
// Voice transcription (Web Audio API PCM → Whisper)
// ---------------------------------------------------------------------------

/// Transcribe mono f32 PCM samples captured by the Web Audio API.
/// Samples must be at 16 kHz, normalised to [-1, 1] — the same format
/// the cpal pipeline produces on desktop.
///
/// Returns the transcribed text, or an error string if the STT engine
/// is not available (voice-stt feature disabled or no whisper model).
#[tauri::command]
pub async fn voice_transcribe_buffer(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    samples: Vec<f32>,
) -> Result<String, String> {
    // IPC-003: only the trusted main webview may feed PCM to the STT engine —
    // the embedded browser webview must not be able to burn CPU on attacker
    // audio or reach the transcription path at all.
    require_main_webview(&webview)?;
    #[cfg(feature = "voice-stt")]
    {
        let engine = state
            .stt_engine
            .as_ref()
            .ok_or_else(|| "Voice transcription unavailable (no whisper model configured)".to_string())?;
        let engine = engine.lock().await;
        engine.transcribe(&samples).str_err()
    }
    #[cfg(not(feature = "voice-stt"))]
    {
        let _ = (state, samples);
        Err("voice-stt feature not compiled in".to_string())
    }
}

// ---------------------------------------------------------------------------
// Share-sheet receiver (OS share → doc in a chosen thread)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct SharedContent {
    pub content_type: String, // "text" | "url"
    pub text: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
}

/// Save content received from the OS share sheet as a new document.
/// The frontend emits this after the user picks a target thread in
/// SharePickerSheet. Returns the new document ID.
#[tauri::command]
pub async fn receive_shared_content(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    content: SharedContent,
    thread_id: String,
) -> Result<String, String> {
    // IPC-001: this writes attacker-influenceable content straight into the DB
    // (it surfaces to the orchestrator as workspace context — an indirect-
    // injection vector), so it must require an unlocked session AND the trusted
    // main webview. Without this gate the embedded browser webview could plant
    // arbitrary documents, and pre-login callers could write into the plaintext
    // bootstrap DB before the EncryptedGraphDB is installed.
    state.require_unlocked(&webview).await?;

    // ANDROID-005: bound the share payload. Any installed app can drive the OS
    // share sheet into this command; reject oversized content rather than write
    // an unbounded blob into the DB (and onward into the orchestrator context).
    const MAX_SHARED_BYTES: usize = 1024 * 1024; // 1 MB
    let shared_len = content.text.as_deref().map_or(0, str::len)
        + content.url.as_deref().map_or(0, str::len)
        + content.title.as_deref().map_or(0, str::len);
    if shared_len > MAX_SHARED_BYTES {
        return Err("Shared content too large".into());
    }

    use sovereign_core::content::ContentFields;
    use sovereign_db::schema::Document;
    use sovereign_db::schema::thing_to_raw;

    let title = content.title.unwrap_or_else(|| {
        content
            .url
            .as_deref()
            .and_then(|u| u.split('/').filter(|s| !s.is_empty()).last())
            .unwrap_or("Shared content")
            .to_string()
    });

    let body = match content.content_type.as_str() {
        "url" => {
            let url = content.url.as_deref().unwrap_or("");
            let extra = content.text.as_deref().unwrap_or("");
            if extra.is_empty() {
                url.to_string()
            } else {
                format!("{url}\n\n{extra}")
            }
        }
        _ => content.text.unwrap_or_default(),
    };

    // Create the document (not is_owned — it's incoming external content)
    let doc = Document::new(title.clone(), thread_id, false);
    let created = state.db.create_document(doc).await.str_err()?;
    let doc_id = created
        .id
        .as_ref()
        .map(thing_to_raw)
        .unwrap_or_default();

    // Save body if non-empty
    if !body.is_empty() {
        let fields = ContentFields {
            body,
            images: vec![],
            videos: vec![],
        };
        state
            .db
            .update_document(&doc_id, Some(&title), Some(&fields.serialize()))
            .await
            .str_err()?;
    }

    Ok(doc_id)
}

// ---------------------------------------------------------------------------
// Connectivity callback (Phase 4.2: P2P sync gating)
// ---------------------------------------------------------------------------

/// Called by the Android plugin's `NetworkCallback` (and by the
/// frontend test harness) to report a connectivity transition. The
/// auto-trigger gate in `sync_startup` reads this state when deciding
/// whether to fire `StartSync` on PeerDiscovered or from the periodic
/// poll.
///
/// Accepted values (case-insensitive): `"wifi"`, `"cellular"`,
/// `"offline"`, `"unknown"`. Anything else is treated as `unknown`.
#[tauri::command]
pub async fn set_connectivity_state(
    webview: tauri::Webview,
    state: State<'_, AppState>,
    kind: String,
) -> Result<(), String> {
    // IPC-003: only the main webview (the Android NetworkCallback and the test
    // harness invoke through it) may drive the P2P sync gate — an embedded
    // browser page must not be able to force/suppress sync triggers.
    require_main_webview(&webview)?;
    #[cfg(feature = "p2p")]
    {
        use sovereign_p2p::ConnectivityState;
        let parsed = match kind.to_lowercase().as_str() {
            "wifi" => ConnectivityState::Wifi,
            "cellular" => ConnectivityState::Cellular,
            "offline" => ConnectivityState::Offline,
            _ => ConnectivityState::Unknown,
        };
        state.set_connectivity_state(parsed);
        tracing::info!("Connectivity transition: {parsed:?}");
        Ok(())
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = (state, kind);
        Ok(())
    }
}

/// Read the current connectivity state. Useful for the Devices panel
/// in Settings (so a user can see the gate state) and for debugging
/// "why isn't sync firing".
#[tauri::command]
pub async fn get_connectivity_state(
    webview: tauri::Webview,
    state: State<'_, AppState>,
) -> Result<String, String> {
    // IPC-003: main webview only — keep the embedded browser webview off the
    // connectivity/sync surface.
    require_main_webview(&webview)?;
    #[cfg(feature = "p2p")]
    {
        use sovereign_p2p::ConnectivityState;
        let s = match state.connectivity_state() {
            ConnectivityState::Wifi => "wifi",
            ConnectivityState::Cellular => "cellular",
            ConnectivityState::Offline => "offline",
            ConnectivityState::Unknown => "unknown",
        };
        Ok(s.into())
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = state;
        Ok("unknown".into())
    }
}
