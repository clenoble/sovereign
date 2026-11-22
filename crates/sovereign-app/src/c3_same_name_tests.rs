//! Pairing a device again, on the app's source side (spec ch. 05, PR1 and PR2;
//! v0.0.13, item C3). The app is a source too: it shows the offer
//! (`generate_pair_qr`) and persists the joining device when the node reports
//! the pairing.
//!
//! Written before the code, by another author; the implementer does not edit
//! this file. The `AppState` is the one `lot1_onboarding_tests::launch` builds
//! over a fresh profile (one test at a time, `SOVEREIGN_DATA_DIR` being process
//! wide), with an account key, a p2p identity key and a paired list. A pairing
//! completes as a started node's does: the node's event goes through this
//! host's own event translator, which persists the new record, then reports it
//! (`DevicePaired`, the `device-paired` event of the screens). A device set up
//! again comes back with a new peer id, under its old name.
//!
//! PR1's « a test first »: `pr1_today_a_device_paired_again_leaves_its_old_record`
//! holds what is true before the code and after it, when she does not answer:
//! both records stay.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sovereign_core::interfaces::OrchestratorEvent;
use sovereign_crypto::account_key::AccountKey;
use sovereign_p2p::pairing::{derive_store_key, PairedDevice, PairingManager};
use sovereign_p2p::{P2pCommand, P2pEvent};

use crate::lot1_onboarding_tests::{fresh_profile, launch};
use crate::tauri_commands::pairing::{answer_same_name_in, list_paired_devices_in, same_name_question_in};
use crate::tauri_state::AppState;

const LOCAL: &str = "12D3KooWThisPhone";

struct Source {
    st: AppState,
    events: tokio::sync::mpsc::Sender<P2pEvent>,
    commands: tokio::sync::mpsc::Receiver<P2pCommand>,
    reported: std::sync::mpsc::Receiver<OrchestratorEvent>,
    store: PathBuf,
    store_key: [u8; 32],
    _guard: std::sync::MutexGuard<'static, ()>,
}

fn block<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

/// An unlocked app whose paired list holds `paired` (peer id, name), with its
/// event translator running.
fn source(tag: &str, paired: &[(&str, &str)]) -> Source {
    let (guard, dir) = fresh_profile(tag);
    let mut st = launch(&dir);
    let (orch_tx, reported) = std::sync::mpsc::channel();
    st.orch_tx = orch_tx;

    let account = Arc::new(AccountKey::from_bytes([7u8; 32]));
    let master = sovereign_crypto::master_key::MasterKey::from_bytes([3u8; 32]);
    let identity = Arc::new(sovereign_crypto::device_key::DeviceKey::derive(&master, "device-c3").unwrap());
    let store_key = derive_store_key(&identity);
    let store = dir.join("paired_devices.json");
    let mut pm = PairingManager::new(store.clone());
    for (peer, name) in paired {
        pm.add_device(PairedDevice::with_key((*peer).into(), (*name).into(), account.derive_pair_key(LOCAL, peer)));
    }
    pm.save(&store_key).unwrap();

    let (command_tx, commands) = tokio::sync::mpsc::channel(64);
    let events = block(async {
        *st.account_key.write().await = Some(account.clone());
        st.set_p2p_identity_key(identity.clone()).await;
        *st.pairing_manager.write().await = Some(pm);
        st.set_p2p_command_tx(command_tx.clone()).await;
        crate::sync_startup::translate_events_for_test(&st, LOCAL, command_tx).await
    });
    Source { st, events, commands, reported, store, store_key, _guard: guard }
}

impl Source {
    /// The node says a device joined: `peer` under `name`. Waits until the
    /// pairing is reported to the screens.
    fn joins(&mut self, peer: &str, name: &str) {
        let ev = P2pEvent::PairingCompleted { peer_id: peer.into(), device_name: name.into() };
        block(self.events.send(ev)).expect("the translator takes the event");
        loop {
            match self.reported.recv_timeout(Duration::from_secs(10)) {
                Ok(OrchestratorEvent::DevicePaired { device_id, .. }) if device_id == peer => return,
                Ok(_) => continue,
                Err(_) => panic!("the pairing of {peer} ({name}) is never reported"),
            }
        }
    }

    fn answer(&mut self, replace: Option<&str>) -> Result<(), String> {
        block(answer_same_name_in(&self.st, replace.map(str::to_string)))
    }

    /// The paired list as the app's store holds it now: (peer id, name).
    fn listed(&self) -> Vec<(String, String)> {
        let mut v: Vec<(String, String)> = block(async {
            self.st
                .pairing_manager
                .read()
                .await
                .as_ref()
                .unwrap()
                .list_devices()
                .iter()
                .map(|d| (d.peer_id.clone(), d.device_name.clone()))
                .collect()
        });
        v.sort();
        v
    }

    /// The paired list as saved on disk.
    fn saved(&self) -> Vec<String> {
        saved_peers(&self.store, &self.store_key)
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

fn saved_peers(store: &Path, key: &[u8; 32]) -> Vec<String> {
    sorted(PairingManager::load(store, key).unwrap().list_devices().iter().map(|d| d.peer_id.clone()).collect())
}

fn pair(peer: &str, name: &str) -> (String, String) {
    (peer.to_string(), name.to_string())
}

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// PR1, the test first: with no answer, the record of the device set up again
/// stays beside the new one. True today, and after the code (« no answer
/// keeps both »).
#[test]
fn pr1_today_a_device_paired_again_leaves_its_old_record() {
    let mut s = source("c3-pr1-first", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWNewPixel", "Pixel");
    assert_eq!(
        s.listed(),
        vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWNewPixel", "Pixel"), pair("12D3KooWOldPixel", "Pixel")],
        "no answer keeps both records"
    );
    assert_eq!(s.saved(), ids(&["12D3KooWLaptop", "12D3KooWNewPixel", "12D3KooWOldPixel"]));
}

/// PR2: a device that joins under a name a paired device holds raises the
/// question, held by the time the pairing is reported (the screens ask for it
/// on `device-paired`), naming the device that joined and the one of that name.
#[test]
fn pr2_joining_under_a_held_name_asks_by_the_time_the_pairing_is_reported() {
    let mut s = source("c3-pr2-asks", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWNewPixel", "Pixel");
    let q = same_name_question_in(&s.st).expect("PR2: the source side asks");
    assert_eq!((q.new_peer_id.as_str(), q.new_device_name.as_str()), ("12D3KooWNewPixel", "Pixel"));
    let candidates: Vec<(String, String)> = q.candidates.iter().map(|d| (d.peer_id.clone(), d.device_name.clone())).collect();
    assert_eq!(candidates, vec![pair("12D3KooWOldPixel", "Pixel")], "the device that just joined is not asked about");
    assert!(q.candidates.iter().all(|d| !d.paired_at.is_empty()), "each is listed with its pairing date");
}

/// PR2: a new name asks nothing.
#[test]
fn pr2_a_new_name_asks_nothing() {
    let mut s = source("c3-pr2-new-name", &[("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWTablet", "Tablet");
    assert!(same_name_question_in(&s.st).is_none(), "no paired device holds that name");
    assert_eq!(s.listed(), vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWTablet", "Tablet")]);
}

/// PR1 and PR2, « Replace it »: one record for the device, the new one. The
/// old peer id leaves the store, the saved file, the allow-list and the
/// sealing map pushed to the node, as at unpairing; the device list shows it
/// once.
#[test]
fn pr1_pr2_replace_it_leaves_one_record_and_drops_the_old_key() {
    let mut s = source("c3-pr2-replace", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWNewPixel", "Pixel");
    let _ = s.last_pushed();
    s.answer(Some("12D3KooWOldPixel")).expect("« Replace it » is taken");

    assert_eq!(s.listed(), vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWNewPixel", "Pixel")], "PR1: one record, the new one");
    assert_eq!(s.saved(), ids(&["12D3KooWLaptop", "12D3KooWNewPixel"]), "the saved list too");
    let (peers, keys) = s.last_pushed();
    let expected = Some(ids(&["12D3KooWLaptop", "12D3KooWNewPixel"]));
    assert_eq!(peers, expected, "the old peer id leaves the allow-list");
    assert_eq!(keys, expected, "and its pair key the sealing map");
    let shown = block(list_paired_devices_in(&s.st)).unwrap();
    assert_eq!(shown.iter().filter(|d| d.device_name == "Pixel").count(), 1, "the device list shows it once");
    assert!(same_name_question_in(&s.st).is_none(), "the question is answered");
}

/// PR2, « Keep both »: every record stays, and nothing is pushed that drops one.
#[test]
fn pr2_keep_both_keeps_both() {
    let mut s = source("c3-pr2-keep", &[("12D3KooWOldPixel", "Pixel")]);
    s.joins("12D3KooWNewPixel", "Pixel");
    assert!(same_name_question_in(&s.st).is_some(), "PR2: the source side asks");
    let _ = s.last_pushed();
    s.answer(None).expect("« Keep both » is taken");
    assert_eq!(s.listed(), vec![pair("12D3KooWNewPixel", "Pixel"), pair("12D3KooWOldPixel", "Pixel")]);
    assert_eq!(s.saved(), ids(&["12D3KooWNewPixel", "12D3KooWOldPixel"]));
    let (peers, _) = s.last_pushed();
    if let Some(peers) = peers {
        assert!(peers.contains(&"12D3KooWOldPixel".to_string()), "nothing drops the old record: {peers:?}");
    }
    assert!(same_name_question_in(&s.st).is_none(), "the question is answered");
}

/// PR2, several paired devices of that name: the question lists them all, and
/// « Replace it » on one removes that one only.
#[test]
fn pr2_several_of_that_name_are_listed_and_one_is_picked() {
    let mut s = source("c3-pr2-several", &[("12D3KooWPixelA", "Pixel"), ("12D3KooWPixelB", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWPixelNew", "Pixel");
    let q = same_name_question_in(&s.st).expect("PR2: the source side asks");
    assert_eq!(sorted(q.candidates.iter().map(|d| d.peer_id.clone()).collect()), ids(&["12D3KooWPixelA", "12D3KooWPixelB"]));
    s.answer(Some("12D3KooWPixelB")).expect("« Replace it » on one is taken");
    assert_eq!(
        s.listed(),
        vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWPixelA", "Pixel"), pair("12D3KooWPixelNew", "Pixel")]
    );
}

/// « Nothing is replaced without her answer »: an answer naming a paired
/// device the question did not ask about removes nothing.
#[test]
fn pr2_an_answer_naming_a_device_not_asked_about_removes_nothing() {
    let mut s = source("c3-pr2-other", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    s.joins("12D3KooWNewPixel", "Pixel");
    let _ = s.answer(Some("12D3KooWLaptop"));
    let _ = s.answer(Some("12D3KooWNewPixel"));
    assert_eq!(
        s.listed(),
        vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWNewPixel", "Pixel"), pair("12D3KooWOldPixel", "Pixel")],
        "neither the other device nor the one that just joined is removed"
    );
    assert_eq!(s.saved(), ids(&["12D3KooWLaptop", "12D3KooWNewPixel", "12D3KooWOldPixel"]));
}

/// With no question held, an answer removes nothing (the command is reachable
/// from the screens at any time).
#[test]
fn pr2_an_answer_with_no_question_removes_nothing() {
    let mut s = source("c3-pr2-none", &[("12D3KooWOldPixel", "Pixel"), ("12D3KooWLaptop", "Laptop")]);
    let _ = s.answer(Some("12D3KooWOldPixel"));
    assert_eq!(s.listed(), vec![pair("12D3KooWLaptop", "Laptop"), pair("12D3KooWOldPixel", "Pixel")]);
    assert_eq!(s.saved(), ids(&["12D3KooWLaptop", "12D3KooWOldPixel"]));
}
