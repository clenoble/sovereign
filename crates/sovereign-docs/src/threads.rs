//! The one thread-write path (`doc/plans/THREAD-sync.md`, S1–S11, decided by
//! Céline 2026-10-18; spec ch. 03, *Thread deletion and the fallback lane*).
//!
//! **S1 — every thread write is stamped.** Creating, renaming (from the lane
//! label, the assistant, or any other path) and deleting a thread go through
//! this module, which sets a hybrid-logical-clock stamp from the same device
//! clock as the documents' (C-DOC G25); the row carries it on the wire (a
//! field added with a default, C-WIRE's rule for new fields). Deleting a
//! thread is [`crate::delete_thread_to_trash`], which calls into this module
//! for the thread record. No production code outside `sovereign-db` and this
//! file calls the store's thread writers (a source scan,
//! `tests/ts1_one_thread_write_path.rs`).
//!
//! S2–S5 and S8 (the later action wins; older peers; the 5-minute bound) are
//! decided where a received thread row is applied
//! ([`DocumentService::thread_apply_remote`]); S6 and S9 (a deleted thread
//! shrinks to its id, 30 days after its deletion stamp) are run by the purge
//! tick ([`crate::purge_tick`]); S7: a marker is never shown (it is a deleted
//! thread with no name, which every list leaves out).
//!
//! How it is stored: the stamp is the thread record's `stamp` field
//! (`wall_ms:counter:device`), written with the state it stamps through the
//! store's whole-record write (`put_thread`). A row with no stamp (written by
//! an older build, or by a store write outside this path) is ordered as if
//! stamped at its modification time — the later of `modified_at` and
//! `deleted_at` —, logical counter 0, its device (S5).

use chrono::{DateTime, Duration, TimeZone, Utc};
use sovereign_db::schema::{raw_to_thing, Thread};
use sovereign_db::traits::GraphDB;
use sovereign_db::DbError;

use crate::service::{store_err, DocumentService};
use crate::types::DocError;
use crate::wire::{decode_stamp, encode_stamp, Stamp, MAX_STAMP_AHEAD_SECS};

/// How long a deleted thread keeps its name, counted from its deletion
/// stamp, before it may shrink to its id (S6, S9).
pub const THREAD_MARKER_AFTER_DAYS: i64 = 30;

/// A stamp as stored on a thread record or in a manifest
/// (`wall_ms:counter:device`).
pub fn parse_thread_stamp(s: &str) -> Option<Stamp> {
    decode_stamp(s)
}

/// The modification time of a thread row in milliseconds: the later of
/// `modified_at` and `deleted_at` (a soft delete by an older build does not
/// move `modified_at`).
fn modified_ms(t: &Thread) -> i64 {
    let m = t.modified_at.timestamp_millis();
    let d = t
        .deleted_at
        .as_deref()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.timestamp_millis());
    d.map_or(m, |d| d.max(m))
}

/// The order of a thread row (S2, S5): its stamp, or — for a row without one
/// — a stamp derived from its modification time, logical counter 0, device
/// `device` (the row's author when known, else this device).
pub fn thread_order(t: &Thread, device: &str) -> Stamp {
    t.stamp.as_deref().and_then(decode_stamp).unwrap_or_else(|| Stamp {
        wall_ms: modified_ms(t),
        counter: 0,
        device: device.to_string(),
    })
}

fn time_of(ms: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now)
}

/// A marker (S6): a deleted thread with no name and no description.
fn is_marker(t: &Thread) -> bool {
    t.deleted_at.is_some() && t.name.is_empty() && t.description.is_empty()
}

fn not_found(id: &str) -> DocError {
    DocError::NotFound(id.to_string())
}

/// Seeds threads into a store that has no document service yet — the decoy
/// workspace, seeded before its first unlock (THREAD-sync S1: the decoy
/// workspace's seeding is a live path). Each thread is stamped as the service
/// would stamp it at `now` on device `device` (`now`, counters 0, 1, 2 …); the
/// service that later opens the store starts its clock after these stamps
/// (it is initialised from every stamp the store holds). Returns the ids, in
/// order.
pub async fn seed_threads(
    db: &dyn GraphDB,
    threads: &[(&str, &str)],
    device: &str,
    now: DateTime<Utc>,
) -> Result<Vec<String>, DocError> {
    let mut ids = Vec::with_capacity(threads.len());
    for (n, (name, description)) in threads.iter().enumerate() {
        let stamp = Stamp { wall_ms: now.timestamp_millis(), counter: n as u32, device: device.to_string() };
        let id = format!("thread:{}", sovereign_docformat::BlockId::random().to_hex());
        let mut t = Thread::new(name.to_string(), description.to_string());
        t.id = raw_to_thing(&id);
        t.created_at = time_of(stamp.wall_ms);
        t.modified_at = t.created_at;
        t.stamp = Some(encode_stamp(&stamp));
        if !db.create_thread_with_id(t).await.map_err(store_err)? {
            return Err(DocError::Store(format!("thread id {id} already exists")));
        }
        ids.push(id);
    }
    Ok(ids)
}

impl DocumentService {
    /// S1: creates a thread named `name` with `description`, stamped by this
    /// device's clock. Returns the new thread's id (`thread:<key>`).
    pub async fn thread_create(&self, name: &str, description: &str) -> Result<String, DocError> {
        let _w = self.write_lock.lock().await;
        self.thread_create_locked(name, description).await
    }

    /// [`Self::thread_create`] with the write lock already held (the orphan
    /// resolution's new thread).
    pub(crate) async fn thread_create_locked(&self, name: &str, description: &str) -> Result<String, DocError> {
        let stamp = self.next_stamp().await?;
        let at = time_of(stamp.wall_ms);
        let id = format!("thread:{}", sovereign_docformat::BlockId::random().to_hex());
        let mut t = Thread::new(name.to_string(), description.to_string());
        t.id = raw_to_thing(&id);
        t.created_at = at;
        t.modified_at = at;
        t.stamp = Some(encode_stamp(&stamp));
        if !self.store().create_thread_with_id(t).await.map_err(store_err)? {
            return Err(DocError::Store(format!("thread id {id} already exists")));
        }
        Ok(id)
    }

    /// S1: renames and/or re-describes thread `id` (`None` leaves a field as
    /// it is), stamped by this device's clock. A deleted (or unknown) thread
    /// is refused; a write that changes nothing writes nothing.
    pub async fn thread_update(&self, id: &str, name: Option<&str>, description: Option<&str>) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        let mut t = self.thread_row(id).await?;
        if t.deleted_at.is_some() {
            return Err(DocError::Store(format!("thread {id} was deleted")));
        }
        let new_name = name.map(str::to_string).unwrap_or_else(|| t.name.clone());
        let new_description = description.map(str::to_string).unwrap_or_else(|| t.description.clone());
        if new_name == t.name && new_description == t.description {
            return Ok(());
        }
        let prior = t.clone();
        let stamp = self.thread_write_stamp(&t).await?;
        t.name = new_name;
        t.description = new_description;
        t.modified_at = time_of(stamp.wall_ms);
        t.stamp = Some(encode_stamp(&stamp));
        self.put_thread(id, t, Some(prior)).await
    }

    /// The stamp of thread `id`'s current state (its last create, rename or
    /// deletion, made here or received); `None` for a row written before
    /// stamps existed (an older build). A deleted thread shrunk to its id
    /// (S6) keeps its deletion stamp.
    pub async fn thread_stamp(&self, id: &str) -> Result<Option<Stamp>, DocError> {
        Ok(self.thread_row(id).await?.stamp.as_deref().and_then(decode_stamp))
    }

    /// S1: deletes thread `id` (soft: its record stays as the tombstone sync
    /// needs), stamped after every stamp issued here — so after the documents
    /// the deletion sent to the trash. Called by
    /// [`crate::delete_thread_to_trash`], never on its own: deleting a
    /// thread moves its documents to the trash first (S5-10).
    pub(crate) async fn thread_delete(&self, id: &str) -> Result<(), DocError> {
        let _w = self.write_lock.lock().await;
        self.thread_delete_locked(id).await
    }

    async fn thread_delete_locked(&self, id: &str) -> Result<(), DocError> {
        let mut t = self.thread_row(id).await?;
        if t.deleted_at.is_some() {
            return Err(DocError::Store(format!("thread {id} was already deleted")));
        }
        let prior = t.clone();
        let stamp = self.thread_write_stamp(&t).await?;
        let at = time_of(stamp.wall_ms);
        t.modified_at = at;
        t.deleted_at = Some(at.to_rfc3339());
        t.stamp = Some(encode_stamp(&stamp));
        self.put_thread(id, t, Some(prior)).await
    }

    /// Takes back a thread this device created a moment ago and that nothing
    /// uses (the all-or-nothing undo of an orphan resolution whose moves
    /// failed). The write lock is held.
    pub(crate) async fn thread_take_back_locked(&self, id: &str) -> Result<(), DocError> {
        self.store().delete_thread(id).await.map_err(store_err)
    }

    /// Merges thread `source_id` into `target_id`: every document of the
    /// source (live or in the trash) moves to the target through the service
    /// (one stamped move each, so the moves travel by sync), then the source
    /// is deleted through the stamped path. Both must be live and different.
    pub async fn thread_merge(&self, target_id: &str, source_id: &str) -> Result<usize, DocError> {
        let _w = self.write_lock.lock().await;
        if target_id == source_id {
            return Err(DocError::Store("a thread cannot be merged into itself".into()));
        }
        for id in [target_id, source_id] {
            if self.thread_row(id).await?.deleted_at.is_some() {
                return Err(DocError::Store(format!("thread {id} was deleted")));
            }
        }
        let ids: Vec<String> = self
            .store()
            .list_documents_including_deleted()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|d| d.thread_id == source_id)
            .filter_map(|d| d.id_string())
            .collect();
        let mut moved = 0;
        for id in &ids {
            match self.move_locked(id, target_id).await {
                Ok(_) => moved += 1,
                // Mid-purge: it is going, not moving.
                Err(DocError::Purged(_)) => {}
                Err(e) => return Err(e),
            }
        }
        self.thread_delete_locked(source_id).await?;
        Ok(moved)
    }

    /// Restores a thread from a guardian backup (C-WIRE 17) under its origin
    /// id, with the stamp it was backed up with: a restore brings a state
    /// back, it is not a new action. An id that already exists here is left
    /// as it is. Returns whether the thread was written.
    pub async fn thread_restore_backup(&self, thread: Thread) -> Result<bool, DocError> {
        let _w = self.write_lock.lock().await;
        let id = thread.id_string().ok_or_else(|| DocError::Store("a backed-up thread has no id".into()))?;
        match self.store().get_thread(&id).await {
            Ok(_) => return Ok(false),
            Err(DbError::NotFound(_)) => {}
            Err(e) => return Err(store_err(e)),
        }
        self.ensure_clock().await?;
        if let Some(st) = thread.stamp.as_deref().and_then(decode_stamp) {
            if st.wall_ms <= self.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000 {
                self.observe_stamp(&st);
            }
        }
        let mut t = thread;
        t.name_nonce = None;
        t.description_nonce = None;
        t.name_token_hashes.clear();
        self.put_thread(&id, t, None).await?;
        Ok(true)
    }

    /// Applies thread row `id` received from device `sender` (S2–S6, S8):
    ///
    /// - its order is its stamp, or — sent by an older build — a stamp
    ///   derived from its modification time (S5);
    /// - more than 5 minutes ahead of this device's clock, it is refused
    ///   (S8, and S5 for a row without a stamp);
    /// - it is written only when its order is higher than the local row's,
    ///   whatever the actions are (S2): a rename that wins over a delete
    ///   brings the thread back (S3), a delete that wins over a rename
    ///   discards it (S4), a marker refuses an older row and is re-created
    ///   from a newer one (S6); equal orders are the same state (S10);
    /// - a received stamp moves this device's clock past it (S1).
    ///
    /// Returns whether the row was written.
    pub async fn thread_apply_remote(&self, id: &str, remote: Thread, sender: &str) -> Result<bool, DocError> {
        let _w = self.write_lock.lock().await;
        let thing = raw_to_thing(id).ok_or_else(|| DocError::Store(format!("malformed thread id '{id}'")))?;
        self.ensure_clock().await?;
        let order = thread_order(&remote, sender);
        let bound = self.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000;
        if order.wall_ms > bound || remote.modified_at.timestamp_millis() > bound {
            return Ok(false);
        }
        // A stamp that does not read as one is no stamp (the row is ordered
        // by its modification time, and stored without it).
        let stamped = remote.stamp.as_deref().and_then(decode_stamp).is_some();
        if stamped {
            self.observe_stamp(&order);
        }
        let local = match self.store().get_thread(id).await {
            Ok(t) => Some(t),
            Err(DbError::NotFound(_)) => None,
            Err(e) => return Err(store_err(e)),
        };
        if let Some(l) = &local {
            if order <= thread_order(l, &self.deps.device) {
                return Ok(false);
            }
        }
        let mut t = Thread::new(remote.name, remote.description);
        t.id = Some(thing);
        t.created_at = local.as_ref().map_or(remote.created_at, |l| l.created_at);
        t.modified_at = remote.modified_at;
        t.deleted_at = remote.deleted_at;
        t.stamp = if stamped { remote.stamp } else { None };
        self.put_thread(id, t, local).await?;
        Ok(true)
    }

    /// S6, S9: every deleted thread whose deletion stamp is 30 days old or
    /// more by this device's clock, and to which no document — live or in
    /// the trash — points, shrinks to a marker: the id and the deletion stamp
    /// only (name and description removed from disk). A marker keeps its
    /// stamp, so it stays in the sync manifest and refuses older rows.
    /// Run by the purge tick. Returns how many shrank.
    pub(crate) async fn shrink_deleted_threads(&self) -> Result<usize, DocError> {
        let _w = self.write_lock.lock().await;
        let threads = self.store().list_threads_including_deleted().await.map_err(store_err)?;
        let candidates: Vec<Thread> = threads.into_iter().filter(|t| t.deleted_at.is_some() && !is_marker(t)).collect();
        if candidates.is_empty() {
            return Ok(0);
        }
        let pointed: std::collections::HashSet<String> = self
            .store()
            .list_documents_including_deleted()
            .await
            .map_err(store_err)?
            .into_iter()
            .map(|d| d.thread_id)
            .collect();
        let now = self.now().timestamp_millis();
        let after = Duration::days(THREAD_MARKER_AFTER_DAYS).num_milliseconds();
        let mut shrunk = 0;
        for t in candidates {
            let Some(id) = t.id_string() else { continue };
            if pointed.contains(&id) {
                continue;
            }
            let deleted_ms = thread_order(&t, &self.deps.device).wall_ms;
            if now.saturating_sub(deleted_ms) < after {
                continue;
            }
            let at = time_of(modified_ms(&t));
            let mut marker = Thread::new(String::new(), String::new());
            marker.id = t.id.clone();
            marker.created_at = at;
            marker.modified_at = at;
            marker.deleted_at = t.deleted_at.clone();
            marker.stamp = t.stamp.clone();
            self.put_thread(&id, marker, Some(t)).await?;
            shrunk += 1;
        }
        Ok(shrunk)
    }

    /// The thread record `id`, decrypted (`NotFound` when there is none).
    async fn thread_row(&self, id: &str) -> Result<Thread, DocError> {
        match self.store().get_thread(id).await {
            Ok(t) => Ok(t),
            Err(DbError::NotFound(_)) | Err(DbError::InvalidId(_)) => Err(not_found(id)),
            Err(e) => Err(store_err(e)),
        }
    }

    /// A stamp for a local write replacing thread state `t`: after every
    /// stamp issued or seen here, and after `t`'s own order when that is not
    /// beyond the 5-minute bound (a row written by an older build is ordered
    /// by its modification time, which this write must follow).
    async fn thread_write_stamp(&self, t: &Thread) -> Result<Stamp, DocError> {
        self.ensure_clock().await?;
        let mut current = thread_order(t, &self.deps.device);
        if current.wall_ms <= self.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000 {
            if t.stamp.as_deref().and_then(decode_stamp).is_none() {
                // An unstamped state is ordered by its modification time,
                // which is finer than a millisecond: the write that replaces
                // it is placed in a later millisecond, so a peer comparing the
                // two by time (an older build, or the row diff) sees it later.
                current = Stamp { wall_ms: current.wall_ms.saturating_add(1), counter: 0, device: current.device };
            }
            self.observe_stamp(&current);
        }
        self.next_stamp().await
    }

    /// Writes thread record `id` whole (name and description in clear; the
    /// store seals them), replacing `prior` — the record as it was read. If
    /// the write fails, `prior` is written back (best effort): a store whose
    /// whole-record write is not one statement must not lose the thread on a
    /// transient failure (S5-10, all or nothing).
    async fn put_thread(&self, id: &str, t: Thread, prior: Option<Thread>) -> Result<(), DocError> {
        let clean = |mut t: Thread| {
            t.id = raw_to_thing(id);
            t.name_nonce = None;
            t.description_nonce = None;
            t.name_token_hashes.clear();
            t
        };
        match self.store().put_thread(clean(t)).await {
            Ok(()) => Ok(()),
            Err(e) => {
                if let Some(p) = prior {
                    let _ = self.store().put_thread(clean(p)).await;
                }
                Err(store_err(e))
            }
        }
    }
}
