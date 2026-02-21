//! Jiminy Bridge — maps orchestrator events to Reachy Mini robot commands.
//!
//! Connects to a Python sidecar (jiminy-bridge) via HTTP and translates
//! `OrchestratorEvent`s into physical robot behaviors: head poses, antenna
//! animations, emotions, and speech.
//!
//! The bridge runs on a background thread and degrades gracefully if the
//! sidecar is unreachable (logs once, then stays silent).

use std::sync::mpsc;
use std::thread;

use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_core::security::BubbleVisualState;

/// Connects Sovereign OS orchestrator events to a Reachy Mini robot
/// via the jiminy-bridge Python sidecar.
pub struct JiminyBridge {
    base_url: String,
    client: reqwest::Client,
    /// Track whether we've already logged a connection failure to avoid spam.
    logged_unreachable: std::sync::atomic::AtomicBool,
}

impl JiminyBridge {
    /// Create a new bridge pointing at the jiminy-bridge sidecar.
    ///
    /// Default URL: `http://127.0.0.1:9100`
    pub fn new(base_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .build()
            .unwrap_or_default();
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
            logged_unreachable: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Spawn a background thread that listens for `OrchestratorEvent`s
    /// and translates them to robot commands via HTTP.
    ///
    /// Returns the thread handle for cleanup.
    pub fn spawn(self, event_rx: mpsc::Receiver<OrchestratorEvent>) -> thread::JoinHandle<()> {
        thread::Builder::new()
            .name("jiminy-bridge".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("Failed to create Jiminy tokio runtime");
                rt.block_on(async {
                    tracing::info!("Jiminy bridge started, connecting to {}", self.base_url);
                    while let Ok(event) = event_rx.recv() {
                        self.handle_event(&event).await;
                    }
                    tracing::info!("Jiminy bridge stopped (event channel closed)");
                });
            })
            .expect("Failed to spawn jiminy-bridge thread")
    }

    /// Map an orchestrator event to robot behavior.
    async fn handle_event(&self, event: &OrchestratorEvent) {
        match event {
            OrchestratorEvent::BubbleState(state) => {
                self.on_bubble_state(*state).await;
            }
            OrchestratorEvent::ChatResponse { text } => {
                self.post("/speak", &serde_json::json!({ "text": text }))
                    .await;
            }
            OrchestratorEvent::SearchResults { .. } => {
                // Quick nod — "found it"
                self.post(
                    "/pose",
                    &serde_json::json!({
                        "head_pitch": -5.0,
                        "duration": 0.2
                    }),
                )
                .await;
            }
            OrchestratorEvent::ActionExecuted { success, .. } => {
                if *success {
                    // Happy antenna bounce
                    self.post(
                        "/pose",
                        &serde_json::json!({
                            "antenna_left": 0.5,
                            "antenna_right": 0.5,
                            "duration": 0.3
                        }),
                    )
                    .await;
                } else {
                    // Sad droop
                    self.post(
                        "/pose",
                        &serde_json::json!({
                            "head_pitch": 10.0,
                            "antenna_left": -0.3,
                            "antenna_right": -0.3,
                            "duration": 0.5
                        }),
                    )
                    .await;
                }
            }
            OrchestratorEvent::ActionRejected { .. } => {
                // Slight head shake
                self.post(
                    "/pose",
                    &serde_json::json!({
                        "head_yaw": -8.0,
                        "duration": 0.2
                    }),
                )
                .await;
            }
            OrchestratorEvent::InjectionDetected { .. } => {
                // Alert pose — antennas flat, head back
                self.post(
                    "/pose",
                    &serde_json::json!({
                        "head_pitch": -15.0,
                        "antenna_left": -0.5,
                        "antenna_right": -0.5,
                        "duration": 0.3
                    }),
                )
                .await;
            }
            OrchestratorEvent::Suggestion { .. } => {
                // Lean in — "I have an idea"
                self.post(
                    "/pose",
                    &serde_json::json!({
                        "head_pitch": -3.0,
                        "head_roll": 5.0,
                        "antenna_left": 0.3,
                        "antenna_right": 0.3,
                        "duration": 0.4
                    }),
                )
                .await;
            }
            // Most events don't need physical expression
            _ => {}
        }
    }

    /// Translate bubble visual state to robot pose.
    async fn on_bubble_state(&self, state: BubbleVisualState) {
        let body = match state {
            BubbleVisualState::Idle => {
                // Return to neutral
                serde_json::json!({})
            }
            BubbleVisualState::ProcessingOwned => {
                // Slight tilt, antennas up — thinking
                serde_json::json!({
                    "head_roll": 5.0,
                    "head_pitch": -3.0,
                    "antenna_left": 0.2,
                    "antenna_right": 0.2,
                    "duration": 0.5
                })
            }
            BubbleVisualState::ProcessingExternal => {
                // Wider tilt, cautious
                serde_json::json!({
                    "head_roll": -8.0,
                    "head_pitch": -5.0,
                    "antenna_left": 0.4,
                    "antenna_right": 0.4,
                    "duration": 0.5
                })
            }
            BubbleVisualState::Proposing => {
                // Lean forward, engaged
                serde_json::json!({
                    "head_pitch": -5.0,
                    "antenna_left": 0.3,
                    "antenna_right": 0.3,
                    "duration": 0.4
                })
            }
            BubbleVisualState::Executing => {
                // Nod — working on it
                serde_json::json!({
                    "head_pitch": -8.0,
                    "duration": 0.3
                })
            }
            BubbleVisualState::Suggesting => {
                // Tilt and lean — curious
                serde_json::json!({
                    "head_roll": 8.0,
                    "head_pitch": -3.0,
                    "antenna_left": 0.2,
                    "antenna_right": -0.1,
                    "duration": 0.5
                })
            }
        };

        if state == BubbleVisualState::Idle {
            self.post("/idle", &body).await;
        } else {
            self.post("/pose", &body).await;
        }
    }

    /// Send a POST request to the sidecar. Logs once on failure, then stays quiet.
    async fn post(&self, path: &str, body: &serde_json::Value) {
        let url = format!("{}{}", self.base_url, path);
        match self.client.post(&url).json(body).send().await {
            Ok(_) => {
                // Reset the unreachable flag on success
                self.logged_unreachable
                    .store(false, std::sync::atomic::Ordering::Relaxed);
            }
            Err(e) => {
                if !self
                    .logged_unreachable
                    .swap(true, std::sync::atomic::Ordering::Relaxed)
                {
                    tracing::warn!("Jiminy sidecar unreachable at {}: {e}", self.base_url);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bridge_creation() {
        let bridge = JiminyBridge::new("http://127.0.0.1:9100");
        assert_eq!(bridge.base_url, "http://127.0.0.1:9100");
    }

    #[test]
    fn bridge_strips_trailing_slash() {
        let bridge = JiminyBridge::new("http://127.0.0.1:9100/");
        assert_eq!(bridge.base_url, "http://127.0.0.1:9100");
    }

    #[tokio::test]
    async fn post_to_unreachable_does_not_panic() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        // Should not panic — just logs and returns
        bridge
            .post("/test", &serde_json::json!({"hello": "world"}))
            .await;
        assert!(bridge
            .logged_unreachable
            .load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn handle_event_bubble_idle() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::BubbleState(BubbleVisualState::Idle);
        // Should not panic even with unreachable sidecar
        bridge.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_chat_response() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::ChatResponse {
            text: "Hello!".into(),
        };
        bridge.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_injection_detected() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::InjectionDetected {
            source: "test.html".into(),
            pattern: "ignore previous".into(),
        };
        bridge.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_action_success() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::ActionExecuted {
            action: "search".into(),
            success: true,
        };
        bridge.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_action_failure() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::ActionExecuted {
            action: "create_thread".into(),
            success: false,
        };
        bridge.handle_event(&event).await;
    }

    #[tokio::test]
    async fn handle_event_unknown_is_noop() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let event = OrchestratorEvent::DocumentOpened {
            doc_id: "doc:123".into(),
        };
        // Should be a no-op (no HTTP call for this event type)
        bridge.handle_event(&event).await;
    }

    #[test]
    fn spawn_and_drop() {
        let bridge = JiminyBridge::new("http://127.0.0.1:1");
        let (tx, rx) = mpsc::channel();
        let handle = bridge.spawn(rx);
        // Drop the sender to close the channel and stop the thread
        drop(tx);
        handle.join().expect("Jiminy thread should exit cleanly");
    }
}
