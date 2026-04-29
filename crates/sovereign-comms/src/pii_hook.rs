//! Hook trait invoked after a message has been persisted, to give the
//! PII pipeline a chance to scan + tokenize the body.
//!
//! Defined here so `sovereign-comms` doesn't depend on `sovereign-ai`
//! (which transitively pulls in `llama-cpp-2` — a heavy build dep that
//! the comms layer should not require). The default Sovereign GE
//! integration provides an implementation in `sovereign-app` that
//! reuses the same `ingest_text` orchestrator that the Document path
//! uses (`sovereign-ai/pii/ingest.rs`).
//!
//! Best-effort: hook errors are logged by the implementation, never
//! propagated. A failed ingest leaves the message body in raw form;
//! the plan's idle-sweep mechanism rescans messages with no
//! `pii_scanned_at` marker.

use async_trait::async_trait;
use sovereign_db::schema::Message;

/// Invoked once per `Message` after `db.create_message` has assigned an
/// ID. Implementations typically call `db.update_message_body` and
/// `db.update_message_pii_fields` to persist canonical body + encrypted
/// raw body + scan timestamp.
///
/// The hook receives the persisted message (with ID set) so it can use
/// the ID as the `source_id` for the PII pipeline's `SourceRef`s.
#[async_trait]
pub trait MessageIngestHook: Send + Sync {
    async fn after_message_created(&self, message: &Message);
}
