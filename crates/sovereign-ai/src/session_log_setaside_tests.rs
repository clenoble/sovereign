//! The orchestrator directory shared by both personas before v2.0.25 became the primary's. If its
//! conversation log does not verify under the primary's key (it may have been minted in a duress
//! session), it is set aside unread — kept on disk, never read — and the primary starts an empty
//! log. **Once only**, at the first primary login after the change: afterwards a log that does not
//! verify fails closed, as before (SESSIONLOG-001), so the rule can never hide later tampering
//! (spec ch. 07, *After login*, *Migration*, v2.0.29; Céline 2026-10-17).
//!
//! `set_aside_unverifiable_once(dir, key, beacon)` is called by the host for the persona's
//! directory before installing the log key. It returns `SetAside::MovedTo(path)` when it set a log
//! aside (the host then tells the user once), `SetAside::Nothing` otherwise. It leaves a marker in
//! `dir` so that it never acts twice.

use super::*;
use std::fs;
use std::path::{Path, PathBuf};

const PRIMARY_KEY: [u8; 32] = [0x11; 32];
const DURESS_KEY: [u8; 32] = [0x22; 32];

fn fresh(tag: &str) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!(
        "session-log-setaside-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&base);
    fs::create_dir_all(&base).unwrap();
    (base.join("orchestrator"), base.join("session_log.beacon"))
}

/// A log with content written under `key`, reopened once so its beacon exists too.
fn write_log(dir: &Path, key: [u8; 32], beacon: &Path) {
    {
        let mut log = SessionLog::open_encrypted(dir, key, Some(beacon)).unwrap();
        log.log_user_input("chat", "an earlier conversation", "chat");
    }
    let _ = SessionLog::open_encrypted(dir, key, Some(beacon)).unwrap();
}

fn read(p: &Path) -> Vec<u8> {
    fs::read(p).unwrap_or_default()
}

#[test]
fn a_log_minted_under_another_key_is_set_aside_unread_and_the_primary_starts_empty() {
    let (dir, beacon) = fresh("other-key");
    write_log(&dir, DURESS_KEY, &beacon);
    let original = read(&dir.join("session_log.jsonl"));
    assert!(!original.is_empty());

    let outcome = set_aside_unverifiable_once(&dir, PRIMARY_KEY, Some(&beacon)).unwrap();
    let moved = match outcome {
        SetAside::MovedTo(p) => p,
        SetAside::Nothing => panic!("an unverifiable log must be set aside at migration"),
    };
    // Kept on disk, byte for byte, somewhere the log is never read from.
    assert_eq!(read(&moved.join("session_log.jsonl")), original, "kept unchanged");
    assert!(!dir.join("session_log.jsonl").exists(), "the live log starts empty");

    // The primary's log now opens and appends without a tamper or genesis-wipe error, and the
    // set-aside entries are not read back as history.
    let slot = SessionLogSlot::open(&dir);
    slot.install_key(PRIMARY_KEY, Some(&beacon)).expect("the primary's log opens after the set-aside");
    slot.with_log(|l| l.log_user_input("chat", "after the migration", "chat"));
    let recent = slot.load_recent(50);
    assert_eq!(recent.len(), 1, "only the new entry is read: {recent:?}");
}

#[test]
fn it_acts_once_only_so_later_tampering_still_fails_closed() {
    let (dir, beacon) = fresh("once");
    write_log(&dir, PRIMARY_KEY, &beacon);
    assert!(matches!(
        set_aside_unverifiable_once(&dir, PRIMARY_KEY, Some(&beacon)).unwrap(),
        SetAside::Nothing
    ));

    // Later, the anchor is forged.
    fs::write(dir.join("session_log.anchor"), b"forged").unwrap();
    let before = read(&dir.join("session_log.jsonl"));

    // The migration does not act a second time...
    assert!(matches!(
        set_aside_unverifiable_once(&dir, PRIMARY_KEY, Some(&beacon)).unwrap(),
        SetAside::Nothing
    ));
    assert_eq!(read(&dir.join("session_log.jsonl")), before, "nothing moved the second time");
    // ...so the ordinary fail-closed check still sees the tampering.
    let slot = SessionLogSlot::open(&dir);
    assert!(slot.install_key(PRIMARY_KEY, Some(&beacon)).is_err(), "tampering must still fail closed");
}

#[test]
fn a_log_that_verifies_is_kept_as_it_is() {
    let (dir, beacon) = fresh("valid");
    write_log(&dir, PRIMARY_KEY, &beacon);
    let before = read(&dir.join("session_log.jsonl"));
    assert!(matches!(
        set_aside_unverifiable_once(&dir, PRIMARY_KEY, Some(&beacon)).unwrap(),
        SetAside::Nothing
    ));
    assert_eq!(read(&dir.join("session_log.jsonl")), before);
    let slot = SessionLogSlot::open(&dir);
    slot.install_key(PRIMARY_KEY, Some(&beacon)).unwrap();
    assert_eq!(slot.load_recent(50).len(), 1, "the earlier entry is still read");
}

#[test]
fn an_empty_directory_needs_nothing() {
    let (dir, beacon) = fresh("empty");
    assert!(matches!(
        set_aside_unverifiable_once(&dir, PRIMARY_KEY, Some(&beacon)).unwrap(),
        SetAside::Nothing
    ));
    let slot = SessionLogSlot::open(&dir);
    slot.install_key(PRIMARY_KEY, Some(&beacon)).unwrap();
}
