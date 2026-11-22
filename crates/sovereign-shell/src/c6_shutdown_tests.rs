//! C6 (v0.0.13 scope, C. v0.0.12 leftovers): the shell closes its database
//! before `main` returns. Céline, 2026-10-28: « éviter un crash ou une
//! possible corruption, même si rare, et ça coûte peu ». BACKLOG §13: on quit,
//! stop P2P and the other database tasks, drop the `Surreal` handle, then
//! return; the wait is bounded, so a stuck task never turns quitting into a
//! frozen window.
//!
//! Written before the code, by another author: whoever implements C6 makes
//! these pass **without editing this file**; a test that looks wrong is
//! reported, not worked around.
//!
//! No spec section holds C6; the guarantees below are this author's reading of
//! the backlog's words, numbered here only to name them:
//!
//! - **G1, no handle left:** after `App::close_before_exit`, the shell holds
//!   no handle on the store: its own handle is dropped (the `Surreal` handle
//!   with it) and so are the ones its two database tasks held, though they still
//!   ran when the close started; the report says so.
//! - **Not held here: that the RocksDB files are closed when `main` returns.**
//!   Measured on PANDA2, 2026-11-17 (a throwaway probe, not committed):
//!   surrealdb 2.6.5 keeps a store whose indexes were defined (`init_schema`)
//!   open after its last handle is dropped, runtime ended or not (its path still
//!   refused a second open 15 s later). A test that reopens the store after the
//!   close could not pass; the shell can only release what it holds.
//! - **G2, P2P first:** the P2P node is asked to stop (it receives Shutdown,
//!   or sees its command channel close) rather than cut off, and at that moment
//!   the App still holds its own handle on the store.
//! - **G3, bounded:** a task that never ends, a blocking one holding the
//!   store, does not hold the close past its bound (with a generous margin);
//!   the report then does not say the handles were released, since a task
//!   still holds one.
//! - **G4, called:** `main` calls the close after the event loop returns.
//!
//! A child module of `app` (declared in `app.rs`, like `csup_tests`), on a
//! real headless `App` (`App::new()` needs no window). `SOVEREIGN_DATA_DIR` is
//! set to a fresh temporary folder before any `App` is made, and each store is
//! a fresh folder of its own (`sov-c6-…` in the temporary folder).

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::time::{Duration, Instant};

use sovereign_db::traits::GraphDB;
use sovereign_p2p::P2pCommand;

use super::{App, ExitClose};
use crate::canvas::open_db_at;

/// The bound the tests give the close.
const BOUND: Duration = Duration::from_secs(2);
/// How far past its bound the close may return before a test calls it late:
/// generous, so a slow machine does not fail the test.
const MARGIN: Duration = Duration::from_secs(5);
/// When the watchdog releases the tests' own tasks, so that a close that waits
/// for them forever still ends the test (as a failure).
const WATCHDOG: Duration = Duration::from_secs(20);

fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let d = std::env::temp_dir().join(format!("sov-c6-data-{}-{}", std::process::id(), nanos()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

fn nanos() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default()
}

/// A headless, unlocked shell holding a real store (RocksDB) of its own, in
/// the returned folder (`sov-c6-…`, left behind: the engine keeps it open, see
/// the header).
fn shell_with_store(tag: &str) -> (App, PathBuf) {
    ensure_test_data_dir();
    let mut app = App::new();
    let folder = std::env::temp_dir().join(format!("sov-c6-{tag}-{}-{}", std::process::id(), nanos()));
    let db = app.rt.block_on(open_db_at(&folder.join("db"))).expect("a fresh store opens");
    app.db = Some(db);
    app.locked = false;
    (app, folder)
}

/// A reference to the shell's store that does not keep it: it counts the
/// handles held, and holds none itself.
fn watch(app: &App) -> Weak<dyn GraphDB> {
    Arc::downgrade(app.db.as_ref().expect("the shell holds a store"))
}

fn panic_text(payload: &(dyn Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic with no message".into())
}

/// Runs the close with `bound` and measures how long it took to return. A
/// watchdog sets `release` after [`WATCHDOG`], so a close that waits forever
/// for the tests' tasks still ends; the test sets it once the close returned.
/// A panic of the close (or of the App's drop) fails the test, with how long
/// it took.
fn timed_close(app: App, bound: Duration, release: &Arc<AtomicBool>) -> (ExitClose, Duration) {
    let (report, took, _) = timed_close_counting(app, bound, release, &Weak::<sovereign_db::surreal::SurrealGraphDB>::new());
    (report, took)
}

/// As [`timed_close`], and how many handles on `store` were held when the
/// close returned, counted before the test's own tasks are released.
fn timed_close_counting<T: ?Sized>(
    app: App,
    bound: Duration,
    release: &Arc<AtomicBool>,
    store: &Weak<T>,
) -> (ExitClose, Duration, usize) {
    let done = Arc::new(AtomicBool::new(false));
    let dog = {
        let (done, release) = (done.clone(), release.clone());
        std::thread::spawn(move || {
            let start = Instant::now();
            while !done.load(Ordering::SeqCst) && start.elapsed() < WATCHDOG {
                std::thread::sleep(Duration::from_millis(20));
            }
            release.store(true, Ordering::SeqCst);
        })
    };
    let start = Instant::now();
    let out = catch_unwind(AssertUnwindSafe(move || app.close_before_exit(bound)));
    let took = start.elapsed();
    let left = store.strong_count();
    done.store(true, Ordering::SeqCst);
    release.store(true, Ordering::SeqCst);
    let _ = dog.join();
    match out {
        Ok(report) => (report, took, left),
        Err(payload) => panic!("App::close_before_exit panicked after {took:?}: {}", panic_text(payload.as_ref())),
    }
}

/// A task on the shell's runtime that holds the store until it is stopped
/// (cancelled at an await) or released, as the App's purge and migration tasks
/// hold theirs.
fn holding_task(app: &App, release: &Arc<AtomicBool>) -> tokio::task::JoinHandle<()> {
    let held = app.db.clone().expect("the shell holds a store");
    let release = release.clone();
    app.rt.spawn(async move {
        let _held = held;
        while !release.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
}

/// G1: after the close, the shell holds no handle on the store, and the
/// report says so. The App's purge and migration tasks hold the store and
/// still run when the close starts; they end only when stopped.
#[test]
fn c6_g1_after_the_close_the_shell_holds_no_handle_on_the_store() {
    let (mut app, _folder) = shell_with_store("g1");
    let release = Arc::new(AtomicBool::new(false));
    app.purge_task = Some(holding_task(&app, &release));
    app.migration_task = Some(holding_task(&app, &release));
    let store = watch(&app);
    assert_eq!(store.strong_count(), 3, "precondition: the App and its two tasks hold the store");

    let (report, took, left) = timed_close_counting(app, BOUND, &release, &store);

    assert!(took <= BOUND + MARGIN, "the close returned after {took:?}, past its bound {BOUND:?} (+{MARGIN:?})");
    assert_eq!(
        left, 0,
        "when the close returned, {left} handle(s) on the store were still held \
         (the App's own, or the ones its purge and migration tasks hold)"
    );
    assert!(report.handles_released, "every handle was released, but the report says not: {report:?}");
}

/// What the test's P2P node saw when it ended.
#[derive(Debug)]
struct NodeSaw {
    /// Another holder than the node held the store at that moment: the App's
    /// own handle (checked first to be the only other one).
    app_held: bool,
}

/// G2: the P2P node is asked to stop, not cut off, and before the App drops
/// its handle. The node is supervised as a real one is (same stop grace), and
/// holds the store as the real sync service does.
#[test]
fn c6_g2_p2p_is_stopped_before_the_handle_is_dropped() {
    let (mut app, _folder) = shell_with_store("g2");
    let saw: Arc<Mutex<Option<NodeSaw>>> = Arc::new(Mutex::new(None));
    let node_db = app.db.clone().expect("the shell holds a store");
    let record = saw.clone();
    let handle = crate::p2p::P2pHandle::fake_supervised(&app.rt, move |mut commands| async move {
        loop {
            // Shutdown, or the command channel closed: either is a stop asked.
            match commands.recv().await {
                Some(P2pCommand::Shutdown) | None => {}
                Some(_) => continue,
            }
            let app_held = Arc::strong_count(&node_db) >= 2;
            *record.lock().unwrap() = Some(NodeSaw { app_held });
            return Ok(());
        }
    });
    app.p2p = Some(handle);
    assert_eq!(
        Arc::strong_count(app.db.as_ref().unwrap()),
        2,
        "precondition: only the App and the test's node hold the store"
    );
    let release = Arc::new(AtomicBool::new(false));

    let (_report, took) = timed_close(app, BOUND, &release);

    assert!(took <= BOUND + MARGIN, "the close returned after {took:?}, past its bound {BOUND:?} (+{MARGIN:?})");
    let saw = saw.lock().unwrap().take();
    let saw = saw.unwrap_or_else(|| {
        panic!(
            "the P2P node never saw its stop: it was cut off (aborted, or its runtime ended) \
             instead of being asked to stop"
        )
    });
    assert!(
        saw.app_held,
        "when the P2P node was asked to stop, the App no longer held its handle on the store: \
         the handle was dropped before P2P was stopped ({saw:?})"
    );
}

/// G3: a task that never ends, a blocking one holding the store, does not
/// hold the close past its bound; the report does not say the store closed.
/// (Released by the test once the close returned, or by the watchdog.)
#[test]
fn c6_g3_a_stuck_task_does_not_hold_the_close_past_its_bound() {
    let (app, _folder) = shell_with_store("g3");
    let release = Arc::new(AtomicBool::new(false));
    let started = Arc::new(AtomicBool::new(false));
    {
        let held = app.db.clone().expect("the shell holds a store");
        let (release, started) = (release.clone(), started.clone());
        let _stuck = app.rt.spawn_blocking(move || {
            let _held = held;
            started.store(true, Ordering::SeqCst);
            while !release.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
        });
    }
    let wait = Instant::now();
    while !started.load(Ordering::SeqCst) {
        assert!(wait.elapsed() < Duration::from_secs(10), "precondition: the stuck task started");
        std::thread::sleep(Duration::from_millis(5));
    }

    let (report, took) = timed_close(app, BOUND, &release);

    assert!(
        took <= BOUND + MARGIN,
        "with a stuck task, the close returned after {took:?}, past its bound {BOUND:?} (+{MARGIN:?}): \
         quitting would freeze the window"
    );
    assert!(
        !report.handles_released,
        "a task still held the store when the close returned, but the report says every handle \
         was released: {report:?}"
    );
}

/// `main.rs` with its `//` comments removed (a comment naming the close does
/// not call it).
fn main_source() -> String {
    include_str!("main.rs")
        .lines()
        .map(|l| match l.find("//") {
            Some(i) => &l[..i],
            None => l,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// G4: `main` calls the close after the event loop returns (`run_app`), so
/// the store is closed before `main` returns.
#[test]
fn c6_g4_main_closes_the_store_after_the_event_loop() {
    let src = main_source();
    let body_at = src.find("fn main()").expect("main.rs has fn main()");
    let body = &src[body_at..];
    let run = body.find(".run_app(").expect("main runs the event loop with run_app");
    let close = body.find(".close_before_exit(").unwrap_or_else(|| {
        panic!("main never calls App::close_before_exit: the App is only dropped when main returns")
    });
    assert!(close > run, "main calls close_before_exit before the event loop has run (run_app)");
}
