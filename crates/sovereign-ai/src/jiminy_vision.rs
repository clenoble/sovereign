//! Vision-state poller for the jiminy-vision service.
//!
//! Polls `GET /vision/state` from the vision sidecar (default port 9101), keeps
//! the latest gesture + scene description in a shared store the orchestrator
//! reads for context, and fires a callback once per fresh, distinct gesture
//! (e.g. `shush` -> stop speaking). Degrades gracefully if the service is down.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

/// Latest vision state: written by the poller, read by the orchestrator / UI.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VisionState {
    /// Most recent detected gesture (e.g. "shush", "open_palm"), if any.
    pub gesture: Option<String>,
    /// Latest VLM scene description while a vision window is open.
    pub scene: Option<String>,
    /// Whether the VLM scene-understanding window is currently active.
    pub window_active: bool,
    /// Whether the vision service has a working camera.
    pub camera_ok: bool,
}

/// Shared vision store — poller writes, orchestrator / UI read.
pub type SharedVision = Arc<Mutex<VisionState>>;

/// Create a new, empty shared vision store.
pub fn shared_vision() -> SharedVision {
    Arc::new(Mutex::new(VisionState::default()))
}

/// Raw JSON shape returned by `GET /vision/state`.
#[derive(Debug, Deserialize)]
struct VisionStateDto {
    #[serde(default)]
    gesture: Option<String>,
    #[serde(default)]
    gesture_age_s: Option<f64>,
    #[serde(default)]
    scene: String,
    #[serde(default)]
    window_active: bool,
    #[serde(default)]
    camera_ok: bool,
}

/// A gesture is "actionable" (worth firing) when it is present, fresh (younger
/// than `fresh_max` seconds), and distinct from the last one we fired — so
/// holding a pose fires once, not on every poll. Returns the gesture to fire.
fn actionable_gesture(
    gesture: Option<&str>,
    gesture_age_s: Option<f64>,
    last_fired: Option<&str>,
    fresh_max: f64,
) -> Option<String> {
    let g = gesture?;
    let age = gesture_age_s?;
    if age <= fresh_max && Some(g) != last_fired {
        Some(g.to_string())
    } else {
        None
    }
}

/// Map the wire DTO into the shared state shape (an empty scene becomes `None`).
fn to_state(dto: &VisionStateDto) -> VisionState {
    VisionState {
        gesture: dto.gesture.clone(),
        scene: if dto.scene.trim().is_empty() {
            None
        } else {
            Some(dto.scene.clone())
        },
        window_active: dto.window_active,
        camera_ok: dto.camera_ok,
    }
}

/// Whether a detected gesture should stop the robot mid-speech (barge-in).
/// Currently `shush` (finger-to-lips); extend as the gesture vocabulary grows.
pub fn gesture_stops_speech(gesture: &str) -> bool {
    matches!(gesture, "shush")
}

/// Whether a detected gesture should open the mic / start listening. Currently
/// `talking_hand` (the chatterbox mime — see jiminy-vision); mirrors
/// `gesture_stops_speech`. The app wires this to the voice pipeline.
pub fn gesture_starts_listening(gesture: &str) -> bool {
    matches!(gesture, "talking_hand")
}

/// Spawn a background thread that polls `GET /vision/state` at ~5 Hz, updates
/// `vision`, calls `on_gesture(name)` once per fresh, distinct gesture, and —
/// when `bridge_url` is set — POSTs `/stop` to the jiminy-bridge sidecar for any
/// gesture that should halt speech (see `gesture_stops_speech`, e.g. shush).
///
/// `fresh_max` is the maximum gesture age (seconds) still considered actionable.
pub fn spawn_poller(
    base_url: &str,
    vision: SharedVision,
    bridge_url: Option<String>,
    on_gesture: impl Fn(String) + Send + 'static,
    fresh_max: f64,
) -> std::thread::JoinHandle<()> {
    let url = format!("{}/vision/state", base_url.trim_end_matches('/'));

    std::thread::Builder::new()
        .name("jiminy-vision".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create vision poller runtime");

            rt.block_on(async {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .default_headers(crate::sidecar::auth_headers())
                    .build()
                    .unwrap_or_default();

                let mut logged_error = false;
                let mut last_fired: Option<String> = None;

                loop {
                    match client.get(&url).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            if let Ok(dto) = resp.json::<VisionStateDto>().await {
                                // Reset the debounce when the gesture clears, so
                                // the same gesture can fire again next time.
                                if dto.gesture.is_none() {
                                    last_fired = None;
                                }
                                if let Some(g) = actionable_gesture(
                                    dto.gesture.as_deref(),
                                    dto.gesture_age_s,
                                    last_fired.as_deref(),
                                    fresh_max,
                                ) {
                                    on_gesture(g.clone());
                                    // Reaction: shush -> stop the robot mid-speech.
                                    if gesture_stops_speech(&g) {
                                        if let Some(ref burl) = bridge_url {
                                            let _ = client
                                                .post(format!("{}/stop", burl.trim_end_matches('/')))
                                                .send()
                                                .await;
                                            tracing::info!("Vision '{g}' -> stop speaking");
                                        }
                                    }
                                    last_fired = Some(g);
                                }
                                if let Ok(mut v) = vision.lock() {
                                    *v = to_state(&dto);
                                }
                                if logged_error {
                                    tracing::info!("Vision feed recovered");
                                    logged_error = false;
                                }
                            }
                        }
                        Ok(resp) => {
                            if !logged_error {
                                tracing::warn!("Vision endpoint returned {}", resp.status());
                                logged_error = true;
                            }
                        }
                        Err(e) => {
                            if !logged_error {
                                tracing::warn!("Vision poll failed: {e}");
                                logged_error = true;
                            }
                        }
                    }

                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
            });
        })
        .expect("Failed to spawn vision poller thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_vision_starts_empty() {
        let v = shared_vision();
        assert_eq!(*v.lock().unwrap(), VisionState::default());
    }

    #[test]
    fn shush_stops_speech_others_do_not() {
        assert!(gesture_stops_speech("shush"));
        assert!(!gesture_stops_speech("open_palm"));
        assert!(!gesture_stops_speech("point"));
        assert!(!gesture_stops_speech(""));
    }

    #[test]
    fn talking_hand_starts_listening_others_do_not() {
        assert!(gesture_starts_listening("talking_hand"));
        assert!(!gesture_starts_listening("shush"));
        assert!(!gesture_starts_listening("open_palm"));
        assert!(!gesture_starts_listening(""));
    }

    #[test]
    fn parses_full_vision_state() {
        let json = r#"{"gesture":"shush","gesture_age_s":0.2,"scene":"a person waving",
            "scene_age_s":1.0,"window_active":true,"window_remaining_s":120.0,
            "camera_ok":true,"frame_age_s":0.05}"#;
        let dto: VisionStateDto = serde_json::from_str(json).unwrap();
        let st = to_state(&dto);
        assert_eq!(st.gesture.as_deref(), Some("shush"));
        assert_eq!(st.scene.as_deref(), Some("a person waving"));
        assert!(st.window_active);
        assert!(st.camera_ok);
    }

    #[test]
    fn empty_scene_and_null_gesture_become_none() {
        let json = r#"{"gesture":null,"scene":"","window_active":false,"camera_ok":true}"#;
        let dto: VisionStateDto = serde_json::from_str(json).unwrap();
        let st = to_state(&dto);
        assert_eq!(st.gesture, None);
        assert_eq!(st.scene, None);
        assert!(st.camera_ok);
    }

    #[test]
    fn fires_fresh_distinct_gesture() {
        assert_eq!(
            actionable_gesture(Some("shush"), Some(0.2), None, 1.0).as_deref(),
            Some("shush")
        );
    }

    #[test]
    fn suppresses_repeat_of_held_gesture() {
        assert_eq!(actionable_gesture(Some("shush"), Some(0.2), Some("shush"), 1.0), None);
    }

    #[test]
    fn fires_when_gesture_changes() {
        assert_eq!(
            actionable_gesture(Some("open_palm"), Some(0.1), Some("shush"), 1.0).as_deref(),
            Some("open_palm")
        );
    }

    #[test]
    fn ignores_stale_gesture() {
        assert_eq!(actionable_gesture(Some("shush"), Some(3.0), None, 1.0), None);
    }

    #[test]
    fn ignores_absent_gesture() {
        assert_eq!(actionable_gesture(None, None, Some("shush"), 1.0), None);
    }
}
