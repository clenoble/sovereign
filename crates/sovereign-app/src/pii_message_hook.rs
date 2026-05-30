//! `MessageIngestHook` implementation that runs the PII pipeline over
//! the body of every freshly-persisted message.
//!
//! Sovereign-app's contribution to the message ingest path. Lives
//! behind both `encryption` and `comms` features — without crypto,
//! there's no DeviceKey to encrypt PII values; without comms, there's
//! no channel to attach to.
//!
//! The hook runs after `db.create_message` has assigned an ID, so the
//! body briefly lands in the DB raw before being rewritten. The window
//! is only between the create_message return and the
//! `update_message_body` call this hook issues — both inside the same
//! channel sync transaction.

use std::sync::Arc;

use async_trait::async_trait;
use sovereign_ai::pii::ingest::{ingest_text, GraphDbPiiSink};
use sovereign_ai::pii::pipeline::PipelineConfig;
use sovereign_comms::pii_hook::MessageIngestHook;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_db::schema::{thing_to_raw, Message, SourceKind};
use sovereign_db::traits::GraphDB;

pub struct PiiMessageHook {
    db: Arc<dyn GraphDB>,
    device_key: Arc<DeviceKey>,
}

impl PiiMessageHook {
    pub fn new(db: Arc<dyn GraphDB>, device_key: Arc<DeviceKey>) -> Self {
        Self { db, device_key }
    }

    async fn run_ingest(&self, id: &str, message: &Message) -> anyhow::Result<()> {
        let entities = self.db.list_entities().await?;
        let contacts = self.db.list_contacts().await?;
        let sink = GraphDbPiiSink::new(self.db.clone());
        let config = PipelineConfig::default();

        // Regex-only for now — same reasoning as the document path.
        // NER on every inbound message would be too expensive to run
        // synchronously inside a sync loop.
        let result = ingest_text(
            &message.body,
            id,
            SourceKind::Message,
            &config,
            None,
            None,
            &entities,
            &contacts,
            &sink,
            self.device_key.as_ref(),
        )
        .await?;

        self.db
            .update_message_body(id, &result.canonical_body, None)
            .await?;
        self.db
            .update_message_pii_fields(
                id,
                Some(&result.body_raw_encrypted),
                Some(&result.body_raw_nonce),
                Some(result.pii_scanned_at),
            )
            .await?;

        tracing::info!(
            "PII ingest: msg {id} → {} records, {} proposed entities",
            result.record_ids.len(),
            result.created_entity_ids.len()
        );
        Ok(())
    }
}

#[async_trait]
impl MessageIngestHook for PiiMessageHook {
    async fn after_message_created(&self, message: &Message) {
        let id = match message.id.as_ref() {
            Some(t) => thing_to_raw(t),
            None => {
                tracing::warn!("PII hook: message has no id, skipping");
                return;
            }
        };
        if message.pii_scanned_at.is_some() {
            return;
        }
        if let Err(e) = self.run_ingest(&id, message).await {
            tracing::warn!("PII hook: ingest failed for {id}: {e}");
        }
    }
}
