//! C-SWEEP in the native shell (spec ch. 09, v2.0.111-112): what the shell
//! shows and when it runs the sweep.
//!
//! - **SW6:** the status bar counts the suspected values waiting
//!   (`{n} personal data to review`); at login a notice says if the queue has
//!   grown (`Personal data to review` / `{k} more since your last login, {n} in
//!   all. They wait in the PII dashboard.`), and it yields to every other notice
//!   due at that login (the Fable maille, by delegation of Céline, 2026-11-17).
//!   No window opens by itself.
//! - **SW5:** the review window grouped by type, each row's state in words, on
//!   the row and in the accessibility line; a card shows a token as its label.
//! - **SW1, when it runs:** after a login once the session is open, and after a
//!   sync round; held by reading the source (its limits below).
//!
//! A child module of `app` (declared in `app.rs`, like `csup_tests`): a real,
//! headless `App`, `SOVEREIGN_DATA_DIR` set to a fresh temporary folder before
//! any `App` is made.
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not edit this file.

use std::path::PathBuf;
use std::sync::OnceLock;

use sovereign_ai::pii::review::{ReviewGroup, ReviewRow};
use sovereign_ai::pii::signal::LoginSignal;
use sovereign_db::schema::{PiiKind, ReviewState};

use super::first_run_tests::{notice_text, W};
use super::App;

const H: f64 = 800.0;

fn ensure_test_data_dir() {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        if let Some(d) = std::env::var_os("SOVEREIGN_DATA_DIR") {
            return d.into();
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let d = std::env::temp_dir().join(format!("sov-csweep-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        std::env::set_var("SOVEREIGN_DATA_DIR", &d);
        d
    });
}

/// A headless shell with an open session's screen (unlocked).
fn open_shell() -> App {
    ensure_test_data_dir();
    let mut app = App::new();
    app.locked = false;
    app
}

fn status(app: &App) -> String {
    app.status_notice().map(|(s, _)| s).unwrap_or_default()
}

// ======================================================================= SW6

/// SW6: the status bar counts the suspected values waiting, and says nothing
/// of them when none waits.
#[test]
fn sw6_the_status_bar_counts_what_waits() {
    let mut app = open_shell();
    app.pii_waiting = 12;
    assert!(status(&app).contains("12 personal data to review"), "the status line: {:?}", status(&app));
    app.pii_waiting = 0;
    assert!(!status(&app).contains("personal data to review"), "nothing when none waits: {:?}", status(&app));
}

/// SW6: the count stays beside another notice of the status line (an alert),
/// it does not replace it.
#[test]
fn sw6_the_count_sits_beside_the_other_status_notices() {
    let mut app = open_shell();
    app.alert_notices = vec!["A change from your phone looks suspicious".into()];
    app.pii_waiting = 3;
    let line = status(&app);
    assert!(line.contains("looks suspicious"), "{line:?}");
    assert!(line.contains("3 personal data to review"), "{line:?}");
}

/// SW6: the notice at login, in its words, when the queue has grown; no
/// window opens.
#[test]
fn sw6_the_login_notice_says_how_much_more() {
    let mut app = open_shell();
    let windows = app.windows.len();
    app.pii_login_notice(LoginSignal { waiting: 12, more_since_last_login: 3 });
    assert_eq!(
        notice_text(&app).as_deref(),
        Some("Personal data to review\n3 more since your last login, 12 in all. They wait in the PII dashboard.")
    );
    assert_eq!(app.windows.len(), windows, "no window opens by itself");
}

/// SW6: no notice when the queue has not grown.
#[test]
fn sw6_no_login_notice_when_the_queue_has_not_grown() {
    let mut app = open_shell();
    app.pii_login_notice(LoginSignal { waiting: 5, more_since_last_login: 0 });
    assert!(notice_text(&app).is_none());
}

/// SW6: the notice yields to every other notice due at that login (here the
/// interrupted setup's).
#[test]
fn sw6_the_login_notice_yields_to_another_notice() {
    let mut app = open_shell();
    app.set_notice("Setup not finished", super::INTERRUPTED_SETUP);
    let before = notice_text(&app);
    app.pii_login_notice(LoginSignal { waiting: 12, more_since_last_login: 3 });
    assert_eq!(notice_text(&app), before, "the other notice stays");
    app.build_scene(W, H);
}

// ======================================================================= SW5

/// SW5: the review window groups by type, and each row says its state in
/// words in its accessibility line, never the code's name.
#[test]
fn sw5_the_window_groups_by_type_and_says_the_state_in_words() {
    let row = |id: &str, state: ReviewState, words: &str| ReviewRow {
        record_id: id.into(),
        state,
        state_words: words.into(),
        documents: vec!["document:a".into()],
    };
    let groups = vec![
        ReviewGroup {
            kind: PiiKind::Iban,
            rows: vec![
                row("pii_record:1", ReviewState::Unreviewed, "PII suspected - TBC"),
                row("pii_record:2", ReviewState::Dismissed, "PII dismissed"),
            ],
        },
        ReviewGroup { kind: PiiKind::Email, rows: vec![row("pii_record:3", ReviewState::Confirmed, "PII confirmed")] },
    ];
    let panel = crate::panels::PiiPanel::from_groups(groups);
    let lines = panel.a11y_lines();
    let at = |needle: &str| lines.iter().position(|l| l.contains(needle));
    let iban = at("IBAN").expect("a heading for the IBAN group");
    let suspected = at("PII suspected - TBC").expect("the suspected row in words");
    let dismissed = at("PII dismissed").expect("the dismissed row in words");
    let email = lines.iter().position(|l| l.to_lowercase().contains("e-mail") || l.to_lowercase().contains("email")).expect("the e-mail group");
    assert!(iban < suspected && suspected < email && dismissed < email, "the rows under their type: {lines:?}");
    assert!(at("PII confirmed").is_some_and(|c| c > email), "{lines:?}");
    for code in ["Unreviewed", "Confirmed {", "ReviewState"] {
        assert!(lines.iter().all(|l| !l.contains(code)), "the code's name {code} is said: {lines:?}");
    }
}

/// SW5 (Céline, 2026-11-17: both hosts): a card shows a token in its text as
/// its type's label, never in its stored form.
#[test]
fn sw5_a_card_shows_a_token_as_its_label() {
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    rt.block_on(async {
        let store = sovereign_ai::docs_host::testing::encrypted_memory_store("sw5-card").await;
        let docs = sovereign_ai::docs_host::build_document_service(
            store.clone(),
            Some(sovereign_ai::docs_host::testing::account_key()),
            "t".into(),
        );
        let new = sovereign_ai::docs_host::new_user_document("Bank", "thread:t", "pay to CH93 0076 2011 6238 5295 7");
        let id = sovereign_ai::docs_host::create(&docs, new).await.unwrap().id;
        let stored = sovereign_db::traits::GraphDB::get_document(store.as_ref(), &id).await.unwrap().content;
        assert!(stored.contains("[pii:"), "test setup: the stored text holds a token");
        let shown = crate::canvas::shown_text(Some(&docs), &id, &stored).await;
        assert!(!shown.contains("[pii:"), "never the stored form: {shown:?}");
        assert!(!shown.contains("CH93"), "never the value: {shown:?}");
        assert!(shown.to_lowercase().contains("[iban]"), "the type's label: {shown:?}");
    });
}

// ======================================================================= SW1

/// The body of the first non-test `fn name(` in `src` (braces matched; the
/// text of strings and comments is not removed, so this reads names only).
fn fn_body<'a>(src: &'a str, name: &str) -> Option<&'a str> {
    let at = src.find(&format!("fn {name}("))?;
    let open = at + src[at..].find('{')?;
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&src[open..open + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// SW1 and SW6, read in the source: the login, once the session is open, runs
/// the sweep and reads the queue's signal, after the login's other notices.
/// Its limits: it reads the names a function body holds, not that the code
/// runs; a call moved into a helper is followed one level only.
#[test]
fn sw1_sw6_the_login_sweeps_and_says_the_signal_after_its_other_notices() {
    let app_rs = include_str!("app.rs");
    let body = fn_body(app_rs, "unlock_done").expect("unlock_done");
    let mentions = |needle: &str| {
        body.contains(needle)
            || body
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .filter(|w| !w.is_empty())
                .any(|w| fn_body(app_rs, w).is_some_and(|b| b.contains(needle) && !b.contains("fn unlock_done")))
    };
    assert!(mentions("run_sweep_pass") || mentions("sweep"), "the login starts the sweep");
    assert!(mentions("login_signal"), "the login reads the queue's signal");
    let notice = body.find("pii_login_notice").expect("the login shows the personal-data notice");
    let setup = body.find("Setup not finished").expect("the setup notice, as built");
    assert!(notice > setup, "the personal-data notice comes after the login's other notices");
}

/// SW1, read in the source: a sync round that brought content not yet
/// analysed is followed by a sweep: some non-test function of the shell that
/// handles the round's end (`SyncCompleted`) starts it. Its limits: as above.
#[test]
fn sw1_a_sync_round_is_followed_by_a_sweep() {
    let sources = [include_str!("app.rs"), include_str!("p2p.rs")];
    let found = sources.iter().any(|src| {
        let mut from = 0;
        while let Some(at) = src[from..].find("fn ") {
            let start = from + at;
            from = start + 3;
            let name: String = src[start + 3..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if name.is_empty() || name.ends_with("_tests") {
                continue;
            }
            if let Some(b) = fn_body(&src[start..], &name) {
                if b.contains("SyncCompleted") && b.contains("sweep") {
                    return true;
                }
            }
        }
        false
    });
    assert!(found, "no function that handles the end of a sync round starts the sweep");
}
