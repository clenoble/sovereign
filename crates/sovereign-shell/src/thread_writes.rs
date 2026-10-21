//! The shell's seam to the one stamped thread-write path (`doc/plans/THREAD-sync.md`
//! S1: "Creating, renaming (from the lane label, the assistant, or any other
//! path) and deleting a thread go through **one** thread-write path that sets
//! an HLC stamp").
//!
//! The renaming of a thread in its lane label (`THREAD-name.md` R1–R8,
//! `crate::lane_rename`) reaches S1 only through [`ThreadWrites`], so the
//! rename never calls the store's `update_thread` itself (an unstamped write
//! would lose to an older rename made on another device, S2).
//! [`StampedThreadWrites`] is the binding the shell uses: the open
//! workspace's document service, whose `thread_update` is S1's rename
//! (`sovereign-docs/src/threads.rs`). The tests use a recording fake
//! (`thread_rename_tests.rs`).
//!
//! The trait and its signature are the test author's; the binding's body is
//! the implementer's.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use sovereign_docs::DocumentService;

/// What a thread write returns: `Err` carries a message for the user's notice.
pub(crate) type WriteFuture<'a> = Pin<Box<dyn Future<Output = Result<(), String>> + 'a>>;

/// The one stamped thread-write path (S1), as the shell uses it.
pub(crate) trait ThreadWrites {
    /// Rename thread `thread_id` to `name` (already trimmed and on one line),
    /// stamped (S1). The description is left as it is.
    fn rename_thread<'a>(&'a self, thread_id: &'a str, name: &'a str) -> WriteFuture<'a>;
}

/// The shell's binding of [`ThreadWrites`]: the open workspace's document
/// service, S1's one thread-write path (`DocumentService::thread_update`,
/// stamped by this device's clock; it refuses a deleted thread and writes
/// nothing for an unchanged name).
pub(crate) struct StampedThreadWrites(pub(crate) Arc<DocumentService>);

impl ThreadWrites for StampedThreadWrites {
    fn rename_thread<'a>(&'a self, thread_id: &'a str, name: &'a str) -> WriteFuture<'a> {
        Box::pin(async move {
            self.0
                .thread_update(thread_id, Some(name), None)
                .await
                .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))
        })
    }
}
