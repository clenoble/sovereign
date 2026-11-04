//! The one-time migration at unlock (C-DOC adoption step 6, contract M1–M5 in
//! `doc/plans/C-DOC-adoption.md`, *Step 6 — the one-time migration and the
//! end of the tolerant read*; spec ch. 03, *Content encoding*, decided by
//! Céline in v2.0.42: it runs once, in the background, at the first unlock
//! after the update, then a notice).
//!
//! Tests: `tests/step6_unlock_migration.rs`, `tests/step6_scans.rs`,
//! `sovereign-app/tests/step6_duress_disk.rs` (`TESTS-step6.md`).
//!
//! What a host does (M4): at every unlock, once its document service is
//! built, it calls [`start_unlock_migration`] with its own way of spawning a
//! task and its own way of showing a notice. The seam takes no persona, no
//! path and no clock (M5: the per-store record is the store's own; nothing
//! here can tell which persona it serves).
//!
//! **The record.** One store record (`GraphDB::put_store_record`) under
//! [`RECORD_KEY`], written exactly when a run completes: the counts of the
//! notice and whether it has been shown (M1, M2b). Its presence is what
//! "completed" means. It is a few numbers, a flag, a version and a list of
//! ids (below), the same shape in every persona's store, sealed at rest by
//! the encrypted layer under the persona's own keys (M8), never synced and
//! never backed up. A record found in clear
//! (written below the encrypted layer, as only the unreleased first step-6
//! build did) reads as absent: the migration runs again, idempotently.
//!
//! **The counts.** `converted` is every document the migration converted in
//! this store over all its runs (M3: an interrupted run's conversions are
//! counted by the run that finishes): the documents this run converted, plus
//! the v2 documents whose `last_writer` is still the migration's (converted
//! by an earlier run, not written since). A document converted by a save
//! before the migration reached it is not counted (G3: that write converted
//! it). `not_converted` is the documents that could not be read (a newer
//! format is left out of both counts): those whose content does not parse
//! (V6) and those that cannot be decrypted (v0.0.12 item 7, V1).
//!
//! **The undecryptable rows** (v0.0.12 item 7). The record also keeps the
//! build version that wrote it (`DocumentService::with_build_version`, the
//! host's) and the rows found and not decrypted, by id and last modified date
//! (no title, no content): the hosts' Alerts list them
//! ([`unlock_migration_undecryptable`]). At an unlock under **another** build
//! version those ids, and only those, are tried again; a row that now
//! decrypts is converted like any other and leaves the list (V3). A record
//! written before v0.0.12 has no list: its first unlock under this build
//! looks for undecryptable rows, lists them and converts nothing.

use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sovereign_db::encrypted::UndecryptableDocument;
use sovereign_db::GraphDB;

use crate::service::{store_err, DocumentService, MigrationScope};
use crate::types::DocError;

/// The key of the one-time migration's record in the persona's store. Names
/// the format change, nothing about the persona (M5).
pub const RECORD_KEY: &str = "doc_format_v2_migration";

/// What the person is told when the migration completes having converted at
/// least one document, or having found at least one it could not read (M2
/// revised, M3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationNotice {
    /// Documents converted to format v2 by the migration, over every run of it
    /// in this store (an interrupted run and the one that finished it, M3).
    pub converted: usize,
    /// Documents it could not read, left as they are (M3).
    pub not_converted: usize,
}

impl MigrationNotice {
    /// The notice's text (M2, wording **[choice]**): "N documents converted to
    /// the new format — each one's earlier state is in its History." and, when
    /// some could not be read, "…; M could not be read and were left as they
    /// are"; with N = 0, only "M documents could not be read and were left as
    /// they are." No document title or text.
    pub fn text(&self) -> String {
        let converted = match self.converted {
            0 => None,
            1 => Some("1 document converted to the new format — its earlier state is in its History".to_string()),
            n => Some(format!("{n} documents converted to the new format — each one's earlier state is in its History")),
        };
        let unread = match (self.not_converted, converted.is_some()) {
            (0, _) => None,
            (1, true) => Some("1 could not be read and was left as it is".to_string()),
            (m, true) => Some(format!("{m} could not be read and were left as they are")),
            (1, false) => Some("1 document could not be read and was left as it is".to_string()),
            (m, false) => Some(format!("{m} documents could not be read and were left as they are")),
        };
        match (converted, unread) {
            (Some(c), Some(u)) => format!("{c}; {u}."),
            (Some(c), None) => format!("{c}."),
            (None, Some(u)) => format!("{u}."),
            (None, None) => String::new(),
        }
    }

    /// Whether there is anything to tell (M2 revised: N ≥ 1 or M ≥ 1).
    fn worth_telling(&self) -> bool {
        self.converted > 0 || self.not_converted > 0
    }
}

/// What one call of [`run_unlock_migration`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationRun {
    /// This store's record says the migration has completed: nothing ran
    /// (M1: later unlocks do not run it again).
    AlreadyComplete,
    /// It ran to its end and the store's record now says so, with the counts
    /// and the notice not yet shown (M2b). `notice` is `None` when it
    /// converted nothing over all its runs and found nothing unreadable (M2
    /// revised: N = 0 and M = 0, no notice).
    Completed { notice: Option<MigrationNotice> },
    /// It stopped before its end (a store failure); the record is not written
    /// and the next unlock resumes it (M3). `failed`: the documents a store
    /// failure stopped (a count only, for the log).
    Interrupted { failed: usize },
}

/// The record as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Record {
    notice: MigrationNotice,
    shown: bool,
    /// The build version of the run that last wrote the record (v0.0.12 item
    /// 7, V3); `None` when the host gave none, or in a record written before.
    build_version: Option<String>,
    /// The rows found and not decrypted, left as they were (V1, V2), retried
    /// at an unlock under another build version (V3). `None`: a record
    /// written before v0.0.12 (Readings, 1), whose list is unknown, not empty.
    undecryptable: Option<Vec<UndecryptableDocument>>,
}

impl Record {
    fn to_json(&self) -> String {
        let mut v = json!({
            "converted": self.notice.converted,
            "not_converted": self.notice.not_converted,
            "notice_shown": self.shown,
            "build_version": self.build_version,
        });
        if let Some(list) = &self.undecryptable {
            v["undecryptable"] = list
                .iter()
                .map(|u| json!({ "id": u.id, "modified_at": u.modified_at.to_rfc3339(), "deleted": u.deleted }))
                .collect();
        }
        v.to_string()
    }

    /// A record that cannot be read still says the migration completed (its
    /// presence is the completion); it has nothing left to tell.
    fn from_json(s: &str) -> Record {
        let v: Value = serde_json::from_str(s).unwrap_or(Value::Null);
        let count = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
        let undecryptable = v.get("undecryptable").and_then(Value::as_array).map(|list| {
            list.iter()
                .filter_map(|e| {
                    let id = e.get("id")?.as_str()?.to_string();
                    let modified_at = e
                        .get("modified_at")
                        .and_then(Value::as_str)
                        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
                        .map(|t| t.with_timezone(&Utc))
                        .unwrap_or_default();
                    let deleted = e.get("deleted").and_then(Value::as_bool).unwrap_or(false);
                    Some(UndecryptableDocument { id, modified_at, deleted })
                })
                .collect()
        });
        Record {
            notice: MigrationNotice { converted: count("converted"), not_converted: count("not_converted") },
            shown: v.get("notice_shown").and_then(Value::as_bool).unwrap_or(true),
            build_version: v.get("build_version").and_then(Value::as_str).map(str::to_string),
            undecryptable,
        }
    }

    /// The notice to record after a later pass found something to tell:
    /// `fresh`, plus an earlier notice not yet shown (M2b: never lost).
    /// `retried`: `fresh` is a retry of rows the earlier notice already
    /// counted as not read — those it converted move from one count to the
    /// other; otherwise (a first listing) its rows are new to the count.
    fn merged(&self, fresh: MigrationNotice, retried: bool) -> MigrationNotice {
        if self.shown || !self.notice.worth_telling() {
            return fresh;
        }
        let not_converted = if retried {
            self.notice.not_converted.saturating_sub(fresh.converted)
        } else {
            self.notice.not_converted + fresh.not_converted
        };
        MigrationNotice { converted: self.notice.converted + fresh.converted, not_converted }
    }
}

async fn read_record(docs: &DocumentService) -> Result<Option<Record>, DocError> {
    let raw = docs.store().get_store_record(RECORD_KEY).await.map_err(store_err)?;
    Ok(raw.as_deref().map(Record::from_json))
}

async fn write_record(docs: &DocumentService, record: &Record) -> Result<(), DocError> {
    docs.store().put_store_record(RECORD_KEY, &record.to_json()).await.map_err(store_err)
}

/// Runs the one-time migration over this service's store unless the store's
/// record says it has completed; on completion writes that record (M1) and
/// returns the notice (M2, M3). Every document is left either converted or
/// untouched (G3).
pub async fn run_unlock_migration(docs: &DocumentService) -> Result<MigrationRun, DocError> {
    // v0.0.12 item 3, L12: every stored purge marker's fingerprint is erased
    // at unlock — before the early return below, so a store whose format
    // conversion completed under v0.0.11 is erased too. Idempotent; the purge
    // tick and the manifest's read of the markers erase as well.
    docs.erase_marker_fingerprints().await?;
    let Some(record) = read_record(docs).await? else {
        return first_run(docs).await;
    };
    match &record.undecryptable {
        // Completed before v0.0.12: the rows it could not decrypt were never
        // counted. Moving to this build is a version change (Readings, 1):
        // they are looked for and listed, once, converting nothing (M1).
        None => list_undecryptable(docs, record).await,
        // v0.0.12 item 7, V3: retried after a version change only.
        Some(list) if !list.is_empty() && version_changed(docs, &record) => retry(docs, record).await,
        Some(_) => Ok(MigrationRun::AlreadyComplete),
    }
}

/// Whether this service runs under another build than the one the record
/// names. A service whose host gave no version never retries.
fn version_changed(docs: &DocumentService, record: &Record) -> bool {
    docs.build_version().is_some_and(|v| record.build_version.as_deref() != Some(v))
}

/// The first run (and its resumptions, M3): every document.
async fn first_run(docs: &DocumentService) -> Result<MigrationRun, DocError> {
    let (report, listed) = docs.migration_pass(MigrationScope::All).await?;
    if !report.failed.is_empty() {
        // A store failure on a document is an interruption, not an unreadable
        // document: nothing is recorded, the next unlock converts what is left.
        return Ok(MigrationRun::Interrupted { failed: report.failed.len() });
    }
    let notice = MigrationNotice {
        converted: report.migrated.len() + report.previously_migrated.len(),
        // Unparseable rows (V6) and, beside them, undecryptable ones (V1).
        not_converted: report.unreadable.len() + report.undecryptable.len(),
    };
    let worth = notice.worth_telling();
    // Nothing to tell is recorded as already shown: nothing pending (M2).
    let record = Record {
        notice: notice.clone(),
        shown: !worth,
        build_version: docs.build_version().map(str::to_string),
        undecryptable: Some(listed),
    };
    write_record(docs, &record).await?;
    Ok(MigrationRun::Completed { notice: worth.then_some(notice) })
}

/// A record written before v0.0.12 (no list): the rows that cannot be
/// decrypted are listed, nothing is converted, and a notice says once how
/// many could not be read.
async fn list_undecryptable(docs: &DocumentService, record: Record) -> Result<MigrationRun, DocError> {
    let (_, listed) = docs.migration_pass(MigrationScope::ListOnly).await?;
    let found = listed.len();
    let mut next = Record {
        build_version: docs.build_version().map(str::to_string),
        undecryptable: Some(listed),
        ..record.clone()
    };
    let notice = (found > 0).then(|| record.merged(MigrationNotice { converted: 0, not_converted: found }, false));
    if let Some(n) = &notice {
        next.notice = n.clone();
        next.shown = false;
    }
    write_record(docs, &next).await?;
    Ok(MigrationRun::Completed { notice })
}

/// The retry after a version change (V3): the listed ids only. A row that
/// now decrypts is converted like any other and leaves the list; one that
/// still does not stays, under the new version. A retry that converts says
/// so in a notice (Readings, 4).
async fn retry(docs: &DocumentService, record: Record) -> Result<MigrationRun, DocError> {
    let ids: HashSet<String> =
        record.undecryptable.iter().flatten().map(|u| u.id.clone()).collect();
    let (report, listed) = docs.migration_pass(MigrationScope::Only(&ids)).await?;
    if !report.failed.is_empty() {
        // Interrupted: the record is left as it was, so the next unlock
        // retries again.
        return Ok(MigrationRun::Interrupted { failed: report.failed.len() });
    }
    let mut next = Record {
        build_version: docs.build_version().map(str::to_string),
        undecryptable: Some(listed),
        ..record.clone()
    };
    let converted = report.migrated.len();
    let notice = (converted > 0).then(|| {
        let still = report.undecryptable.len() + report.unreadable.len();
        record.merged(MigrationNotice { converted, not_converted: still }, true)
    });
    if let Some(n) = &notice {
        next.notice = n.clone();
        next.shown = false;
    }
    write_record(docs, &next).await?;
    Ok(MigrationRun::Completed { notice })
}

/// The documents this store's completion record lists as found and not
/// decrypted (v0.0.12 item 7, V2: the Alerts window lists them by a short id
/// and their last modified date). Empty before completion and once a retry
/// has converted them all. Read from the store: each persona's own.
pub async fn unlock_migration_undecryptable(docs: &DocumentService) -> Result<Vec<UndecryptableDocument>, DocError> {
    Ok(read_record(docs).await?.and_then(|r| r.undecryptable).unwrap_or_default())
}

/// The heading of the hosts' list of documents that could not be read
/// (v0.0.12 item 7 V2b, B1: one wording, used by the shell and the app).
pub const UNREADABLE_HEADING: &str = "Documents that could not be read";

/// What the list means, in one line (B1): kept as it was, tried again after
/// the next update.
pub const UNREADABLE_MEANING: &str =
    "Each one is kept as it was, untouched. Sovereign will try to read it again after its next update.";

/// One document's line in that list, its date in the device's local time
/// (B1): "Document <short id>[ (in the trash)] — last modified YYYY-MM-DD HH:MM".
/// No title: none can be read.
pub fn unreadable_line(u: &UndecryptableDocument) -> String {
    unreadable_line_in(u, &chrono::Local)
}

/// [`unreadable_line`] with the date in `tz`. The short id is the first 8
/// characters of the id's key (after the last `:`, without `⟨ ⟩`).
pub fn unreadable_line_in<Tz: chrono::TimeZone>(u: &UndecryptableDocument, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let key = u.id.rsplit(':').next().unwrap_or(&u.id).trim_matches(|c| c == '\u{27e8}' || c == '\u{27e9}');
    let short_id: String = key.chars().take(8).collect();
    let trash = if u.deleted { " (in the trash)" } else { "" };
    let when = u.modified_at.with_timezone(tz).format("%Y-%m-%d %H:%M");
    format!("Document {short_id}{trash} \u{2014} last modified {when}")
}

/// Whether this service's store records that the one-time migration has
/// completed (M1). The record is in the store: a new service over the same
/// store reads it, another store (another persona) has its own.
pub async fn unlock_migration_complete(docs: &DocumentService) -> Result<bool, DocError> {
    Ok(read_record(docs).await?.is_some())
}

/// The notice this store's record holds as **not yet shown** (M2b): the
/// counts of the completed migration, until [`unlock_migration_notice_shown`]
/// is called. `None` before completion, when there was nothing to tell, and
/// once shown. Read from the store: it survives a restart.
pub async fn unlock_migration_pending_notice(docs: &DocumentService) -> Result<Option<MigrationNotice>, DocError> {
    Ok(read_record(docs)
        .await?
        .filter(|r| !r.shown && r.notice.worth_telling())
        .map(|r| r.notice))
}

/// Records in this store that the migration's notice has been shown (M2b).
/// Nothing to do before completion or once shown.
pub async fn unlock_migration_notice_shown(docs: &DocumentService) -> Result<(), DocError> {
    match read_record(docs).await? {
        Some(r) if !r.shown => write_record(docs, &Record { shown: true, ..r }).await,
        _ => Ok(()),
    }
}

/// A task a host spawns on its runtime.
pub type MigrationTask = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

/// The host seam (M1, M4): called by the shell and the phone at every unlock,
/// after the persona's document service is built. It **returns at once**,
/// having touched nothing: the work is the task handed to `spawn`, which runs
/// [`run_unlock_migration`] and then, if the record holds a notice not yet
/// shown — from this run, or from an earlier one whose process died before
/// showing it (M2b) — calls `notify` with it and records it shown. The unlock
/// and the canvas never wait for it.
///
/// A run that fails or is interrupted shows nothing and records nothing: the
/// next unlock resumes it. Dropping the task (a lock, a quit, a later unlock
/// replacing it) is an interruption too.
pub fn start_unlock_migration<S, N>(docs: Arc<DocumentService>, spawn: S, notify: N)
where
    S: FnOnce(MigrationTask),
    N: FnOnce(MigrationNotice) + Send + 'static,
{
    spawn(Box::pin(async move {
        match run_unlock_migration(&docs).await {
            Ok(MigrationRun::Completed { .. }) | Ok(MigrationRun::AlreadyComplete) => {}
            Ok(MigrationRun::Interrupted { failed }) => {
                tracing::warn!(
                    "document format migration interrupted: {failed} document(s) not converted after a store failure; it resumes at the next unlock"
                );
                return;
            }
            Err(e) => {
                // A store error; it names no document title or content.
                tracing::warn!("document format migration failed ({e}); it resumes at the next unlock");
                return;
            }
        }
        if let Ok(Some(notice)) = unlock_migration_pending_notice(&docs).await {
            notify(notice);
            // Recorded after it is shown: a stop in between shows it once
            // more at the next unlock rather than never (M2b).
            let _ = unlock_migration_notice_shown(&docs).await;
        }
    }));
}
