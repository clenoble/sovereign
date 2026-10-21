//! Live documents whose thread was deleted here (S5-12, contract W1–W6 in
//! `doc/plans/C-DOC-adoption.md`, *S5-12 — a live document in a deleted
//! thread asks*; spec ch. 03, *Thread deletion and the fallback lane*,
//! decided by Céline 2026-10-17).
//!
//! The warning is a **condition the service computes**, never a stored alert:
//! every live document (not in the trash, not purged) whose thread is a
//! tombstone here. It covers both directions of sync (a document arriving for
//! a thread deleted here; a thread's deletion arriving while documents of it
//! are live here), survives restarts, and disappears exactly when the
//! document is resolved — moved to an existing thread, moved to a new thread,
//! or put in the trash. Peer-audit alerts are untouched (W6).
//!
//! Tests: `tests/s5_12_orphans.rs` (W1–W4, W6, W7), the sync over sealed
//! envelopes in `sovereign-p2p/tests/wire_orphans.rs`, the hosts' own suites
//! (`TESTS-s5-12.md`).

use std::collections::HashMap;
use std::fmt;

use sovereign_db::schema::{Document, DocumentPatch};
use sovereign_db::traits::GraphDB;

use crate::hosts::active_thread;
use crate::service::{current_stamp_of, store_err, DocumentService};
use crate::types::{ChangeKind, DocError, DocId};

/// One live document whose thread was deleted here (W1). No content.
#[derive(Clone, PartialEq, Eq)]
pub struct OrphanedDocument {
    pub id: DocId,
    pub title: String,
    /// The deleted thread the document is still in.
    pub thread_id: String,
    /// That thread's name when it was deleted, read from its tombstone;
    /// `None` when the tombstone's name cannot be read.
    pub deleted_thread_name: Option<String>,
    /// The device the document's current state came from (this device for a
    /// write made here).
    pub device: String,
}

impl fmt::Debug for OrphanedDocument {
    /// Never prints the title or the thread's name (G17).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OrphanedDocument")
            .field("id", &self.id)
            .field("title_bytes", &self.title.len())
            .field("thread_id", &self.thread_id)
            .field("deleted_thread_name_bytes", &self.deleted_thread_name.as_ref().map(String::len))
            .field("device", &self.device)
            .finish()
    }
}

/// One of the three ways out for a document whose thread was deleted (W4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OrphanResolution {
    /// Move it to this existing, live thread.
    MoveTo(String),
    /// Create a new thread with this name (trimmed; an empty name creates
    /// nothing, as in the new-thread gesture) and move it there.
    NewThread(String),
    /// Put it in the trash (restorable for 30 days).
    Trash,
}

/// What a resolution did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrphanResolved {
    /// The documents resolved (one, or every live document of the group).
    pub documents: Vec<DocId>,
    /// The thread they now are in: the chosen thread, or the new one; `None`
    /// when they went to the trash.
    pub thread_id: Option<String>,
}

fn refused(why: &str) -> DocError {
    DocError::OrphanRefused(format!("{why} Nothing was changed."))
}

/// What a resolution writes, once every check has passed.
enum Target {
    /// An existing, live thread.
    Thread(String),
    /// A thread to create, with this (trimmed, non-empty) name.
    New(String),
    Trash,
}

impl DocumentService {
    /// The deleted threads of this device (their tombstones), id → old name
    /// (`None` when the name is empty or cannot be read).
    async fn deleted_threads(&self) -> Result<HashMap<String, Option<String>>, DocError> {
        Ok(self
            .store()
            .list_threads_including_deleted()
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|t| t.deleted_at.is_some())
            .filter_map(|t| {
                let id = t.id_string()?;
                let name = (!t.name.trim().is_empty()).then_some(t.name);
                Some((id, name))
            })
            .collect())
    }

    fn orphan_of(&self, d: Document, deleted: &HashMap<String, Option<String>>) -> Option<OrphanedDocument> {
        let name = deleted.get(&d.thread_id)?.clone();
        let id = d.id_string()?;
        let device = current_stamp_of(&d, &self.deps.device).device;
        Some(OrphanedDocument { id, title: d.title, thread_id: d.thread_id, deleted_thread_name: name, device })
    }

    /// Every live document whose thread is a tombstone here (W1): not those
    /// in the trash, not purged ones, not those whose thread never existed
    /// here. Computed on each call; raises and replaces no alert (W6).
    pub async fn orphans_of_deleted_threads(&self) -> Result<Vec<OrphanedDocument>, DocError> {
        let deleted = self.deleted_threads().await?;
        if deleted.is_empty() {
            return Ok(Vec::new());
        }
        // `list_documents` leaves out the trash; a purged document has no row
        // (or, mid-purge, a trashed one).
        let docs = self.store().list_documents(None).await.map_err(store_err)?;
        Ok(docs.into_iter().filter(|d| d.deleted_at.is_none()).filter_map(|d| self.orphan_of(d, &deleted)).collect())
    }

    /// Resolves one listed document (W4), through the service (stamped,
    /// versioned per its rules). A target thread that is deleted or unknown,
    /// an empty new-thread name, or a document that is not listed, is refused
    /// and nothing changes.
    pub async fn resolve_orphan(&self, id: &str, resolution: OrphanResolution) -> Result<OrphanResolved, DocError> {
        let _w = self.write_lock.lock().await;
        let deleted = self.deleted_threads().await?;
        let d = match self.load(id).await {
            Ok(d) => d,
            Err(DocError::NotFound(_)) | Err(DocError::Purged(_)) => {
                return Err(refused("This document no longer exists."));
            }
            Err(e) => return Err(e),
        };
        if d.deleted_at.is_some() {
            return Err(refused("This document is in the trash."));
        }
        if !deleted.contains_key(&d.thread_id) {
            return Err(refused("This document's thread was not deleted: it is not waiting for a decision."));
        }
        let target = self.check_target(&resolution).await?;
        self.resolve_locked(&[id.to_string()], target).await
    }

    /// Resolves every listed document of the deleted thread
    /// `deleted_thread_id` together ("all of <old name>", W4): one new thread
    /// for the group, or one move target, or the trash for all. Refused, with
    /// nothing changed, as [`DocumentService::resolve_orphan`].
    pub async fn resolve_orphans_of_thread(
        &self,
        deleted_thread_id: &str,
        resolution: OrphanResolution,
    ) -> Result<OrphanResolved, DocError> {
        let _w = self.write_lock.lock().await;
        let deleted = self.deleted_threads().await?;
        if !deleted.contains_key(deleted_thread_id) {
            return Err(refused("This thread was not deleted (or does not exist): there is nothing to decide for it."));
        }
        let ids: Vec<String> = self
            .store()
            .list_documents(Some(deleted_thread_id))
            .await
            .map_err(store_err)?
            .into_iter()
            .filter(|d| d.deleted_at.is_none() && d.thread_id == deleted_thread_id)
            .filter_map(|d| d.id_string())
            .collect();
        if ids.is_empty() {
            return Err(refused("No document of this deleted thread is waiting for a decision."));
        }
        let target = self.check_target(&resolution).await?;
        self.resolve_locked(&ids, target).await
    }

    /// Checks a resolution's target before anything is written.
    async fn check_target(&self, resolution: &OrphanResolution) -> Result<Target, DocError> {
        match resolution {
            OrphanResolution::MoveTo(thread) => match active_thread(self.store(), thread).await.map_err(store_err)? {
                Some(_) => Ok(Target::Thread(thread.clone())),
                None => Err(refused(
                    "That thread does not exist (it was deleted, or never existed): choose an existing thread.",
                )),
            },
            OrphanResolution::NewThread(name) => {
                let name = name.trim();
                if name.is_empty() {
                    Err(refused("A new thread needs a name."))
                } else {
                    Ok(Target::New(name.to_string()))
                }
            }
            OrphanResolution::Trash => Ok(Target::Trash),
        }
    }

    /// Writes a checked resolution of `ids` (the write lock held). All or
    /// nothing: a store failure part-way moves back what was moved, takes
    /// back the thread it created, and takes out of the trash what it put in.
    async fn resolve_locked(&self, ids: &[String], target: Target) -> Result<OrphanResolved, DocError> {
        match target {
            Target::Trash => {
                let mut done: Vec<String> = Vec::with_capacity(ids.len());
                for id in ids {
                    if let Err(e) = self.trash_locked(id).await {
                        for back in &done {
                            let _ = self.untrash_locked(back).await;
                        }
                        return Err(e);
                    }
                    done.push(id.clone());
                }
                Ok(OrphanResolved { documents: done, thread_id: None })
            }
            Target::Thread(thread) => {
                self.move_all_locked(ids, &thread).await?;
                Ok(OrphanResolved { documents: ids.to_vec(), thread_id: Some(thread) })
            }
            Target::New(name) => {
                // Through the one thread-write path, stamped (THREAD-sync S1).
                let created = self.thread_create_locked(&name, "").await?;
                if let Err(e) = self.move_all_locked(ids, &created).await {
                    let _ = self.thread_take_back_locked(&created).await;
                    return Err(e);
                }
                Ok(OrphanResolved { documents: ids.to_vec(), thread_id: Some(created) })
            }
        }
    }

    /// Moves every document of `ids` to `thread`, or none.
    async fn move_all_locked(&self, ids: &[String], thread: &str) -> Result<(), DocError> {
        let mut moved: Vec<(String, String)> = Vec::with_capacity(ids.len());
        for id in ids {
            match self.move_locked(id, thread).await {
                Ok(from) => moved.push((id.clone(), from)),
                Err(e) => {
                    for (back, from) in &moved {
                        let _ = self.move_locked(back, from).await;
                    }
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Moves one document to `thread`: a stamped write of its thread alone
    /// (title, content and versions unchanged), so the move travels by sync.
    /// Returns the thread it was in.
    pub(crate) async fn move_locked(&self, id: &str, thread: &str) -> Result<String, DocError> {
        let d = self.load(id).await?;
        let from = d.thread_id.clone();
        let mut patch = DocumentPatch { thread_id: Some(thread.to_string()), ..Default::default() };
        self.stamp_local(&d, &mut patch).await?;
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::Saved);
        Ok(from)
    }

    /// Takes a document back out of the trash (the write lock held): the
    /// all-or-nothing undo of a group sent to the trash.
    async fn untrash_locked(&self, id: &str) -> Result<(), DocError> {
        let d = self.load(id).await?;
        if d.deleted_at.is_none() {
            return Ok(());
        }
        let mut patch = DocumentPatch { deleted_at: Some(None), ..Default::default() };
        self.stamp_local(&d, &mut patch).await?;
        self.store().update_document_fields(id, patch).await.map_err(store_err)?;
        self.emit(id, ChangeKind::RestoredFromTrash);
        Ok(())
    }
}

