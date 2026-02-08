#[cfg(feature = "wake-word")]
mod inner {
    use anyhow::{Context, Result};
    use rustpotter::{Rustpotter, RustpotterConfig, SampleFormat};

    /// Wake word detector using rustpotter.
    pub struct WakeWordDetector {
        detector: Rustpotter,
    }

    impl WakeWordDetector {
        pub fn new(model_path: &str, sample_rate: usize) -> Result<Self> {
            let mut config = RustpotterConfig::default();
            config.fmt.sample_rate = sample_rate;
            config.fmt.channels = 1;
            config.fmt.sample_format = SampleFormat::F32;
            config.detector.threshold = 0.4;

            let mut detector =
                Rustpotter::new(&config).context("Failed to create rustpotter detector")?;
            detector
                .add_wakeword_from_file("sovereign", model_path)
                .context("Failed to load wake word model")?;

            tracing::info!("Wake word detector loaded from {model_path}");
            Ok(Self { detector })
        }

        pub fn samples_per_frame(&self) -> usize {
            self.detector.get_samples_per_frame()
        }

        pub fn process(&mut self, samples: &[f32]) -> bool {
            self.detector.process_f32(samples).is_some()
        }
    }
}

#[cfg(feature = "wake-word")]
pub use inner::WakeWordDetector;

/// Stub wake word detector when rustpotter is not available.
/// Always returns false (no detection). Voice pipeline will only
/// work via manual activation (Ctrl+F, then speak).
#[cfg(not(feature = "wake-word"))]
pub struct WakeWordDetector;

#[cfg(not(feature = "wake-word"))]
impl WakeWordDetector {
    pub fn new(_model_path: &str, _sample_rate: usize) -> anyhow::Result<Self> {
        tracing::warn!(
            "Wake word detection disabled (built without 'wake-word' feature). \
             Use Ctrl+F to activate voice search manually."
        );
        Ok(Self)
    }

    pub fn samples_per_frame(&self) -> usize {
        // ~100ms at 16kHz
        1600
    }

    pub fn process(&mut self, _samples: &[f32]) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_detector_never_triggers() {
        let mut detector = WakeWordDetector::new("unused", 16000).unwrap();
        let samples = vec![0.0f32; detector.samples_per_frame()];
        assert!(!detector.process(&samples));
    }

    #[test]
    fn stub_frame_size_is_reasonable() {
        let detector = WakeWordDetector::new("unused", 16000).unwrap();
        let frame = detector.samples_per_frame();
        assert!(frame > 0);
        assert!(frame <= 16000); // at most 1 second
    }
}
