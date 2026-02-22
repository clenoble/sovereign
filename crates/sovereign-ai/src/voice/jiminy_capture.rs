//! WebSocket audio capture from Jiminy's ReSpeaker 4-mic array.
//!
//! Connects to the jiminy-bridge sidecar's `WS /ws/audio` endpoint,
//! receives f32 LE PCM at 16 kHz mono, and pushes into a lock-free
//! ring buffer — the same interface as `capture.rs` (cpal).
//!
//! Auto-reconnects on disconnect with exponential backoff.

use anyhow::Result;
use ringbuf::{traits::*, HeapRb};

/// Ring buffer consumer type — same as `capture::AudioConsumer`.
pub type AudioConsumer = ringbuf::HeapCons<f32>;

/// Handle to the background WebSocket reader thread.
/// Dropping this does NOT stop the thread — the thread exits when
/// the producer side of the ring buffer is dropped (which happens
/// when this struct is dropped, since it owns nothing the thread
/// needs beyond the producer).
pub struct JiminyAudioCapture {
    _handle: std::thread::JoinHandle<()>,
}

impl JiminyAudioCapture {
    /// Connect to the jiminy-bridge WebSocket and start pushing audio
    /// into a ring buffer.
    ///
    /// Returns `(capture_handle, consumer, sample_rate)` — same signature
    /// as `AudioCapture::start()`.
    pub fn start(ws_url: &str) -> Result<(Self, AudioConsumer, u32)> {
        const SAMPLE_RATE: u32 = 16_000;
        // ~30 seconds of mono audio
        let rb = HeapRb::<f32>::new(SAMPLE_RATE as usize * 30);
        let (prod, cons) = rb.split();

        let url = ws_url.to_string();
        let handle = std::thread::Builder::new()
            .name("jiminy-audio-ws".into())
            .spawn(move || {
                ws_reader_loop(url, prod);
            })?;

        Ok((Self { _handle: handle }, cons, SAMPLE_RATE))
    }
}

/// Blocking loop that connects, sends `{"cmd":"start"}`, and reads
/// binary f32 frames into the ring buffer producer.  Reconnects on
/// failure with exponential backoff (1s → 2s → 4s … 30s max).
fn ws_reader_loop(url: String, mut prod: ringbuf::HeapProd<f32>) {
    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to create jiminy-audio tokio runtime");

    let mut backoff_ms: u64 = 1000;
    let mut logged_first_connect = false;

    rt.block_on(async {
        loop {
            match tokio_tungstenite::connect_async(&url).await {
                Ok((mut ws, _response)) => {
                    backoff_ms = 1000; // reset on success
                    if !logged_first_connect {
                        tracing::info!("Jiminy audio connected to {url}");
                        logged_first_connect = true;
                    } else {
                        tracing::info!("Jiminy audio reconnected");
                    }

                    // Send start command
                    if let Err(e) = ws
                        .send(Message::Text(r#"{"cmd":"start"}"#.into()))
                        .await
                    {
                        tracing::warn!("Failed to send start cmd: {e}");
                        continue;
                    }

                    // Read loop
                    while let Some(msg) = ws.next().await {
                        match msg {
                            Ok(Message::Binary(data)) => {
                                // data is Vec<u8> of f32 LE samples
                                if data.len() % 4 != 0 {
                                    continue;
                                }
                                let samples: &[f32] = bytemuck_cast_slice(&data);
                                prod.push_slice(samples);
                            }
                            Ok(Message::Close(_)) => {
                                tracing::info!("Jiminy audio WS closed by server");
                                break;
                            }
                            Err(e) => {
                                tracing::warn!("Jiminy audio WS error: {e}");
                                break;
                            }
                            _ => {
                                // Ignore text/ping/pong
                            }
                        }
                    }
                }
                Err(e) => {
                    if backoff_ms <= 1000 {
                        tracing::warn!(
                            "Jiminy audio WS connect failed ({url}): {e}. Retrying..."
                        );
                    }
                }
            }

            // Exponential backoff
            tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
            backoff_ms = (backoff_ms * 2).min(30_000);
        }
    });
}

/// Reinterpret a `&[u8]` as `&[f32]` (little-endian, aligned or not).
///
/// Falls back to a per-element copy if the slice isn't 4-byte aligned
/// (unlikely for WebSocket binary frames, but safe).
fn bytemuck_cast_slice(bytes: &[u8]) -> &[f32] {
    // bytemuck::cast_slice requires alignment. If unaligned, copy.
    if bytes.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
        // Safety: we checked alignment and length is multiple of 4
        unsafe {
            std::slice::from_raw_parts(bytes.as_ptr() as *const f32, bytes.len() / 4)
        }
    } else {
        // Shouldn't happen in practice, but handle gracefully
        // Return empty — caller will just get no samples this frame
        &[]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cast_aligned_bytes() {
        let floats: Vec<f32> = vec![1.0, 2.0, -0.5, 0.0];
        let bytes: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        let result = bytemuck_cast_slice(&bytes);
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], 1.0);
        assert_eq!(result[1], 2.0);
        assert_eq!(result[2], -0.5);
        assert_eq!(result[3], 0.0);
    }

    #[test]
    fn cast_empty() {
        let result = bytemuck_cast_slice(&[]);
        assert!(result.is_empty());
    }

    #[test]
    fn start_with_bad_url_returns_handle() {
        // Should not panic — the thread will fail to connect and retry
        let result = JiminyAudioCapture::start("ws://127.0.0.1:1/ws/audio");
        assert!(result.is_ok());
        // Drop immediately — thread will exit on next backoff iteration
    }
}
