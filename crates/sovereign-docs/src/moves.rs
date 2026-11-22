//! Moving a document to another thread (v0.0.12 lanes lot, item 15,
//! `doc/plans/V0012-lot-lanes.md`, M1 and M2).
//!
//! The one public move: under the write lock and stamped (the crate-private
//! `move_locked` of orphan resolution), so a move is a state like any write
//! and travels by sync (M3). Every host moves through it: the shell's drag,
//! the assistant (orchestrator and tool), the Tauri command, and the restore
//! into a thread, which is one operation here
//! ([`DocumentService::restore_from_trash_into`]). No production code outside
//! the store calls `GraphDB::move_document_to_thread` (the scan in
//! `tests/lanes_m2_one_move_path.rs`).
//!
//! Tests: `tests/lanes_m1_move.rs`, `tests/lanes_m2_one_move_path.rs`;
//! two devices in `sovereign-p2p/tests/wire_lanes_moves.rs`.

use sovereign_db::schema::DocumentPatch;
use sovereign_db::traits::GraphDB;

use crate::hosts::active_thread;
use crate::service::{store_err, DocumentService};
use crate::types::{ChangeKind, DocError};

/// The refusal of a thread that does not exist or was deleted.
const NO_SUCH_THREAD: &str =
    "That thread does not exist (it was deleted, or never existed): choose an existing thread. Nothing was moved.";

/// The refusal of a move to the thread the document is already in.
const SAME_THREAD: &str = "The document is already in that thread: nothing was moved.";

impl DocumentService {
    /// Moves document `id` to thread `thread_id` (M1): a stamped write of its
    /// thread alone — title, content and versions unchanged — under the
    /// write lock; it emits the change, and the move travels by sync.
    ///
    /// Refused, with its reason, and nothing written:
    /// - a document in the trash ([`DocError::InTrash`]) or purged
    ///   ([`DocError::Purged`]), or unknown ([`DocError::NotFound`]);
    /// - a thread that does not exist or is deleted
    ///   ([`DocError::MoveRefused`]);
    /// - the thread it is already in, which would change nothing
    ///   ([`DocError::MoveRefused`], no stamp).
    ///
    /// A document whose thread was deleted (an orphan) can be moved to any
    /// live thread: that is how it leaves the Unfiled lane.
    pub async fn move_to_thread(&self, id: &str, thread_id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if d.deleted_at.is_some() {
            return Err(DocError::InTrash(id.to_string()));
        }
        let target = self.live_thread(thread_id).await?;
        if d.thread_id == target {
            return Err(DocError::MoveRefused(SAME_THREAD.into()));
        }
        self.refuse_ended(&target).await?;
        self.move_locked(id, &target).await?;
        Ok(())
    }

    /// Brings document `id` back from the trash into thread `thread_id` (M2):
    /// one stamped state under one lock — out of the trash and in the thread
    /// — replacing the trashed state, with no intermediate state (moved but
    /// still trashed, or restored into its old thread). Into the thread it was
    /// already in, it is a plain restore.
    ///
    /// Refused as a whole, nothing written: a document not in the trash
    /// ([`DocError::NotInTrash`]), purged or unknown; a thread that does not
    /// exist or is deleted ([`DocError::MoveRefused`]).
    pub async fn restore_from_trash_into(&self, id: &str, thread_id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let d = self.load(id).await?;
        if d.deleted_at.is_none() {
            return Err(DocError::NotInTrash(id.to_string()));
        }
        let target = self.live_thread(thread_id).await?;
        // C-MERGE MG3: a restore goes back to the document's own thread, even
        // ended; into another thread, an ended one is refused.
        if d.thread_id != target {
            self.refuse_ended(&target).await?;
        }
        let mut patch = DocumentPatch { deleted_at: Some(None), ..Default::default() };
        if d.thread_id != target {
            patch.thread_id = Some(target);
        }
        self.stamp_local(&d, &mut patch).await?;
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::RestoredFromTrash);
        Ok(())
    }

    /// The id (raw form, as the store gives it) of live thread `thread_id`,
    /// or the refusal of an unknown or deleted one.
    async fn live_thread(&self, thread_id: &str) -> Result<String, DocError> {
        match active_thread(self.store(), thread_id).await.map_err(store_err)? {
            Some(t) => Ok(t.id_string().unwrap_or_else(|| thread_id.to_string())),
            None => Err(DocError::MoveRefused(NO_SUCH_THREAD.into())),
        }
    }

    /// C-MERGE MG3: an ended thread takes no new or moved document. Refused
    /// with the contract's answer to a request that names it.
    pub(crate) async fn refuse_ended(&self, thread_id: &str) -> Result<(), DocError> {
        let Some(t) = active_thread(self.store(), thread_id).await.map_err(store_err)? else { return Ok(()) };
        if !t.is_ended() {
            return Ok(());
        }
        let answer = sovereign_db::ended_thread_answer(self.store(), &t, &sovereign_db::ended_day(&t)).await.map_err(store_err)?;
        Err(DocError::MoveRefused(answer))
    }

    /// C-MERGE MG3: where a copy of a document of thread `thread_id` goes —
    /// the thread itself, or its live end when it has ended (« a duplicate of
    /// one of its documents go[es] to its live end »); refused with the
    /// contract's sentence when it has none. An unknown or deleted thread is
    /// left as it is (the fallback lane, as before).
    pub(crate) async fn live_end_of(&self, thread_id: &str) -> Result<String, DocError> {
        let Some(t) = active_thread(self.store(), thread_id).await.map_err(store_err)? else {
            return Ok(thread_id.to_string());
        };
        if !t.is_ended() {
            return Ok(thread_id.to_string());
        }
        match sovereign_db::live_end(self.store(), &t).await.map_err(store_err)? {
            Some(end) => Ok(end.id_string().unwrap_or_else(|| thread_id.to_string())),
            None => {
                let target = sovereign_db::merged_into_name(self.store(), &t).await;
                Err(DocError::MoveRefused(sovereign_db::no_live_end_words(t.name.trim(), &target)))
            }
        }
    }
}
