//! PR2 (spec ch. 05; v0.0.13, item C3): which paired devices the source side
//! asks about when a device joins under a name one of them already has.

use super::*;

fn store(devices: &[(&str, &str)]) -> PairingManager {
    let dir = std::env::temp_dir().join(format!("sov-same-name-{}-{}", std::process::id(), devices.len()));
    let mut pm = PairingManager::new(dir.join("paired.json"));
    for (i, (peer, name)) in devices.iter().enumerate() {
        pm.add_device(PairedDevice::with_key((*peer).into(), (*name).into(), [i as u8 + 1; 32]));
    }
    pm
}

fn peers(found: Vec<&PairedDevice>) -> Vec<String> {
    let mut v: Vec<String> = found.into_iter().map(|d| d.peer_id.clone()).collect();
    v.sort();
    v
}

/// The paired device of that name is asked about; the device that just joined
/// (recorded already) is not.
#[test]
fn pr2_the_device_of_that_name_is_the_one_asked_about() {
    let pm = store(&[("peer-old", "Pixel"), ("peer-new", "Pixel"), ("peer-laptop", "Laptop")]);
    assert_eq!(peers(pm.same_name("Pixel", "peer-new")), vec!["peer-old".to_string()]);
}

/// A name no paired device holds asks nothing.
#[test]
fn pr2_a_new_name_asks_nothing() {
    let pm = store(&[("peer-laptop", "Laptop"), ("peer-new", "Tablet")]);
    assert!(pm.same_name("Tablet", "peer-new").is_empty());
}

/// Several paired devices of that name: all of them are listed.
#[test]
fn pr2_several_of_that_name_are_all_listed() {
    let pm = store(&[("peer-a", "Pixel"), ("peer-b", "Pixel"), ("peer-new", "Pixel"), ("peer-c", "Laptop")]);
    assert_eq!(peers(pm.same_name("Pixel", "peer-new")), vec!["peer-a".to_string(), "peer-b".to_string()]);
}
