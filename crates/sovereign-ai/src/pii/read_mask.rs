//! C-SWEEP SW3 (spec ch. 09): masked where it is read, for every path to the
//! model. The mask is computed on the text as it is read, each time, with the
//! content's id, by the same detection as ingest (the regex stage, Swiss
//! locale). It rests neither on the sweep having passed nor on stored spans.
//!
//! - a token becomes its type's label (`[IBAN]`);
//! - a detected value becomes its type's label;
//! - a value she dismissed (SW5) in that content is left as written, so that
//!   her dismissal reaches the assistant too.
//!
//! Without the persona's account key the dismissed values cannot be read, so
//! none is let through: every detected value is masked.

use std::collections::HashMap;

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState, SourceKind};
use sovereign_db::traits::GraphDB;

use super::resolve::{preview_label_for_kind, resolve_to_preview};
use super::Locale;

/// What masking at read needs, read once for a prompt.
#[derive(Default)]
pub struct ReadMask {
    records: Vec<PiiRecord>,
    /// The dismissed values of each content, by its id.
    dismissed: HashMap<String, Vec<(PiiKind, String)>>,
}

impl ReadMask {
    /// The records of the persona's store, and, with its account key, the
    /// values she dismissed. A store that cannot be read gives an empty mask
    /// over the records, which still masks every detected value.
    pub async fn load(db: &dyn GraphDB, account_key: Option<&AccountKey>) -> Self {
        let records = db.list_pii_records(None, None, None).await.unwrap_or_default();
        let mut dismissed: HashMap<String, Vec<(PiiKind, String)>> = HashMap::new();
        if let Some(key) = account_key {
            for r in records.iter().filter(|r| {
                r.review_state == ReviewState::Dismissed && r.deleted_at.is_none() && !r.stored_secret
            }) {
                let Ok(value) =
                    EncryptedBlob::from_pair(r.value_encrypted.clone(), r.value_nonce.clone()).decrypt_to_string(key)
                else {
                    continue;
                };
                for s in r.sources.iter().filter(|s| s.source_kind == SourceKind::Document) {
                    dismissed.entry(s.source_id.clone()).or_default().push((r.kind.clone(), value.clone()));
                }
            }
        }
        Self { records, dismissed }
    }

    /// `text` of the content `content_id`, masked.
    pub fn mask(&self, content_id: &str, text: &str) -> String {
        let allowed = self.dismissed.get(content_id).map(Vec::as_slice).unwrap_or(&[]);
        self.mask_allowing(text, |kind, value| allowed.iter().any(|(k, v)| k == kind && v == value))
    }

    /// Text that belongs to no content (the assistant's own words, read back
    /// into the next prompt): masked, with every value she dismissed anywhere
    /// left as written.
    pub fn mask_unattributed(&self, text: &str) -> String {
        self.mask_allowing(text, |kind, value| {
            self.dismissed.values().flatten().any(|(k, v)| k == kind && v == value)
        })
    }

    fn mask_allowing(&self, text: &str, allowed: impl Fn(&PiiKind, &str) -> bool) -> String {
        let mut out = resolve_to_preview(text, &self.records);
        let findings = super::regex::regex_stage(&out, Locale::Swiss);
        for f in findings.iter().rev() {
            if allowed(&f.kind, &f.sample) {
                continue;
            }
            if f.start <= f.end && f.end <= out.len() && out.is_char_boundary(f.start) && out.is_char_boundary(f.end) {
                out.replace_range(f.start..f.end, preview_label_for_kind(&f.kind));
            }
        }
        out
    }
}
