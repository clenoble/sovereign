//! Contract C-LOG, the host lot on the app, second set (spec ch. 08, DG4; Céline, 2026-11-18:
//! « Mettre l'export du téléphone au backlog »). Written before the code by its test author: the
//! implementer does not edit this file.
//!
//! The app carries out Keep and Delete, not Export, so the assistant it builds is offered those
//! two only (`Orchestrator::set_log_choices`, named by the implementer, given
//! `tauri_commands::quarantined_log::choices()`). The orchestrator the app builds at startup needs
//! a model, which a test does not have: the list is held, then where `init_backend` gives it, then
//! what an orchestrator given it offers the model.

use std::sync::Arc;

use sovereign_ai::docs_host::testing;
use sovereign_ai::log_diagnosis::{self as dg, Choice};
use sovereign_ai::orchestrator::OrchestratorStatePaths;
use sovereign_ai::session_log::SessionLog;
use sovereign_core::auth::PersonaKind;

use crate::lot1_onboarding_tests::fresh_profile;
use crate::tauri_commands::quarantined_log::choices;

/// The app's list: Keep, then Delete.
#[test]
fn the_apps_choices_are_keep_and_delete() {
    assert_eq!(choices(), vec![Choice::Keep, Choice::Delete]);
}

/// `init_backend` gives that list to the orchestrator it builds. Read in the source: building it
/// needs a model.
#[test]
fn the_app_gives_its_list_where_it_builds_its_orchestrator() {
    let source = include_str!("lib.rs");
    let start = source.find("async fn init_backend(").expect("init_backend");
    let built = source[start..].find("sovereign_ai::Orchestrator::new(").expect("init_backend builds the orchestrator");
    let after = &source[start + built..];
    let end = after.find("\n}\n").unwrap_or(after.len());
    let body = &after[..end];
    assert!(
        body.contains("set_log_choices(") && body.contains("quarantined_log::choices()"),
        "init_backend gives the orchestrator it builds `quarantined_log::choices()` through `set_log_choices`"
    );
}

/// An orchestrator given the app's list, over a quarantined log of the primary's: Keep and Delete
/// are offered, Keep the default, and what the model receives offers no export.
#[test]
fn an_orchestrator_given_the_apps_list_offers_no_export() {
    let (_g, profile) = fresh_profile("clog-hosts-v2-choices");
    let db = tauri::async_runtime::block_on(testing::encrypted_memory_store("clog-hosts-v2"));
    let (tx, _rx) = std::sync::mpsc::channel();
    let paths = OrchestratorStatePaths::for_persona(&profile, PersonaKind::Primary);
    let orch = sovereign_ai::Orchestrator::for_host_tests(db as Arc<dyn sovereign_db::GraphDB>, tx, paths.dir.clone());
    orch.set_log_choices(choices());

    let key = testing::account_key().derive_session_log_key();
    {
        let mut log = SessionLog::open_encrypted(&paths.dir, key, Some(&paths.beacon)).unwrap();
        log.log_user_input("chat", "a turn", "chat");
    }
    {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().append(true).open(paths.dir.join("session_log.jsonl")).unwrap();
        f.write_all(b"not a sealed line\n").unwrap();
    }
    orch.set_session_log_key_for(&paths, key);
    let d = orch.log_diagnosis().expect("fixture: the log is quarantined at the login");

    assert_eq!(d.choices, vec![Choice::Keep, Choice::Delete], "the app offers what it carries out");
    assert_eq!(d.default_choice, Choice::Keep, "Keep stays the default");
    let text = d.for_the_model();
    assert!(
        !text.contains(&dg::choice_sentence(Choice::ExportForAnExpert)),
        "what the model receives offers no export:\n{text}"
    );
    for c in [Choice::Keep, Choice::Delete] {
        assert!(text.contains(&dg::choice_sentence(c)), "{c:?} is offered:\n{text}");
    }
}

/// The event that tells the frontend a turn's diagnosis tool found a quarantine reaches it under
/// the name the frontend listens to, `quarantined-log-diagnosed` (held from the other side by
/// `events.quarantinedLog.test.ts`). Read in the source: the event loop runs only inside a Tauri
/// app.
#[test]
fn the_event_reaches_the_frontend_under_its_name() {
    let source = include_str!("tauri_events.rs");
    let at = source.find("OrchestratorEvent::QuarantinedLogDiagnosed").expect("the event loop handles the event");
    let arm = &source[at..source.len().min(at + 400)];
    assert!(arm.contains("\"quarantined-log-diagnosed\""), "it is emitted as « quarantined-log-diagnosed »:\n{arm}");
}
