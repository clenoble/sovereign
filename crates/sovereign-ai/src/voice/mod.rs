#[cfg(feature = "voice-stt")]
pub mod capture;
#[cfg(feature = "jiminy")]
pub mod jiminy_capture;
pub mod pipeline;
#[cfg(feature = "voice-stt")]
pub mod stt;
pub mod tts;
#[cfg(feature = "voice-stt")]
pub mod wake;

pub use pipeline::VoicePipeline;
