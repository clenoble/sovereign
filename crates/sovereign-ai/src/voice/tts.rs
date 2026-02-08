use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result};

/// Text-to-speech engine using Piper as a subprocess.
pub struct TtsEngine {
    piper_binary: String,
    model_path: String,
    config_path: String,
}

impl TtsEngine {
    pub fn new(piper_binary: &str, model_path: &str, config_path: &str) -> Self {
        Self {
            piper_binary: piper_binary.to_string(),
            model_path: model_path.to_string(),
            config_path: config_path.to_string(),
        }
    }

    /// Speak the given text. Blocks until playback finishes.
    pub fn speak(&self, text: &str) -> Result<()> {
        if text.is_empty() {
            return Ok(());
        }

        tracing::debug!("TTS speaking: {text}");

        // piper --model X --config Y --output-raw | aplay -r 22050 -f S16_LE -c 1
        let mut piper = Command::new(&self.piper_binary)
            .args([
                "--model",
                &self.model_path,
                "--config",
                &self.config_path,
                "--output-raw",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn piper")?;

        // Write text to piper's stdin
        if let Some(mut stdin) = piper.stdin.take() {
            stdin
                .write_all(text.as_bytes())
                .context("Failed to write to piper stdin")?;
            // stdin is dropped here, closing the pipe
        }

        // Pipe piper's stdout to aplay
        let piper_stdout = piper.stdout.take().context("No piper stdout")?;

        let aplay = Command::new("aplay")
            .args(["-r", "22050", "-f", "S16_LE", "-c", "1", "-q"])
            .stdin(piper_stdout)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn aplay")?;

        let _ = piper.wait();
        let _ = aplay.wait_with_output();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speak_empty_text_is_noop() {
        let tts = TtsEngine::new("nonexistent-piper", "model.onnx", "model.onnx.json");
        // Empty text should return Ok without spawning any subprocess
        assert!(tts.speak("").is_ok());
    }

    #[test]
    fn speak_with_missing_binary_fails() {
        let tts = TtsEngine::new("/nonexistent/piper", "model.onnx", "model.onnx.json");
        let result = tts.speak("hello world");
        assert!(result.is_err(), "Should fail when piper binary doesn't exist");
    }
}
