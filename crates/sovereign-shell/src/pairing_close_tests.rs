//! Spec ch. 05, *Offer lifetime and limits*, PO1 (v0.0.13; the Fable maille, by delegation of Céline
//! (2026-11-14), on 2026-11-19): closing the desktop's pairing window ends the offer, as the app's
//! closing panel already does. The window closed by « Done », a click outside it, Escape or Enter
//! disarms the offer on the node (`P2pCommand::ClearPairingOffer`); the duress session's decoy arms
//! nothing and disarms nothing. That the node refuses a proof once the offer is disarmed is held by
//! `sovereign-p2p`'s `e2e_pairing` (`a_cleared_offer_refuses_the_right_code`).
//!
//! These tests drive a real, headless `App` on a fake node (`P2pHandle::fake`), which records every
//! command the shell sends it. The clicks go through `App::handle_click` at the drawn rectangles
//! (`panels::pairing_modal_layout`). The keys cannot reach `window_event` from a test, so the window
//! closes through one function, the seam below, and a reading of the shell's source holds that no
//! other place closes it.
//!
//! **Seam, signature by the test author; the body is the implementer's:**
//! - `App::close_pairing(&mut self)`: closes the pairing window by any of its gestures (« Done », a
//!   click outside, Escape, Enter), and ends the offer it showed.
//!
//! **Protected:** this file, and the test added to `sovereign-p2p/tests/e2e_pairing.rs`. Whoever
//! codes the lot does not change them; a test that looks wrong is reported on the channel.

use sovereign_core::auth::PersonaKind;
use sovereign_p2p::pairing::PairingManager;
use sovereign_p2p::P2pCommand;

use super::device_names_tests::{peer_id, unlocked, Unlocked};
use super::App;

const W: f64 = 1280.0;
const H: f64 = 800.0;

struct Paired {
    u: Unlocked,
    rx: tokio::sync::mpsc::Receiver<P2pCommand>,
    _dir: tempfile::TempDir,
}

impl Paired {
    fn app(&mut self) -> &mut App {
        &mut self.u.app
    }
    /// The commands the shell sent the node since the last call.
    fn sent(&mut self) -> Vec<P2pCommand> {
        let mut v = Vec::new();
        while let Ok(c) = self.rx.try_recv() {
            v.push(c);
        }
        v
    }
    fn cleared(&mut self) -> usize {
        self.sent().into_iter().filter(|c| matches!(c, P2pCommand::ClearPairingOffer)).count()
    }
}

impl Drop for Paired {
    fn drop(&mut self) {
        super::first_run_tests::shut_runtime(&mut self.u.app);
    }
}

/// An unlocked primary session on a fake node, its pairing window open on an armed offer.
fn offer_shown(tag: &str) -> Paired {
    let mut u = unlocked(tag);
    u.app.test_view = Some((W, H));
    let dir = tempfile::tempdir().unwrap();
    let pm = PairingManager::new(dir.path().join("paired_devices.json"));
    let (h, rx) = crate::p2p::P2pHandle::fake(&peer_id(), "Desk", pm, [9u8; 32]);
    u.app.p2p = Some(h);
    // A pairing offer releases the MasterKey salt: the profile needs one.
    std::fs::create_dir_all(crate::crypto::crypto_dir()).unwrap();
    std::fs::write(crate::crypto::crypto_dir().join("salt"), [7u8; 32]).unwrap();
    let mut p = Paired { u, rx, _dir: dir };
    p.app().open_pairing();
    assert!(
        p.app().pairing_modal.is_some(),
        "precondition: the pairing window is open (notice: {:?})",
        p.app().notice.as_ref().map(|n| n.said.clone())
    );
    let armed = p.sent().into_iter().filter(|c| matches!(c, P2pCommand::SetPairingOffer { .. })).count();
    assert_eq!(armed, 1, "precondition: the node was armed with the offer");
    p.app().build_scene(W, H);
    p
}

fn click(app: &mut App, x: f64, y: f64) {
    app.handle_click(x, y);
    app.build_scene(W, H);
}

#[test]
fn po1_done_closes_the_window_and_ends_the_offer() {
    let mut p = offer_shown("po1-done");
    let (_card, _qr, _copy, done) = crate::panels::pairing_modal_layout(W, H);
    let c = done.center();
    click(p.app(), c.x, c.y);
    assert!(p.app().pairing_modal.is_none(), "« Done » closes the window");
    assert_eq!(p.cleared(), 1, "and the node is told to disarm the offer, once");
}

#[test]
fn po1_a_click_outside_closes_the_window_and_ends_the_offer() {
    let mut p = offer_shown("po1-outside");
    let (card, _qr, _copy, _done) = crate::panels::pairing_modal_layout(W, H);
    let (x, y) = (card.x0 - 20.0, card.y0 + 20.0);
    assert!(x > 0.0 && !card.contains(vello::kurbo::Point::new(x, y)), "precondition: the point is outside the card");
    click(p.app(), x, y);
    assert!(p.app().pairing_modal.is_none(), "a click outside closes the window");
    assert_eq!(p.cleared(), 1, "and the node is told to disarm the offer, once");
}

#[test]
fn po1_the_closing_gesture_of_the_keys_ends_the_offer() {
    let mut p = offer_shown("po1-keys");
    p.app().close_pairing();
    assert!(p.app().pairing_modal.is_none(), "the window closes");
    assert_eq!(p.cleared(), 1, "and the node is told to disarm the offer, once");
}

#[test]
fn po1_a_click_inside_the_card_keeps_the_window_and_the_offer() {
    let mut p = offer_shown("po1-inside");
    let (card, qr, _copy, _done) = crate::panels::pairing_modal_layout(W, H);
    // On the QR: inside the card, on no button.
    let c = qr.center();
    assert!(card.contains(c), "precondition: the QR is inside the card");
    click(p.app(), c.x, c.y);
    assert!(p.app().pairing_modal.is_some(), "a click inside the card leaves the window open");
    assert_eq!(p.cleared(), 0, "and the offer armed");
}

#[test]
fn po1_the_duress_decoy_arms_nothing_and_its_closing_disarms_nothing() {
    let mut u = unlocked("po1-duress");
    u.app.test_view = Some((W, H));
    u.app.persona = Some(PersonaKind::Duress);
    u.app.p2p = None; // no node runs in the duress session (C-AREA AR3)
    u.app.open_pairing();
    assert!(u.app.pairing_modal.is_some(), "precondition: the decoy window is shown");
    u.app.close_pairing();
    assert!(u.app.pairing_modal.is_none(), "it closes");
    assert!(u.app.p2p.is_none(), "and no node was started to disarm anything");
    super::first_run_tests::shut_runtime(&mut u.app);
}

/// Every place in the shell's source that closes the pairing window goes through `close_pairing`:
/// the keys (Escape, Enter) included, which a test cannot press through `window_event`.
#[test]
fn po1_nothing_else_in_the_shell_closes_the_pairing_window() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    let mut dirs = vec![src];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let path = e.path();
            if path.is_dir() {
                dirs.push(path);
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !name.ends_with(".rs") || name.ends_with("_tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).unwrap();
            let mut in_close = false;
            for (i, line) in text.lines().enumerate() {
                let l = line.trim_start();
                if l.starts_with("//") {
                    continue;
                }
                if l.contains("fn close_pairing(") {
                    in_close = true;
                } else if in_close && (l.starts_with("fn ") || l.starts_with("pub(crate) fn ") || l.starts_with("pub fn ")) {
                    in_close = false;
                }
                let closes = l.contains("pairing_modal = None") || l.contains("pairing_modal.take()");
                if closes && !in_close {
                    offenders.push(format!("{name}:{}: {}", i + 1, l));
                }
            }
        }
    }
    assert!(offenders.is_empty(), "the pairing window is closed elsewhere than in close_pairing:\n{}", offenders.join("\n"));
}
