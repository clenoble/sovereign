//! A login with the real passphrase clears the recovery in progress on this device, and a
//! recovery step already running when the login happens cannot bring it back once it ends
//! (spec ch. 06, *Owner cancellation of a takeover*, 2026-10-17; cloud review of v0.0.10,
//! finding 8).
//!
//! The step in question is the phone's `access_recovery_poll`: it `load`s the recovery, runs a
//! guardian round of up to ~20 s, then `save`s. If the owner's login `cancel`s in between, that
//! final `save` must not recreate the recovery. A recovery *started after* the cancel, and the
//! ordinary save of a recovery nobody cancelled, must still save.

use super::*;
use sovereign_crypto::recovery_roster::RecoverySetup;

const SEAL: [u8; 32] = [0x5e; 32];

fn recovery(id: &str) -> AccessRecovery {
    let kek = Kek::from_bytes([0x61; 32]);
    let ak = AccountKey::from_bytes([0x62; 32]);
    let setup = RecoverySetup::new(&kek, &ak, 1).unwrap();
    AccessRecovery::new(
        id.into(),
        "T_g-test".into(),
        1,
        3,
        (0..5).map(|i| (format!("g{i}"), format!("/addr/{i}"))).collect(),
        setup.bundle,
    )
}

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "sovereign-access-recovery-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_step_that_loaded_before_the_cancel_cannot_recreate_the_recovery() {
    let dir = temp_dir("inflight");
    recovery("rec-inflight").save(&dir, &SEAL).unwrap();

    // The poll loads the recovery and starts its guardian round...
    let mut in_flight = AccessRecovery::load(&dir).expect("saved recovery loads");
    in_flight.unseal(&SEAL).unwrap();

    // ...the owner logs in on this device meanwhile...
    AccessRecovery::cancel(&dir);
    assert!(AccessRecovery::load(&dir).is_none(), "login cleared it");

    // ...and the round ends with its save. Whether that save reports an error or not, the
    // recovery must stay cancelled.
    let _ = in_flight.save(&dir, &SEAL);
    assert!(
        AccessRecovery::load(&dir).is_none(),
        "a step that loaded before the owner's login brought the recovery back"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recovery_started_after_the_cancel_saves_normally() {
    let dir = temp_dir("restart");
    recovery("rec-old").save(&dir, &SEAL).unwrap();
    AccessRecovery::cancel(&dir);

    recovery("rec-new").save(&dir, &SEAL).unwrap();
    let back = AccessRecovery::load(&dir).expect("a new recovery is not blocked by an old cancel");
    assert_eq!(back.recovery_id, "rec-new");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_recovery_nobody_cancelled_keeps_saving_round_after_round() {
    let dir = temp_dir("rounds");
    recovery("rec-rounds").save(&dir, &SEAL).unwrap();
    for _ in 0..3 {
        let mut r = AccessRecovery::load(&dir).expect("still there");
        r.unseal(&SEAL).unwrap();
        r.save(&dir, &SEAL).unwrap();
    }
    assert_eq!(AccessRecovery::load(&dir).unwrap().recovery_id, "rec-rounds");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_when_nothing_is_in_progress_is_harmless() {
    let dir = temp_dir("empty");
    AccessRecovery::cancel(&dir);
    assert!(AccessRecovery::load(&dir).is_none());
    recovery("rec-after").save(&dir, &SEAL).unwrap();
    assert!(AccessRecovery::load(&dir).is_some());

    let _ = std::fs::remove_dir_all(&dir);
}
