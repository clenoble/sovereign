pub mod action_gate;
pub mod chat_trace;
pub mod consolidation;
pub mod docs_host;
#[cfg(feature = "encrypted-log")]
pub mod encrypted_log;
pub mod events;
pub mod injection;
pub mod intent;
#[cfg(feature = "jiminy")]
pub mod jiminy;
#[cfg(feature = "jiminy")]
pub mod jiminy_camera;
#[cfg(feature = "vision")]
pub mod jiminy_vision;
#[cfg(any(feature = "jiminy", feature = "vision"))]
pub mod sidecar;
pub mod llm;
pub mod local_time;
// C-LOG, the diagnosis of a quarantined log (spec ch. 08, DG1-DG6).
#[cfg(feature = "encrypted-log")]
pub mod log_diagnosis;
// C-LOG, the log's format and its one check (spec ch. 08, LG1-LG4, LG6).
#[cfg(feature = "encrypted-log")]
pub mod log_format;
pub mod media_store;
pub mod model_integrity;
pub mod orchestrator;
pub mod peer_audit;
pub mod pii;
pub mod reliability;
pub mod seed;
pub mod session_log;
pub(crate) mod document_titles;
pub(crate) mod thread_names;
pub mod tools;
pub mod trust;
pub mod voice;

pub use events::{OrchestratorEvent, VoiceEvent};
pub use orchestrator::{Orchestrator, OrchestratorStatePaths};
pub use session_log::SessionLog;
