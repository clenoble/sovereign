//! Typing timings are optional input to login and onboarding (spec ch. 07, *Keystroke
//! dynamics*, 2026-10-17): a missing, partial or malformed sample is ignored and never makes
//! the command's arguments fail to decode. In v0.0.10 a password filled in by Android's
//! autofill sent samples without `key`, and the whole `validate_password` call was refused
//! ("invalid args `keystrokes` … missing field `key`") before the password was checked
//! (cloud review, finding 20).
//!
//! These tests decode the argument types exactly as the commands declare them
//! (`validate_password(keystrokes: Vec<KeystrokeSampleDto>)`, onboarding
//! `keystrokes: Vec<Vec<KeystrokeSampleDto>>`).

use super::KeystrokeSampleDto;
use serde_json::{json, Value};

fn decode_login(v: Value) -> Result<Vec<KeystrokeSampleDto>, serde_json::Error> {
    serde_json::from_value(v)
}

#[test]
fn a_sample_without_a_key_decodes() {
    // What JSON.stringify produces from `{ key: undefined, press_ms, release_ms }`.
    decode_login(json!([{ "press_ms": 1, "release_ms": 2 }]))
        .expect("a key-less sample must not refuse the login");
}

#[test]
fn a_sample_without_timings_decodes() {
    decode_login(json!([{ "key": "a" }])).expect("a partial sample must not refuse the login");
}

#[test]
fn malformed_samples_do_not_fail_the_list() {
    decode_login(json!([
        { "key": "a", "press_ms": 10, "release_ms": 25 },
        { "key": 7, "press_ms": "x" },
        null,
        "junk",
        []
    ]))
    .expect("malformed samples must be ignored, not refuse the login");
}

#[test]
fn a_well_formed_sample_keeps_its_values_next_to_malformed_ones() {
    let v = decode_login(json!([
        { "press_ms": 1 },
        { "key": "a", "press_ms": 10, "release_ms": 25 },
        null
    ]))
    .expect("decodes");
    let a = v
        .iter()
        .find(|s| s.key == "a")
        .expect("the well-formed sample is still there");
    assert_eq!((a.press_ms, a.release_ms), (10, 25));
}

#[test]
fn an_empty_list_decodes() {
    assert!(decode_login(json!([])).expect("decodes").is_empty());
}

#[test]
fn onboarding_samples_decode_the_same_way() {
    serde_json::from_value::<Vec<Vec<KeystrokeSampleDto>>>(json!([
        [{ "press_ms": 1 }],
        [null, { "key": "b", "press_ms": 3, "release_ms": 4 }],
        []
    ]))
    .expect("onboarding must not be refused for its typing samples either");
}
