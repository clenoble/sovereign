//! `auth.store` is never torn (spec ch. 07, *Never a torn store*, AS1-AS3).
//!
//! A torn `auth.store` locks an account out for good: its presence sends to
//! login, and its load fails at every attempt. These tests stand a crash at
//! every point of a write. Each write runs in a child process (this test
//! binary, `child_entry`), which aborts just before its n-th filesystem
//! operation (`fs_private::fault`), with no unwinding and no cleanup, as a
//! crash or a power loss would. The parent then reads what the crash left, for
//! n = 0, 1, 2, … until a write completes.
//!
//! Hard links are refused in some runs, as in the app's data directory on
//! Céline's phone (measured 2026-10-21), where the first write used to fall back
//! to writing the file in place.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use crate::auth::AuthStore;
use crate::fs_private::fault::{self, Plan};

const CHILD_VAR: &str = "AUTH_STORE_WRITE_CHILD";
const DIR_VAR: &str = "AUTH_STORE_WRITE_DIR";
const SRC_VAR: &str = "AUTH_STORE_WRITE_SRC";
const ABORT_VAR: &str = "AUTH_STORE_WRITE_ABORT";
const REFUSE_VAR: &str = "AUTH_STORE_WRITE_REFUSE_LINKS";
const HALF_VAR: &str = "AUTH_STORE_WRITE_HALF";

const STARTED: &str = "AUTH-STORE-WRITE started";
const COMPLETED: &str = "AUTH-STORE-WRITE completed ops=";

/// More operations than any write of one small file needs; reaching it means
/// the walk never saw a write complete.
const MAX_OPS: usize = 64;

// ================================================================ the stores

/// Two real stores, made once: the one a write replaces, and the one it writes.
fn stores() -> &'static (Vec<u8>, Vec<u8>) {
    static STORES: OnceLock<(Vec<u8>, Vec<u8>)> = OnceLock::new();
    STORES.get_or_init(|| {
        let old = AuthStore::create(b"Old!Primary12345", b"Old!Duress123456", b"old-salt-32-bytes-long-padding!!", "device-old")
            .expect("the old store");
        let new = AuthStore::create(b"New!Primary12345", b"New!Duress123456", b"new-salt-32-bytes-long-padding!!", "device-new")
            .expect("the new store");
        let old = serde_json::to_vec_pretty(&old).unwrap();
        let new = serde_json::to_vec_pretty(&new).unwrap();
        assert_ne!(old, new);
        (old, new)
    })
}

fn old_bytes() -> &'static [u8] {
    &stores().0
}

fn new_bytes() -> &'static [u8] {
    &stores().1
}

// ============================================================ the child side

#[test]
#[ignore = "a write, run by its parent test in a child process that may abort"]
fn child_entry() {
    let Ok(mode) = std::env::var(CHILD_VAR) else { return };
    let dir = PathBuf::from(std::env::var_os(DIR_VAR).expect("the directory"));
    let src = PathBuf::from(std::env::var_os(SRC_VAR).expect("the store to write"));
    let abort_before = std::env::var(ABORT_VAR).ok().map(|n| n.parse::<usize>().expect("an operation number"));
    let refuse_hard_links = std::env::var(REFUSE_VAR).is_ok();
    let half_write = std::env::var(HALF_VAR).is_ok();

    // Read before the plan is installed, so that reading counts no operation.
    let store = AuthStore::load(&src).expect("the store to write loads");
    fault::install(Plan { refuse_hard_links, abort_before, half_write });
    println!("{STARTED}");
    let path = dir.join("auth.store");
    match mode.as_str() {
        "first" => {
            store.save_new(&path).expect("the first write");
        }
        "replace" => store.save(&path).expect("the replacing write"),
        other => panic!("unknown mode {other}"),
    }
    println!("{COMPLETED}{}", fault::ops());
}

// =========================================================== the parent side

fn module() -> &'static str {
    module_path!()
}

enum Run {
    /// The child aborted, as planned, before it finished the write.
    Crashed,
    /// The child finished the write, after this many operations.
    Completed(usize),
}

fn run_child(mode: &str, dir: &Path, src: &Path, abort_before: Option<usize>, refuse: bool, half: bool) -> Run {
    let rel = module().split_once("::").map(|(_, r)| r).unwrap_or(module());
    let exe = std::env::current_exe().expect("the test binary");
    let mut cmd = Command::new(exe);
    cmd.args(["--exact", &format!("{rel}::child_entry"), "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_VAR, mode)
        .env(DIR_VAR, dir)
        .env(SRC_VAR, src)
        .env_remove(ABORT_VAR)
        .env_remove(REFUSE_VAR)
        .env_remove(HALF_VAR);
    if let Some(n) = abort_before {
        cmd.env(ABORT_VAR, n.to_string());
    }
    if refuse {
        cmd.env(REFUSE_VAR, "1");
    }
    if half {
        cmd.env(HALF_VAR, "1");
    }
    let o = cmd.output().expect("spawn the child write");
    let stdout = String::from_utf8_lossy(&o.stdout);
    let stderr = String::from_utf8_lossy(&o.stderr);
    let context = || format!("mode {mode}, abort before {abort_before:?}, links refused {refuse}, half write {half}\n--- stdout\n{stdout}\n--- stderr\n{stderr}");
    assert!(stdout.contains(STARTED), "the child never reached the write: {}", context());
    assert!(!stdout.contains("panicked") && !stderr.contains("panicked"), "the child failed, it did not crash: {}", context());
    if let Some(line) = stdout.lines().find(|l| l.starts_with(COMPLETED)) {
        assert!(o.status.success(), "the write completed but the child failed: {}", context());
        let n = line[COMPLETED.len()..].trim().parse().expect("an operation count");
        return Run::Completed(n);
    }
    assert!(!o.status.success(), "the child neither completed nor crashed: {}", context());
    Run::Crashed
}

/// A fresh directory, with the store to write beside it (never in it).
fn fresh(base: &Path, name: &str) -> (PathBuf, PathBuf) {
    let case = base.join(name);
    let dir = case.join("crypto");
    std::fs::create_dir_all(&dir).unwrap();
    let src = case.join("to-write.json");
    std::fs::write(&src, new_bytes()).unwrap();
    (dir, src)
}

/// What the directory holds besides `auth.store`.
fn others(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n != "auth.store")
        .collect()
}

/// Walks a crash through every operation of one kind of write, and checks
/// what each crash left with `check`. Returns the number of crashes seen.
fn walk(mode: &str, refuse: bool, half: bool, check: impl Fn(usize, &Path, &Path)) -> usize {
    let base = tempfile::Builder::new().prefix("sov-auth-store-write-").tempdir().expect("a temporary base");
    let mut crashes = 0;
    for n in 0..MAX_OPS {
        let (dir, src) = fresh(base.path(), &format!("n{n}"));
        if mode == "replace" {
            std::fs::write(dir.join("auth.store"), old_bytes()).unwrap();
        }
        match run_child(mode, &dir, &src, Some(n), refuse, half) {
            Run::Crashed => {
                crashes += 1;
                check(n, &dir, &src);
            }
            Run::Completed(total) => {
                assert_eq!(total, n, "a write that completes with a crash planned before operation {n} performed {total} operations");
                assert!(total >= 3, "a write performed only {total} operations through the fault plan: its operations do not all go through it");
                assert_eq!(std::fs::read(dir.join("auth.store")).unwrap(), new_bytes(), "a completed write holds the new store");
                assert!(others(&dir).is_empty(), "a completed write leaves nothing beside the store: {:?}", others(&dir));
                return crashes;
            }
        }
    }
    panic!("no write completed within {MAX_OPS} operations ({mode}, links refused {refuse}, half write {half})");
}

/// AS1 and AS3 after a crash in a first write: no store, or the whole new one,
/// and what the crash left beside it never blocks the next first write.
fn check_first(n: usize, dir: &Path, src: &Path) {
    let path = dir.join("auth.store");
    if path.exists() {
        let held = std::fs::read(&path).unwrap();
        assert_eq!(held, new_bytes(), "a crash before operation {n} of a first write left a torn auth.store ({} of {} bytes)", held.len(), new_bytes().len());
        AuthStore::load(&path).expect("a store present after a crash loads, or it locks out for good");
    } else {
        // AS3: a stray file sends nowhere, and the next setup writes the store.
        assert!(!path.exists(), "no auth.store: the app shows setup, not login");
        let store = AuthStore::load(src).unwrap();
        store
            .save_new(&path)
            .unwrap_or_else(|e| panic!("after a crash before operation {n}, with {:?} left beside it, the next first write is refused: {e}", others(dir)));
        assert_eq!(std::fs::read(&path).unwrap(), new_bytes());
        AuthStore::load(&path).expect("the next first write loads");
    }
}

/// AS2 and AS3 after a crash in a replacing write: the previous store or the
/// new one, never a torn file, never none; and the next write still works.
fn check_replace(n: usize, dir: &Path, src: &Path) {
    let path = dir.join("auth.store");
    assert!(path.exists(), "a crash before operation {n} of a replacing write left no auth.store");
    let held = std::fs::read(&path).unwrap();
    assert!(
        held == old_bytes() || held == new_bytes(),
        "a crash before operation {n} of a replacing write left a torn auth.store ({} bytes; the old store has {}, the new {})",
        held.len(),
        old_bytes().len(),
        new_bytes().len(),
    );
    AuthStore::load(&path).expect("the store after a crash loads");
    let store = AuthStore::load(src).unwrap();
    store
        .save(&path)
        .unwrap_or_else(|e| panic!("after a crash before operation {n}, with {:?} left beside it, the next write fails: {e}", others(dir)));
    assert_eq!(std::fs::read(&path).unwrap(), new_bytes());
}

// ================================================================= the tests

/// AS1, where hard links work: a crash at any point of the first write leaves
/// no `auth.store` or the whole new one.
#[test]
fn as1_a_first_write_cut_anywhere_leaves_no_store_or_the_whole_one() {
    let crashes = walk("first", false, false, check_first);
    assert!(crashes >= 3, "only {crashes} crash points were walked");
}

/// AS1 where hard links are refused, as on Céline's phone: the same, and
/// this is where the first write used to be written in place.
#[test]
fn as1_where_hard_links_are_refused_a_first_write_is_never_torn() {
    let crashes = walk("first", true, false, check_first);
    assert!(crashes >= 3, "only {crashes} crash points were walked");
}

/// AS1 with half of a write's bytes on disk when the crash comes, with hard
/// links refused and allowed.
#[test]
fn as1_a_first_write_cut_in_the_middle_of_its_bytes_is_never_torn() {
    for refuse in [true, false] {
        let crashes = walk("first", refuse, true, check_first);
        assert!(crashes >= 3, "only {crashes} crash points were walked (links refused {refuse})");
    }
}

/// AS2: a crash at any point of a write that replaces an existing
/// `auth.store` leaves the previous file or the new one.
#[test]
fn as2_a_replacing_write_cut_anywhere_leaves_the_old_store_or_the_new_one() {
    let crashes = walk("replace", false, false, check_replace);
    assert!(crashes >= 3, "only {crashes} crash points were walked");
}

/// AS2 with half of a write's bytes on disk when the crash comes, and with
/// hard links refused (a replacing write must not depend on them either).
#[test]
fn as2_a_replacing_write_cut_in_the_middle_of_its_bytes_is_never_torn() {
    for refuse in [false, true] {
        let crashes = walk("replace", refuse, true, check_replace);
        assert!(crashes >= 3, "only {crashes} crash points were walked (links refused {refuse})");
    }
}

/// AS3: what a crash leaves beside the store is never at the store's path,
/// so it never sends to login; when the crash came before the store was
/// published, the directory has no `auth.store` and the app shows setup.
#[test]
fn as3_a_crash_before_publication_leaves_nothing_at_the_stores_path() {
    let base = tempfile::Builder::new().prefix("sov-auth-store-write-").tempdir().expect("a temporary base");
    for refuse in [false, true] {
        // The first operation of a write cannot have published the store.
        let (dir, src) = fresh(base.path(), &format!("first-op-{refuse}"));
        match run_child("first", &dir, &src, Some(1), refuse, false) {
            Run::Crashed => {}
            Run::Completed(n) => panic!("a first write completed in {n} operation(s)"),
        }
        assert!(!dir.join("auth.store").exists(), "a crash at the start of a first write left an auth.store (links refused {refuse})");
        for name in others(&dir) {
            assert_ne!(name.trim().to_ascii_lowercase(), "auth.store", "a stray file is named as the store");
        }
    }
}
