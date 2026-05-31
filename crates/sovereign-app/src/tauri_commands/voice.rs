//! Voice control commands (push-to-talk surface).
//!
//! The speech-to-text loop itself runs autonomously on the voice pipeline
//! thread (wake-word triggered, see `sovereign_ai::voice::VoicePipeline`),
//! and surfaces its state to the frontend via the `voice-event` Tauri emit
//! (see `tauri_events::spawn_voice_forwarder`). These commands give the
//! frontend an explicit push-to-talk affordance: they emit a synthetic
//! `voice-event` so the mic button can reflect listening/idle immediately.

use tauri::Emitter;

use crate::tauri_events::VoiceEventPayload;

/// Signal that the user wants to start voice input (push-to-talk).
///
/// Emits a `voice-event` `{kind:"listening"}` so the frontend reflects the
/// listening state. The actual capture/transcription continues to be driven
/// by the wake-word pipeline; this is the explicit UI entry point.
#[tauri::command]
pub async fn start_listening(app: tauri::AppHandle) -> Result<(), String> {
    app.emit(
        "voice-event",
        VoiceEventPayload { kind: "listening".into(), text: None },
    )
    .map_err(|e| e.to_string())?;
    tracing::info!("start_listening requested");
    Ok(())
}

/// Signal that the user wants to stop voice input.
///
/// Emits a `voice-event` `{kind:"idle"}` so the frontend returns the mic
/// button to its idle state.
#[tauri::command]
pub async fn stop_listening(app: tauri::AppHandle) -> Result<(), String> {
    app.emit(
        "voice-event",
        VoiceEventPayload { kind: "idle".into(), text: None },
    )
    .map_err(|e| e.to_string())?;
    tracing::info!("stop_listening requested");
    Ok(())
}
