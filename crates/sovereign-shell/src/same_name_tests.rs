//! Pairing a device again, on the desktop (spec ch. 05, PR1 and PR2; v0.0.13,
//! item C3).
//!
//! A pairing completes as a started node's does: the node's event goes
//! through this host's event translator (which persists the new record), then
//! to the app. A device set up again comes back with a new peer id, under its
//! old name.
//!
//! PR1's « a test first »: `pr1_today_a_device_paired_again_leaves_its_old_record`
//! holds what is true before the code and after it, when she does not answer:
//! both records stay.

use std::path::{Path, PathBuf};
use std::time::Duration;

use sovereign_p2p::pairing::{PairedDevice, PairingManager};
use sovereign_p2p::{P2pCommand, P2pEvent};

use super::device_names_tests::{unlocked, Unlocked};
use super::first_run_tests::{frames_until, is_button, read_tree};
use crate::same_name::SameNameAnswer;

const STORE_KEY: [u8; 32] = [11u8; 32];

struct Paired {
    u: Unlocked,
    commands: tokio::sync::mpsc::Receiver<P2pCommand>,
    events: tokio::sync::mpsc::Sender<P2pEvent>,
    store: PathBuf,
    _dir: tempfile::TempDir,
}

/// An unlocked desktop whose paired list holds `paired` (peer id, name).
fn paired(tag: &str, paired: &[(&str, &str)]) -> Paired {
    let mut u = unlocked(tag);
    let dir = tempfile::tempdir().unwrap();
    let store = dir.path().join("paired_devices.json");
    let mut pm = PairingManager::new(store.clone());
    for (i, (peer, name)) in paired.iter().enumerate() {
        pm.add_device(PairedDevice::with_key((*peer).into(), (*name).into(), [i as u8 + 1; 32]));
    }
    pm.save(&STORE_KEY).unwrap();
    let (h, commands) = crate::p2p::P2pHandle::fake("12D3KooWThisDesktop", "This desktop", pm, STORE_KEY);
    let events = h.translate_events_for_test(&u.app.rt, u.app.orch_tx.clone());
    u.app.p2p = Some(h);
    Paired { u, commands, events, store, _dir: dir }
}

impl Paired {
    /// The node says a device joined: `peer` under `name`. Waits until the app
    /// has the pairing (its notice, or the question).
    fn joins(&mut self, peer: &str, name: &str) {
        let ev = P2pEvent::PairingCompleted { peer_id: peer.into(), device_name: name.into() };
        self.u.app.rt.block_on(self.events.send(ev)).expect("the translator takes the event");
        let said = format!("Paired with {name}");
        let seen = frames_until(&mut self.u.app, 10, |app| {
            app.same_name_question().is_some()
                || app.notice.as_ref().is_some_and(|n| n.said.1.contains(&said) || n.said.0.contains(&said))
        });
        assert!(seen, "the pairing of {peer} ({name}) reaches the app");
    }

    /// The paired list as the app's store holds it now: peer id → name.
    fn listed(&self) -> Vec<(String, String)> {
        let h = self.u.app.p2p.as_ref().unwrap();
        let mut v: Vec<(String, String)> = self
            .u
            .app
            .rt
            .block_on(async { h.pairing_manager.read().await.list_devices().iter().map(|d| (d.peer_id.clone(), d.device_name.clone())).collect() });
        v.sort();
        v
    }

    /// The paired list as saved on disk.
    fn saved(&self) -> Vec<String> {
        saved_peers(&self.store)
    }

    /// The last allow-list and key map pushed to the node, if any.
    fn last_pushed(&mut self) -> (Option<Vec<String>>, Option<Vec<String>>) {
        let (mut peers, mut keys) = (None, None);
        while let Ok(c) = self.commands.try_recv() {
            match c {
                P2pCommand::UpdatePairedPeers { peer_ids } => peers = Some(sorted(peer_ids)),
                P2pCommand::UpdatePairKeys { keys: k } => keys = Some(sorted(k.0.into_keys().collect())),
                _ => {}
            }
        }
        (peers, keys)
    }
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

fn saved_peers(store: &Path) -> Vec<String> {
    sorted(PairingManager::load(store, &STORE_KEY).unwrap().list_devices().iter().map(|d| d.peer_id.clone()).collect())
}

fn pair(peer: &str, name: &str) -> (String, String) {
    (peer.to_string(), name.to_string())
}

/// PR1, the test first: with no answer, the record of the device set up again
/// stays beside the new one. True today, and after the code (« no answer
/// keeps both »).
#[test]
fn pr1_today_a_device_paired_again_leaves_its_old_record() {
    let mut p = paired("pr1-first", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    p.joins("12D3KooWNewPixel", "Pixel");
    if p.u.app.same_name_question().is_some() {
        p.u.app.dismiss_same_name();
    }
    assert_eq!(
        p.listed(),
        vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWNewPixel", "Pixel"), pair("12D3KooWOldPixel", "Pixel")],
        "no answer keeps both records"
    );
}

/// PR2: a device that joins under a name a paired device holds raises the
/// question, naming that device, with « Replace it » and « Keep both ».
#[test]
fn pr2_joining_under_a_held_name_asks_which() {
    let mut p = paired("pr2-asks", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    p.joins("12D3KooWNewPixel", "Pixel");
    let q = p.u.app.same_name_question().expect("PR2: the source side asks");
    assert_eq!(q.new_peer(), "12D3KooWNewPixel");
    assert_eq!(q.candidates(), vec![pair("12D3KooWOldPixel", "Pixel")]);

    let tree = read_tree(&mut p.u.app);
    let buttons: Vec<String> = tree.all.iter().filter(|s| is_button(s.role)).map(|s| s.spoken.clone()).collect();
    for label in ["Replace it", "Keep both"] {
        assert!(buttons.iter().any(|b| b.contains(label)), "PR2: « {label} » is offered, and spoken: {buttons:?}");
    }
}

/// PR2: a new name asks nothing.
#[test]
fn pr2_a_new_name_asks_nothing() {
    let mut p = paired("pr2-new-name", &[("12D3KooWLaptop", "Laptop")]);
    p.joins("12D3KooWTablet", "Tablet");
    assert!(p.u.app.same_name_question().is_none(), "no paired device holds that name");
    assert_eq!(p.listed(), vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWTablet", "Tablet")]);
}

/// PR1 and PR2, « Replace it »: one record for the device, the new one. The
/// old peer id leaves the store, the saved file, the allow-list and the
/// sealing map pushed to the node, as at unpairing.
#[test]
fn pr1_pr2_replace_it_leaves_one_record_and_drops_the_old_key() {
    let mut p = paired("pr2-replace", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    p.joins("12D3KooWNewPixel", "Pixel");
    let _ = p.last_pushed();
    p.u.app.answer_same_name(SameNameAnswer::Replace("12D3KooWOldPixel".into()));
    std::thread::sleep(Duration::from_millis(50));

    let want = vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWNewPixel", "Pixel")];
    assert_eq!(p.listed(), want, "PR1: one record for the device, the new one");
    assert_eq!(p.saved(), vec!["12D3KooWLaptop".to_string(), "12D3KooWNewPixel".to_string()], "the saved list too");
    let (peers, keys) = p.last_pushed();
    let expected = Some(vec!["12D3KooWLaptop".to_string(), "12D3KooWNewPixel".to_string()]);
    assert_eq!(peers, expected, "the old peer id leaves the allow-list");
    assert_eq!(keys, expected, "and its pair key the sealing map");
    assert!(p.u.app.same_name_question().is_none(), "the question is answered");
}

/// PR2, « Keep both »: every record stays, and nothing is pushed that drops one.
#[test]
fn pr2_keep_both_keeps_both() {
    let mut p = paired("pr2-keep", &[("12D3KooWOldPixel", "Pixel")]);
    p.joins("12D3KooWNewPixel", "Pixel");
    assert!(p.u.app.same_name_question().is_some(), "PR2: the source side asks");
    let _ = p.last_pushed();
    p.u.app.answer_same_name(SameNameAnswer::KeepBoth);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(p.listed(), vec![pair("12D3KooWNewPixel", "Pixel"), pair("12D3KooWOldPixel", "Pixel")]);
    let (peers, _) = p.last_pushed();
    if let Some(peers) = peers {
        assert!(peers.contains(&"12D3KooWOldPixel".to_string()), "nothing drops the old record: {peers:?}");
    }
    assert!(p.u.app.same_name_question().is_none(), "the question is answered");
}

/// PR2, several paired devices of that name: the question lists them all, and
/// « Replace it » on one removes that one only.
#[test]
fn pr2_several_of_that_name_are_listed_and_one_is_picked() {
    let mut p = paired("pr2-several", &[("12D3KooWPixelA", "Pixel"), ("12D3KooWPixelB", "Pixel")]);
    p.joins("12D3KooWPixelNew", "Pixel");
    let q = p.u.app.same_name_question().expect("PR2: the source side asks");
    assert_eq!(sorted_pairs(q.candidates()), vec![pair("12D3KooWPixelA", "Pixel"), pair("12D3KooWPixelB", "Pixel")]);
    p.u.app.answer_same_name(SameNameAnswer::Replace("12D3KooWPixelB".into()));
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(p.listed(), vec![pair("12D3KooWPixelA", "Pixel"), pair("12D3KooWPixelNew", "Pixel")]);
}

fn sorted_pairs(mut v: Vec<(String, String)>) -> Vec<(String, String)> {
    v.sort();
    v
}
