//! C-SUP — the node's supervision (spec ch. 05, *The node's supervision —
//! contract (C-SUP)*), at the supervisor both hosts share
//! (`sovereign_p2p::supervision`).
//!
//! "SU1 — The node's end is observed, whether its task returns, returns an
//! error or panics. It is never silent." "SU5 — An end the session asked for
//! is not a stop: a logout, a switch, a lock or the application's exit end the
//! node and say nothing." "SU3 — [...] Nothing restarts the node by itself."
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. The node is a stand-in future, so each end is made on purpose.
//! A bound of 10 s only says the test failed; nothing here depends on timing.
//! The hosts' side (SU2, SU4, SU6) is in their own tests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use sovereign_p2p::supervision::{NodeEnd, NodeSupervisor};
use tokio::sync::oneshot;

const BOUND: Duration = Duration::from_secs(10);

/// Runs `node` under supervision; returns the supervisor, what `on_stop`
/// receives, and how many times it was called.
fn supervise<F>(node: F) -> (NodeSupervisor, oneshot::Receiver<NodeEnd>, Arc<AtomicUsize>)
where
    F: std::future::Future<Output = Result<(), String>> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    let calls = Arc::new(AtomicUsize::new(0));
    let c = calls.clone();
    let sup = NodeSupervisor::spawn(&tokio::runtime::Handle::current(), node, move |end| {
        c.fetch_add(1, Ordering::SeqCst);
        let _ = tx.send(end);
    });
    (sup, rx, calls)
}

// SU1: a node whose task returns is observed.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su1_a_node_that_returns_is_observed() {
    let (sup, rx, calls) = supervise(async { Ok(()) });
    let end = tokio::time::timeout(BOUND, rx).await.expect("the end is observed").expect("on_stop is called");
    assert_eq!(end, NodeEnd::Returned);
    assert_eq!(calls.load(Ordering::SeqCst), 1, "said once");
    assert!(!sup.is_running(), "the node no longer runs");
}

// SU1: a node whose task returns an error is observed, with its reason (for the log).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su1_a_node_that_returns_an_error_is_observed() {
    let (_sup, rx, calls) = supervise(async { Err("listener closed".to_string()) });
    let end = tokio::time::timeout(BOUND, rx).await.expect("the end is observed").expect("on_stop is called");
    assert_eq!(end, NodeEnd::Failed("listener closed".to_string()));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// SU1: a node whose task panics is observed; the panic does not go unseen.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su1_a_node_that_panics_is_observed() {
    let (_sup, rx, calls) = supervise(async {
        panic!("swarm poll failed");
        #[allow(unreachable_code)]
        Ok(())
    });
    let end = tokio::time::timeout(BOUND, rx).await.expect("the end is observed").expect("on_stop is called");
    match end {
        NodeEnd::Panicked(message) => assert!(message.contains("swarm poll failed"), "the panic's message, for the log: {message}"),
        other => panic!("a panic was observed as {other:?}"),
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

// SU5: an end the session asked for (a logout, a switch, a lock, the exit) says nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su5_an_end_the_session_asked_for_says_nothing() {
    let (sup, mut rx, calls) = supervise(std::future::pending());
    assert!(sup.is_running(), "precondition: the node runs");
    tokio::time::timeout(BOUND, sup.stop()).await.expect("the node ends when asked");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "an asked end was said as a stop");
    assert!(rx.try_recv().is_err(), "nothing was sent to the host");
}

// SU5: an end the session asked for, whose node then returns as asked (the app's node ends on a Shutdown command), says nothing either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su5_a_node_that_returns_because_it_was_asked_says_nothing() {
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
    let (sup, mut rx, calls) = supervise(async move {
        let _ = shutdown_rx.await; // the node's loop ends on its Shutdown command
        Ok(())
    });
    let stopping = tokio::spawn(sup.stop());
    let _ = shutdown_tx.send(());
    tokio::time::timeout(BOUND, stopping).await.expect("the node ends when asked").expect("stop completes");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "a return the session asked for was said as a stop");
    assert!(rx.try_recv().is_err());
}

// SU3: nothing restarts the node by itself: after its end, the node's work is not started again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn su3_nothing_restarts_the_node_by_itself() {
    let started = Arc::new(AtomicUsize::new(0));
    let s = started.clone();
    let (sup, rx, _calls) = supervise(async move {
        s.fetch_add(1, Ordering::SeqCst);
        Err("ended".to_string())
    });
    tokio::time::timeout(BOUND, rx).await.expect("the end is observed").expect("on_stop is called");
    // Give a retry every chance to happen: many scheduler turns, then look.
    for _ in 0..200 {
        tokio::task::yield_now().await;
    }
    assert_eq!(started.load(Ordering::SeqCst), 1, "the node was started again without the user");
    assert!(!sup.is_running());
}
