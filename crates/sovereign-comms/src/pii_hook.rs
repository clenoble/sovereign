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
use sovereign_db::schema::{Contact, Message};

/// Invoked by each channel's `send_message` after the outbound
/// `Message` has been persisted AND the per-message PII pipeline
/// (`MessageIngestHook`) has tokenized its body.
///
/// Implementations parse the canonical body for `[pii:<record_id>]`
/// tokens and write a `ShareRecord` per (token × recipient entity)
/// pair, building the sharing ledger. Outbound-only — inbound
/// messages don't fire this hook because receiving PII isn't a
/// disclosure.
///
/// Lives in `sovereign-comms` (alongside the message + contact
/// hooks) so the comms layer can fire it without depending on
/// `sovereign-ai`.
#[async_trait]
pub trait ShareIngestHook: Send + Sync {
    async fn after_outbound_message(&self, message: &Message);
}

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

/// Invoked once per `Contact` after `db.create_contact` has assigned an
/// ID. Channels create stub contacts whenever they encounter a new
/// address (resolve_contact_id), so this fires on first sight of every
/// counterparty.
///
/// Implementations typically:
///   - write a `PiiRecord` per `ChannelAddress` (Email / Phone kind)
///     so the dashboard inventory knows the contact's identifiers
///     without modifying the addresses themselves (addresses ARE the
///     identifier — they stay raw, per the plan)
///   - if `notes` is non-empty, scan + tokenize and rewrite via
///     `db.update_contact`
///   - call `db.update_contact_pii_fields` to set `pii_scanned_at`.
#[async_trait]
pub trait ContactIngestHook: Send + Sync {
    async fn after_contact_created(&self, contact: &Contact);
}
