//! rc2, Q1 of the laptop's second position (claude-coord
//! `from-laptop/0108-rc2-second-position.md`): `write_private_new` publishes
//! the new file by a hard link, or — where links are refused (possibly the
//! Android app sandbox) — by an exclusive in-place write. Which path it took is
//! returned, so the app can log it and Céline's phone test shows it in logcat.
//!
//! Written before the code, by another author: whoever implements it makes
//! these pass without editing this file.
//!
//! Contract (settled by the coordinator, 2026-10-19):
//! `pub enum NewWrite { Linked, InPlace }` in `sovereign_crypto::fs_private`
//! (`Debug, Clone, Copy, PartialEq, Eq`); `write_private_new(path, bytes) ->
//! io::Result<NewWrite>`; `AuthStore::save_new(path) -> CryptoResult<NewWrite>`.
//!
//! These tests run on a filesystem where hard links work (NTFS on the Windows
//! machines, ext4/tmpfs under WSL), so a fresh write must report `Linked`. On a
//! filesystem without hard links they would fail by design: the mode is the
//! fact they report.

use sovereign_crypto::auth::AuthStore;
use sovereign_crypto::fs_private::{write_private_new, NewWrite};

fn assert_mode_traits<T: std::fmt::Debug + Clone + Copy + PartialEq + Eq>() {}

#[test]
fn new_write_has_the_two_modes_and_its_traits() {
    assert_mode_traits::<NewWrite>();
    assert_ne!(NewWrite::Linked, NewWrite::InPlace);
    // The log line shows the mode by name ({:?}).
    assert_eq!(format!("{:?}", NewWrite::Linked), "Linked");
    assert_eq!(format!("{:?}", NewWrite::InPlace), "InPlace");
}

#[test]
fn a_fresh_write_private_new_reports_linked_where_hard_links_work() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fresh");
    let mode = write_private_new(&path, b"secret").expect("a fresh exclusive write");
    assert_eq!(
        mode,
        NewWrite::Linked,
        "on a filesystem with hard links the new file must be published by a link"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"secret");
}

#[test]
fn a_refused_write_private_new_reports_no_mode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("taken");
    std::fs::write(&path, b"first").unwrap();
    let r = write_private_new(&path, b"second");
    assert_eq!(
        r.as_ref().map_err(|e| e.kind()),
        Err(std::io::ErrorKind::AlreadyExists),
        "an existing file is refused with AlreadyExists"
    );
    assert_eq!(std::fs::read(&path).unwrap(), b"first");
}

#[test]
fn save_new_reports_the_mode_it_published_with() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.store");
    let store = AuthStore::create(
        b"Primary!Pass1234",
        b"Duress!Pass5678",
        b"test-salt-32-bytes-long-padding!",
        "test-device-mode",
    )
    .unwrap();
    let mode: NewWrite = store.save_new(&path).expect("save_new on an absent path");
    assert_eq!(mode, NewWrite::Linked, "save_new publishes auth.store by a link where links work");
    assert!(AuthStore::load(&path).is_ok());
}
