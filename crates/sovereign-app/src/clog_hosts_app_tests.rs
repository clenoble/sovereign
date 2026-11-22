//! Contract C-LOG, the host lot on the app (spec ch. 08, DG3, DG4 and DG6; the Fable maille, by
//! delegation of Céline, 2026-11-18). Written before the code by its test author, against the
//! layers of `tauri_commands::quarantined_log`: whoever builds the lot makes these pass without
//! editing this file; a test that looks wrong is reported, not worked around.
//!
//! Each test is a launch over a profile of its own (`lot1_onboarding_tests::launch`), with the test
//! account key installed and the session's persona recorded, as a login does. Before it, the
//! persona's log is written under the session's log key and broken, then quarantined as a login
//! quarantines it.
//!
//! The log: three turns. The first holds a word of its own, a direction-changing character and a
//! control character; the second and third hold a marker. The second and third lines are swapped,
//! so the chain breaks at entry 2 of 3: one entry still verifies, two are counted.

use std::path::{Path, PathBuf};

use sovereign_ai::docs_host::testing;
use sovereign_ai::orchestrator::OrchestratorStatePaths;
use sovereign_ai::session_log::{quarantine_unverifiable, SessionLog};
use sovereign_core::auth::PersonaKind;

use crate::lot1_onboarding_tests::{fresh_profile, launch};
use crate::tauri_commands::quarantined_log::{
    delete_quarantined_log_in, quarantined_log_readable_in, ReadableLogDto,
};
use crate::tauri_state::AppState;

fn block<F: std::future::Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

struct Words {
    readable: String,
    broken: String,
}

impl Words {
    fn new(tag: &str) -> Words {
        let n = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        Words { readable: format!("READABLE{tag}{n}"), broken: format!("BROKEN{tag}{n}") }
    }
}

fn log_key() -> [u8; 32] {
    testing::account_key().derive_session_log_key()
}

fn paths(profile: &Path, persona: PersonaKind) -> OrchestratorStatePaths {
    OrchestratorStatePaths::for_persona(profile, persona)
}

/// Three turns under the session's log key, the second and third lines swapped, then quarantined
/// as a login quarantines it.
fn quarantined_log(profile: &Path, persona: PersonaKind, w: &Words) -> PathBuf {
    let p = paths(profile, persona);
    {
        let mut log = SessionLog::open_encrypted(&p.dir, log_key(), Some(&p.beacon)).unwrap();
        log.log_user_input("chat", &format!("{} \u{202E}reversed\u{7}bell", w.readable), "chat");
        log.log_user_input("chat", &format!("{} second", w.broken), "chat");
        log.log_user_input("chat", &format!("{} third", w.broken), "chat");
    }
    let file = p.dir.join("session_log.jsonl");
    let mut lines: Vec<String> =
        std::fs::read_to_string(&file).unwrap().lines().filter(|l| !l.trim().is_empty()).map(String::from).collect();
    assert_eq!(lines.len(), 3, "fixture: three entries");
    lines.swap(1, 2);
    std::fs::write(&file, format!("{}\n", lines.join("\n"))).unwrap();
    quarantine_unverifiable(&p.dir, log_key(), Some(&p.beacon)).expect("the check runs").expect("fixture: quarantined")
}

/// A launch with an open session. The session's persona is kept for the whole process
/// (`AppState::set_session_persona`), so dropping this puts it back to « no session », on panic
/// too: a test that runs after one of these never inherits its persona (WSL found a duress persona
/// left behind, 2026-11-19).
struct Session(AppState);

impl std::ops::Deref for Session {
    type Target = AppState;
    fn deref(&self) -> &AppState {
        &self.0
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.0.set_session_persona(None);
    }
}

/// A launch over `profile` with an open session of `persona`.
fn session(profile: &Path, persona: PersonaKind) -> Session {
    let st = launch(profile);
    block(async { *st.account_key.write().await = Some(testing::account_key()) });
    st.set_session_persona(Some(persona));
    Session(st)
}

fn quarantines(profile: &Path, persona: PersonaKind) -> Vec<PathBuf> {
    let dir = paths(profile, persona).dir;
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("quarantine-"))
                })
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn by_name(root: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.push((p.file_name().unwrap().to_string_lossy().into_owned(), std::fs::read(&p).unwrap_or_default()));
            }
        }
    }
    out.sort();
    out
}

fn readable(st: &AppState) -> ReadableLogDto {
    block(quarantined_log_readable_in(st)).expect("it runs").expect("the persona has a quarantine")
}

// ── DG3 ───────────────────────────────────────────────────────────────────

/// The window's content: the entry before the break, as plain text with visible marks; the two
/// from the break on, counted and not shown; not the older format.
#[test]
fn dg3_the_window_gets_what_still_verifies_and_the_count_of_the_rest() {
    let (_g, profile) = fresh_profile("clog-hosts-readable");
    let w = Words::new("readable");
    quarantined_log(&profile, PersonaKind::Primary, &w);
    let st = session(&profile, PersonaKind::Primary);
    let r = readable(&st);
    assert_eq!(r.entries.len(), 1, "one entry still verifies");
    assert!(r.entries[0].contains(&w.readable), "it is the first: {:?}", r.entries[0]);
    assert!(r.entries[0].contains("[U+202E]") && r.entries[0].contains("[U+0007]"), "with visible marks: {:?}", r.entries[0]);
    assert!(!r.entries[0].contains('\u{202E}') && !r.entries[0].contains('\u{7}'), "no character applied");
    assert_eq!(r.after_the_break, 2, "the breaking entry and the one after it are counted: shown and counted make 3");
    assert!(!r.order_not_guaranteed, "a log in the new format");
    assert!(r.earlier.is_empty(), "no earlier quarantine");
    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains(&w.broken), "nothing from the break on reaches the window");
    for key in ["\"entries\"", "\"after_the_break\"", "\"order_not_guaranteed\"", "\"earlier\""] {
        assert!(json.contains(key), "the frontend reads {key}: {json}");
    }
}

/// No quarantine: nothing to show. No session: nothing either.
#[test]
fn dg3_no_quarantine_or_no_session_shows_nothing() {
    let (_g, profile) = fresh_profile("clog-hosts-none");
    let st = session(&profile, PersonaKind::Primary);
    assert!(matches!(block(quarantined_log_readable_in(&st)), Ok(None)), "no quarantine, nothing to show");

    let w = Words::new("nosession");
    quarantined_log(&profile, PersonaKind::Primary, &w);
    let locked = launch(&profile);
    locked.set_session_persona(None);
    assert!(
        !matches!(block(quarantined_log_readable_in(&locked)), Ok(Some(_))),
        "no open session, nothing is shown"
    );
}

/// DG1 in the window: an earlier quarantine is named, by its number.
#[test]
fn dg3_the_window_names_an_earlier_quarantine() {
    let (_g, profile) = fresh_profile("clog-hosts-earlier");
    quarantined_log(&profile, PersonaKind::Primary, &Words::new("first"));
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let w = Words::new("second");
    quarantined_log(&profile, PersonaKind::Primary, &w);
    let st = session(&profile, PersonaKind::Primary);
    let r = readable(&st);
    assert!(r.entries.iter().any(|e| e.contains(&w.readable)), "the window is of the most recent");
    assert_eq!(r.earlier.len(), 1, "one earlier quarantine: {:?}", r.earlier);
    assert!(r.earlier[0].contains("number 1"), "named by its number: {:?}", r.earlier[0]);
}

// ── DG4 ───────────────────────────────────────────────────────────────────

/// Delete: refused and nothing removed without her yes; with it, the most recent quarantine is
/// gone, the earlier one and the live log stay.
#[test]
fn dg4_delete_removes_nothing_unconfirmed_then_the_most_recent_only() {
    let (_g, profile) = fresh_profile("clog-hosts-delete");
    let earlier = quarantined_log(&profile, PersonaKind::Primary, &Words::new("earlier"));
    let earlier_bytes = by_name(&earlier);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let latest = quarantined_log(&profile, PersonaKind::Primary, &Words::new("latest"));
    let st = session(&profile, PersonaKind::Primary);

    assert!(block(delete_quarantined_log_in(&st, false)).is_err(), "unconfirmed, the deletion is refused");
    assert!(latest.exists() && earlier.exists(), "and removes nothing");

    block(delete_quarantined_log_in(&st, true)).expect("confirmed, it runs");
    assert!(!latest.exists(), "the most recent quarantine is gone");
    assert_eq!(quarantines(&profile, PersonaKind::Primary), vec![earlier.clone()], "the earlier one stays");
    assert_eq!(by_name(&earlier), earlier_bytes, "and keeps every byte");
    let r = readable(&st);
    assert!(r.earlier.is_empty(), "the earlier one is now the one shown: {:?}", r.earlier);
}

// ── DG6 ───────────────────────────────────────────────────────────────────

/// In a duress session the window and Delete reach the duress persona's quarantine only: none of
/// the primary's words is shown, and it keeps every byte.
#[test]
fn dg6_a_duress_session_reaches_its_own_quarantine_only() {
    let (_g, profile) = fresh_profile("clog-hosts-dg6");
    let wp = Words::new("primary");
    let primary_q = quarantined_log(&profile, PersonaKind::Primary, &wp);
    let primary_bytes = by_name(&primary_q);
    let wd = Words::new("duress");
    quarantined_log(&profile, PersonaKind::Duress, &wd);
    let st = session(&profile, PersonaKind::Duress);

    let r = readable(&st);
    let json = serde_json::to_string(&r).unwrap();
    assert!(json.contains(&wd.readable), "the duress persona's own entry");
    assert!(!json.contains(&wp.readable), "nothing of the primary's log");

    block(delete_quarantined_log_in(&st, true)).expect("it runs");
    assert!(quarantines(&profile, PersonaKind::Duress).is_empty(), "the duress quarantine is deleted");
    assert_eq!(quarantines(&profile, PersonaKind::Primary), vec![primary_q.clone()], "the primary's stays");
    assert_eq!(by_name(&primary_q), primary_bytes, "and keeps every byte");
}

/// These tests leave no persona behind: once a session of theirs ends, on panic too, the next test
/// in the process starts with no session's persona (a duress persona left behind made LQ's test
/// run as a duress session, 2026-11-19).
#[test]
fn a_session_of_these_tests_leaves_no_persona_behind() {
    let (_g, profile) = fresh_profile("clog-hosts-no-leak");
    {
        let s = session(&profile, PersonaKind::Duress);
        assert_eq!(s.session_persona(), Some(PersonaKind::Duress), "fixture: a duress session is open");
    }
    assert_eq!(launch(&profile).session_persona(), None, "the persona is put back when the session ends");
}
