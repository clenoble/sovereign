//! C-SWEEP SW5 (spec ch. 09): her review of personal data, grouped by type,
//! the same on both hosts.
//!
//! - Any record can be confirmed or dismissed, whether the sweep suspected it
//!   or a save replaced it: one, or several selected at once. No gesture acts
//!   on a whole type (Céline, 2026-11-17).
//! - Confirming a suspected value keeps its token and makes it confirmed.
//! - Dismissing a value restores it as written from the encrypted original,
//!   one new version per document (origin [`sovereign_docs::Origin::PiiReview`]).
//!   The dismissal holds for as long as that value stands in that content: its
//!   key is the content's identity and a keyed hash of the value under the
//!   persona's account key, and ingest reads it at every save, on every device.
//! - Both can be taken back. A dismissal taken back puts the token back,
//!   confirmed, one new version per document; a confirmation taken back,
//!   whoever confirmed it, makes the value suspected again, and its token
//!   stays (v2.0.112).
//! - The review state and the dismissals travel by sync.
//!
//! *Signatures by the test author of lot LS(a); the bodies are the implementer's.*
//!
//! **As built here:** the dismissal is the record itself, in state
//! `Dismissed`, linked to the document by its sources and holding the value
//! encrypted under the account key; ingest matches a value against it by
//! decrypting it (`docs_host::RegexDocumentIngest`). No separate keyed hash is
//! stored.

use std::sync::Arc;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState, SourceKind};
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Edit, EditContent, Origin, TextField, Writable};

/// The words of a record's state, on each row and in the accessibility line:
/// `PII suspected - TBC`, `PII confirmed`, `PII dismissed` (Céline,
/// 2026-11-17: « je préfère mon libellé, Suspected seul ne dit pas ce qui est
/// suspected »).
pub fn state_words(state: &ReviewState) -> &'static str {
    match state {
        ReviewState::Unreviewed => "PII suspected - TBC",
        ReviewState::Confirmed => "PII confirmed",
        ReviewState::Dismissed => "PII dismissed",
    }
}

/// One type of personal data in the review window, with its records.
#[derive(Debug, Clone)]
pub struct ReviewGroup {
    pub kind: PiiKind,
    pub rows: Vec<ReviewRow>,
}

/// One record in the review window. Its value is never shown raw here.
#[derive(Debug, Clone)]
pub struct ReviewRow {
    pub record_id: String,
    pub state: ReviewState,
    /// [`state_words`] of `state`.
    pub state_words: String,
    /// The documents the record is found in (their ids).
    pub documents: Vec<String>,
}

/// What a review gesture did.
#[derive(Debug, Clone, Default)]
pub struct ReviewOutcome {
    /// The documents that got one new version (tagged `pii-review`).
    pub rewritten: Vec<String>,
}

/// The review of one persona's personal data, over its document service (its
/// store) and its account key.
pub struct PiiReview {
    docs: Arc<DocumentService>,
    account_key: Arc<AccountKey>,
}

/// A selected record, with its value.
struct Selected {
    id: String,
    record: PiiRecord,
    value: String,
}

impl PiiReview {
    pub fn new(docs: Arc<DocumentService>, account_key: Arc<AccountKey>) -> Self {
        Self { docs, account_key }
    }

    fn store(&self) -> &dyn GraphDB {
        self.docs.store()
    }

    /// The review window: every discovered record of this persona (not the
    /// vault's, not deleted), whatever its state, grouped by type.
    ///
    /// Groups come in the order of their type's label; rows, newest first, as
    /// the store lists them.
    pub async fn groups(&self) -> anyhow::Result<Vec<ReviewGroup>> {
        let records = self
            .store()
            .list_pii_records(None, None, Some(false))
            .await
            .map_err(|e| anyhow::anyhow!("list_pii_records: {e}"))?;
        let mut groups: Vec<ReviewGroup> = vec![];
        for r in records.into_iter().filter(|r| r.deleted_at.is_none() && !r.stored_secret) {
            let Some(record_id) = r.id_string() else { continue };
            let row = ReviewRow {
                record_id,
                state: r.review_state.clone(),
                state_words: state_words(&r.review_state).to_string(),
                documents: documents_of(&r),
            };
            match groups.iter_mut().find(|g| g.kind == r.kind) {
                Some(g) => g.rows.push(row),
                None => groups.push(ReviewGroup { kind: r.kind.clone(), rows: vec![row] }),
            }
        }
        groups.sort_by_key(|g| crate::pii::resolve::preview_label_for_kind(&g.kind));
        Ok(groups)
    }

    /// Confirms the records selected.
    ///
    /// A suspected record becomes confirmed, its token kept, no version. A
    /// dismissed record confirmed is a dismissal taken back
    /// ([`Self::take_back_dismissal`]): its token again, one version per
    /// document. A confirmed one is left as it is.
    pub async fn confirm(&self, record_ids: &[String]) -> anyhow::Result<ReviewOutcome> {
        let mut dismissed = vec![];
        for s in self.selected(record_ids).await? {
            match s.record.review_state {
                ReviewState::Unreviewed => self.set_state(&s.id, ReviewState::Confirmed).await?,
                ReviewState::Dismissed => dismissed.push(s.id),
                ReviewState::Confirmed => {}
            }
        }
        if dismissed.is_empty() {
            return Ok(ReviewOutcome::default());
        }
        self.take_back_dismissal(&dismissed).await
    }

    /// Dismisses the records selected: each value restored as written, one new
    /// version per document, the dismissal recorded.
    pub async fn dismiss(&self, record_ids: &[String]) -> anyhow::Result<ReviewOutcome> {
        let selected: Vec<Selected> = self
            .selected(record_ids)
            .await?
            .into_iter()
            .filter(|s| s.record.review_state != ReviewState::Dismissed)
            .collect();
        // Dismissed first: the review's own save, which runs ingest, then
        // leaves the value as written.
        for s in &selected {
            self.set_state(&s.id, ReviewState::Dismissed).await?;
        }
        let restore = |text: &str| -> String {
            let mut out = text.to_string();
            for s in &selected {
                out = out.replace(&format!("[pii:{}]", s.id), &s.value);
            }
            out
        };
        self.rewrite_documents(&selected, restore).await
    }

    /// Takes back the dismissal of the records selected: the token again,
    /// confirmed, one new version per document.
    pub async fn take_back_dismissal(&self, record_ids: &[String]) -> anyhow::Result<ReviewOutcome> {
        let selected: Vec<Selected> = self
            .selected(record_ids)
            .await?
            .into_iter()
            .filter(|s| s.record.review_state == ReviewState::Dismissed)
            .collect();
        for s in &selected {
            self.set_state(&s.id, ReviewState::Confirmed).await?;
        }
        // Each value found where it stands, by the same detection as ingest,
        // replaced by its record's token.
        let tokenise = |text: &str| -> String {
            let findings = crate::pii::regex::regex_stage(text, crate::pii::Locale::Swiss);
            let mut out = text.to_string();
            for f in findings.iter().rev() {
                let Some(s) = selected.iter().find(|s| s.record.kind == f.kind && s.value == f.sample) else {
                    continue;
                };
                if f.end <= out.len() && out.is_char_boundary(f.start) && out.is_char_boundary(f.end) {
                    out.replace_range(f.start..f.end, &format!("[pii:{}]", s.id));
                }
            }
            out
        };
        self.rewrite_documents(&selected, tokenise).await
    }

    /// Takes back the confirmation of the records selected, whoever confirmed
    /// them (her review, or a save in the editor; v2.0.112): suspected again,
    /// the token kept, no version. A record not confirmed is left as it is.
    pub async fn take_back_confirmation(&self, record_ids: &[String]) -> anyhow::Result<ReviewOutcome> {
        for s in self.selected(record_ids).await? {
            if s.record.review_state == ReviewState::Confirmed {
                self.set_state(&s.id, ReviewState::Unreviewed).await?;
            }
        }
        Ok(ReviewOutcome::default())
    }

    // ------------------------------------------------------------ helpers --

    /// The discovered, live records among `record_ids`, each once, with their
    /// values. An id that names no record is an error; a vault entry or a
    /// deleted record is left out.
    async fn selected(&self, record_ids: &[String]) -> anyhow::Result<Vec<Selected>> {
        let mut out: Vec<Selected> = vec![];
        for id in record_ids {
            if out.iter().any(|s| &s.id == id) {
                continue;
            }
            let record = self.store().get_pii_record(id).await.map_err(|e| anyhow::anyhow!("get_pii_record: {e}"))?;
            if record.stored_secret || record.deleted_at.is_some() {
                continue;
            }
            let value = EncryptedBlob::from_pair(record.value_encrypted.clone(), record.value_nonce.clone())
                .decrypt_to_string(&self.account_key)
                .map_err(|e| anyhow::anyhow!("a record's value could not be decrypted: {e}"))?;
            out.push(Selected { id: id.clone(), record, value });
        }
        Ok(out)
    }

    /// Her review sets the state with its own stamp (`review_stamp`), which
    /// ingest never writes: so the review travels by sync, a later review wins,
    /// and a state ingest wrote never replaces it (SW1, SW5).
    async fn set_state(&self, id: &str, state: ReviewState) -> anyhow::Result<()> {
        let stamp = review_stamp_now();
        self.store()
            .update_pii_record_review(id, state, Some(&stamp))
            .await
            .map_err(|e| anyhow::anyhow!("update_pii_record_review: {e}"))
    }

    /// Every document the selected records are found in gets its text fields
    /// passed through `change`, and one version if anything changed (a save
    /// whose origin is her review). A document in the trash or read-only is
    /// left as it is.
    async fn rewrite_documents(
        &self,
        selected: &[Selected],
        change: impl Fn(&str) -> String,
    ) -> anyhow::Result<ReviewOutcome> {
        let mut documents: Vec<String> = vec![];
        for s in selected {
            for d in documents_of(&s.record) {
                if !documents.contains(&d) {
                    documents.push(d);
                }
            }
        }
        let mut outcome = ReviewOutcome::default();
        for id in documents {
            if self.rewrite(&id, &change).await? {
                outcome.rewritten.push(id);
            }
        }
        Ok(outcome)
    }

    async fn rewrite(&self, id: &str, change: &impl Fn(&str) -> String) -> anyhow::Result<bool> {
        let view = match self.docs.read(id).await {
            Ok(v) => v,
            Err(sovereign_docs::DocError::NotFound(_)) | Err(sovereign_docs::DocError::Purged(_)) => return Ok(false),
            Err(e) => return Err(anyhow::anyhow!("{id}: {e}")),
        };
        if view.trashed_at.is_some() || !matches!(view.writable, Writable::Yes) {
            return Ok(false);
        }
        let revision = view.revision.clone();
        let old_title = view.title.clone();
        let Some(mut doc) = view.doc else { return Ok(false) };
        let title = change(&old_title);
        let mut edits: Vec<(TextField, String)> = vec![];
        for b in doc.blocks() {
            let fields = [
                (TextField::Text(b.id()), b.text()),
                (TextField::Caption(b.id()), b.caption()),
                (TextField::Alt(b.id()), b.image_alt()),
            ];
            for (field, text) in fields {
                if let Some(text) = text {
                    let changed = change(text);
                    if changed != text {
                        edits.push((field, changed));
                    }
                }
            }
        }
        if title == old_title && edits.is_empty() {
            return Ok(false);
        }
        for (field, text) in &edits {
            let r = match field {
                TextField::Text(b) => doc.set_text(*b, text),
                TextField::Caption(b) => doc.set_caption(*b, text),
                TextField::Alt(b) => doc.set_image_alt(*b, text),
                TextField::Title => Ok(()),
            };
            r.map_err(|e| anyhow::anyhow!("{id}: {e:?}"))?;
        }
        let edit = Edit {
            base: revision,
            title: (title != old_title).then_some(title),
            content: Some(EditContent::V2(doc.to_json_string())),
            origin: Origin::PiiReview,
            explicit: false,
            pasted_tokens: vec![],
        };
        let out = self.docs.save(id, edit).await.map_err(|e| anyhow::anyhow!("{id}: {e}"))?;
        Ok(out.changed)
    }
}

/// A review's stamp: now, RFC 3339, UTC, to the microsecond, so that two
/// stamps compare as strings.
pub fn review_stamp_now() -> String {
    sovereign_core::clock::now().format("%Y-%m-%dT%H:%M:%S%.6fZ").to_string()
}

/// The documents a record is found in, each once.
fn documents_of(r: &PiiRecord) -> Vec<String> {
    let mut out: Vec<String> = vec![];
    for s in &r.sources {
        if s.source_kind == SourceKind::Document && !out.contains(&s.source_id) {
            out.push(s.source_id.clone());
        }
    }
    out
}
