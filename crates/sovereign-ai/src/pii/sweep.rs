//! C-SWEEP (spec ch. 09, *Idle sweep*): the personal-data sweep, one routine
//! in the crate both hosts link (SW9), run by the shell and by the phone app.
//!
//! The sweep is the ingest that comes late (SW1, SW7; Céline, 2026-11-17:
//! « C'est le même processus »): each document not yet analysed goes through
//! the document service's [`DocumentService::sweep`], which replaces each value
//! detected by the regex stage (SW8) by its token, suspected, keeps the raw
//! original encrypted, makes one version tagged `pii-sweep` and marks the
//! document analysed.
//!
//! **When it runs** (SW1): after a login, once the session is open, and after
//! a sync round that brought content not yet analysed, in bounded batches; on
//! the phone, only while the app is open.
//!
//! *Signatures by the test author of lot LS(a); the bodies are the implementer's.*

use sovereign_docs::{DocumentService, PiiOutcome};

/// What one pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// The documents analysed by this pass, in the order they were swept.
    pub analysed: Vec<String>,
    /// The documents this pass could not analyse (left not analysed; the next
    /// pass tries them again).
    pub failed: Vec<String>,
    /// Whether documents not yet analysed remain after this pass (the batch
    /// was full).
    pub remaining: bool,
}

/// One bounded pass: at most `batch` documents not yet analysed
/// ([`DocumentService::unanalysed`]), each swept ([`DocumentService::sweep`]).
/// No model is called (SW8).
///
/// `remaining` says that more documents than the batch were waiting when the
/// pass began: a host runs the next pass then. A document that fails stays not
/// analysed and is tried again by a later pass; it is named in `failed`, never
/// its text. A document another write analysed between the listing and its
/// sweep is in neither list.
pub async fn run_sweep_pass(docs: &DocumentService, batch: usize) -> SweepReport {
    let mut report = SweepReport::default();
    if batch == 0 {
        return report;
    }
    let mut todo = match docs.unanalysed(batch.saturating_add(1)).await {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("personal-data sweep: the documents not yet analysed could not be listed ({e})");
            return report;
        }
    };
    report.remaining = todo.len() > batch;
    todo.truncate(batch);
    for id in todo {
        match docs.sweep(&id).await {
            Ok(out) => match out.pii {
                PiiOutcome::Scanned => report.analysed.push(id),
                PiiOutcome::NotRun => {}
                PiiOutcome::Failed { message } => {
                    tracing::warn!("personal-data sweep: {id} not analysed ({message})");
                    report.failed.push(id);
                }
            },
            Err(e) => {
                tracing::warn!("personal-data sweep: {id} not analysed ({e})");
                report.failed.push(id);
            }
        }
    }
    report
}
