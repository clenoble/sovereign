use std::path::Path;
use std::sync::mpsc;

use anyhow::Result;
use ringbuf::traits::*;
use sovereign_core::config::VoiceConfig;

use crate::events::VoiceEvent;

use super::capture::AudioCapture;
use super::stt::SttEngine;
use super::tts::TtsEngine;
use super::wake::WakeWordDetector;

/// Voice pipeline state machine.
#[derive(Debug, Clone, Copy, PartialEq)]
enum PipelineState {
    Idle,
    Listening,
    Transcribing,
}

/// Runs the voice pipeline on a dedicated std::thread.
/// Communicates with the UI via VoiceEvent channel
/// and with the orchestrator via the query callback.
pub struct VoicePipeline;

impl VoicePipeline {
    /// Spawn the voice pipeline on a dedicated thread.
    /// Returns the thread handle for cleanup.
    pub fn spawn(
        config: VoiceConfig,
        voice_tx: mpsc::Sender<VoiceEvent>,
        query_callback: Box<dyn Fn(String) + Send + 'static>,
    ) -> Result<std::thread::JoinHandle<()>> {
        // Validate config files exist before spawning thread
        if !Path::new(&config.wake_word_model).exists() {
            anyhow::bail!(
                "Wake word model not found: {}",
                config.wake_word_model
            );
        }
        if !Path::new(&config.whisper_model).exists() {
            anyhow::bail!("Whisper model not found: {}", config.whisper_model);
        }

        let handle = std::thread::Builder::new()
            .name("voice-pipeline".into())
            .spawn(move || {
                if let Err(e) = run_pipeline(config, voice_tx.clone(), query_callback) {
                    tracing::error!("Voice pipeline error: {e}");
                    let _ = voice_tx.send(VoiceEvent::ListeningStopped);
                }
            })?;

        Ok(handle)
    }
}

fn run_pipeline(
    config: VoiceConfig,
    voice_tx: mpsc::Sender<VoiceEvent>,
    query_callback: Box<dyn Fn(String) + Send + 'static>,
) -> Result<()> {
    const TARGET_SAMPLE_RATE: u32 = 16000;

    // Initialize components
    let (_capture, mut audio_cons, actual_rate) = AudioCapture::start(TARGET_SAMPLE_RATE)?;
    tracing::info!("Audio capture started at {actual_rate}Hz");

    let mut wake_detector = WakeWordDetector::new(&config.wake_word_model, actual_rate as usize)?;
    let stt = SttEngine::new(&config.whisper_model)?;

    let _tts = TtsEngine::new(&config.piper_binary, &config.piper_model, &config.piper_config);

    let frame_size = wake_detector.samples_per_frame();
    let mut frame_buf = vec![0.0f32; frame_size];

    let mut state = PipelineState::Idle;
    let mut recording_buf: Vec<f32> = Vec::new();
    let mut silence_frames = 0u32;
    let silence_threshold = (actual_rate as f32 * 2.0 / frame_size as f32) as u32; // ~2s silence

    tracing::info!("Voice pipeline running (frame_size={frame_size})");

    loop {
        // Read a frame of audio from the ring buffer
        let read = audio_cons.pop_slice(&mut frame_buf);
        if read < frame_size {
            // Not enough data yet, sleep briefly
            std::thread::sleep(std::time::Duration::from_millis(50));
            continue;
        }

        match state {
            PipelineState::Idle => {
                if wake_detector.process(&frame_buf) {
                    tracing::info!("Wake word detected!");
                    let _ = voice_tx.send(VoiceEvent::WakeWordDetected);
                    let _ = voice_tx.send(VoiceEvent::ListeningStarted);
                    state = PipelineState::Listening;
                    recording_buf.clear();
                    silence_frames = 0;
                }
            }
            PipelineState::Listening => {
                recording_buf.extend_from_slice(&frame_buf[..read]);

                // Check for silence (simple energy-based VAD)
                let energy: f32 =
                    frame_buf.iter().map(|s| s * s).sum::<f32>() / frame_size as f32;
                if energy < 0.001 {
                    silence_frames += 1;
                } else {
                    silence_frames = 0;
                }

                // Stop after 2s of silence or 30s max recording
                if silence_frames >= silence_threshold
                    || recording_buf.len() > actual_rate as usize * 30
                {
                    tracing::info!(
                        "Recording complete: {} samples ({:.1}s)",
                        recording_buf.len(),
                        recording_buf.len() as f32 / actual_rate as f32
                    );
                    state = PipelineState::Transcribing;
                }
            }
            PipelineState::Transcribing => {
                let _ = voice_tx.send(VoiceEvent::ListeningStopped);

                match stt.transcribe(&recording_buf) {
                    Ok(text) if !text.is_empty() => {
                        tracing::info!("Transcription: {text}");
                        let _ =
                            voice_tx.send(VoiceEvent::TranscriptionReady(text.clone()));

                        // Send to orchestrator
                        query_callback(text);
                    }
                    Ok(_) => {
                        tracing::debug!("Empty transcription, ignoring");
                    }
                    Err(e) => {
                        tracing::error!("Transcription error: {e}");
                    }
                }

                recording_buf.clear();
                state = PipelineState::Idle;
            }
        }
    }
}
