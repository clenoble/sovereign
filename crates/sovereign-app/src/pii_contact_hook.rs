//! `ContactIngestHook` implementation that runs the PII pipeline over
//! a freshly-created contact's `notes` and writes a `PiiRecord` per
//! `ChannelAddress` so the dashboard inventory knows the contact's
//! identifiers.
//!
//! Per the plan, `ChannelAddress.address` stays raw (addresses ARE the
//! identifier — replacing them with `[pii:<id>]` tokens would break
//! lookups). The address-derived PiiRecord uses span `0..0` with
//! `source_kind = Contact`; the dashboard distinguishes
//! address-derived records from in-body findings via the zero-width
//! span.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use sovereign_ai::pii::ingest::{ingest_text, GraphDbPiiSink};
use sovereign_ai::pii::pipeline::PipelineConfig;
use sovereign_comms::pii_hook::ContactIngestHook;
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{
    thing_to_raw, ChannelType, Contact, PiiKind, PiiRecord, ReviewState, SourceKind,
    SourceRef,
};
use sovereign_db::traits::GraphDB;

pub struct PiiContactHook {
    db: Arc<dyn GraphDB>,
    account_key: Arc<AccountKey>,
}

impl PiiContactHook {
    pub fn new(db: Arc<dyn GraphDB>, account_key: Arc<AccountKey>) -> Self {
        Self { db, account_key }
    }

    async fn run_ingest(&self, contact_id: &str, contact: &Contact) -> anyhow::Result<()> {
        // 1. PiiRecord per ChannelAddress.
        let now = Utc::now();
        for addr in &contact.addresses {
            let kind = match addr.channel {
                ChannelType::Email => PiiKind::Email,
                ChannelType::Phone | ChannelType::Sms | ChannelType::Signal
                | ChannelType::WhatsApp => PiiKind::Phone,
                ChannelType::Matrix | ChannelType::Custom(_) => PiiKind::Other,
            };
            let blob = EncryptedBlob::encrypt_str(&addr.address, self.account_key.as_ref())
                .map_err(|e| anyhow::anyhow!("vault encrypt addr: {e}"))?;
            let record = PiiRecord {
                id: None,
                kind,
                value_encrypted: blob.ciphertext_b64,
                value_nonce: blob.nonce_b64,
                label: addr.display_name.clone(),
                entity_id: contact.entity_id.clone(),
                stored_secret: false,
                confidence: 1.0,
                sources: vec![SourceRef {
                    source_kind: SourceKind::Contact,
                    source_id: contact_id.to_string(),
                    // Addresses aren't embedded in a tokenized body —
                    // zero-width span signals "the contact's address
                    // field, not a body span".
                    span_start: 0,
                    span_end: 0,
                }],
                discovered_at: now,
                last_revealed_at: None,
                use_count: 0,
                review_state: ReviewState::Confirmed,
                deleted_at: None,
            };
            self.db.create_pii_record(record).await?;
        }

        // 2. Scan + tokenize notes if non-empty. Contact has no
        // body_raw_encrypted field, so the at-rest preservation lives
        // in EncryptedGraphDB's per-doc-key layer (which already
        // encrypts notes). We just rewrite to canonical-form and
        // persist.
        if !contact.notes.trim().is_empty() {
            let entities = self.db.list_entities().await?;
            let contacts = self.db.list_contacts().await?;
            let sink = GraphDbPiiSink::new(self.db.clone());
            let config = PipelineConfig::default();
            let result = ingest_text(
                &contact.notes,
                contact_id,
                SourceKind::Contact,
                &config,
                None,
                None,
                &entities,
                &contacts,
                &sink,
                self.account_key.as_ref(),
            )
            .await?;
            self.db
                .update_contact(contact_id, None, Some(&result.canonical_body), None)
                .await?;
        }

        // 3. Mark scanned.
        self.db
            .update_contact_pii_fields(contact_id, Some(now))
            .await?;

        tracing::info!(
            "PII ingest: contact {contact_id} → {} addresses recorded",
            contact.addresses.len()
        );
        Ok(())
    }
}

#[async_trait]
impl ContactIngestHook for PiiContactHook {
    async fn after_contact_created(&self, contact: &Contact) {
        let id = match contact.id.as_ref() {
            Some(t) => thing_to_raw(t),
            None => {
                tracing::warn!("PII contact hook: contact has no id, skipping");
                return;
            }
        };
        if contact.pii_scanned_at.is_some() {
            return;
        }
        if let Err(e) = self.run_ingest(&id, contact).await {
            tracing::warn!("PII contact hook: ingest failed for {id}: {e}");
        }
    }
}
