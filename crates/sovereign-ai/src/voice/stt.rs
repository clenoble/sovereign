use anyhow::Result;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// Speech-to-text engine using whisper.cpp (CPU-only to avoid VRAM contention).
pub struct SttEngine {
    ctx: WhisperContext,
}

impl SttEngine {
    /// Load a whisper GGML model file.
    pub fn new(model_path: &str) -> Result<Self> {
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| anyhow::anyhow!("Failed to load whisper model: {:?}", e))?;
        tracing::info!("Whisper STT model loaded from {model_path}");
        Ok(Self { ctx })
    }

    /// Transcribe f32 mono 16kHz audio samples to text.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String> {
        let mut state = self
            .ctx
            .create_state()
            .map_err(|e| anyhow::anyhow!("Failed to create whisper state: {:?}", e))?;

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_no_timestamps(true);

        state
            .full(params, samples)
            .map_err(|e| anyhow::anyhow!("Whisper transcription failed: {:?}", e))?;

        let mut text = String::new();
        let n_segments = state.full_n_segments();
        for i in 0..n_segments {
            if let Some(segment) = state.get_segment(i) {
                text.push_str(&segment.to_string());
            }
        }

        Ok(text.trim().to_string())
    }
}
