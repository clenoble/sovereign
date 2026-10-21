//! After login, every persona's state stays its own; the orchestrator's state (conversation log
//! with its anchor and beacon, trust state) is per persona, and the directory used before this
//! change becomes the primary persona's (spec ch. 07, *After login — every persona's state stays
//! its own*, v2.0.25; Céline Q2 (a), 2026-10-16; cloud review of v0.0.10, finding 2).
//!
//! `orchestrator_state_dir(base, persona)` is the one place that says where a persona's
//! orchestrator state lives under the data directory `base`. The host passes its result to the
//! orchestrator; no code after login uses a persona-blind `base/orchestrator` of its own.

use super::{identity_profile_dir, orchestrator_state_dir, session_log_beacon_path, PersonaKind};
use std::path::Path;

fn base() -> &'static Path {
    Path::new("data-root")
}

#[test]
fn the_primary_keeps_the_directory_used_before() {
    // Q2 (a): the shared directory becomes the primary's — no move, no copy.
    assert_eq!(
        orchestrator_state_dir(base(), PersonaKind::Primary),
        base().join("orchestrator")
    );
}

#[test]
fn the_duress_persona_has_a_directory_of_its_own() {
    let p = orchestrator_state_dir(base(), PersonaKind::Primary);
    let d = orchestrator_state_dir(base(), PersonaKind::Duress);
    assert_ne!(d, p);
    assert!(!d.starts_with(&p), "the duress state must not live inside the primary's: {d:?}");
    assert!(!p.starts_with(&d), "the primary state must not live inside the duress one: {p:?}");
}

#[test]
fn both_live_under_the_data_directory() {
    for persona in [PersonaKind::Primary, PersonaKind::Duress] {
        let dir = orchestrator_state_dir(base(), persona);
        assert!(dir.starts_with(base()), "{dir:?}");
        assert_ne!(dir, base().to_path_buf(), "a persona's state is not the data root itself");
    }
}

// The conversation log's beacon lives outside the log directory on purpose (so wiping the log
// directory does not remove the proof that the log had content). It is part of the
// orchestrator's state, so it is per persona too: a duress session that writes the shared
// beacon makes the primary's log look wiped afterwards.

#[test]
fn the_primary_keeps_the_beacon_used_before() {
    assert_eq!(
        session_log_beacon_path(base(), PersonaKind::Primary),
        base().join("crypto").join("session_log.beacon")
    );
}

#[test]
fn the_duress_persona_has_a_beacon_of_its_own_outside_its_log_directory() {
    let p = session_log_beacon_path(base(), PersonaKind::Primary);
    let d = session_log_beacon_path(base(), PersonaKind::Duress);
    assert_ne!(d, p);
    for persona in [PersonaKind::Primary, PersonaKind::Duress] {
        let beacon = session_log_beacon_path(base(), persona);
        let log_dir = orchestrator_state_dir(base(), persona);
        assert!(beacon.starts_with(base()), "{beacon:?}");
        assert!(
            !beacon.starts_with(&log_dir),
            "the beacon must survive a wipe of the log directory: {beacon:?} in {log_dir:?}"
        );
    }
}

// The shell's user profile is split (spec v2.0.29, Céline 2026-10-17): device preferences read
// before login (theme, bubble style) stay in the profile file at `base`, shared; the persona's
// identity (display name, nickname, designation) lives in `identity_profile_dir(base, persona)`
// and is read only after login. The primary keeps the existing file's directory, so at migration
// the existing identity is the primary's; the duress persona starts with defaults.

#[test]
fn the_primary_keeps_the_existing_profile_location() {
    assert_eq!(identity_profile_dir(base(), PersonaKind::Primary), base().to_path_buf());
}

#[test]
fn the_duress_identity_lives_apart_from_everything_else() {
    let d = identity_profile_dir(base(), PersonaKind::Duress);
    assert!(d.starts_with(base()) && d != base().to_path_buf(), "{d:?}");
    for other in [
        identity_profile_dir(base(), PersonaKind::Primary),
        orchestrator_state_dir(base(), PersonaKind::Primary),
        orchestrator_state_dir(base(), PersonaKind::Duress),
    ] {
        assert_ne!(d, other, "the duress identity has a directory of its own");
    }
    assert!(
        !d.starts_with(orchestrator_state_dir(base(), PersonaKind::Duress)),
        "the identity profile is not part of the orchestrator's state: {d:?}"
    );
}

#[test]
fn the_same_persona_always_gets_the_same_directory() {
    for persona in [PersonaKind::Primary, PersonaKind::Duress] {
        assert_eq!(
            orchestrator_state_dir(base(), persona),
            orchestrator_state_dir(base(), persona)
        );
    }
}
