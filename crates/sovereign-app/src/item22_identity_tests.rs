//! v0.0.12 item 22 in the app (the phone, the Tauri desktop, the CLI) — a
//! device's identity in its stamps is its libp2p peer id, not its display
//! name (D1, D6).
//!
//! Contract: `doc/plans/V0012-item22-device-identity.md` (finding: claude-coord
//! `from-windows/0142-device-identity-finding.md`). Written before the code,
//! by another author: whoever implements item 22 makes these pass **without
//! editing this file**; a test that looks wrong is reported, not worked around.
//!
//! The commands run for real, as in `lot1_onboarding_tests` (whose harness —
//! `launch`, `state`, `fresh_profile`, `data` — this file shares, under the
//! same `ENV_LOCK`): `complete_onboarding` and `validate_password` over an
//! `AppState` with an in-memory store, P2P off in the configuration. The
//! identity is read as the installed document service writes it: a new
//! document's stamp `device`, `author_device`, state `device`; the decoy's
//! thread stamps for the duress persona.
//!
//! "The persona's peer id" is
//! `sovereign_p2p::identity::derive_keypair(device_key)` for the device key
//! the login installed (`AppState::p2p_identity_key`), as a string.
//!
//! The behaviour tests need the `p2p` feature (the phone's build has it,
//! `mobile = [.., "p2p"]`); run them with
//! `cargo test -p sovereign-app --features p2p item22`. The source scans run
//! in every build with encryption.
//!
//! CONTRACT-QUESTION (the build without `p2p`): the default desktop/CLI build
//! of `sovereign-app` (`default = ["encrypted-log", "encryption", "rocksdb",
//! "llama"]`) does not compile `sovereign-p2p`, where `derive_keypair` lives,
//! yet the CLI's `open_primary_documents` (`commands.rs`) and the Tauri
//! login (`tauri_commands/auth.rs`) build a document service there too. D1
//! says "shell and phone alike"; read here as every host, so the peer id must
//! be derivable without the `p2p` feature (the scans below hold in the
//! default build). How (a dependency made non-optional, or the derivation
//! moved) is the implementer's.

use std::path::PathBuf;

// ============================================================ source scans (D1)

/// The argument lists of every `<name>(` call in `src` (balanced
/// parentheses).
fn calls(src: &str, name: &str) -> Vec<String> {
    let mut out = vec![];
    let key = format!("{name}(");
    let mut from = 0;
    while let Some(i) = src[from..].find(&key) {
        let open = from + i + key.len() - 1;
        let mut depth = 0usize;
        let mut end = open;
        for (j, c) in src[open..].char_indices() {
            match c {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + j;
                        break;
                    }
                }
                _ => {}
            }
        }
        out.push(src[open..=end].to_string());
        from = end + 1;
    }
    out
}

fn walk(dir: &std::path::Path) -> Vec<PathBuf> {
    let mut out = vec![];
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// The app's production sources (test files left out; test modules inside
/// a production file pass literals and are not matched).
fn production_sources() -> Vec<(PathBuf, String)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    walk(&dir)
        .into_iter()
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            n.ends_with(".rs") && !n.ends_with("_tests.rs")
        })
        .map(|p| {
            let s = std::fs::read_to_string(&p).unwrap();
            (p, s)
        })
        .collect()
}

// D1 (source): no host of the app builds its document service from a
// display name — the phone and Tauri login (`tauri_commands/auth.rs`), the
// CLI (`commands.rs`, `open_primary_documents`): no `build_document_service(`
// call passes `config.p2p.device_name` or any other `device_name`.
#[test]
fn d1_no_host_of_the_app_builds_the_document_service_from_a_display_name() {
    let mut seen = vec![];
    for (path, src) in production_sources() {
        for call in calls(&src, "build_document_service") {
            assert!(
                !call.contains("device_name"),
                "D1: {}: a document service built from a display name: build_document_service{call}",
                path.display()
            );
            seen.push(path.file_name().unwrap().to_string_lossy().into_owned());
        }
    }
    for host in ["auth.rs", "commands.rs"] {
        assert!(seen.iter().any(|f| f == host), "precondition: the scan sees {host}'s call ({seen:?})");
    }
}

// D1 + D6 (source): the duress decoy's threads are stamped as the persona's
// document service stamps (`duress::seed_duress_db`'s doc: "this device's
// name, the one the persona's document service stamps with"), so its callers
// pass the persona's identity, never `config.p2p.device_name`.
#[test]
fn d1_d6_the_decoy_is_never_stamped_with_a_display_name() {
    let mut seen = 0;
    for (path, src) in production_sources() {
        for name in ["seed_duress_db", "seed_threads"] {
            for call in calls(&src, name) {
                if call.contains(": &") {
                    continue; // the definition, not a call
                }
                seen += 1;
                assert!(
                    !call.contains("device_name"),
                    "D1/D6: {}: the decoy stamped with a display name: {name}{call}",
                    path.display()
                );
            }
        }
    }
    assert!(seen > 0, "precondition: the scan sees the decoy's seeding");
}

// ============================================================ behaviour (D1, D6)

#[cfg(feature = "p2p")]
mod behaviour {
    use crate::lot1_onboarding_tests::{data, fresh_profile, launch, state, DURESS, MAIN, PHRASE};
    use crate::tauri_commands::auth::{complete_onboarding, validate_password};
    use crate::tauri_state::AppState;
    use sovereign_db::GraphDB;

    /// The display name every desktop had by default until items 14 + 19.
    const DISPLAY_NAME: &str = "Sovereign Device";

    fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tauri::async_runtime::block_on(f)
    }

    /// The installed session's peer id: the device key the login installed,
    /// as P2P derives its id.
    fn session_peer_id(app: &AppState) -> String {
        let dk = block_on(app.p2p_identity_key()).expect("the login installs the per-device key");
        sovereign_p2p::identity::derive_keypair(&dk).expect("derive").public().to_peer_id().to_string()
    }

    /// What the installed document service writes as its device: (stamp,
    /// author_device, state's device) of a new document, and its thread's
    /// stamp.
    fn written(app: &AppState, title: &str) -> ((String, Option<String>, String), String) {
        block_on(async {
            let docs = app.docs.read().await.clone().expect("the login installs the document service");
            let thread = docs.thread_create(title, "").await.expect("a thread");
            let t = docs.store().get_thread(&thread).await.expect("the thread reads");
            let id = docs
                .create(sovereign_ai::docs_host::new_user_document(title, &thread, "text"))
                .await
                .expect("create")
                .id;
            let st = docs.stored_state(&id).await.expect("the stored state reads");
            (
                (st.meta.stamp.expect("stamped").device, st.meta.author_device, st.document.device),
                t.stamp.expect("a thread write is stamped"),
            )
        })
    }

    fn assert_identity(app: &AppState, expected: &str, why: &str) {
        let ((stamp, author, device), thread) = written(app, why);
        assert_eq!(stamp, expected, "{why}: the new stamp's device is the persona's peer id");
        assert_eq!(author.as_deref(), Some(expected), "{why}: the local stamping's author");
        assert_eq!(device, expected, "{why}: the state's device");
        assert!(thread.ends_with(&format!(":{expected}")), "{why}: a thread stamp's device: {thread}");
        assert_ne!(stamp, app.config.p2p.device_name, "{why}: never the configured display name");
    }

    // D1: on the phone (and the Tauri desktop), the document service the setup
    // and the login install writes this persona's peer id, never
    // `config.p2p.device_name`; the same id at the next launch.
    #[test]
    fn d1_the_phones_setup_and_login_write_the_personas_peer_id() {
        let (_g, dir) = fresh_profile("item22-d1");
        let app = launch(&dir);
        assert_eq!(app.config.p2p.device_name, DISPLAY_NAME, "precondition: the default display name");
        block_on(complete_onboarding(state(&app), data(MAIN, Some(DURESS), Some(PHRASE)))).expect("setup succeeds");
        let first = session_peer_id(&app);
        assert_identity(&app, &first, "D1 (the setup's session)");

        let again = launch(&dir);
        let persona = block_on(validate_password(state(&again), MAIN.into(), vec![])).expect("the login succeeds");
        assert_eq!(persona, "primary");
        assert_eq!(session_peer_id(&again), first, "precondition: the same device key at the next launch");
        assert_identity(&again, &first, "D1 (the login's session)");
    }

    // D6: the duress persona's service writes its own peer id, derived from its
    // own device key — different from the primary's — and the decoy seeded at
    // its first login is stamped with it, not with the display name.
    #[test]
    fn d6_the_phones_duress_persona_writes_its_own_peer_id() {
        let (_g, dir) = fresh_profile("item22-d6");
        let app = launch(&dir);
        block_on(complete_onboarding(state(&app), data(MAIN, Some(DURESS), Some(PHRASE)))).expect("setup succeeds");
        let primary = session_peer_id(&app);

        let coerced = launch(&dir);
        let persona = block_on(validate_password(state(&coerced), DURESS.into(), vec![])).expect("the duress login");
        assert_eq!(persona, "duress");
        let duress = session_peer_id(&coerced);
        assert_ne!(duress, primary, "precondition (P2P): the duress persona has its own device key");
        assert_identity(&coerced, &duress, "D6 (the duress persona)");

        // The decoy's threads, seeded at this first duress login.
        let stamps: Vec<String> = block_on(async {
            let docs = coerced.docs.read().await.clone().unwrap();
            docs.store().list_threads().await.unwrap().into_iter().filter_map(|t| t.stamp).collect()
        });
        assert!(stamps.len() >= 3, "precondition: the decoy's threads are seeded and stamped: {stamps:?}");
        for s in &stamps {
            assert!(!s.ends_with(&format!(":{DISPLAY_NAME}")), "D6: a decoy thread stamped with the display name: {s}");
            assert!(!s.ends_with(&format!(":{primary}")), "D6: nothing of the primary's identity in the decoy: {s}");
            assert!(s.ends_with(&format!(":{duress}")), "D6: the decoy is stamped with the duress persona's peer id: {s}");
        }
    }
}
