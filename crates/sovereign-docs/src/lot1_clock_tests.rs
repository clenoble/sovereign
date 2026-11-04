//! v0.0.12 lot 1, item 2 — C1 at the choke point: the bound is inside the
//! clock.
//!
//! Contract `doc/plans/V0012-lot1.md`, C1: "This device's clock never moves
//! more than `MAX_STAMP_AHEAD_SECS` ahead of its wall clock (`device_clock_ms`,
//! not the clock itself) because of a stamp it received or read.
//! `observe_stamp` enforces it for every caller and reports whether the stamp
//! was taken. A caller cannot forget it."
//!
//! Written before the code, by another author: whoever implements item 2
//! makes these pass **without editing this file**. A unit test (a child of the
//! crate, registered in `lib.rs`) because `observe_stamp`, `next_stamp` and
//! `device_clock_ms` are crate-private: the guarantee is about the choke
//! point itself, not about one caller (those are in
//! `tests/lot1_c_stamp_bound.rs`).
//!
//! API this file names: `observe_stamp(&self, &Stamp) -> bool`, true when the
//! stamp was taken (moved the clock or was already covered), false when it was
//! refused for being beyond the bound.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};
use sovereign_crypto::device_key::DeviceKey;
use sovereign_crypto::index_key::IndexKey;
use sovereign_crypto::kek::Kek;
use sovereign_crypto::key_db::KeyDatabase;
use sovereign_crypto::master_key::MasterKey;
use sovereign_db::encrypted::EncryptedGraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
use sovereign_db::traits::GraphDB;

use crate::host::{Clock, DefaultCommitPolicy, DocumentEvents, IngestRequest, IngestedItem, MediaFiles, PiiError, PiiIngest, ServiceDeps};
use crate::types::DocumentChanged;
use crate::wire::{Stamp, MAX_STAMP_AHEAD_SECS};
use crate::DocumentService;

struct FixedClock(Mutex<DateTime<Utc>>);
impl Clock for FixedClock {
    fn now(&self) -> DateTime<Utc> {
        *self.0.lock().unwrap()
    }
}

struct NoEvents;
impl DocumentEvents for NoEvents {
    fn emit(&self, _: DocumentChanged) {}
}

struct NoMedia;
impl MediaFiles for NoMedia {
    fn sha256(&self, _: &str) -> Option<String> {
        None
    }
}

struct PassThrough;
#[async_trait]
impl PiiIngest for PassThrough {
    async fn ingest(&self, request: IngestRequest) -> Result<Vec<IngestedItem>, PiiError> {
        Ok(request.items.iter().map(|i| IngestedItem { field: i.field, text: i.text.clone() }).collect())
    }
}

/// A service over an in-memory store, its wall clock fixed at `now`.
async fn service(tag: &str) -> (DocumentService, DateTime<Utc>) {
    let raw = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
    raw.connect().await.unwrap();
    raw.init_schema().await.unwrap();
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir();
    let kdb = |s: &str| {
        Arc::new(tokio::sync::RwLock::new(KeyDatabase::new(
            dir.join(format!("lot1-clock-{}-{nanos}-{seq}-{tag}-{s}.db", std::process::id())),
        )))
    };
    let mk = MasterKey::generate();
    let store = Arc::new(EncryptedGraphDB::new(
        Arc::new(raw) as Arc<dyn GraphDB>,
        kdb("doc"),
        kdb("msg"),
        kdb("thr"),
        kdb("conv"),
        kdb("con"),
        kdb("shr"),
        Arc::new(Kek::generate()),
        Arc::new(IndexKey::generate()),
        Arc::new(DeviceKey::derive(&mk, "test-device").unwrap()),
    ));
    let now = Utc.with_ymd_and_hms(2026, 10, 10, 12, 0, 0).unwrap();
    let svc = DocumentService::new(ServiceDeps {
        store,
        commit_policy: Arc::new(DefaultCommitPolicy::default()),
        pii: Arc::new(PassThrough),
        media: Arc::new(NoMedia),
        clock: Arc::new(FixedClock(Mutex::new(now))),
        events: Arc::new(NoEvents),
        device: "laptop".into(),
    });
    svc.ensure_clock().await.unwrap();
    (svc, now)
}

fn at(now: DateTime<Utc>, secs: i64, counter: u32) -> Stamp {
    Stamp { wall_ms: (now + Duration::seconds(secs)).timestamp_millis(), counter, device: "phone".into() }
}

fn bound(svc: &DocumentService) -> i64 {
    svc.device_clock_ms() + MAX_STAMP_AHEAD_SECS * 1000
}

// C1: a stamp within the bound is taken, and `observe_stamp` says so; the
// next stamp is after it (G25).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_observe_stamp_takes_a_stamp_within_the_bound_and_says_so() {
    let (svc, now) = service("within").await;
    let s = at(now, 60, 4);
    assert!(svc.observe_stamp(&s), "a stamp within the bound is taken");
    let next = svc.next_stamp().await.unwrap();
    assert!(next > s, "{next:?} after the taken {s:?}");
}

// C1: a stamp beyond the bound is refused, `observe_stamp` says so, and the
// clock does not move.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_observe_stamp_refuses_a_stamp_beyond_the_bound_and_says_so() {
    let (svc, now) = service("beyond").await;
    for secs in [MAX_STAMP_AHEAD_SECS + 1, 24 * 3600, 365 * 24 * 3600] {
        let s = at(now, secs, 0);
        assert!(!svc.observe_stamp(&s), "{secs} s ahead: refused");
        let next = svc.next_stamp().await.unwrap();
        assert!(next.wall_ms <= bound(&svc), "{secs} s ahead: the clock moved to {next:?}");
    }
}

// C1: the edge — exactly at the bound is within it (as `apply_wire` reads it
// today: refused only when `wall_ms > device_clock_ms + bound`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_a_stamp_exactly_at_the_bound_is_taken() {
    let (svc, now) = service("edge").await;
    let s = at(now, MAX_STAMP_AHEAD_SECS, 0);
    assert!(svc.observe_stamp(&s));
}

// C1: "ahead of its wall clock (`device_clock_ms`, not the clock itself)" —
// two stamps each within the bound of the previous one cannot walk the clock
// past the wall clock's bound.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c1_the_bound_is_measured_from_the_wall_clock_not_from_the_logical_clock() {
    let (svc, now) = service("walk").await;
    let first = at(now, MAX_STAMP_AHEAD_SECS - 10, 0);
    assert!(svc.observe_stamp(&first));
    // Within the bound of the logical clock (now at `first`), beyond the
    // bound of the wall clock.
    let second = at(now, 2 * MAX_STAMP_AHEAD_SECS - 20, 0);
    assert!(!svc.observe_stamp(&second), "the bound is the wall clock's, not the logical clock's");
    let next = svc.next_stamp().await.unwrap();
    assert!(next.wall_ms <= bound(&svc), "{next:?}");
}

// C1 + C4 at the choke point: a taken stamp at the maximum counter, then the
// next local stamp is the next millisecond, counter 0 (no wrap, no panic).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn c4_after_observing_the_maximum_counter_the_next_stamp_is_the_next_millisecond() {
    let (svc, now) = service("max").await;
    let s = at(now, 60, u32::MAX);
    assert!(svc.observe_stamp(&s));
    let next = svc.next_stamp().await.unwrap();
    assert_eq!((next.wall_ms, next.counter), (s.wall_ms + 1, 0));
    let again = svc.next_stamp().await.unwrap();
    assert_eq!((again.wall_ms, again.counter), (s.wall_ms + 1, 1));
}
