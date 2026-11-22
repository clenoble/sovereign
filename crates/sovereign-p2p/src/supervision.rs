//! The node's supervision (contract C-SUP, spec ch. 05, *The node's
//! supervision*): the P2P node runs under a supervisor that observes its end,
//! whether its task returns, returns an error or panics (SU1), tells an end
//! the session asked for from one it did not (SU5), and never restarts it by
//! itself (SU3). Shared by both hosts (the shell, and the app built with P2P).
//!
//! Types and signatures by the test author of lot LQ; the tests are
//! `tests/csup_supervision.rs` here and the hosts' own (SU2, SU4, SU6).

use std::any::Any;
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How a supervised node ended, when nobody asked it to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeEnd {
    /// Its task returned.
    Returned,
    /// Its task returned an error; the reason goes to the log, not to the user.
    Failed(String),
    /// Its task panicked; the panic's message, for the log.
    Panicked(String),
}

/// A node under supervision.
pub struct NodeSupervisor {
    /// Set when the session asks for the end (SU5); the watcher reads it
    /// once the node has ended, and then says nothing.
    asked: Arc<AtomicBool>,
    /// The node's own task.
    node: tokio::task::AbortHandle,
    /// Awaits the node's end and tells it; finished once that is done.
    watcher: tokio::task::JoinHandle<()>,
    /// How long `stop` waits for the node to end by itself before it aborts
    /// it (`with_stop_grace`). Zero, the default: it aborts at once.
    grace: Duration,
}

impl NodeSupervisor {
    /// Runs `node` on `runtime` under supervision. If it ends without the
    /// session asking for it, by a return, an error or a panic, `on_stop` is
    /// called exactly once with how it ended (SU1). Nothing restarts it
    /// (SU3): `node` runs once.
    pub fn spawn<F, S>(runtime: &tokio::runtime::Handle, node: F, on_stop: S) -> NodeSupervisor
    where
        F: Future<Output = Result<(), String>> + Send + 'static,
        S: FnOnce(NodeEnd) + Send + 'static,
    {
        let asked = Arc::new(AtomicBool::new(false));
        let task = runtime.spawn(node);
        let abort = task.abort_handle();
        let seen = asked.clone();
        let watcher = runtime.spawn(async move {
            let end = match task.await {
                Ok(Ok(())) => NodeEnd::Returned,
                Ok(Err(reason)) => NodeEnd::Failed(reason),
                Err(e) if e.is_panic() => NodeEnd::Panicked(panic_message(e.into_panic())),
                // Cancelled: only `stop` aborts the node, or the runtime
                // shutting down, which is the application's exit (SU5).
                Err(_) => return,
            };
            if !seen.load(Ordering::SeqCst) {
                on_stop(end);
            }
        });
        NodeSupervisor { asked, node: abort, watcher, grace: Duration::ZERO }
    }

    /// How long `stop` waits for the node to end by itself (on the Shutdown
    /// the host sends after calling `stop`) before it aborts its task. A node
    /// ended between two handlers leaves no write half done; an abort drops
    /// it at its next await, wherever that is.
    pub fn with_stop_grace(mut self, grace: Duration) -> NodeSupervisor {
        self.grace = grace;
        self
    }

    /// An end the session asked for (a logout, a switch, a lock, the
    /// application's exit; SU5): the node ends, and `on_stop` is never
    /// called. Returns once the node has ended.
    ///
    /// The end is recorded when `stop` is called, before the returned future
    /// is first polled, so a node that returns on a Shutdown the host sends
    /// right after the call is not said as a stop. The future then waits for
    /// the node to end, up to the grace set by `with_stop_grace`, and aborts
    /// its task only if it has not ended by then (at once with no grace).
    pub fn stop(self) -> impl Future<Output = ()> + Send + 'static {
        self.asked.store(true, Ordering::SeqCst);
        let NodeSupervisor { node, mut watcher, grace, .. } = self;
        async move {
            if !grace.is_zero() && tokio::time::timeout(grace, &mut watcher).await.is_ok() {
                return;
            }
            node.abort();
            let _ = watcher.await;
        }
    }

    /// Whether the node still runs.
    pub fn is_running(&self) -> bool {
        !self.node.is_finished()
    }
}

#[cfg(test)]
mod tests {
    //! The grace of `stop` (the implementer's own tests; C-SUP's are in
    //! `tests/csup_supervision.rs`). Short bounds, so nothing sleeps for long.
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    fn counted() -> (Arc<AtomicUsize>, impl FnOnce(NodeEnd) + Send + 'static) {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        (calls, move |_| {
            c.fetch_add(1, Ordering::SeqCst);
        })
    }

    // A node that ignores Shutdown is aborted once the grace has run out, and nothing is said.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_node_that_ignores_shutdown_is_aborted_after_the_grace_and_nothing_is_said() {
        let (calls, on_stop) = counted();
        let grace = Duration::from_millis(200);
        let sup = NodeSupervisor::spawn(&tokio::runtime::Handle::current(), std::future::pending(), on_stop)
            .with_stop_grace(grace);
        let started = Instant::now();
        tokio::time::timeout(Duration::from_secs(5), sup.stop()).await.expect("stop ends once the grace has run out");
        assert!(started.elapsed() >= grace, "aborted before the grace ran out");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "an asked end was said as a stop");
    }

    // A node that ends on Shutdown within the grace finishes what it was doing: it is not aborted.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_node_that_ends_on_shutdown_within_the_grace_is_not_cut() {
        let (calls, on_stop) = counted();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
        let finished = Arc::new(AtomicBool::new(false));
        let f = finished.clone();
        let node = async move {
            let _ = shutdown_rx.await;
            // The rest of a handler: an await between two writes.
            tokio::time::sleep(Duration::from_millis(50)).await;
            f.store(true, Ordering::SeqCst);
            Ok(())
        };
        let sup = NodeSupervisor::spawn(&tokio::runtime::Handle::current(), node, on_stop)
            .with_stop_grace(Duration::from_secs(5));
        let stopping = sup.stop();
        let _ = shutdown_tx.send(());
        tokio::time::timeout(Duration::from_secs(5), stopping).await.expect("the node ends on Shutdown");
        assert!(finished.load(Ordering::SeqCst), "the node was cut before it finished");
        assert_eq!(calls.load(Ordering::SeqCst), 0, "an asked end was said as a stop");
    }
}

/// A panic's message, for the log.
fn panic_message(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(s) => *s,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(s) => (*s).to_string(),
            Err(_) => "a panic without a message".to_string(),
        },
    }
}
