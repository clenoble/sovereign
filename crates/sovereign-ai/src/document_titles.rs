//! The assistant's side of exact document titles (v0.0.12 item 23, T1;
//! `doc/plans/V0012-item23-document-titles.md`): a document named in an
//! action resolves through `sovereign_db::resolve_document_title` — the one
//! exact-title resolution, live documents only — and when it does not
//! resolve, the user (or, for a chat tool, the model) is told why, in words:
//! the close titles to choose from (at most five) and a question, that no
//! document has the title, or the documents that share it, each with its
//! thread and its last modified date, and that the action can be done from
//! the document itself on the canvas.

use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{Document, PiiRecord, SourceKind};
use sovereign_db::{same_document_title, DbResult, DocumentTitleMatch, GraphDB, MAX_CLOSE_TITLES};

/// Why a title named no single document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// No live document has the title (with or without close titles).
    NoMatch(String),
    /// Two or more live documents have it: none was picked.
    Ambiguous(String),
    /// The store could not be read.
    Unreadable(String),
}

impl Unresolved {
    /// The sentence that says why.
    pub(crate) fn message(&self) -> &str {
        match self {
            Unresolved::NoMatch(m) | Unresolved::Ambiguous(m) | Unresolved::Unreadable(m) => m,
        }
    }
}

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// C-TPII (spec ch. 08, *A title that holds personal data*): the one resolver
/// of documents by title, comparing the words given with the three forms of
/// each title (TP1) and counting the matches by document (TP2):
///
/// - **as stored** (a dismissed value stands as written there);
/// - **with the values replaced by their tokens**: each value the save's
///   detection finds in the words given (TP5) is looked up among the
///   personal-data records, which `account_key` opens; for each record of that
///   value, the value is replaced by the record's token and the result is
///   compared with the stored title of the documents the record names. Without
///   a key no record can be read, and this form gives nothing;
/// - **as shown**: each token as its type's label, in the one form of ch. 09
///   SW5 (`Transfer [IBAN]`).
///
/// Live documents only, ignoring case and surrounding spaces, as
/// `sovereign_db::resolve_document_title`.
///
/// The documents it returns are **as stored**; a reply shows their titles as
/// shown ([`Named::shown`], [`shown_title`]). A value is compared with a record's value
/// ignoring case and surrounding spaces, as a title is; without a key the
/// second form is silent (the Fable maille, by delegation of Céline,
/// 2026-11-17).
///
/// The callers go through [`resolve_document_or_explain`], which shares this
/// body ([`forms_with_records`]) and keeps the records it read to show the
/// titles; this entry is the contract's, which its tests call.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) async fn resolve_document_title_forms(
    db: &dyn GraphDB,
    account_key: Option<&AccountKey>,
    title: &str,
) -> DbResult<DocumentTitleMatch> {
    Ok(forms_with_records(db, account_key, title).await?.0)
}

/// As [`resolve_document_title_forms`], with the records it read (which show
/// the titles as shown).
async fn forms_with_records(
    db: &dyn GraphDB,
    account_key: Option<&AccountKey>,
    title: &str,
) -> DbResult<(DocumentTitleMatch, Vec<PiiRecord>)> {
    let wanted = title.trim();
    if wanted.is_empty() {
        return Ok((DocumentTitleMatch::NoMatch { close: vec![] }, vec![]));
    }
    // Deleted records included: their tokens show the placeholder (SW5), as
    // the document service shows them; the second form skips them itself.
    let records = db.list_pii_records_including_deleted().await?;
    let mut matched: Vec<Document> = vec![];
    let mut others: Vec<Document> = vec![];
    let add = |list: &mut Vec<Document>, d: Document| {
        if d.deleted_at.is_none() && !list.iter().any(|x| x.id_string() == d.id_string()) {
            list.push(d);
        }
    };

    // As stored.
    for d in db.document_title_candidates(wanted).await? {
        if same_document_title(&d.title, wanted) {
            add(&mut matched, d);
        } else {
            add(&mut others, d);
        }
    }

    // With the values replaced by their tokens.
    if let Some(key) = account_key {
        for d in by_tokens(db, &records, key, wanted).await? {
            add(&mut matched, d);
        }
    }

    // As shown: candidates by the longest run of words outside the labels,
    // compared once their tokens are shown as labels.
    let shown_candidates = match longest_unlabelled_run(wanted) {
        Some(words) => db.document_title_candidates(&words).await?,
        None => db.list_documents(None).await?,
    };
    for d in shown_candidates {
        if same_document_title(&shown_title_with(&records, &d), wanted) {
            add(&mut matched, d);
        } else {
            add(&mut others, d);
        }
    }

    let ids: Vec<Option<String>> = matched.iter().map(|d| d.id_string()).collect();
    others.retain(|d| !ids.contains(&d.id_string()));
    let m = match matched.len() {
        1 => DocumentTitleMatch::Found(matched.remove(0)),
        0 => {
            // One per distinct title as shown.
            let mut close: Vec<Document> = vec![];
            let mut seen: Vec<String> = vec![];
            for d in others {
                let t = shown_title_with(&records, &d);
                let t = t.trim();
                if !t.is_empty() && !seen.iter().any(|c| same_document_title(c, t)) {
                    seen.push(t.to_string());
                    close.push(d);
                }
                if close.len() == MAX_CLOSE_TITLES {
                    break;
                }
            }
            DocumentTitleMatch::NoMatch { close }
        }
        _ => DocumentTitleMatch::Ambiguous(matched),
    };
    Ok((m, records))
}

/// A document named by its title: the document **as stored**, and its title
/// **as shown** (C-TPII, TP3). A reply names it by `shown`; a write, a
/// revision or a title made from it uses `doc`, so a label never takes the
/// place of a token.
#[derive(Debug, Clone)]
pub(crate) struct Named {
    pub(crate) doc: Document,
    pub(crate) shown: String,
}

/// TP1's second form: the documents whose stored title equals `wanted` once
/// each value the save's detection finds in it (TP5) is replaced by the token
/// of that value's record for the document. A value with no record for a
/// document stays as written there.
async fn by_tokens(db: &dyn GraphDB, records: &[PiiRecord], key: &AccountKey, wanted: &str) -> DbResult<Vec<Document>> {
    let findings = crate::pii::regex::regex_stage(wanted, crate::docs_host::SAVE_LOCALE);
    if findings.is_empty() {
        return Ok(vec![]);
    }
    // For each finding, the records of its value: (record id, document id).
    let mut per_finding: Vec<Vec<(String, String)>> = vec![];
    for f in &findings {
        let mut of_value = vec![];
        for r in records.iter().filter(|r| r.deleted_at.is_none() && r.kind == f.kind) {
            let Some(rid) = r.id_string() else { continue };
            let Ok(value) =
                EncryptedBlob::from_pair(r.value_encrypted.clone(), r.value_nonce.clone()).decrypt_to_string(key)
            else {
                continue;
            };
            if !same_document_title(&value, &f.sample) {
                continue;
            }
            for s in r.sources.iter().filter(|s| s.source_kind == SourceKind::Document) {
                of_value.push((rid.clone(), s.source_id.clone()));
            }
        }
        per_finding.push(of_value);
    }
    let mut doc_ids: Vec<&str> = per_finding.iter().flatten().map(|(_, d)| d.as_str()).collect();
    doc_ids.sort();
    doc_ids.dedup();

    let mut out = vec![];
    for doc_id in doc_ids {
        let mut form = String::with_capacity(wanted.len());
        let mut last = 0;
        for (f, of_value) in findings.iter().zip(&per_finding) {
            if f.start < last || f.end > wanted.len() {
                continue;
            }
            let Some((rid, _)) = of_value.iter().find(|(_, d)| d == doc_id) else { continue };
            form.push_str(&wanted[last..f.start]);
            form.push_str(&format!("[pii:{rid}]"));
            last = f.end;
        }
        form.push_str(&wanted[last..]);
        let Ok(d) = db.get_document(doc_id).await else { continue };
        if same_document_title(&d.title, &form) {
            out.push(d);
        }
    }
    Ok(out)
}

/// `d`'s title as shown (TP1's third form, TP3): each token of a record
/// linked to `d` as its type's label (ch. 09 SW5), as a placeholder when that
/// record is deleted; other token-form text stays as written.
fn shown_title_with(records: &[PiiRecord], d: &Document) -> String {
    let Some(doc_id) = d.id_string() else { return d.title.clone() };
    let title = &d.title;
    let mut out = String::with_capacity(title.len());
    let mut last = 0;
    for t in crate::pii::resolve::parse_tokens(title) {
        let linked = records.iter().find(|r| {
            r.id_string().as_deref() == Some(t.record_id.as_str())
                && r.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == doc_id)
        });
        let label = match linked {
            Some(r) if r.deleted_at.is_none() => crate::pii::resolve::preview_label_for_kind(&r.kind),
            Some(_) => sovereign_docs::types::PLACEHOLDER,
            None => continue,
        };
        out.push_str(&title[last..t.start]);
        out.push_str(label);
        last = t.end;
    }
    out.push_str(&title[last..]);
    out
}

/// The title `d` as shown (C-TPII, TP3), for a caller that holds a document
/// read by its id: each token of a record linked to `d` as its type's label
/// (ch. 09 SW5), as the placeholder when that record is deleted; other
/// token-form text stays as written. It agrees with the document service's
/// `DocumentView::labelled_title`.
pub(crate) async fn shown_title(db: &dyn GraphDB, d: &Document) -> String {
    if crate::pii::resolve::parse_tokens(&d.title).is_empty() {
        return d.title.clone();
    }
    match db.list_pii_records_including_deleted().await {
        Ok(records) => shown_title_with(&records, d),
        Err(_) => d.title.clone(),
    }
}

/// The longest run of `wanted` outside its bracketed groups (a label, the
/// placeholder), trimmed; `None` when nothing is left. It only picks the
/// candidates: the comparison with the title as shown decides.
fn longest_unlabelled_run(wanted: &str) -> Option<String> {
    let mut runs = vec![];
    let mut rest = wanted;
    while let Some(open) = rest.find('[') {
        runs.push(&rest[..open]);
        match rest[open..].find(']') {
            Some(close) => rest = &rest[open + close + 1..],
            None => {
                rest = &rest[open..];
                break;
            }
        }
    }
    runs.push(rest);
    runs.into_iter().map(str::trim).filter(|r| !r.is_empty()).max_by_key(|r| r.len()).map(str::to_string)
}

/// The one live document titled `title` (ignoring case and surrounding
/// spaces; by any of the three forms of C-TPII, which `account_key` opens),
/// or why there is none to act on.
pub(crate) async fn resolve_document_or_explain(
    db: &dyn GraphDB,
    account_key: Option<&AccountKey>,
    title: &str,
) -> Result<Named, Unresolved> {
    let wanted = title.trim();
    let (m, records) = forms_with_records(db, account_key, wanted)
        .await
        .map_err(|e| Unresolved::Unreadable(format!("the documents could not be read ({e})")))?;
    let shown = |d: &Document| shown_title_with(&records, d);
    match m {
        DocumentTitleMatch::Found(d) => Ok(Named { shown: shown(&d), doc: d }),
        DocumentTitleMatch::NoMatch { close } if close.is_empty() => {
            Err(Unresolved::NoMatch(format!("no document has the title '{wanted}'")))
        }
        DocumentTitleMatch::NoMatch { close } => {
            let titles: Vec<String> = close.iter().take(MAX_CLOSE_TITLES).map(|d| format!("'{}'", shown(d).trim())).collect();
            Err(Unresolved::NoMatch(format!(
                "no document is titled exactly '{wanted}'. Close titles: {}. Which one do you mean?",
                titles.join(", ")
            )))
        }
        DocumentTitleMatch::Ambiguous(docs) => {
            let mut listed = vec![];
            for d in docs.iter().take(MAX_CLOSE_TITLES) {
                let thread = match db.get_thread(&d.thread_id).await {
                    Ok(t) if t.deleted_at.is_none() => format!("in '{}'", t.name),
                    _ => "in no thread".to_string(),
                };
                listed.push(format!(
                    "'{}' {thread}, modified {}",
                    shown(d).trim(),
                    crate::local_time::history_format(d.modified_at)
                ));
            }
            let more = if docs.len() > MAX_CLOSE_TITLES {
                format!(" and {} more", docs.len() - MAX_CLOSE_TITLES)
            } else {
                String::new()
            };
            Err(Unresolved::Ambiguous(format!(
                "{} documents are titled '{wanted}': {}{more}. None was picked: the assistant cannot tell them \
                 apart, so do this from the document itself on the canvas.",
                docs.len(),
                listed.join("; ")
            )))
        }
    }
}

