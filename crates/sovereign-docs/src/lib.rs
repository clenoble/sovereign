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
//! - [`media`] — the media store (C-MEDIA): lexical refusals, resolution
//!   inside the store, streamed and bounded reads;
//! - [`hosts`] — what every host runs over the service (the purge tick, the
//!   trash listing);
//! - [`orphans`] — live documents whose thread was deleted here (S5-12);
//! - [`threads`] — the one thread-write path (THREAD-sync S1–S7);
//! - [`unlock_migration`] — the one-time migration at unlock (adoption
//!   step 6, M1–M5).
//!
//! The v2 model is re-exported from `sovereign-docformat`.

pub mod history_names;
pub mod host;
pub mod hosts;
pub mod media;
mod media_pass;
mod moves;
pub mod orphans;
pub mod service;
pub mod threads;
pub mod types;
pub mod unlock_migration;
pub mod wire;
// v0.0.12 lot 1 (C1 at the choke point): tests written before the code, by
// another author; the implementer does not edit them.
#[cfg(test)]
mod lot1_clock_tests;
#[cfg(test)]
mod history_names_tests;

pub use wire::{
    analysis_text, backup_document_from_row, decode_provenance, encode_provenance, normalized_hash,
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
pub use media::{
    check_media_name, check_outside_path, copy_stream, hex_digest, is_medium, is_svg, resolve_in_store, sha256_in_store,
    sha256_stream, stream_bounded, MediaError, MediaMark, MediaMigrationReport, MediaPresence, NeutralisedMedium,
    NeutralisedReason, StoredMedium, MEDIA_SIZE_CAP, MEDIUM_SNIFF_BYTES, SVG_SNIFF_BYTES, neutralised_reason_of,
};
pub use media_pass::{media_pass_task, run_media_pass_at_unlock, MediaPassRun, MEDIA_PASS_FAILED, MEDIA_PASS_RECORD_KEY};
pub use orphans::{OrphanResolution, OrphanResolved, OrphanedDocument};
pub use threads::{parse_thread_stamp, seed_threads, thread_order, THREAD_MARKER_AFTER_DAYS};
pub use service::DocumentService;
pub use unlock_migration::{
    run_unlock_migration, start_unlock_migration, unlock_migration_complete, unlock_migration_notice_shown,
    unlock_migration_pending_notice, unlock_migration_undecryptable, unreadable_line, unreadable_line_in,
    MigrationNotice, MigrationRun, MigrationTask, UNREADABLE_HEADING, UNREADABLE_MEANING,
};

// What the Alerts window and the app's Alerts say in a list's place when that
// list could not be read (spec ch. 03, *What cannot be read is never shown as
// empty*): one wording, for both hosts. The words were decided on 2026-11-17
// by the Fable maille, by delegation of Céline.

/// Said in place of the alerts on changes from paired devices when they
/// could not be read.
pub const COULD_NOT_CHECK_ALERTS: &str = "Could not check for suspicious changes from your devices.";

/// Said in place of the documents whose thread was deleted when they could
/// not be read.
pub const COULD_NOT_CHECK_ORPHANS: &str = "Could not check for documents whose thread was deleted.";

/// Said under [`UNREADABLE_HEADING`], without [`UNREADABLE_MEANING`], when
/// the list of documents that could not be read could not itself be read.
pub const COULD_NOT_CHECK_UNREADABLE: &str = "Could not check for unreadable documents.";

// What a document's History says when it cannot be read, whole or one version
// (spec ch. 03, *What cannot be read is never shown as empty*; item C5 of
// v0.0.13, decided 2026-11-18 by the Fable maille, by delegation of Céline):
// worded here, for every host; the frontend holds no copy. The first sentence
// is the spec's; the two others were decided on 2026-11-18 by the Fable
// maille, by delegation of Céline: a singular and a plural, never « (s) »,
// which a screen reader says or drops.

/// Said in place of a document's versions when they could not be read, never
/// « No saved versions yet ».
pub const COULD_NOT_READ_HISTORY: &str = "This document's history could not be read.";

/// Said in place of one version's title and preview when that version could
/// not be read; never its ciphertext.
pub const VERSION_COULD_NOT_BE_READ: &str = "This version could not be read.";

/// What the phone's sync alerts say of the documents whose history this
/// session could not read: `n` of them, at least one.
pub fn histories_not_read_sentence(n: usize) -> String {
    if n == 1 {
        "The history of 1 document could not be read.".to_string()
    } else {
        format!("The histories of {n} documents could not be read.")
    }
}

pub use sovereign_db::encrypted::UndecryptableDocument;
pub use sovereign_docformat::{Block, BlockId, DocumentV2, EditError, FormatError, SourceForm};
pub use types::*;

/// How long a document stays in the trash before it is purged (spec ch. 03,
/// *Deletion*: 30 days).
pub const TRASH_RETENTION_DAYS: i64 = 30;
