//! The document service (C-DOC with C-ENC; spec ch. 03, *Document service —
//! contract (C-DOC)*).
//!
//! The one door through which documents are created, read, saved, deleted,
//! restored and purged, so that encoding, commits, personal-data ingest,
//! provenance and change events happen in one place. The suite in `tests/`,
//! written before the service by another author, defines what it must do
//! (`TESTS.md`).
//!
//! Module map:
//! - [`host`] — what the service is given: the store, the commit policy, the
//!   personal-data hook, media hashing, a clock, an event sink;
//! - [`types`] — what goes in and out: trust, provenance, origins, edits,
//!   views, events, reports, errors;
//! - [`service`] — [`DocumentService`] and its operations;
//! - [`hosts`] — what every host runs over the service (the purge tick, the
//!   trash listing);
//! - [`orphans`] — live documents whose thread was deleted here (S5-12);
//! - [`threads`] — the one thread-write path (THREAD-sync S1–S7);
//! - [`unlock_migration`] — the one-time migration at unlock (adoption
//!   step 6, M1–M5).
//!
//! The v2 model is re-exported from `sovereign-docformat`.

pub mod host;
pub mod hosts;
pub mod orphans;
pub mod service;
pub mod threads;
pub mod types;
pub mod unlock_migration;
pub mod wire;

pub use wire::{
    analysis_text, backup_document_from_row, decode_provenance, encode_provenance, normalized_hash, purge_fingerprint,
    purge_marker_wire, BackupDocument, BackupError, BackupReport, IncomingAlert, ModelCheck,
    PurgeApplied, PurgeMarkerWire, Stamp, WireApplied, WireMeta, WireOutcome, WireState,
    MAX_STAMP_AHEAD_SECS,
};
pub use host::{
    change_amount, Clock, CommitContext, CommitDecision, CommitPolicy, DefaultCommitPolicy,
    DocumentEvents, IngestItem, IngestRequest, IngestedItem, MediaFiles, PiiError, PiiIngest,
    ServiceDeps, WritePath, NON_TEXT_BLOCK_WEIGHT,
};
pub use hosts::{
    active_thread, days_left, delete_thread_to_trash, list_trash_named, preview_thread_delete, purge_tick,
    restore_to_thread, ThreadDeleteError,
    ThreadDeletePreview, ThreadDeleteReport, TrashListing, PURGE_TICK_INTERVAL,
};
pub use orphans::{OrphanResolution, OrphanResolved, OrphanedDocument};
pub use threads::{parse_thread_stamp, seed_threads, thread_order, THREAD_MARKER_AFTER_DAYS};
pub use service::DocumentService;
pub use unlock_migration::{
    run_unlock_migration, start_unlock_migration, unlock_migration_complete, unlock_migration_notice_shown,
    unlock_migration_pending_notice, MigrationNotice, MigrationRun, MigrationTask,
};
pub use sovereign_docformat::{Block, BlockId, DocumentV2, EditError, FormatError, SourceForm};
pub use types::*;

/// How long a document stays in the trash before it is purged (spec ch. 03,
/// *Deletion*: 30 days).
pub const TRASH_RETENTION_DAYS: i64 = 30;
