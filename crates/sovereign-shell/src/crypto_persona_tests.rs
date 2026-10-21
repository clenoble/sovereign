//! Guardian access recovery belongs to the primary persona: the recovery roster, its bundle and
//! the enrollment queue are the primary's, and a duress session never reads or writes them
//! (spec ch. 07, *After login — every persona's state stays its own*, v2.0.25; cloud review of
//! v0.0.10, finding 1).
//!
//! `recovery_store_for(persona)` is the only way the shell reaches the recovery store after
//! login: `Some` for the primary persona, `None` for the duress persona. Which callers go through
//! it is checked at review: guardian enrollment (`open_guardian_enroll`, the `g` key), the fold of
//! confirmed enrollments, Settings → Recovery, and the p2p translator's enrollment queue.
//!
//! spec v2.0.29 (Céline, 2026-10-17): in a duress session guardian enrollment shows the ordinary
//! enrollment screen and **waits for a guardian forever** — a code and a QR of the same form, but
//! no offer is armed (no share, no enrollment key derived, nothing registered with the node,
//! nothing recorded). This replaces v2.0.27's stop with a message, which contradicted a
//! running-sync screen. `guardian_enroll_mode(persona)` says which path `open_guardian_enroll`
//! takes; `crate::p2p::decoy_guardian_offer(..)` builds the decoy offer.

use super::{
    crypto_dir, guardian_enroll_mode, recovery_store_for, EnrollMode, ENROLL_NEEDS_LOGIN,
    ENROLL_SYNC_NOT_RUNNING,
};
use crate::p2p::decoy_guardian_offer;
use sovereign_core::auth::PersonaKind as CorePersona;
use sovereign_p2p::guardian_enroll::GuardianEnrollOffer;

#[test]
fn the_primary_persona_reaches_the_recovery_store() {
    let store = recovery_store_for(Some(CorePersona::Primary))
        .expect("the primary persona owns guardian recovery");
    assert_eq!(store.dir(), crypto_dir().as_path());
}

#[test]
fn the_duress_persona_never_reaches_the_recovery_store() {
    assert!(
        recovery_store_for(Some(CorePersona::Duress)).is_none(),
        "a duress session must not read or write the primary's roster, bundle or queue"
    );
}

#[test]
fn the_primary_persona_enrolls_for_real() {
    assert_eq!(guardian_enroll_mode(Some(CorePersona::Primary)), EnrollMode::Real);
    assert_eq!(guardian_enroll_mode(None), EnrollMode::Real);
}

#[test]
fn the_duress_persona_gets_the_decoy_enrollment() {
    assert_eq!(guardian_enroll_mode(Some(CorePersona::Duress)), EnrollMode::Decoy);
}

fn decoy() -> crate::p2p::GuardianOfferOut {
    decoy_guardian_offer(
        "12D3KooWDecoyOwnerPeerIdForTestsOnly000000000000".to_string(),
        "Owner".to_string(),
        vec!["/ip4/192.0.2.10/udp/4001/quic-v1".to_string()],
    )
}

#[test]
fn the_decoy_qr_has_the_form_of_a_real_offer() {
    let out = decoy();
    let offer = GuardianEnrollOffer::decode(&out.qr_payload)
        .expect("the decoy QR decodes as a guardian offer, like a real one");
    assert_eq!(offer.owner_peer_id, "12D3KooWDecoyOwnerPeerIdForTestsOnly000000000000");
}

#[test]
fn the_decoy_code_has_the_form_of_a_real_code() {
    let real = sovereign_crypto::pair_payload::generate_pairing_code();
    let shape = |s: &str| -> Vec<bool> { s.chars().map(|c| c.is_ascii_alphanumeric()).collect() };
    let out = decoy();
    assert_eq!(out.code.len(), real.len(), "{:?} vs {:?}", out.code, real);
    assert_eq!(shape(&out.code), shape(&real), "{:?} vs {:?}", out.code, real);
}

#[test]
fn the_decoy_is_the_first_slot_of_an_empty_roster() {
    // A duress session reads as a persona with no roster yet: its first offer is slot 1 of 5.
    assert_eq!(decoy().slot_ordinal, 1);
}

#[test]
fn each_decoy_offer_is_new_like_a_real_one() {
    let (a, b) = (decoy(), decoy());
    assert_ne!(a.qr_payload, b.qr_payload);
    assert_ne!(a.code, b.code);
}

#[test]
fn the_ordinary_messages_say_nothing_about_personas() {
    for msg in [ENROLL_NEEDS_LOGIN, ENROLL_SYNC_NOT_RUNNING] {
        let lower = msg.to_lowercase();
        assert!(!lower.contains("duress") && !lower.contains("persona") && !lower.contains("decoy"), "{msg:?}");
    }
}
