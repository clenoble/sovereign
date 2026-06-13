//! Camera frame poller for Jiminy bridge.
//!
//! Fetches JPEG frames from the sidecar's `GET /camera/frame` endpoint
//! at ~5 fps and stores the latest frame in a shared `Arc<Mutex<..>>`.
//! The UI reads from this shared state to display a live camera panel.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Shared frame store — UI reads, poller writes.
/// Contains the latest JPEG bytes (or None if no frame yet).
pub type SharedFrame = Arc<Mutex<Option<Vec<u8>>>>;

/// Create a new shared frame store.
pub fn shared_frame() -> SharedFrame {
    Arc::new(Mutex::new(None))
}

/// Spawn a background thread that polls the camera endpoint at ~5 fps.
///
/// Degrades gracefully: logs once on failure, retries with backoff,
/// logs again on reconnect.
pub fn spawn_poller(
    base_url: &str,
    frame: SharedFrame,
    quality: u32,
    max_width: u32,
) -> std::thread::JoinHandle<()> {
    let url = format!(
        "{}/camera/frame?quality={quality}&max_width={max_width}",
        base_url.trim_end_matches('/')
    );

    std::thread::Builder::new()
        .name("jiminy-camera".into())
        .spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("Failed to create camera poller runtime");

            rt.block_on(async {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(3))
                    .default_headers(crate::sidecar::auth_headers())
                    .build()
                    .unwrap_or_default();

                let mut logged_error = false;
                let mut consecutive_errors = 0u32;

                loop {
                    match client.get(&url).send().await {
                        Ok(resp) if resp.status().is_success() => {
                            if let Ok(bytes) = resp.bytes().await {
                                if bytes.len() > 100 {
                                    if let Ok(mut f) = frame.lock() {
                                        *f = Some(bytes.to_vec());
                                    }
                                    if logged_error {
                                        tracing::info!("Camera feed recovered");
                                        logged_error = false;
                                    }
                                    consecutive_errors = 0;
                                }
                            }
                        }
                        Ok(resp) => {
                            if !logged_error {
                                tracing::warn!(
                                    "Camera endpoint returned {}: {}",
                                    resp.status(),
                                    resp.status().canonical_reason().unwrap_or("?")
                                );
                                logged_error = true;
                            }
                            consecutive_errors += 1;
                        }
                        Err(e) => {
                            if !logged_error {
                                tracing::warn!("Camera poll failed: {e}");
                                logged_error = true;
                            }
                            consecutive_errors += 1;
                        }
                    }

                    // 5 fps on success, slower backoff on errors
                    let delay = if consecutive_errors == 0 {
                        200 // 5 fps
                    } else {
                        (200 * consecutive_errors as u64).min(5000)
                    };
                    tokio::time::sleep(Duration::from_millis(delay)).await;
                }
            });
        })
        .expect("Failed to spawn camera poller thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_frame_starts_empty() {
        let sf = shared_frame();
        assert!(sf.lock().unwrap().is_none());
    }
}
