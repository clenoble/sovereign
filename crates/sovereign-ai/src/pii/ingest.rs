//! End-to-end ingest hook: text → pipeline → tokenize → DB writes →
//! canonical body + encrypted raw body ready for the source row update.
//!
//! Step 4b of the PII management & dashboard plan. Glue between the
//! detection pipeline (3a–3e), the tokenization primitive (4a), and
//! whatever DB writes the caller wants to perform via [`PiiSink`].
//!
//! Two reasons this lives behind a trait instead of taking
//! `&dyn GraphDB` directly:
//!   1. Tests can use a mock sink — no need for a SurrealDB instance to
//!      validate the orchestration logic.
//!   2. Step 4c will add concrete `create_pii_record` / `create_entity`
//!      methods to `GraphDB` and wire them through an adapter
//!      `impl PiiSink for SurrealGraphDB`. Until then this trait is
//!      the single place that knows the AI-layer's needs from the DB.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_core::interfaces::ModelBackend;
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{Contact, Entity, PiiRecord, ReviewState, SourceKind, SourceRef};

use crate::llm::format::PromptFormatter;

use super::commit::{substitute_record_ids, tokenize};
use super::pipeline::{run_pipeline, PipelineConfig, ScannedFinding};

/// Minimal DB surface the ingest hook needs. Step 4c implements this
/// over `sovereign-db::traits::GraphDB`.
#[async_trait]
pub trait PiiSink: Send + Sync {
    /// Write a new `PiiRecord` and return its raw ID string
    /// (`"pii_record:abc"`).
    async fn create_pii_record(&self, record: PiiRecord) -> anyhow::Result<String>;

    /// Update an existing `PiiRecord`'s `sources` list. Used after
    /// canonical-body substitution so that record sources point at the
    /// final placeholder spans, not the indexed-placeholder spans.
    async fn update_pii_record_sources(
        &self,
        id: &str,
        sources: Vec<SourceRef>,
    ) -> anyhow::Result<()>;

    /// Write a new `Entity` and return its raw ID string. Used for
    /// disambiguator-proposed entities (`is_owned == false`).
    async fn create_entity(&self, entity: Entity) -> anyhow::Result<String>;
}

/// Output of [`ingest_text`].
///
/// The ingest hook does NOT write the source row (Document/Message/
/// Contact) itself — it returns the values to splice into a follow-up
/// `update_*` call. That keeps the hook usable by every source kind
/// without needing to know which DB method to call for each.
#[derive(Debug, Clone)]
pub struct IngestResult {
    /// Body to store in `Document.content` / `Message.body` / etc.
    /// PII spans replaced with `[pii:<record_id>]` tokens; deferred
    /// (Unreviewed) findings left in raw form.
    pub canonical_body: String,
    /// Base64 ciphertext of the original body, encrypted under
    /// `DeviceKey`. Stored in `*.body_raw_encrypted`.
    pub body_raw_encrypted: String,
    /// Base64 nonce paired with `body_raw_encrypted`. Stored in
    /// `*.body_raw_nonce`.
    pub body_raw_nonce: String,
    /// Timestamp the source was scanned, to be stored in
    /// `*.pii_scanned_at`.
    pub pii_scanned_at: DateTime<Utc>,
    /// IDs of every PiiRecord written during this ingest pass.
    pub record_ids: Vec<String>,
    /// IDs of every newly-created Entity (proposals from the
    /// disambiguator).
    pub created_entity_ids: Vec<String>,
}

/// Run the full ingest pipeline over `text` and write to `sink`.
///
/// `source_id` is the ID of the row whose body is being ingested
/// (e.g. `"document:abc"`). `source_kind` discriminates which schema
/// table the source lives in.
///
/// `entities` and `contacts` come from the caller's most recent DB
/// snapshot; the disambiguator uses them to attach findings to known
/// entities. Newly-proposed entities are written via
/// [`PiiSink::create_entity`] and surfaced in [`IngestResult::created_entity_ids`].
pub async fn ingest_text(
    text: &str,
    source_id: &str,
    source_kind: SourceKind,
    config: &PipelineConfig,
    backend: Option<&dyn ModelBackend>,
    formatter: Option<&dyn PromptFormatter>,
    entities: &[Entity],
    contacts: &[Contact],
    sink: &dyn PiiSink,
    device_key: &DeviceKey,
) -> anyhow::Result<IngestResult> {
    let now = Utc::now();

    // Stage 1 — pipeline + tokenize.
    let pipeline_result = run_pipeline(text, config, backend, formatter, entities, contacts).await;
    let tokenized = tokenize(text, &pipeline_result.findings);

    // Stage 2 — write PiiRecord per slot (Confirmed) and per deferred
    // (Unreviewed) finding. For deferred records, the canonical body
    // still holds the raw value at the original span — no further
    // substitution needed for their `sources`.
    let mut slot_record_ids: Vec<String> = Vec::with_capacity(tokenized.slots.len());
    let mut all_record_ids: Vec<String> = Vec::new();

    for slot in &tokenized.slots {
        let record = build_pii_record(
            &slot.scanned,
            source_id,
            source_kind.clone(),
            // Indexed-placeholder span; will be updated post-substitution.
            slot.canonical_start,
            slot.canonical_end,
            now,
            device_key,
        )?;
        let id = sink.create_pii_record(record).await?;
        slot_record_ids.push(id.clone());
        all_record_ids.push(id);
    }

    for deferred in &tokenized.deferred {
        // The body wasn't rewritten for these — original span IS the
        // canonical span.
        let record = build_pii_record(
            deferred,
            source_id,
            source_kind.clone(),
            deferred.finding.start,
            deferred.finding.end,
            now,
            device_key,
        )?;
        let id = sink.create_pii_record(record).await?;
        all_record_ids.push(id);
    }

    // Stage 3 — substitute the integer placeholders with real record IDs.
    let (canonical_body, final_slots) =
        substitute_record_ids(&tokenized.canonical, &tokenized.slots, &slot_record_ids);

    // Stage 4 — update each Confirmed record's sources to point at the
    // final post-substitution span.
    for (slot, id) in final_slots.iter().zip(slot_record_ids.iter()) {
        let source = SourceRef {
            source_kind: source_kind.clone(),
            source_id: source_id.to_string(),
            span_start: slot.canonical_start,
            span_end: slot.canonical_end,
        };
        sink.update_pii_record_sources(id, vec![source]).await?;
    }

    // Stage 5 — write disambiguator-proposed entities.
    let mut created_entity_ids: Vec<String> = Vec::new();
    for entity in pipeline_result.proposed_entities {
        let id = sink.create_entity(entity).await?;
        created_entity_ids.push(id);
    }

    // Stage 6 — encrypt the original body for L3-gated reveal.
    let raw_blob = EncryptedBlob::encrypt_str(text, device_key)
        .map_err(|e| anyhow::anyhow!("vault encrypt failed: {e}"))?;
    let (body_raw_encrypted, body_raw_nonce) = raw_blob.into_pair();

    Ok(IngestResult {
        canonical_body,
        body_raw_encrypted,
        body_raw_nonce,
        pii_scanned_at: now,
        record_ids: all_record_ids,
        created_entity_ids,
    })
}

fn build_pii_record(
    scanned: &ScannedFinding,
    source_id: &str,
    source_kind: SourceKind,
    canonical_start: usize,
    canonical_end: usize,
    now: DateTime<Utc>,
    device_key: &DeviceKey,
) -> anyhow::Result<PiiRecord> {
    let blob = EncryptedBlob::encrypt_str(&scanned.finding.sample, device_key)
        .map_err(|e| anyhow::anyhow!("vault encrypt failed: {e}"))?;
    Ok(PiiRecord {
        id: None,
        kind: scanned.finding.kind.clone(),
        value_encrypted: blob.ciphertext_b64,
        value_nonce: blob.nonce_b64,
        label: None,
        entity_id: scanned.entity_id.clone(),
        stored_secret: false,
        confidence: scanned.finding.confidence,
        sources: vec![SourceRef {
            source_kind,
            source_id: source_id.to_string(),
            span_start: canonical_start,
            span_end: canonical_end,
        }],
        discovered_at: now,
        last_revealed_at: None,
        use_count: 0,
        review_state: scanned.review_state.clone(),
        deleted_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;
    use sovereign_crypto::master_key::MasterKey;
    use std::sync::Mutex;

    /// Mock sink that captures every write.
    #[derive(Default)]
    struct MockSink {
        records: Mutex<Vec<PiiRecord>>,
        record_id_counter: Mutex<usize>,
        source_updates: Mutex<Vec<(String, Vec<SourceRef>)>>,
        entities: Mutex<Vec<Entity>>,
        entity_id_counter: Mutex<usize>,
    }

    #[async_trait]
    impl PiiSink for MockSink {
        async fn create_pii_record(&self, mut record: PiiRecord) -> Result<String> {
            let mut c = self.record_id_counter.lock().unwrap();
            *c += 1;
            let id = format!("pii_record:r{}", *c);
            // Mirror what a real DB would do — assign an ID.
            record.id = Some(sovereign_db::schema::raw_to_thing(&id).unwrap());
            self.records.lock().unwrap().push(record);
            Ok(id)
        }
        async fn update_pii_record_sources(
            &self,
            id: &str,
            sources: Vec<SourceRef>,
        ) -> Result<()> {
            self.source_updates
                .lock()
                .unwrap()
                .push((id.to_string(), sources));
            Ok(())
        }
        async fn create_entity(&self, mut entity: Entity) -> Result<String> {
            let mut c = self.entity_id_counter.lock().unwrap();
            *c += 1;
            let id = format!("entity:e{}", *c);
            entity.id = Some(sovereign_db::schema::raw_to_thing(&id).unwrap());
            self.entities.lock().unwrap().push(entity);
            Ok(id)
        }
    }

    fn test_device_key() -> DeviceKey {
        let mk = MasterKey::from_passphrase(b"ingest-test", b"salt").unwrap();
        DeviceKey::derive(&mk, "dev-ingest").unwrap()
    }

    // --- regex-only ingest end-to-end ---

    #[tokio::test]
    async fn ingest_email_writes_record_and_encrypts_body() {
        let dk = test_device_key();
        let sink = MockSink::default();
        let text = "Email me at alice@example.ch.";
        let result = ingest_text(
            text,
            "document:doc1",
            SourceKind::Document,
            &PipelineConfig::default(),
            None,
            None,
            &[],
            &[],
            &sink,
            &dk,
        )
        .await
        .unwrap();

        // Canonical body has the email replaced with a [pii:<id>] token.
        assert!(
            result.canonical_body.contains("[pii:pii_record:r1]"),
            "canonical={:?}",
            result.canonical_body
        );
        assert!(!result.canonical_body.contains("alice@example.ch"));

        // One PiiRecord written.
        let records = sink.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        let r = &records[0];
        assert_eq!(r.kind, sovereign_db::schema::PiiKind::Email);
        assert_eq!(r.review_state, ReviewState::Confirmed);
        // Decrypting the value with the device key recovers the original.
        let blob = EncryptedBlob::from_pair(r.value_encrypted.clone(), r.value_nonce.clone());
        let decrypted = blob.decrypt_to_string(&dk).unwrap();
        assert_eq!(decrypted, "alice@example.ch");

        // Sources updated post-substitution to point at the final placeholder span.
        let updates = sink.source_updates.lock().unwrap();
        assert_eq!(updates.len(), 1);
        let (_id, sources) = &updates[0];
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].source_id, "document:doc1");
        assert_eq!(sources[0].source_kind, SourceKind::Document);
        // The reported span resolves to the placeholder in canonical body.
        let span = &result.canonical_body[sources[0].span_start..sources[0].span_end];
        assert_eq!(span, "[pii:pii_record:r1]");

        // Email's domain is unknown → one Service entity proposed and written.
        let ents = sink.entities.lock().unwrap();
        assert_eq!(ents.len(), 1);
        assert_eq!(ents[0].kind, sovereign_db::schema::EntityKind::Service);
        assert!(!ents[0].is_owned);
        assert_eq!(result.created_entity_ids.len(), 1);

        // body_raw_encrypted decrypts to the original text.
        let raw_blob = EncryptedBlob::from_pair(result.body_raw_encrypted, result.body_raw_nonce);
        let recovered = raw_blob.decrypt_to_string(&dk).unwrap();
        assert_eq!(recovered, text);
    }

    // --- multiple findings, ordering preserved ---

    #[tokio::test]
    async fn ingest_multiple_findings_correct_substitution() {
        let dk = test_device_key();
        let sink = MockSink::default();
        let text = "Call 555-123-4567 or email alice@example.ch.";
        let result = ingest_text(
            text,
            "document:doc2",
            SourceKind::Document,
            &PipelineConfig::default(),
            None,
            None,
            &[],
            &[],
            &sink,
            &dk,
        )
        .await
        .unwrap();

        // Two records, two substitutions, ordering by appearance in text.
        assert_eq!(sink.records.lock().unwrap().len(), 2);
        // r1 = phone (first in text), r2 = email (second).
        assert!(
            result
                .canonical_body
                .contains("[pii:pii_record:r1] or email [pii:pii_record:r2]"),
            "canonical={:?}",
            result.canonical_body
        );

        // Each record's source span resolves correctly in the final body.
        let updates = sink.source_updates.lock().unwrap();
        assert_eq!(updates.len(), 2);
        for (id, sources) in updates.iter() {
            let s = &sources[0];
            let span_text = &result.canonical_body[s.span_start..s.span_end];
            assert_eq!(span_text, format!("[pii:{}]", id));
        }
    }

    // --- deferred (Unreviewed) findings stay in body ---

    #[tokio::test]
    async fn ingest_unreviewed_finding_keeps_text_writes_record_with_unreviewed_state() {
        // Build a low-confidence NER finding by piping through a backend
        // that returns a 0.5-confidence person_name. The default
        // threshold of 0.7 means it'll come back Unreviewed.
        use crate::llm::context::ChatTurn;
        use crate::pii::ner::NerEntity;

        struct CannedBackend(String);
        #[async_trait]
        impl ModelBackend for CannedBackend {
            async fn load(&mut self, _: &str, _: i32) -> Result<()> {
                Ok(())
            }
            async fn generate(&self, _: &str, _: u32) -> Result<String> {
                Ok(self.0.clone())
            }
            async fn unload(&mut self) -> Result<()> {
                Ok(())
            }
        }
        struct PlainFormatter;
        impl PromptFormatter for PlainFormatter {
            fn format_system_user(&self, s: &str, u: &str) -> String {
                format!("{s}\n{u}")
            }
            fn format_conversation(&self, _: &str, _: &[ChatTurn]) -> String {
                String::new()
            }
            fn tool_call_open_tag(&self) -> &str {
                ""
            }
            fn tool_call_close_tag(&self) -> &str {
                ""
            }
            fn format_tool_turn(&self, _: &str) -> String {
                String::new()
            }
            fn chars_per_token(&self) -> f64 {
                4.0
            }
            fn tool_call_format_instruction(&self) -> String {
                String::new()
            }
        }

        let dk = test_device_key();
        let sink = MockSink::default();
        let response = serde_json::to_string(&[NerEntity {
            kind: "person_name".into(),
            value: "Charlie Newcomer".into(),
            confidence: 0.5,
        }])
        .unwrap();
        let backend = CannedBackend(response);
        let formatter = PlainFormatter;
        let text = "Met Charlie Newcomer yesterday.";
        let result = ingest_text(
            text,
            "document:doc3",
            SourceKind::Document,
            &PipelineConfig::default(),
            Some(&backend),
            Some(&formatter),
            &[],
            &[],
            &sink,
            &dk,
        )
        .await
        .unwrap();

        // Body unchanged (deferred) — Charlie Newcomer still readable.
        assert_eq!(result.canonical_body, text);
        // Record was still written, but with Unreviewed state.
        let records = sink.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].review_state, ReviewState::Unreviewed);
        // No source-update calls — we only update sources for slots
        // (Confirmed findings); deferred findings already had the right
        // span in their initial create.
        assert!(sink.source_updates.lock().unwrap().is_empty());
    }

    // --- entity linkage carries through ---

    #[tokio::test]
    async fn ingest_email_links_to_known_entity_no_proposal() {
        let dk = test_device_key();
        let sink = MockSink::default();
        let mut acme = Entity::new("Acme Corp".into(), sovereign_db::schema::EntityKind::Org);
        acme.domains = vec!["acme.com".into()];
        acme.id = Some(sovereign_db::schema::raw_to_thing("entity:acme").unwrap());

        let result = ingest_text(
            "ping alice@acme.com",
            "document:doc4",
            SourceKind::Document,
            &PipelineConfig::default(),
            None,
            None,
            &[acme],
            &[],
            &sink,
            &dk,
        )
        .await
        .unwrap();

        let records = sink.records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].entity_id.as_deref(), Some("entity:acme"));
        // Existing entity → no proposal written.
        assert!(sink.entities.lock().unwrap().is_empty());
        assert!(result.created_entity_ids.is_empty());
    }

    // --- empty / no-PII ---

    #[tokio::test]
    async fn ingest_clean_text_writes_nothing_but_still_encrypts_raw() {
        let dk = test_device_key();
        let sink = MockSink::default();
        let text = "Just plain prose, no PII here.";
        let result = ingest_text(
            text,
            "document:doc5",
            SourceKind::Document,
            &PipelineConfig::default(),
            None,
            None,
            &[],
            &[],
            &sink,
            &dk,
        )
        .await
        .unwrap();

        assert_eq!(result.canonical_body, text);
        assert!(sink.records.lock().unwrap().is_empty());
        assert!(sink.entities.lock().unwrap().is_empty());
        // Even with no findings, we still encrypt the raw body — having
        // body_raw_encrypted unconditionally lets the L3 reveal flow
        // work without a special-case for "no PII".
        let raw = EncryptedBlob::from_pair(result.body_raw_encrypted, result.body_raw_nonce);
        assert_eq!(raw.decrypt_to_string(&dk).unwrap(), text);
    }
}
