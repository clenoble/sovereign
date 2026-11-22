//! The media migration pass at unlock (contract C-MEDIA, spec ch. 03, *The
//! media store*): what a host runs, in the background, once its document
//! service is built. No sync is applied before it: the service runs the pass
//! before it applies any received state.
//!
//! [`run_media_pass_at_unlock`] runs [`DocumentService::migrate_media_into_store`]
//! unless this store's record says a pass has completed. The record (one store
//! record under [`MEDIA_PASS_RECORD_KEY`], sealed by the encrypted layer under
//! the persona's own keys, never synced, never backed up, like the format
//! conversion's) is written only when the format conversion has completed too:
//! a legacy document converted after the pass could otherwise still name an
//! outside path. Until then every unlock runs the pass again, which finds only
//! what is left.
//!
//! The record of *what* the pass neutralised is not this one: it is the media
//! store's own (`MediaFiles::record_neutralised`), in the persona's area.

use std::sync::Arc;

use serde_json::json;
use sovereign_db::GraphDB as _;

use crate::media::MediaMigrationReport;
use crate::service::{store_err, DocumentService};
use crate::types::DocError;
use crate::unlock_migration::MigrationTask;

/// The key of the media pass's completion record in the persona's store.
/// Names the change, nothing about the persona.
pub const MEDIA_PASS_RECORD_KEY: &str = "media_store_migration";

/// What one call of [`run_media_pass_at_unlock`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaPassRun {
    /// This store's record says the pass has completed: nothing ran.
    AlreadyComplete,
    /// It ran over every document. `recorded`: the completion record was
    /// written (the format conversion had completed too).
    Ran { report: MediaMigrationReport, recorded: bool },
}

impl MediaPassRun {
    /// What the person is told after the pass, when there is anything to
    /// tell (spec v2.0.86, Céline: « Ajouter le registre »): how many were
    /// copied, how many could not be, and that the list of what was not copied
    /// is kept, encrypted, on this device. No path, no title. Worded by the
    /// Fable maille, by delegation of Céline (2026-11-14): only what is built.
    pub fn notice(&self) -> Option<String> {
        let MediaPassRun::Ran { report, .. } = self else { return None };
        let mut parts: Vec<String> = vec![];
        match report.copied {
            0 => {}
            1 => parts.push(
                "1 image or video was copied into Sovereign's encrypted store; the original stays where it is.".into(),
            ),
            n => parts.push(format!(
                "{n} images and videos were copied into Sovereign's encrypted store; the originals stay where they are."
            )),
        }
        match report.neutralised {
            0 => {}
            m => {
                parts.push(if m == 1 {
                    "1 could not be copied.".to_string()
                } else {
                    format!("{m} could not be copied.")
                });
                parts.push(
                    "The list of what was not copied is kept, encrypted, on this device."
                        .into(),
                );
            }
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

/// Runs the media migration pass over this service's store unless its
/// completion record says it has completed. A host runs it in the background
/// at unlock ([`media_pass_task`]); no sync is applied before it, because the
/// service runs the pass itself before it applies any received state.
pub async fn run_media_pass_at_unlock(docs: &DocumentService) -> Result<MediaPassRun, DocError> {
    let mut done = docs.media_pass_done.lock().await;
    if docs.store().get_store_record(MEDIA_PASS_RECORD_KEY).await.map_err(store_err)?.is_some() {
        *done = true;
        return Ok(MediaPassRun::AlreadyComplete);
    }
    let report = docs.media_pass().await?;
    *done = true;
    // Complete only when no legacy document is left for the format conversion
    // to write (it could still name an outside path).
    let recorded = crate::unlock_migration::unlock_migration_complete(docs).await?;
    if recorded {
        let value = json!({ "copied": report.copied, "neutralised": report.neutralised }).to_string();
        docs.store().put_store_record(MEDIA_PASS_RECORD_KEY, &value).await.map_err(store_err)?;
    }
    Ok(MediaPassRun::Ran { report, recorded })
}

/// The text said when a pass fails (wording **[choice]**). No path, no title.
pub const MEDIA_PASS_FAILED: &str = "Some images and videos could not be moved into Sovereign's encrypted store yet. This is tried again before anything is received from your other devices, and at the next unlock.";

/// The media pass as a background task for a host's unlock (C-MEDIA):
/// [`run_media_pass_at_unlock`], then `notify` with what there is to say:
/// its notice, or [`MEDIA_PASS_FAILED`] when it fails (the failure is also
/// logged, without content). The login does not wait for it, so no login
/// takes longer for a persona whose documents hold media (SIDECHANNEL-001);
/// the order before sync is held by the service itself, which runs the pass
/// before it applies any received state.
pub fn media_pass_task<N>(docs: Arc<DocumentService>, notify: N) -> MigrationTask
where
    N: FnOnce(String) + Send + 'static,
{
    Box::pin(async move {
        match run_media_pass_at_unlock(&docs).await {
            Ok(run) => {
                if let Some(text) = run.notice() {
                    notify(text);
                }
            }
            Err(e) => {
                // A store error; it names no document title, content or path.
                tracing::warn!("the media migration pass failed ({e}); it runs again before any sync and at the next unlock");
                notify(MEDIA_PASS_FAILED.to_string());
            }
        }
    })
}
