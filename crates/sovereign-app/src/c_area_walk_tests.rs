//! C-AREA on the phone app's backend — **AR1** (the walk), **AR2** (shared
//! files are named), **AR5** (the walk grows with each release) and **AR6**
//! (reads count too: a differential walk). Spec ch. 07, *One persona, one area
//! — contract (C-AREA)*. AR3, AR4 and AR7 are not here; AR8 is
//! `sovereign-core/tests/c_area_ar8_shared_paths.rs`; the shell's walk is
//! `sovereign-shell/src/c_area_walk_tests.rs`.
//!
//! Written before the code, by another author than whoever moves the shared
//! files: whoever implements L1c makes these pass **without editing this
//! file**; a test that looks wrong is reported, not worked around. Several
//! fail on today's code, and are meant to.
//!
//! **The host is the backend as the app builds it.** Each launch boots it as
//! `run_tauri` does — `AppConfig::load_or_default(None)`, `init_backend` over
//! `sovereign_dir()` (a persistent database, the skills, the orchestrator),
//! and the `AppState` built from it field by field as `lib.rs` builds it —
//! and calls the **commands themselves** (`#[tauri::command]` functions), with
//! a `State` made by `lot1_onboarding_tests::state` (its documented seam).
//! `SOVEREIGN_DATA_DIR` is process wide, so each launch is a child process
//! of this test binary on its own profile, as in the shell's walk:
//! `setup` (`complete_onboarding` with a primary and a duress password),
//! `primary` (a primary login that leaves a document, written through the
//! session's document service), then `walk` on three copies (`walk`,
//! `control`, `stripped`). The child snapshots the profile after the boot
//! and before the duress login, and after the walk; it records each
//! command's result (what the frontend would show).
//!
//! **Every command the app registers is named** (AR1, AR5): the list is read
//! from `run_tauri`'s `generate_handler!` in `lib.rs`, so a command added
//! there is in the walk at once, and fails it until it is driven. Each is:
//! - **driven:** its function is called in the duress session;
//! - **outside a session:** account creation and recovery, before any login
//!   (named in [`OUTSIDE_A_SESSION`], with the reason) — the duress session
//!   cannot reach them;
//! - **desktop only:** under `#[cfg(not(any(target_os = "android",
//!   target_os = "ios")))]` in the handler — not a feature of the phone;
//! - **not driven — a failure:** every other command. Today, all those that
//!   take a `tauri::Webview` (IPC-001's main-webview check) or a
//!   `tauri::AppHandle`: a test cannot build either (tauri's mock app does
//!   not load in a Windows test binary — `lot1_onboarding_tests`' header —
//!   and `Webview` has no layout a test could stand in for). Driving them
//!   needs a seam (a function under each command taking the state, as
//!   `doc_trash`, `doc_orphans`, `doc_saves` have), or tauri's `test`
//!   feature on Linux only: that is a decision, reported, not made here.
//! - **Pairing and sync** need the `p2p` feature (the phone's build has it,
//!   the default test build has not) and `generate_pair_qr` takes a
//!   `Webview`: not driven, a failure.
//!
//! **Second round (2026-11-14, claude-coord from-windows/0241 and
//! from-fable-2nde-position/0016):**
//! - AR1's commands test is a ratchet: the 93 commands not driven yet are
//!   named in [`NOT_DRIVEN_YET`]; a new undriven one, or a listed one driven
//!   or unregistered, fails.
//! - **AR4:** an « incomplete » setup then a duress login in one process; a
//!   primary → duress → primary switch; whose KEK and device key the state
//!   holds after each (read against the profile's `auth.store`), and, in p2p
//!   builds, whether the node was restarted. The primary's raw store is
//!   snapshotted around a duress session (`primary_store`).
//! - **Open vs write**, by name and by record id, both directions.
//! - **AR3** (p2p builds): a duress login over the primary's list; a list
//!   made undecryptable, then a primary login. The pairing and the report go
//!   through Webview commands: gated tests name the functions they need.
//! - **AR7:** each launch has its own temporary directory; the walk writes
//!   nothing there.
//! - **The phone's feature set:** a build with `p2p` turns P2P on as the
//!   phone does. Run on Windows with `--no-default-features --features
//!   mobile` (surrealkv, p2p, haptics, encryption, encrypted-log) as well as
//!   with the default features.
//!
//! **Fifth round (2026-11-14, spec v2.0.79, Céline: « Aucun moteur »):** a
//! duress session runs no P2P node: AR4 asserts none runs after a duress
//! login (after an « incomplete » setup and at every switch) and that the
//! primary's node starts again under its own key; AR3: a duress session
//! cannot pair; AR2 follows v2.0.79's table (« written once, by whichever
//! persona logs in first »).
//!
//! **Fourth round (2026-11-14, from-fable-2nde-position/0019 §2, spec
//! v2.0.78):** AR2 follows the spec's verdict table ([`VERDICTS`]); a second
//! order of setup, a fresh account whose first login is the duress one
//! (`store_only`, then the walk), for AR1 and AR2.
//!
//! **Third round (2026-11-14):** the AR3 seams are signatures (their
//! bodies are the implementer's), so `ar3_seams` compiles and fails; open
//! vs write also requires that each store met no row it could not decrypt;
//! the temporary profiles are deleted when every launch-using test passed,
//! and kept, their path printed, when one failed.
//!
//! **Readings chosen:** as in the shell's walk (area entries cover a
//! directory and what is below it, or a file and its `.tmp` siblings; writes
//! compared as paths; results masked for what is random per run: the
//! generated password, times, dates and record ids). The phone has **no
//! logout** command: the session ends with the process.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use std::time::Duration;

use sovereign_db::GraphDB as _;

use crate::lot1_onboarding_tests::{data, state, DURESS, MAIN};
use crate::tauri_state::AppState;

const CHILD_VAR: &str = "C_AREA_APP_WALK_CHILD";
const OUT_VAR: &str = "C_AREA_APP_WALK_OUT";

// ================================================================ the area

/// Whether a session may change a shared file, or only read it (AR2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum May {
    Change,
    ReadOnly,
}

/// AR2's verdict on a file outside the personas' areas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    /// An exception a duress session may change.
    MayChange,
    /// An exception a duress session may only read.
    ReadOnly,
    /// Written once, by whichever persona logs in first; read only after
    /// that (v2.0.79): a duress session that is the device's first login may
    /// write it, a later one may not.
    WrittenOnce,
    /// Moves into the persona's area: where it is today, a duress write fails AR1.
    IntoTheArea,
    /// The primary's only: a duress session neither reads nor writes it.
    PrimaryOnly,
}

/// **AR2: the verdict table of spec v2.0.79** (ch. 07, C-AREA, « The verdicts,
/// file by file »; branch `v0013/spec-2079`), file by file, with its reason.
/// Paths are relative to the Sovereign directory; one ending in `/` covers
/// everything below it. **A file outside both personas' areas that is not
/// here fails** (`ar2_*`).
///
/// The device preferences read before login (theme, bubble style) move to
/// their own file, `device_preferences.json` (the spec's name since v2.0.79);
/// today they sit in `user_profile.json`, which moves into the area.
const VERDICTS: &[(&str, Verdict, &str)] = &[
    ("crypto/auth.store", Verdict::MayChange, "holds both personas by design; a duress login is recorded like any login"),
    ("device_preferences.json", Verdict::MayChange, "the device preferences read before login: the login screen needs them; they say nothing of which persona is open"),
    ("crypto/login_attempts.json", Verdict::MayChange, "the login's rate-limit state (phone): counted before any persona is known"),
    ("crypto/salt", Verdict::ReadOnly, "the key-derivation salt, read before login to open either persona"),
    ("crypto/device_id", Verdict::ReadOnly, "a random install id; never created by a duress session (on the phone it is created at the P2P node's start, and a duress session starts no node, AR4)"),
    ("onboarding_started", Verdict::WrittenOnce, "a setup finished on this device, not by whom: written once, by whichever persona logs in first"),
    ("onboarding_done", Verdict::WrittenOnce, "a setup finished on this device, not by whom: written once, by whichever persona logs in first"),
    ("crypto/account_key.migrated", Verdict::WrittenOnce, "a migration finished on this device, not by whom: written once, by whichever persona logs in first"),
    ("crypto/canary.store", Verdict::ReadOnly, "read at login, before a persona is known (phone)"),
    ("crypto/keystroke.store", Verdict::ReadOnly, "read at login, before a persona is known (phone)"),
    ("crypto/paired_devices.json", Verdict::IntoTheArea, "pairings are a persona's (AR3)"),
    ("crypto/sync_versions.json", Verdict::IntoTheArea, "a persona's sync state"),
    ("user_profile.json", Verdict::IntoTheArea, "it holds the persona's name"),
    ("device_name.txt", Verdict::IntoTheArea, "it is what the persona's paired devices are shown; it differs per persona"),
    ("comms.toml", Verdict::IntoTheArea, "it holds the mail address (C-COMMS: each store's persona)"),
    ("recovery/", Verdict::PrimaryOnly, "an access recovery in progress, before any login; a duress session neither reads nor writes it"),
    // v11, v12: the recovery store is the four `crypto/` files of
    // `sovereign_crypto::recovery_store` on both hosts (`setup::recovery_store`
    // here); `recovery/` is an access recovery in progress (from-fable-2nde-position/0066).
    ("crypto/recovery_card.json", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/recovery_roster.enc", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/recovery.bundle", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/guardian_enrolled.pending", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
];

fn verdict(rel: &str) -> Option<(Verdict, &'static str)> {
    VERDICTS
        .iter()
        .find(|(p, _, _)| rel == *p || (p.ends_with('/') && (rel.starts_with(p) || rel == p.trim_end_matches('/'))))
        .map(|(_, v, why)| (*v, *why))
}

/// Commands that are not a session's: they run before any login.
const OUTSIDE_A_SESSION: &[(&str, &str)] = &[
    ("check_auth_state", "routes between login and setup, before any login"),
    ("onboarding_designation", "account creation"),
    ("validate_password_policy", "account creation (the new password's policy)"),
    ("complete_onboarding", "account creation"),
    ("complete_onboarding_paired", "account creation by pairing"),
    ("consume_pair_qr_preview", "account creation by pairing (the joiner's preview)"),
    ("start_recovery", "backup recovery, before login (B2)"),
    ("recovery_status", "backup recovery, before login"),
    ("recovery_poll", "backup recovery, before login"),
    ("cancel_recovery", "backup recovery, before login"),
    ("recovery_finalize", "backup recovery, before login"),
    ("access_recovery_available", "guardian access recovery, before login"),
    ("start_access_recovery", "guardian access recovery, before login"),
    ("resume_access_recovery", "guardian access recovery, before login"),
    ("access_recovery_poll", "guardian access recovery, before login"),
    ("access_recovery_status", "guardian access recovery, before login"),
    ("access_recovery_finalize", "guardian access recovery, before login"),
    ("cancel_access_recovery", "guardian access recovery, before login"),
];

struct AreaEntry {
    rel: String,
    dir: bool,
    source: &'static str,
}

fn rel(p: &Path) -> String {
    let base = sovereign_core::sovereign_dir();
    p.strip_prefix(&base)
        .unwrap_or_else(|_| panic!("{} is not under {}", p.display(), base.display()))
        .to_string_lossy()
        .replace('\\', "/")
}

/// The duress persona's own area **on the phone host**, derived from the
/// code's persona-path functions (each with its source line at `ba1dc18`).
/// Two names are written inline in the code, with no function to call
/// (`index.duress.key`: setup.rs l. 341, private; `model_tofu.duress.json`:
/// tauri_commands/auth.rs l. 227, a `match` in `install_session_inner`): they
/// are copied here, with their line.
fn duress_area(config: &sovereign_core::config::AppConfig) -> Vec<AreaEntry> {
    persona_area(config, sovereign_core::auth::PersonaKind::Duress)
}

/// The primary's own area, by the same functions and names.
fn primary_area(config: &sovereign_core::config::AppConfig) -> Vec<AreaEntry> {
    persona_area(config, sovereign_core::auth::PersonaKind::Primary)
}

fn persona_area(config: &sovereign_core::config::AppConfig, d: sovereign_core::auth::PersonaKind) -> Vec<AreaEntry> {
    let duress = d == sovereign_core::auth::PersonaKind::Duress;
    let base = sovereign_core::sovereign_dir();
    let crypto = rel(&crate::setup::crypto_dir());
    let db = crate::setup::persona_db_path(config, d);
    let db = if Path::new(&db).is_absolute() { PathBuf::from(&db) } else { base.join(&db) };
    let mut v = vec![
        AreaEntry { rel: rel(&db), dir: true, source: "sovereign-app/src/setup.rs l. 405-411, persona_db_path (opened by create_db in install_session_inner)" },
        AreaEntry {
            rel: rel(&sovereign_core::auth::orchestrator_state_dir(&base, d)),
            dir: true,
            source: "sovereign-core/src/auth.rs l. 18 (via OrchestratorStatePaths::for_persona, tauri_commands/auth.rs l. 262)",
        },
        AreaEntry {
            rel: rel(&sovereign_core::auth::session_log_beacon_path(&base, d)),
            dir: false,
            source: "sovereign-core/src/auth.rs l. 29 (same call)",
        },
        AreaEntry {
            rel: format!("{crypto}/{}", if duress { "index.duress.key" } else { "index.key" }),
            dir: false,
            source: "sovereign-app/src/setup.rs l. 341, persona_index_filename (private: name copied)",
        },
        AreaEntry {
            rel: format!("{crypto}/{}", if duress { "model_tofu.duress.json" } else { "model_tofu.json" }),
            dir: false,
            source: "sovereign-app/src/tauri_commands/auth.rs l. 227 (inline: name copied)",
        },
    ];
    for kdb in ["keys.db", "keys.messages.db", "keys.threads.db", "keys.conversations.db", "keys.contacts.db", "keys.share_records.db"] {
        v.push(AreaEntry {
            rel: format!("{crypto}/{}", crate::setup::persona_key_db_filename(d, kdb)),
            dir: false,
            source: "sovereign-app/src/setup.rs l. 323, persona_key_db_filename (build_encrypted_db; complete_auth l. 278 for keys.duress.db)",
        });
    }
    v
}

/// Whether `rel` (a directory ends with `/`) is in the area: an entry, a
/// path below a directory entry, a file entry's `.tmp` sibling — or a
/// directory that holds an entry (`data/` for `data/sovereign-duress.db/`):
/// creating the parent of the persona's own path is not a shared write.
fn in_area(rel: &str, area: &[(String, bool)]) -> bool {
    if rel.ends_with('/') && area.iter().any(|(a, _)| a.starts_with(rel)) {
        return true;
    }
    let rel = rel.trim_end_matches('/');
    area.iter().any(|(a, dir)| {
        if *dir {
            rel == a || rel.starts_with(&format!("{a}/"))
        } else {
            rel == a || (rel.starts_with(&format!("{a}.")) && rel.ends_with(".tmp"))
        }
    })
}

fn exception(rel: &str) -> Option<May> {
    match verdict(rel) {
        Some((Verdict::MayChange, _)) => Some(May::Change),
        Some((Verdict::ReadOnly | Verdict::WrittenOnce, _)) => Some(May::ReadOnly),
        _ => None,
    }
}

/// [`exception`] for the device's first login: « written once » files may
/// be written there.
fn exception_at_first_login(rel: &str) -> Option<May> {
    match verdict(rel) {
        Some((Verdict::WrittenOnce, _)) => Some(May::Change),
        _ => exception(rel),
    }
}

// ============================================================ the snapshot

type Snap = BTreeMap<String, String>;

fn fnv(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{h:016x}:{}", bytes.len())
}

fn snapshot(root: &Path) -> Snap {
    fn walk(root: &Path, dir: &Path, out: &mut Snap) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let p = e.path();
            let rel = p.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/");
            if p.is_dir() {
                out.insert(format!("{rel}/"), "dir".into());
                walk(root, &p, out);
            } else {
                let h = std::fs::read(&p).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable:{}", e.kind()));
                out.insert(rel, h);
            }
        }
    }
    let mut out = Snap::new();
    walk(root, root, &mut out);
    out
}

/// A path with the digit runs of its last segment masked, for AR6's
/// comparison of what two runs wrote: RocksDB names its files by number and
/// rotates its log under a timestamp (`000017.log`, `LOG.old.<µs>`), so two
/// identical sessions do not write the same names.
fn numbered(p: &str) -> String {
    let (dir, last) = match p.trim_end_matches('/').rsplit_once('/') {
        Some((d, l)) => (format!("{d}/"), l.to_string()),
        None => (String::new(), p.trim_end_matches('/').to_string()),
    };
    let mut out = String::new();
    let mut in_digits = false;
    for c in last.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
            }
            in_digits = true;
        } else {
            out.push(c);
            in_digits = false;
        }
    }
    let slash = if p.ends_with('/') { "/" } else { "" };
    format!("{dir}{out}{slash}")
}

fn diff(before: &Snap, after: &Snap) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    for (k, v) in after {
        match before.get(k) {
            None => out.push(("created", k.clone())),
            Some(b) if b != v && !k.ends_with('/') => out.push(("changed", k.clone())),
            _ => {}
        }
    }
    for k in before.keys() {
        if !after.contains_key(k) {
            out.push(("deleted", k.clone()));
        }
    }
    out
}

fn mask(line: &str) -> String {
    line.split(|c: char| c == ' ' || c == ',' || c == '"')
        .map(|tok| {
            let has_digit = tok.chars().any(|c| c.is_ascii_digit());
            if has_digit && (tok.contains(':') || tok.contains('-') || tok.contains('/') || tok.contains('T')) {
                "<t>".to_string()
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ================================================================ commands

/// How the walk treats a registered command.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Driven,
    Outside(String),
    DesktopOnly,
    NotDriven(String),
}

/// Every command `run_tauri` registers, in order, with whether the line
/// before it restricts it to the desktop. Read from `lib.rs`.
fn registered_commands() -> Vec<(String, bool)> {
    let src = include_str!("lib.rs");
    let start = src.find("generate_handler![").expect("run_tauri's generate_handler!");
    let end = start + src[start..].find("])").expect("the end of generate_handler!");
    let mut out = Vec::new();
    let mut desktop_only = false;
    for line in src[start..end].lines().skip(1) {
        let l = line.trim();
        if l.starts_with("//") || l.is_empty() {
            continue;
        }
        if l.starts_with("#[cfg(") {
            desktop_only = l.contains("not(any(target_os = \"android\"");
            continue;
        }
        if let Some(path) = l.strip_suffix(',') {
            let name = path.rsplit("::").next().unwrap_or(path).trim().to_string();
            out.push((name, desktop_only));
            desktop_only = false;
        }
    }
    out
}

/// The parameter lists of every `fn <name>(` in the app's sources (a
/// command can have a variant per feature).
fn params_of(name: &str) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") && !p.to_string_lossy().ends_with("_tests.rs") {
                out.push(p);
            }
        }
    }
    let mut files = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    let mut out = Vec::new();
    let pat = format!("fn {name}(");
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap_or_default();
        let mut from = 0;
        while let Some(at) = src[from..].find(&pat) {
            let s = from + at + pat.len();
            let e = src[s..].find(") ->").or_else(|| src[s..].find(")")).map(|x| s + x).unwrap_or(s);
            out.push(src[s..e].to_string());
            from = e;
        }
    }
    out
}

fn classify(name: &str, desktop_only: bool, driven: &BTreeSet<&str>) -> Kind {
    if desktop_only {
        return Kind::DesktopOnly;
    }
    if let Some((_, why)) = OUTSIDE_A_SESSION.iter().find(|(n, _)| *n == name) {
        return Kind::Outside(why.to_string());
    }
    if driven.contains(name) {
        return Kind::Driven;
    }
    let params = params_of(name);
    if params.iter().any(|p| p.contains("tauri::Webview")) {
        Kind::NotDriven("takes a tauri::Webview, which a test cannot build".into())
    } else if params.iter().any(|p| p.contains("tauri::AppHandle")) {
        Kind::NotDriven("takes a tauri::AppHandle, which a test cannot build".into())
    } else {
        Kind::NotDriven("the walk has no driver for it (a command added since the walk was written: AR5)".into())
    }
}

/// AR5, the stores a release created that the walk drives by name rather than
/// as a command (added by the test author of lot LS(a)): *v0.0.13:* the
/// personal-data sweep's review queue (ch. 09, C-SWEEP SW4: its findings,
/// dismissals and the count seen at the last login), driven by
/// `drive_pii_sweep`. Its commands, when the app registers them, join
/// `DRIVEN` or `NOT_DRIVEN_YET` by the walk's own rule.
const WALKED_STORES: &[&str] = &["personal-data sweep: review queue"];

/// The commands the walk drives in the duress session, in order (AR5: a
/// release adds one by adding an entry).
const DRIVEN: &[&str] = &[
    "validate_password",
    "greet",
    "get_jiminy_token",
    "get_theme",
    "toggle_theme",
    "get_profile",
    "save_profile",
    "sync_is_stopped",
    // Item 9 (added by its test author), through `thread_choice::thread_for_name`.
    "thread_for_name",
    "get_config",
    "scan_models",
    "assign_model_role",
    "delete_model",
    "generate_password",
    // C-SWEEP (added by the test author of lot LS(a)): the review's commands,
    // driven through their `_in` bodies (`drive_pii_review_commands`).
    "pii_review_groups",
    "pii_review",
    "pii_signal",
    "pii_login_notice",
    "get_document_labelled",
    // Item C5 (added by its test author): through `histories_not_read_listing`.
    "histories_not_read",
    // C-LOG's host lot (added by its test author): the window and Delete,
    // through their `_in` layers.
    "quarantined_log_readable",
    "delete_quarantined_log",
];

// ============================================================ the launches

fn out(line: &str) {
    let path = std::env::var_os(OUT_VAR).expect("the output file");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open the output file");
    writeln!(f, "{}", line.replace('\n', "\\n")).expect("write the output file");
}

fn config() -> sovereign_core::config::AppConfig {
    let mut c = sovereign_core::config::AppConfig::load_or_default(None);
    // The phone turns P2P on in `run_tauri`'s setup (lib.rs l. 547, under
    // `target_os = "android"`/`"ios"`): a build with `p2p` runs as the phone.
    #[cfg(feature = "p2p")]
    {
        c.p2p.enabled = true;
    }
    // The walk's own model directory, never the repository's real models.
    let models = PathBuf::from(std::env::var_os(OUT_VAR).expect("the output file")).with_extension("models");
    std::fs::create_dir_all(&models).unwrap();
    c.ai.model_dir = models.to_string_lossy().into_owned();
    c
}

/// The backend as `run_tauri` builds it (lib.rs l. 546-640), on `sovereign_dir()`.
fn boot() -> (AppState, std::sync::mpsc::Receiver<sovereign_core::interfaces::OrchestratorEvent>) {
    let cfg = config();
    let b = tauri::async_runtime::block_on(crate::init_backend(&cfg, sovereign_core::sovereign_dir())).expect("init_backend");
    let theme_initial =
        sovereign_core::profile::UserProfile::load(&b.profile_dir).map(|p| p.theme).unwrap_or_else(|_| "dark".to_string());
    let st = AppState {
        db: b.db.clone(),
        orchestrator: b.orchestrator.clone(),
        config: b.config.clone(),
        skill_registry: b.skill_registry,
        skill_db: b.skill_db,
        skill_llm: b.skill_llm,
        decision_tx: b.decision_tx,
        feedback_tx: b.feedback_tx,
        orch_tx: b.orch_tx,
        theme: std::sync::Mutex::new(theme_initial),
        model_assignments: std::sync::Mutex::new(b.model_assignments),
        profile_dir: b.profile_dir,
        account_key: tokio::sync::RwLock::new(None),
        kek: tokio::sync::RwLock::new(None),
        recovery_session: tokio::sync::RwLock::new(None),
        p2p_identity_key: tokio::sync::RwLock::new(None),
        pending_pairing: tokio::sync::RwLock::new(None),
        docs: tokio::sync::RwLock::new(None),
        migration_task: std::sync::Mutex::new(None),
        edit_sessions: std::sync::Mutex::new(std::collections::HashMap::new()),
        #[cfg(feature = "p2p")]
        p2p_command_tx: tokio::sync::RwLock::new(None),
        #[cfg(feature = "p2p")]
        pairing_manager: std::sync::Arc::new(tokio::sync::RwLock::new(None)),
        #[cfg(feature = "p2p")]
        same_name: Default::default(),
        #[cfg(feature = "p2p")]
        p2p_listen_addrs: std::sync::Arc::new(std::sync::RwLock::new(Vec::new())),
        #[cfg(feature = "p2p")]
        backup_host: tokio::sync::RwLock::new(None),
        #[cfg(feature = "p2p")]
        connectivity: std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0)),
        #[cfg(feature = "voice-stt")]
        stt_engine: b.stt_engine,
    };
    (st, b.orch_rx)
}

/// C-SWEEP SW4 (added by the test author of lot LS(a)): in the open session, a
/// document received from outside with a value in it (suspected), a pass of
/// the sweep, the review window's groups, a dismissal and its take-back, a
/// confirmation taken back and given again, and the login's signal (the count
/// seen at the last login). What it shows names no id: the same in both runs
/// of the differential walk (AR6).
fn drive_pii_sweep(st: &AppState) -> (String, Result<(), String>) {
    let run = || -> Result<String, String> {
        let docs = block(st.docs())?;
        let key = block(st.account_key.read()).clone().ok_or("no account key in the session")?;
        let new = sovereign_ai::docs_host::new_document(
            "Statement",
            "",
            "pay to CH93 0076 2011 6238 5295 7",
            sovereign_docs::Trust::External,
            sovereign_docs::Provenance::Import { source_path: "shared from another app".into() },
            sovereign_docs::Origin::Import,
        );
        block(sovereign_ai::docs_host::create(&docs, new)).map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
        let report = block(sovereign_ai::pii::sweep::run_sweep_pass(&docs, 10));
        if !report.failed.is_empty() {
            return Err(format!("the sweep could not analyse {} document(s)", report.failed.len()));
        }
        let review = sovereign_ai::pii::review::PiiReview::new(docs.clone(), key);
        let groups = block(review.groups()).map_err(|e| format!("the review window: {e}"))?;
        let id = groups
            .iter()
            .flat_map(|g| g.rows.iter())
            .next()
            .map(|r| r.record_id.clone())
            .ok_or("no record in the review window")?;
        let one = [id];
        block(review.dismiss(&one)).map_err(|e| format!("dismiss: {e}"))?;
        block(review.take_back_dismissal(&one)).map_err(|e| format!("take back the dismissal: {e}"))?;
        block(review.take_back_confirmation(&one)).map_err(|e| format!("take back the confirmation: {e}"))?;
        block(review.confirm(&one)).map_err(|e| format!("confirm: {e}"))?;
        let signal = block(sovereign_ai::pii::signal::login_signal(st.db.as_ref())).map_err(|e| format!("the login's signal: {e}"))?;
        Ok(format!("{} type(s); {} waiting", groups.len(), signal.waiting))
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
        Ok(Ok(show)) => (show, Ok(())),
        Ok(Err(e)) => (format!("Err: {e}"), Err(e)),
        Err(p) => ("not run".into(), Err(format!("panicked: {}", panic_text(&p)))),
    }
}

/// C-SWEEP SW5, SW6 on the phone (added by the test author of lot LS(a)): the
/// review's commands, through their `_in` bodies (the commands take a
/// `tauri::Webview`, which a test cannot build), after `drive_pii_sweep` left
/// a suspected value in the session. What they show names no id (AR6).
fn drive_pii_review_commands(st: &AppState) -> Vec<(&'static str, String, Result<(), String>)> {
    use crate::tauri_commands::pii;
    fn caught<T>(f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
            Ok(r) => r,
            Err(p) => Err(format!("panicked: {}", panic_text(&p))),
        }
    }
    let mut out = Vec::new();
    let groups = caught(|| block(pii::pii_review_groups_in(st)));
    let first = groups.as_ref().ok().and_then(|g| g.iter().flat_map(|g| g.rows.iter()).next().cloned());
    out.push((
        "pii_review_groups",
        match &groups {
            Ok(g) => format!("{} type(s)", g.len()),
            Err(e) => format!("Err: {e}"),
        },
        groups.as_ref().map(|_| ()).map_err(Clone::clone),
    ));
    // A gesture on a whole type does not exist: an unknown gesture is refused.
    let review = match &first {
        Some(row) => caught(|| {
            if block(pii::pii_review_in(st, "dismiss_type", vec!["iban".to_string()])).is_ok() {
                return Err("a gesture on a whole type was accepted".to_string());
            }
            block(pii::pii_review_in(st, "confirm", vec![row.record_id.clone()])).map(|_| ())
        }),
        None => Err("no record in the review window".to_string()),
    };
    out.push(("pii_review", match &review { Ok(()) => "confirmed one".into(), Err(e) => format!("Err: {e}") }, review));
    let signal = caught(|| block(pii::pii_signal_in(st)));
    out.push((
        "pii_signal",
        match &signal { Ok(s) => format!("{} waiting", s.waiting), Err(e) => format!("Err: {e}") },
        signal.as_ref().map(|_| ()).map_err(Clone::clone),
    ));
    let notice = caught(|| block(pii::pii_login_notice_in(st)));
    out.push((
        "pii_login_notice",
        match &notice { Ok(n) => format!("notice: {}", n.is_some()), Err(e) => format!("Err: {e}") },
        notice.as_ref().map(|_| ()).map_err(Clone::clone),
    ));
    let labelled = match first.as_ref().and_then(|r| r.documents.first().cloned()) {
        Some(doc) => caught(|| block(pii::get_document_labelled_in(st, doc))).and_then(|d| {
            if d.body.contains("[pii:") || d.title.contains("[pii:") {
                Err("a token is shown in its stored form".to_string())
            } else {
                Ok(())
            }
        }),
        None => Err("no document to read".to_string()),
    };
    out.push(("get_document_labelled", match &labelled { Ok(()) => "labelled".into(), Err(e) => format!("Err: {e}") }, labelled));
    out
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(f)
}

fn shown<T: serde::Serialize>(r: &Result<T, String>) -> String {
    match r {
        Ok(v) => serde_json::to_string(v).unwrap_or_else(|e| format!("<not serializable: {e}>")),
        Err(e) => format!("Err: {e}"),
    }
}

#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "setup" => child_setup(),
        "primary" => child_primary(),
        "walk" => child_walk(),
        "store_only" => child_store_only(),
        "names_d1" => child_names(DURESS, "d1", None),
        "names_p1" => child_names(MAIN, "p1", Some("Primary renamed")),
        "names_d2" => child_names(DURESS, "d2", Some("Duress renamed")),
        "names_p2" => child_names(MAIN, "p2", None),
        "ar4_incomplete" => child_ar4_incomplete(),
        "ar4_switch" => child_ar4_switch(),
        "ar4_switch_ends" => child_ar4_switch_ends(),
        "primary_store" => child_primary_store(),
        "primary_lists" => child_primary_lists(),
        "bl_get_profile" => v7::child_before_login("get_profile"),
        "bl_save_profile" => v7::child_before_login("save_profile"),
        "bl_onboarding_designation" => v7::child_before_login("onboarding_designation"),
        "bl_validate_password_policy" => v7::child_before_login("validate_password_policy"),
        "bl_complete_onboarding" => v7::child_before_login("complete_onboarding"),
        "bl_complete_onboarding_paired" => v7::child_before_login("complete_onboarding_paired"),
        "in_duress" => v7::child_in_duress(),
        "no_account" => v7::child_no_account(),
        "old_install" => v7::child_old_install(),
        "old_d1" => child_names(DURESS, "o_d1", None),
        "old_d2" => child_names(DURESS, "o_d2", None),
        "old_p" => child_names(MAIN, "o_p", None),
        "old_b" => child_names(DURESS, "o_b", None),
        "unreadable_duress" => v7::child_unreadable_duress(),
        #[cfg(feature = "p2p")]
        "ar3_duress" => child_ar3_duress(),
        #[cfg(feature = "p2p")]
        "ar3_pair" => ar3_seams::child_pair(),
        #[cfg(feature = "p2p")]
        "ar3_list" => ar3_seams::child_list(),
        #[cfg(feature = "p2p")]
        "guardian_primary" => v10::child_guardian_primary(),
        other => panic!("unknown scenario {other}"),
    }
}

fn child_setup() {
    let (st, rx) = boot();
    let r = block(crate::tauri_commands::auth::complete_onboarding(state(&st), data(MAIN, Some(DURESS), None)));
    assert!(r.is_ok(), "complete_onboarding: {:?}", r.err());
    std::thread::sleep(Duration::from_millis(500));
    std::mem::forget((st, rx));
}

/// The second order's account (from-fable-2nde-position/0019 §2): what
/// `complete_onboarding` writes before it installs a session — `auth.store`,
/// then the salt — and nothing else: a setup killed right after the store, as
/// `lot1_onboarding_tests` makes it. Its first login is the walk's, with the
/// duress password.
/// AR2 (v2.0.78-79): one session of the names sequence on the phone: it logs
/// in with `password`, reports its profile's names (`get_profile`:
/// designation, nickname) and, given `rename`, renames the profile as the
/// app does (`save_profile`, nickname) and reports it again. The phone has no
/// session-level device name to read or rename (`get_p2p_settings` takes a
/// Webview, and the name is the config's).
fn child_names(password: &str, tag: &str, rename: Option<&str>) {
    let (st, rx) = boot();
    let l = login(&st, password);
    assert!(l.is_ok(), "the login: {:?}", l.err());
    let p = block(crate::tauri_commands::auth::get_profile(state(&st)));
    out(&format!("KV\t{tag}.designation\t{}", p.as_ref().map(|p| p.designation.clone()).unwrap_or_default()));
    out(&format!("KV\t{tag}.nickname\t{}", p.as_ref().ok().and_then(|p| p.nickname.clone()).unwrap_or_default()));
    // v7: the profile's id and display name.
    out(&format!("KV\t{tag}.user_id\t{}", p.as_ref().map(|p| p.user_id.clone()).unwrap_or_default()));
    out(&format!("KV\t{tag}.display_name\t{}", p.as_ref().ok().and_then(|p| p.display_name.clone()).unwrap_or_default()));
    if let Some(name) = rename {
        let r = block(crate::tauri_commands::auth::save_profile(
            state(&st),
            crate::tauri_commands::SaveProfileDto { nickname: Some(name.into()), bubble_style: None, display_name: None },
        ));
        let p = block(crate::tauri_commands::auth::get_profile(state(&st)));
        out(&format!(
            "KV\t{tag}.nickname_after\t{}",
            if r.is_ok() { p.ok().and_then(|p| p.nickname).unwrap_or_default() } else { format!("Err: {:?}", r.err()) }
        ));
    }
    out("END");
    std::mem::forget((st, rx));
}

fn child_store_only() {
    let crypto = crate::setup::crypto_dir();
    std::fs::create_dir_all(&crypto).unwrap();
    let salt: [u8; 32] = [3u8; 32];
    let store = sovereign_crypto::auth::AuthStore::create(MAIN.as_bytes(), DURESS.as_bytes(), &salt, "device-duress-first")
        .expect("create");
    store.save_new(&crypto.join("auth.store")).expect("save_new");
    std::fs::write(crypto.join("salt"), salt).expect("the salt");
}

fn child_primary() {
    let (st, rx) = boot();
    let r = block(crate::tauri_commands::auth::validate_password(state(&st), MAIN.into(), vec![]));
    assert!(r.is_ok(), "the primary login: {:?}", r.err());
    // A primary document, through the session's document service (the
    // commands that write documents take a Webview: see the header).
    block(async {
        let docs = st.docs().await.expect("the primary's document service");
        let thread = st
            .db
            .create_thread(sovereign_db::schema::Thread::new("Primary lane".into(), String::new()))
            .await
            .expect("a primary thread")
            .id_string()
            .unwrap();
        out(&format!("KV\tprimary.id\t{thread}"));
        let d = sovereign_ai::docs_host::create(&docs, sovereign_ai::docs_host::new_user_document("Primary only", thread, "never in a duress session"))
            .await
            .expect("a primary document");
        out(&format!("KV\tprimary.id\t{}", d.id));
    });
    std::thread::sleep(Duration::from_millis(500));
    std::mem::forget((st, rx));
}

fn child_walk() {
    let root = sovereign_core::sovereign_dir();
    let (st, rx) = boot();
    for e in duress_area(&st.config) {
        out(&format!("AREA\t{}\t{}\t{}", e.rel, e.dir, e.source));
    }
    for e in primary_area(&st.config) {
        out(&format!("PAREA\t{}\t{}", e.rel, e.dir));
    }
    let models = PathBuf::from(&st.config.ai.model_dir);
    let before = snapshot(&root);
    // AR7: this process's own temporary directory (set by `launch`).
    let temp = std::env::temp_dir();
    let temp_before = snapshot(&temp);
    let s = || state(&st);
    use crate::tauri_commands::{ai, auth, documents, pii};
    let login = block(auth::validate_password(s(), DURESS.into(), vec![]));
    out(&format!("SHOWN\tvalidate_password\t{}", mask(&shown(&login))));
    let login_ok = login.is_ok();
    out(&format!("DRIVE\tvalidate_password\t{}", if login_ok { "OK\t".to_string() } else { format!("FAIL\t{:?}", login.err()) }));
    let mut results: Vec<(&str, String, Result<(), String>)> = Vec::new();
    if login_ok {
        let _ = std::fs::write(models.join("walk-fake.gguf"), b"not a model");
        let _ = std::fs::write(models.join("walk-delete.gguf"), b"not a model");
        let mut run = |name: &'static str, r: Result<String, String>, ok: Result<(), String>| results.push((name, r.unwrap_or_else(|e| format!("Err: {e}")), ok));
        let greet = block(ai::greet("walk".into()));
        run("greet", Ok(greet), Ok(()));
        let tok = ai::get_jiminy_token();
        run("get_jiminy_token", Ok(format!("{}", tok.is_some())), Ok(()));
        let r = block(documents::get_theme(s()));
        run("get_theme", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(documents::toggle_theme(s()));
        run("toggle_theme", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(auth::get_profile(s()));
        run("get_profile", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(auth::save_profile(
            s(),
            crate::tauri_commands::SaveProfileDto { nickname: Some("Walk".into()), bubble_style: Some("wave".into()), display_name: Some("Walker".into()) },
        ));
        run("save_profile", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        // C-SUP: whether this session's sync has stopped. No node ever runs in a duress
        // session, so it never has: the answer is false, the same with or without the
        // primary's files (AR6), and nothing is read or written for it.
        let r = block(auth::sync_is_stopped(s()));
        let ok = match &r {
            Ok(false) => Ok(()),
            Ok(true) => Err("a duress session says its sync has stopped".to_string()),
            Err(e) => Err(e.clone()),
        };
        run("sync_is_stopped", Ok(shown(&r)), ok);
        let r = block(auth::get_config(s()));
        run("get_config", Ok(shown(&r).replace(&st.config.ai.model_dir.replace('\\', "\\\\"), "<models>")), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(ai::scan_models(s()));
        run("scan_models", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(ai::assign_model_role(s(), "walk-fake.gguf".into(), "router".into()));
        run("assign_model_role", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        let r = block(ai::delete_model(s(), "walk-delete.gguf".into()));
        let gone = !models.join("walk-delete.gguf").exists();
        run("delete_model", Ok(shown(&r)), if r.is_ok() && gone { Ok(()) } else { Err(format!("not deleted: {:?}", r.err())) });
        let r = block(pii::generate_password(sovereign_crypto::password_gen::PasswordPolicy::default()));
        run("generate_password", Ok(format!("{} characters", r.as_ref().map(|p| p.chars().count()).unwrap_or(0))), r.as_ref().map(|_| ()).map_err(Clone::clone));
        // Item 9 (added by its test author): what the browser asks before it
        // saves a page, through the layer under `thread_for_name`, in the
        // duress session's store. Only the kind of answer is shown (no id).
        let r = match block(st.docs()) {
            Ok(docs) => block(crate::thread_choice::thread_for_name(&docs, "Web")),
            Err(e) => Err(e),
        };
        let kind = match &r {
            Ok(crate::thread_choice::ThreadForNameDto::Use { .. }) => "use".to_string(),
            Ok(crate::thread_choice::ThreadForNameDto::Create) => "create".to_string(),
            Ok(crate::thread_choice::ThreadForNameDto::Ask { threads, .. }) => format!("ask among {}", threads.len()),
            Err(e) => format!("Err: {e}"),
        };
        run("thread_for_name", Ok(kind), r.as_ref().map(|_| ()).map_err(Clone::clone));
        // C-SWEEP SW4 (added by the test author of lot LS(a)): the sweep's
        // stores in the duress persona, driven through the routine and the
        // review the app runs (SW9).
        let (show, ok) = drive_pii_sweep(&st);
        run("personal-data sweep: review queue", Ok(show), ok);
        for (name, show, ok) in drive_pii_review_commands(&st) {
            run(name, Ok(show), ok);
        }
        // Item C5 (added by its test author): the count of histories this
        // session could not read, through the layer under the command. The
        // duress session read none, so it counts none.
        let r = block(documents::histories_not_read_listing(block(st.docs())));
        let ok = match &r {
            Ok(d) if d.count == 0 && d.sentence.is_none() => Ok(()),
            Ok(_) => Err("a duress session counts a history it could not read".to_string()),
            Err(e) => Err(e.clone()),
        };
        run("histories_not_read", Ok(shown(&r)), ok);
        // v10: guardian enrollment in the duress session (AR4, its decoy),
        // inside the walk's snapshot (AR1, AR2: `recovery/` is the primary's).
        #[cfg(feature = "p2p")]
        {
            let (show, ok) = v10::drive_guardian(&st, "guardian");
            run("begin_guardian_enrollment", Ok(show), ok);
        }
        // v0.0.13 rc2 (added by its test author; Céline, 2026-11-20): the guardian list in the
        // duress session answers as for a persona with no roster, 0 of the total, not armed, no
        // guardians, and no error, as the decoy enrolment above answers « 0 enrolled ». Inside the
        // walk's snapshot, so AR1 and AR6's differential walk see any read of the recovery store.
        #[cfg(feature = "p2p")]
        {
            let r = block(crate::tauri_commands::guardian::list_guardians_in(&st));
            let ok = match &r {
                Ok(g) if g.enrolled_count == 0
                    && g.total as usize == sovereign_crypto::recovery_roster::GUARDIAN_TOTAL
                    && !g.armed
                    && g.guardians.is_empty() =>
                {
                    Ok(())
                }
                Ok(_) => Err("the duress session's guardian list is not a persona's with no roster".to_string()),
                Err(e) => Err(format!("the duress session's guardian list answers an error: {e}")),
            };
            run("list_guardians", Ok(shown(&r)), ok);
        }
        // Item C3 (added by its test author): the source side's question when
        // a device joins under a held name, through the layer under its two
        // commands. A duress session pairs nothing: nothing is held, and « Keep
        // both » with nothing held changes nothing.
        #[cfg(feature = "p2p")]
        {
            let q = crate::tauri_commands::pairing::same_name_question_in(&st);
            run("same_name_question", Ok(shown(&Ok::<_, String>(&q))), Ok(()));
            let r = block(crate::tauri_commands::pairing::answer_same_name_in(&st, None));
            run("answer_same_name", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
        }
        // C-LOG's host lot (added by its test author): the window of what remains
        // readable and Delete, through the layers under their two commands
        // (Export waits for Céline's answer on the phone). The walk's duress
        // session has no quarantine: nothing is shown (only how many entries,
        // never one), and Delete, unconfirmed, is refused and changes nothing.
        {
            use crate::tauri_commands::quarantined_log as ql;
            let r = block(ql::quarantined_log_readable_in(&st)).map(|o| o.map(|d| (d.entries.len(), d.after_the_break)));
            run("quarantined_log_readable", Ok(shown(&r)), r.as_ref().map(|_| ()).map_err(Clone::clone));
            let r = block(ql::delete_quarantined_log_in(&st, false));
            let refused = if r.is_err() { Ok(()) } else { Err("an unconfirmed deletion ran".to_string()) };
            run("delete_quarantined_log", Ok(if r.is_err() { "refused".into() } else { "ran".into() }), refused);
        }
    }
    for (name, show, ok) in &results {
        out(&format!("SHOWN\t{name}\t{}", mask(show)));
        out(&format!("DRIVE\t{name}\t{}", match ok {
            Ok(()) => "OK\t".to_string(),
            Err(e) => format!("FAIL\t{e}"),
        }));
    }
    // The session's background work (the one-time migration) settles.
    std::thread::sleep(Duration::from_millis(1500));
    let after = snapshot(&root);
    for (k, v) in &before {
        out(&format!("BEFORE\t{k}\t{v}"));
    }
    for (kind, rel) in diff(&before, &after) {
        out(&format!("WRITE\t{kind}\t{rel}"));
    }
    for (kind, rel) in diff(&temp_before, &snapshot(&temp)) {
        out(&format!("TEMPWRITE\t{kind}\t{rel}"));
    }
    out("END");
    std::mem::forget((st, rx));
}

fn tail(s: &str) -> &str {
    let mut at = s.len().saturating_sub(6000);
    while !s.is_char_boundary(at) {
        at += 1;
    }
    &s[at..]
}

fn launch(scenario: &str, profile: &Path, out_file: &Path) {
    let exe = std::env::current_exe().expect("the test binary");
    let temp = out_file.with_extension("tmpdir");
    std::fs::create_dir_all(&temp).unwrap();
    let o = Command::new(exe)
        .args(["--exact", "c_area_walk_tests::child_entry", "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_VAR, scenario)
        .env(OUT_VAR, out_file)
        .env("SOVEREIGN_DATA_DIR", profile)
        .env("TMP", &temp)
        .env("TEMP", &temp)
        .env("TMPDIR", &temp)
        .output()
        .expect("spawn the child launch");
    let stdout = String::from_utf8_lossy(&o.stdout);
    let stderr = String::from_utf8_lossy(&o.stderr);
    assert!(
        o.status.success() && stdout.contains("1 passed"),
        "launch `{scenario}` on {} did not pass (status {:?}).\n--- stdout (tail)\n{}\n--- stderr (tail)\n{}",
        profile.display(),
        o.status,
        tail(&stdout),
        tail(&stderr),
    );
}

/// Like `launch`, but a launch that fails is an `Err` (with the child's
/// output), not a panic: for launches that call a body not written yet.
#[cfg_attr(not(feature = "p2p"), allow(dead_code))]
fn launch_soft(scenario: &str, profile: &Path, out_file: &Path) -> Result<(), String> {
    std::panic::catch_unwind(|| launch(scenario, profile, out_file)).map_err(|p| panic_text(&p))
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t);
        } else {
            std::fs::copy(&p, &t).unwrap_or_else(|err| panic!("copy {}: {err}", p.display()));
        }
    }
}

#[derive(Default)]
struct Run {
    area: Vec<(String, bool, String)>,
    primary_area: Vec<(String, bool)>,
    before: Snap,
    driven: Vec<(String, Result<(), String>)>,
    writes: Vec<(String, String)>,
    shown: Vec<(String, String)>,
    temp_writes: Vec<(String, String)>,
    kv: Vec<(String, String)>,
    complete: bool,
}

fn parse(out_file: &Path) -> Run {
    let text = std::fs::read_to_string(out_file).unwrap_or_default();
    let mut r = Run::default();
    for line in text.lines() {
        let f: Vec<&str> = line.splitn(4, '\t').collect();
        match f.as_slice() {
            ["AREA", rel, dir, src] => r.area.push((rel.to_string(), *dir == "true", src.to_string())),
            ["PAREA", rel, dir] => r.primary_area.push((rel.to_string(), *dir == "true")),
            ["BEFORE", k, v] => {
                r.before.insert(k.to_string(), v.to_string());
            }
            ["DRIVE", name, "OK", _] => r.driven.push((name.to_string(), Ok(()))),
            ["DRIVE", name, "FAIL", why] => r.driven.push((name.to_string(), Err(why.to_string()))),
            ["WRITE", kind, rel] => r.writes.push((kind.to_string(), rel.to_string())),
            ["SHOWN", name, s] => r.shown.push((name.to_string(), s.to_string())),
            ["TEMPWRITE", kind, rel] => r.temp_writes.push((kind.to_string(), rel.to_string())),
            ["KV", k, v] => r.kv.push((k.to_string(), v.to_string())),
            ["END"] => r.complete = true,
            _ => {}
        }
    }
    r
}

struct Walks {
    base: PathBuf,
    /// AR2's names sequence: duress, primary (renames), duress (renames), primary.
    names: Vec<Run>,
    /// The second order: the duress login first, on a fresh account.
    duress_first: Run,
    p0: Snap,
    walk: Run,
    control: Run,
    stripped: Run,
    removed: Vec<String>,
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

/// The temporary base of the walks, set by `run_walks`.
static BASE: OnceLock<PathBuf> = OnceLock::new();
static FINISHED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The tests of this file that use the launches (each holds a [`Kept`]).
const WALK_TESTS: usize = if cfg!(feature = "p2p") { WALK_TESTS_P2P } else { WALK_TESTS_DEFAULT };
const WALK_TESTS_DEFAULT: usize = 21;
const WALK_TESTS_P2P: usize = 32;

/// Held by each test that uses the launches. A failing test keeps the
/// temporary profiles and prints where; when every one of the
/// [`WALK_TESTS`] has finished and none failed, the last deletes them. A run
/// that filters some of them out leaves the profiles in place.
struct Kept;

impl Drop for Kept {
    fn drop(&mut self) {
        use std::sync::atomic::Ordering;
        if std::thread::panicking() {
            FAILED.store(true, Ordering::SeqCst);
            if let Some(b) = BASE.get() {
                eprintln!("C-AREA (phone): the temporary profiles are kept for inspection: {}", b.display());
            }
        }
        let done = FINISHED.fetch_add(1, Ordering::SeqCst) + 1;
        if done == WALK_TESTS && !FAILED.load(Ordering::SeqCst) {
            if let Some(b) = BASE.get() {
                let _ = std::fs::remove_dir_all(b);
            }
        }
    }
}

fn walks() -> &'static Walks {
    static W: OnceLock<Result<Walks, String>> = OnceLock::new();
    match W.get_or_init(|| std::panic::catch_unwind(run_walks).map_err(|p| panic_text(&p))) {
        Ok(w) => w,
        Err(e) => panic!("the walks could not run: {e}"),
    }
}

fn run_walks() -> Walks {
    let base = tempfile::Builder::new().prefix("sov-c-area-app-").tempdir().expect("a temporary base").keep();
    let _ = BASE.set(base.clone());
    let p0 = base.join("p0");
    std::fs::create_dir_all(&p0).unwrap();
    launch("setup", &p0, &base.join("setup.out"));
    launch("primary", &p0, &base.join("primary.out"));
    // v11: the primary's recovery card, naming one enrolled guardian (p2p
    // builds, where the walk drives guardian enrollment).
    #[cfg(feature = "p2p")]
    v10::write_primary_recovery_card(&p0);
    let p0_snap = snapshot(&p0);
    let mut runs = BTreeMap::new();
    let mut removed = Vec::new();
    for name in ["walk", "control", "stripped"] {
        let p = base.join(name);
        copy_dir(&p0, &p);
        let o = base.join(format!("{name}.out"));
        if name == "stripped" {
            // The area as the walk on the full profile reported it.
            let area: Vec<(String, bool)> = runs.get("walk").map(area_of).unwrap_or_default();
            removed = strip_profile(&p, &area);
        }
        launch("walk", &p, &o);
        runs.insert(name, parse(&o));
    }
    let names_profile = base.join("names");
    copy_dir(&p0, &names_profile);
    let mut names = Vec::new();
    for step in ["names_d1", "names_p1", "names_d2", "names_p2"] {
        let o = base.join(format!("{step}.out"));
        launch(step, &names_profile, &o);
        names.push(parse(&o));
    }
    let df = base.join("duress-first");
    std::fs::create_dir_all(&df).unwrap();
    launch("store_only", &df, &base.join("store-only.out"));
    let df_out = base.join("duress-first.out");
    launch("walk", &df, &df_out);
    Walks {
        base: base.clone(),
        names,
        duress_first: parse(&df_out),
        p0: p0_snap,
        walk: runs.remove("walk").unwrap(),
        control: runs.remove("control").unwrap(),
        stripped: runs.remove("stripped").unwrap(),
        removed,
    }
}

fn strip_profile(p: &Path, area: &[(String, bool)]) -> Vec<String> {
    assert!(!area.is_empty(), "the duress area is known before stripping");
    let snap = snapshot(p);
    let mut removed = Vec::new();
    for rel in snap.keys().filter(|k| !k.ends_with('/')) {
        if in_area(rel, area) || exception(rel).is_some() {
            continue;
        }
        std::fs::remove_file(p.join(rel)).unwrap_or_else(|e| panic!("remove {rel}: {e}"));
        removed.push(rel.clone());
    }
    let mut dirs: Vec<&String> = snap.keys().filter(|k| k.ends_with('/')).collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in dirs {
        let rel = d.trim_end_matches('/');
        if in_area(d, area) {
            continue;
        }
        if std::fs::read_dir(p.join(rel)).map(|mut r| r.next().is_none()).unwrap_or(false) {
            let _ = std::fs::remove_dir(p.join(rel));
            removed.push(d.clone());
        }
    }
    removed
}

fn area_of(r: &Run) -> Vec<(String, bool)> {
    r.area.iter().map(|(a, d, _)| (a.clone(), *d)).collect()
}

fn undecided_note(rel: &str) -> String {
    match verdict(rel) {
        Some((v, why)) => format!(" [verdict: {v:?} — {why}]"),
        None => " [no verdict: not in the table]".to_string(),
    }
}

// =================================================================== tests

/// v10: commands driven in builds with the `p2p` feature (the phone's):
/// guardian enrollment, through `guardian::begin_guardian_enrollment_in`; the guardian
/// list (v0.0.13 rc2), through `guardian::list_guardians_in`.
/// Item C3: the same-name question and its answer, through
/// `pairing::same_name_question_in` and `answer_same_name_in`.
#[cfg(feature = "p2p")]
const DRIVEN_IN_THIS_BUILD: &[&str] = &["begin_guardian_enrollment", "same_name_question", "answer_same_name", "list_guardians"];
#[cfg(not(feature = "p2p"))]
const DRIVEN_IN_THIS_BUILD: &[&str] = &[];

/// v10: in a build without `p2p` (the desktop test build), guardian
/// enrollment is not a feature of the host as built: the command answers
/// « requires the p2p + encryption features » in every session. It is not
/// driven there, and named. Item C3: the same-name commands are registered
/// only with `p2p`; their layer does not exist without it.
#[cfg(not(feature = "p2p"))]
const NOT_DRIVEN_IN_THIS_BUILD: &[&str] = &["begin_guardian_enrollment", "same_name_question", "answer_same_name", "list_guardians"];
#[cfg(feature = "p2p")]
const NOT_DRIVEN_IN_THIS_BUILD: &[&str] = &[];

/// AR1 (phone): the commands of a session the walk cannot drive yet, **named**
/// (from-fable-2nde-position/0016: the ratchet form of AR8). The test fails
/// when a command is not driven and not here, when one here is driven, or
/// when one here is no longer registered: the list only shrinks, by hand,
/// with the commands that become driven. Order is not the test author's to
/// decide. At `c8ef638`: the 91 that take a `tauri::Webview`, and the 2 that
/// take a `tauri::AppHandle`.
const NOT_DRIVEN_YET: &[&str] = &[
    "get_status",
    "chat_message",
    "search_documents",
    "search_query",
    "approve_action",
    "reject_action",
    "accept_suggestion",
    "dismiss_suggestion",
    "get_trust_entries",
    "reset_trust_action",
    "reset_trust_all",
    "list_documents",
    "list_threads",
    "get_document",
    "save_document",
    "create_document",
    "close_document",
    "delete_document",
    "list_trash",
    "restore_from_trash",
    "empty_trash",
    "list_orphaned_documents",
    "resolve_orphan",
    "resolve_orphans_of_thread",
    "list_unreadable_documents",
    "list_commits",
    "restore_commit",
    "list_skills_for_doc",
    "execute_skill",
    "list_all_skills",
    "import_file",
    "canvas_load",
    "update_document_position",
    "canvas_load_messages",
    "create_thread",
    "update_thread",
    "delete_thread",
    "move_document_to_thread",
    "list_contacts",
    "get_contact_detail",
    "list_conversations",
    "list_messages",
    "mark_message_read",
    "create_relationship",
    "get_comms_config",
    "save_comms_config",
    "fetch_web_page",
    "save_web_page",
    "assess_reliability",
    "reassess_reliability",
    "list_pending_suggestions",
    "accept_link_suggestion",
    "dismiss_link_suggestion",
    "trigger_consolidation",
    "list_sync_alerts",
    "dismiss_sync_alert",
    "list_previous_values",
    "restore_previous_value",
    "forget_previous_value",
    "resolve_pii_tokens",
    "list_pii_entities",
    "get_pii_entity",
    "list_pii_records",
    "confirm_pii_record",
    "dismiss_pii_record",
    "redact_pii_record",
    "reveal_pii_record",
    "create_vault_entry",
    "list_share_records_for_entity",
    "autofill_pii_record",
    "list_cookies_for_entity",
    "delete_cookie",
    "clear_entity_cookies",
    "commit_signup_capture",
    "generate_pair_qr",
    "list_paired_devices",
    "forget_paired_device",
    "get_local_peer_id",
    "trigger_sync_now",
    "cancel_pairing",
    // C-SUP (lot LQ): the restart of a stopped sync. It takes a tauri::Webview, like the
    // pairing commands above; its duress guard is held at its call site (csup_call_sites,
    // su4_restart_sync_goes_through_the_duress_guard).
    "restart_sync",
    "get_p2p_settings",
    "backup_now",
    "backup_status",
    "approve_shard_release",
    "deny_shard_release",
    "voice_transcribe_buffer",
    "receive_shared_content",
    "set_connectivity_state",
    "get_connectivity_state",
    "start_listening",
    "stop_listening",
    // Item 9 (added by its test author): remembering an answer needs two live
    // threads of a name, which the duress walk does not make; its layer is
    // held by `fc_hosts_tests` and the lookup's tests.
    "remember_thread_choice",
];

#[test]
fn ar1_app_duress_walk_drives_every_command_of_a_session() {
    let _kept = Kept;
    let w = walks();
    assert!(w.walk.complete, "the walk did not finish (its output has no END)");
    let driven_set: BTreeSet<&str> = DRIVEN.iter().chain(DRIVEN_IN_THIS_BUILD).copied().collect();
    let registered = registered_commands();
    let mut problems = Vec::new();
    let mut outside = Vec::new();
    let mut desktop = Vec::new();
    let mut undriven = BTreeMap::new();
    for (name, desktop_only) in &registered {
        match classify(name, *desktop_only, &driven_set) {
            Kind::Driven => match w.walk.driven.iter().find(|(n, _)| n == name) {
                Some((_, Ok(()))) => {}
                Some((_, Err(e))) => {
                    undriven.insert(name.clone(), format!("driven, and it failed: {e}"));
                }
                None => {
                    undriven.insert(name.clone(), "not reached (the login failed?)".to_string());
                }
            },
            Kind::Outside(_) => outside.push(name.clone()),
            Kind::DesktopOnly => desktop.push(name.clone()),
            Kind::NotDriven(why) => {
                undriven.insert(name.clone(), why);
            }
        }
    }
    for (name, why) in &undriven {
        if !NOT_DRIVEN_YET.contains(&name.as_str()) && !NOT_DRIVEN_IN_THIS_BUILD.contains(&name.as_str()) {
            problems.push(format!("not driven, and not on the list (new): {name}: {why}"));
        }
    }
    // C-SWEEP SW4 (added by the test author of lot LS(a)): the sweep's stores,
    // driven by name, not as a command (the routine and the review the app runs).
    for name in WALKED_STORES {
        match w.walk.driven.iter().find(|(n, _)| n.as_str() == *name) {
            Some((_, Ok(()))) => {}
            Some((_, Err(e))) => problems.push(format!("{name}: driven, and it failed: {e}")),
            None => problems.push(format!("{name}: not reached (the login failed?)")),
        }
    }
    for listed in NOT_DRIVEN_YET.iter().chain(NOT_DRIVEN_IN_THIS_BUILD) {
        if !registered.iter().any(|(n, _)| n == listed) {
            problems.push(format!("on the list, and no longer registered: {listed}"));
        } else if !undriven.contains_key(*listed) {
            problems.push(format!("driven now, still on the list (remove it): {listed}"));
        }
    }
    println!(
        "C-AREA AR1 (phone): {} commands not driven yet, named; outside a session: {outside:?}; desktop only: {desktop:?}",
        NOT_DRIVEN_YET.len() + NOT_DRIVEN_IN_THIS_BUILD.len()
    );
    assert!(
        problems.is_empty(),
        "C-AREA AR1 (phone): the list of commands not driven yet is not exact:\n  {}",
        problems.join("\n  ")
    );
}

#[test]
fn ar1_app_duress_walk_writes_only_its_own_area() {
    let _kept = Kept;
    let w = walks();
    assert!(w.walk.complete, "the walk did not finish (its output has no END)");
    let area = area_of(&w.walk);
    let outside: Vec<String> = w
        .walk
        .writes
        .iter()
        .filter(|(_, rel)| !in_area(rel, &area) && exception(rel) != Some(May::Change))
        .map(|(k, rel)| format!("{k} {rel}{}", undecided_note(rel)))
        .collect();
    let listing: Vec<String> = w.walk.area.iter().map(|(a, d, s)| format!("  {a}{} — {s}", if *d { "/" } else { "" })).collect();
    assert!(
        outside.is_empty(),
        "C-AREA AR1 (phone): the duress walk created, changed or deleted {} path(s) outside its own area:\n  {}\n\
         The duress area (derived from the code):\n{}",
        outside.len(),
        outside.join("\n  "),
        listing.join("\n")
    );
}

#[test]
fn ar2_app_every_shared_file_has_its_verdict_and_keeps_it() {
    let _kept = Kept;
    let w = walks();
    let mut problems = Vec::new();
    if !w.p0.contains_key("crypto/auth.store") {
        problems.push("crypto/auth.store, an exception that may change, is not in the profile".to_string());
    }
    // In both orders: a « read only » or « the primary's only » file the
    // duress session wrote.
    for (order, r) in [("primary first", &w.walk), ("duress first", &w.duress_first)] {
        assert!(r.complete, "the {order} walk did not finish");
        for (kind, rel) in &r.writes {
            match verdict(rel) {
                Some((Verdict::ReadOnly, why)) => {
                    problems.push(format!("{rel} is « read only » ({why}), and the duress session ({order}) {kind} it"))
                }
                // The duress-first walk is the device's first login: it may
                // write them; after a primary login, no session may.
                Some((Verdict::WrittenOnce, why)) if order == "primary first" => problems.push(format!(
                    "{rel} is « written once, by whichever persona logs in first » ({why}); the primary logged in \
                     first, and the duress session {kind} it"
                )),
                Some((Verdict::PrimaryOnly, why)) => {
                    problems.push(format!("{rel} is « the primary's only » ({why}), and the duress session ({order}) {kind} it"))
                }
                _ => {}
            }
        }
    }
    // A file outside both personas' areas that is not in the table.
    let duress: Vec<(String, bool)> = area_of(&w.walk);
    let primary: Vec<(String, bool)> = w.walk.primary_area.clone();
    let mut files: BTreeSet<String> = w.p0.keys().cloned().collect();
    for r in [&w.walk, &w.duress_first] {
        files.extend(r.before.keys().cloned());
        files.extend(r.writes.iter().filter(|(k, _)| k != "deleted").map(|(_, p)| p.clone()));
    }
    for f in files.iter().filter(|f| !f.ends_with('/')) {
        if !in_area(f, &duress) && !in_area(f, &primary) && verdict(f).is_none() {
            problems.push(format!("{f} is shared (outside both personas' areas) and not in the verdict table"));
        }
    }
    assert!(problems.is_empty(), "C-AREA AR2 (phone):\n  {}", problems.join("\n  "));
}

/// AR1 in the second order (from-fable-2nde-position/0019 §2): a fresh
/// profile whose first login is the duress one, with no primary session
/// before it (the account as a setup interrupted right after the store
/// leaves it). What the first login writes is caught here, not in the
/// primary-first walk.
#[test]
fn ar1_app_duress_first_walk_writes_only_its_own_area() {
    let _kept = Kept;
    let w = walks();
    assert!(w.duress_first.complete, "the duress-first walk did not finish");
    let area = area_of(&w.duress_first);
    let outside: Vec<String> = w
        .duress_first
        .writes
        .iter()
        .filter(|(_, rel)| !in_area(rel, &area) && exception_at_first_login(rel) != Some(May::Change))
        .map(|(kind, rel)| format!("{kind} {rel}{}", undecided_note(rel)))
        .collect();
    assert!(
        outside.is_empty(),
        "C-AREA AR1 (phone, duress first): the duress walk created, changed or deleted {} path(s) outside its own area:\n  {}",
        outside.len(),
        outside.join("\n  ")
    );
}

#[test]
fn ar6_app_duress_walk_shows_and_writes_the_same_without_the_primarys_files() {
    let _kept = Kept;
    let w = walks();
    for (n, r) in [("walk", &w.walk), ("control", &w.control), ("stripped", &w.stripped)] {
        assert!(r.complete, "the {n} run did not finish");
    }
    let shown_diff = |a: &Run, b: &Run| -> Vec<String> {
        let mut out = Vec::new();
        let names: BTreeSet<&String> = a.shown.iter().map(|(n, _)| n).chain(b.shown.iter().map(|(n, _)| n)).collect();
        for n in names {
            let sa = a.shown.iter().find(|(x, _)| x == n).map(|(_, s)| s.clone());
            let sb = b.shown.iter().find(|(x, _)| x == n).map(|(_, s)| s.clone());
            if sa != sb {
                out.push(format!("{n}: with the files {sa:?}; without {sb:?}"));
            }
        }
        out
    };
    let writes = |r: &Run| -> BTreeSet<(String, String)> {
        r.writes.iter().map(|(k, p)| (k.clone(), numbered(p))).collect()
    };
    let noise_shown = shown_diff(&w.walk, &w.control);
    let noise_writes: Vec<_> = writes(&w.walk).symmetric_difference(&writes(&w.control)).cloned().collect();
    assert!(
        noise_shown.is_empty() && noise_writes.is_empty(),
        "C-AREA AR6 (phone): the comparison is not stable — two walks on the same full profile already differ. \
         Shown:\n  {}\nWritten: {:?}",
        noise_shown.join("\n  "),
        noise_writes
    );
    let mut problems = shown_diff(&w.walk, &w.stripped);
    let (a, b) = (writes(&w.walk), writes(&w.stripped));
    for x in a.difference(&b) {
        problems.push(format!("written with the files only: {} {}{}", x.0, x.1, undecided_note(&x.1)));
    }
    for x in b.difference(&a) {
        problems.push(format!("written without the files only: {} {}{}", x.0, x.1, undecided_note(&x.1)));
    }
    assert!(
        problems.is_empty(),
        "C-AREA AR6 (phone): the duress session shows or writes differently when the primary's files and the unnamed \
         shared files are absent ({} removed: {:?}):\n  {}",
        w.removed.len(),
        w.removed,
        problems.join("\n  ")
    );
}

/// AR2 (v2.0.78-79): the duress persona's own profile names on the phone.
/// After setup, its designation and nickname differ from the primary's and
/// are not copied from them; a later rename of the primary's profile does not
/// reach the duress session; the duress session renames its own and the
/// primary's stays. What a generated name looks like is not asserted.
#[test]
fn ar2_app_the_duress_session_has_names_of_its_own() {
    let _kept = Kept;
    let w = walks();
    let [d1, p1, d2, p2] = [&w.names[0], &w.names[1], &w.names[2], &w.names[3]];
    for r in [d1, p1, d2, p2] {
        assert!(r.complete, "a session of the names sequence did not finish");
    }
    let mut problems = Vec::new();
    for key in ["designation", "nickname"] {
        let (d, p) = (kv1(d1, &format!("d1.{key}")), kv1(p1, &format!("p1.{key}")));
        if d == p {
            problems.push(format!("after setup, the duress session's {key} is the primary's: {d:?}"));
        }
    }
    let renamed = kv1(p1, "p1.nickname_after");
    if renamed != "Primary renamed" {
        problems.push(format!("(precondition) the primary did not rename its profile: {renamed:?}"));
    }
    for key in ["designation", "nickname"] {
        let (before, after) = (kv1(d1, &format!("d1.{key}")), kv1(d2, &format!("d2.{key}")));
        if before != after {
            problems.push(format!("the duress session's {key} changed after a primary rename: {before:?} -> {after:?}"));
        }
    }
    let own = kv1(d2, "d2.nickname_after");
    if own != "Duress renamed" {
        problems.push(format!("the duress session could not rename its own profile: {own:?}"));
    }
    let primary_after = kv1(p2, "p2.nickname");
    if primary_after != "Primary renamed" {
        problems.push(format!("the duress session's rename reached the primary's profile: {primary_after:?}"));
    }
    assert!(problems.is_empty(), "C-AREA AR2 (phone), the duress persona's names:\n  {}", problems.join("\n  "));
}

/// AR5: every registered command is classified, and every driven one is
/// registered (a command removed leaves no stale driver).
#[test]
fn ar5_app_walk_names_every_registered_command() {
    let reg = registered_commands();
    assert!(reg.len() > 100, "the handler list was read ({} commands)", reg.len());
    let names: BTreeSet<&str> = reg.iter().map(|(n, _)| n.as_str()).collect();
    for d in DRIVEN.iter().chain(DRIVEN_IN_THIS_BUILD) {
        assert!(names.contains(d), "the walk drives {d}, which run_tauri no longer registers");
    }
    for (o, _) in OUTSIDE_A_SESSION {
        assert!(names.contains(o), "{o} is named outside a session, and run_tauri no longer registers it");
    }
}

// ========================================== AR3, AR4, open vs write, AR7

/// The phone's own objects at P0 (`child_primary`), and what the duress
/// session creates in `primary_store` (open vs write).
const PRIMARY_THREAD: &str = "Primary lane";
const PRIMARY_DOC: &str = "Primary only";
const DURESS_THREAD: &str = "Duress-only lane";
const DURESS_DOC: &str = "Duress-only document";
/// Titles the duress login seeds (`duress.rs`, `seed_duress_db`; a sample).
const DECOY_TITLES: [&str; 3] = ["Grocery List", "Meeting Notes - Q1 Review", "Banana Bread"];

fn kv(r: &Run, key: &str) -> Vec<String> {
    r.kv.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect()
}

fn kv1(r: &Run, key: &str) -> String {
    kv(r, key).into_iter().next().unwrap_or_else(|| "<not reported>".into())
}

/// A persona's KEK and device key, read here from the profile's `auth.store`
/// with its password (the test's own reference, not the app's state).
fn persona_keys(password: &str) -> (Vec<u8>, Vec<u8>) {
    let store = sovereign_crypto::auth::AuthStore::load(&crate::setup::crypto_dir().join("auth.store")).expect("auth.store");
    let a = store.authenticate(password.as_bytes()).expect("authenticate");
    (a.kek.as_bytes().to_vec(), a.device_key.as_bytes().to_vec())
}

fn whose(v: &Option<Vec<u8>>, primary: &[u8], duress: &[u8]) -> &'static str {
    match v {
        None => "none",
        Some(b) if b == primary => "primary",
        Some(b) if b == duress => "duress",
        Some(_) => "other",
    }
}

/// Reports, under `phase`, whose KEK and whose device key the state holds.
fn report_keys(st: &AppState, phase: &str) {
    let (pk, pd) = persona_keys(MAIN);
    let (dk, dd) = persona_keys(DURESS);
    let kek = block(st.kek()).map(|k| k.as_bytes().to_vec());
    let id = block(st.p2p_identity_key()).map(|k| k.as_bytes().to_vec());
    out(&format!("KV\t{phase}.kek\t{}", whose(&kek, &pk, &dk)));
    out(&format!("KV\t{phase}.device_key\t{}", whose(&id, &pd, &dd)));
}

/// The running node's command channel (p2p builds), to tell a node that
/// was restarted from one that was kept.
#[cfg(feature = "p2p")]
type NodeTx = Option<tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>>;
#[cfg(not(feature = "p2p"))]
type NodeTx = Option<()>;

fn node_tx(st: &AppState) -> NodeTx {
    #[cfg(feature = "p2p")]
    {
        block(st.p2p_command_tx())
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = st;
        None
    }
}

/// Reports whether the node running after `phase` is the one that ran
/// before it (`kept`), a new one (`restarted`), or none; and whether the old
/// one stopped.
fn report_node(st: &AppState, phase: &str, before: &NodeTx) {
    #[cfg(feature = "p2p")]
    {
        let after = node_tx(st);
        let state = match (before, &after) {
            (_, None) => "none",
            (None, Some(_)) => "started",
            (Some(b), Some(a)) if a.same_channel(b) => "kept",
            (Some(_), Some(_)) => "restarted",
        };
        out(&format!("KV\t{phase}.node\t{state}"));
        out(&format!("KV\t{phase}.old_node_stopped\t{}", before.as_ref().map_or(true, |b| b.is_closed())));
    }
    #[cfg(not(feature = "p2p"))]
    {
        let _ = (st, before);
        out(&format!("KV\t{phase}.node\tno p2p in this build"));
    }
}

/// How many rows the session's store met and could not decrypt (open vs
/// write, from-fable-2nde-position/0017 §1), or why it is not known.
fn unreadable(st: &AppState) -> String {
    let docs = match block(st.docs()) {
        Ok(d) => d,
        Err(e) => return format!("no document service: {e}"),
    };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| docs.store().undecryptable_rows_met())) {
        Ok(n) => n.to_string(),
        Err(p) => format!("not known: {}", panic_text(&p)),
    }
}

fn login(st: &AppState, password: &str) -> Result<String, String> {
    let r = block(crate::tauri_commands::auth::validate_password(state(st), password.into(), vec![]));
    // The node starts in the background of the login (p2p builds).
    std::thread::sleep(Duration::from_millis(1500));
    r
}

/// AR4: a setup that ends « incomplete » (the safety phrase cannot be
/// written, as `lot1_onboarding_tests` makes it), then a duress login in the
/// same process.
fn child_ar4_incomplete() {
    std::fs::create_dir_all(crate::setup::crypto_dir().join("canary.store").join("blocked")).unwrap();
    let (st, rx) = boot();
    let r = block(crate::tauri_commands::auth::complete_onboarding(
        state(&st),
        data(MAIN, Some(DURESS), Some(crate::lot1_onboarding_tests::PHRASE)),
    ));
    out(&format!("KV\tsetup.incomplete\t{}", r.as_ref().map(|o| o.incomplete).unwrap_or(false)));
    std::thread::sleep(Duration::from_millis(1500));
    report_keys(&st, "setup");
    let before = node_tx(&st);
    let l = login(&st, DURESS);
    out(&format!("KV\tduress.login\t{}", shown(&l)));
    report_keys(&st, "duress");
    report_node(&st, "duress", &before);
    out("END");
    std::mem::forget((st, rx));
}

/// AR4: persona switches in one process: primary, duress, primary again.
fn child_ar4_switch() {
    let (st, rx) = boot();
    let l = login(&st, MAIN);
    out(&format!("KV\tprimary.login\t{}", shown(&l)));
    report_keys(&st, "primary");
    let before = node_tx(&st);
    let l = login(&st, DURESS);
    out(&format!("KV\tduress.login\t{}", shown(&l)));
    report_keys(&st, "duress");
    report_node(&st, "duress", &before);
    let before = node_tx(&st);
    let l = login(&st, MAIN);
    out(&format!("KV\tprimary_again.login\t{}", shown(&l)));
    report_keys(&st, "primary_again");
    report_node(&st, "primary_again", &before);
    out("END");
    std::mem::forget((st, rx));
}

/// The primary's raw store, as `run_tauri` opens it at boot.
fn primary_store_dir(st: &AppState) -> PathBuf {
    let p = PathBuf::from(&st.config.database.path);
    if p.is_absolute() {
        p
    } else {
        sovereign_core::sovereign_dir().join(p)
    }
}

/// AR4 (« the primary's store is not written during a duress session ») and
/// open vs write: a duress session that lists, creates a thread and a
/// document through its session's document service, and changes the theme
/// and the profile; the primary's raw store is snapshotted around it.
fn child_primary_store() {
    let (st, rx) = boot();
    let store = primary_store_dir(&st);
    let before = snapshot(&store);
    let l = login(&st, DURESS);
    out(&format!("KV\tduress.login\t{}", shown(&l)));
    block(async {
        for d in st.db.list_documents(None).await.unwrap_or_default() {
            out(&format!("KV\tduress.doc\t{}", d.title));
            out(&format!("KV\tduress.seen_id\t{}", d.id_string().unwrap_or_default()));
        }
        for t in st.db.list_threads().await.unwrap_or_default() {
            out(&format!("KV\tduress.thread\t{}", t.name));
            out(&format!("KV\tduress.seen_id\t{}", t.id_string().unwrap_or_default()));
        }
        let docs = st.docs().await.expect("the duress session's document service");
        let thread = docs.thread_create(DURESS_THREAD, "").await.expect("a duress thread");
        out(&format!("KV\tduress.created_id\t{thread}"));
        let d = sovereign_ai::docs_host::create(&docs, sovereign_ai::docs_host::new_user_document(DURESS_DOC, thread, "created under duress"))
            .await
            .expect("a duress document");
        out(&format!("KV\tduress.created_id\t{}", d.id));
    });
    let _ = block(crate::tauri_commands::documents::toggle_theme(state(&st)));
    let _ = block(crate::tauri_commands::auth::save_profile(
        state(&st),
        crate::tauri_commands::SaveProfileDto { nickname: Some("Walk".into()), bubble_style: None, display_name: None },
    ));
    // The session's background work (the one-time migration) settles.
    std::thread::sleep(Duration::from_millis(2500));
    for (kind, rel) in diff(&before, &snapshot(&store)) {
        out(&format!("KV\tprimary_store.write\t{kind} {rel}"));
    }
    out(&format!("KV\tduress.unreadable\t{}", unreadable(&st)));
    out("END");
    std::mem::forget((st, rx));
}

/// After `primary_store`: a primary session, and what it lists.
fn child_primary_lists() {
    let (st, rx) = boot();
    let l = login(&st, MAIN);
    out(&format!("KV\tprimary.login\t{}", shown(&l)));
    block(async {
        for d in st.db.list_documents(None).await.unwrap_or_default() {
            out(&format!("KV\tprimary.doc\t{}", d.title));
            out(&format!("KV\tprimary.seen_id\t{}", d.id_string().unwrap_or_default()));
        }
        for t in st.db.list_threads().await.unwrap_or_default() {
            out(&format!("KV\tprimary.thread\t{}", t.name));
            out(&format!("KV\tprimary.seen_id\t{}", t.id_string().unwrap_or_default()));
        }
    });
    out(&format!("KV\tprimary.unreadable\t{}", unreadable(&st)));
    block(async {
        if let Ok(docs) = st.docs().await {
            if let Ok((readable, _)) = docs.store().list_documents_including_deleted_and_undecryptable().await {
                for d in readable.into_iter().filter(|d| d.title == TRASHED_DOC) {
                    let id = d.id_string().unwrap_or_default();
                    let versions = docs.versions(&id).await.map(|v| v.len()).unwrap_or(0);
                    out(&format!("KV\tprimary.trashed_doc\tpresent, {versions} versions"));
                }
            }
        }
    });
    #[cfg(feature = "p2p")]
    block(async {
        if let Some(m) = st.pairing_manager.read().await.as_ref() {
            for d in m.list_devices() {
                out(&format!("KV\tprimary.paired\t{}", d.device_name));
            }
        }
    });
    out("END");
    std::mem::forget((st, rx));
}

/// AR3 (p2p builds): the primary's paired list, written here as the primary
/// (its store key, from its device key), then a duress login in the same
/// process. The bytes before the login are reported.
#[cfg(feature = "p2p")]
fn child_ar3_duress() {
    let (st, rx) = boot();
    let device_key = sovereign_crypto::auth::AuthStore::load(&crate::setup::crypto_dir().join("auth.store"))
        .expect("auth.store")
        .authenticate(MAIN.as_bytes())
        .expect("the primary")
        .device_key;
    let path = crate::setup::crypto_dir().join("paired_devices.json");
    let mut m = sovereign_p2p::pairing::PairingManager::new(path.clone());
    m.add_device(sovereign_p2p::pairing::PairedDevice::with_key(
        "12D3KooWPrimaryPairedDeviceFixture".into(),
        "Primary's paired device".into(),
        [7u8; 32],
    ));
    m.save(&sovereign_p2p::pairing::derive_store_key(&device_key)).expect("the primary's list");
    out(&format!("KV	fixture.hash	{}", fnv(&std::fs::read(&path).unwrap())));
    let l = login(&st, DURESS);
    out(&format!("KV\tduress.login\t{}", shown(&l)));
    std::thread::sleep(Duration::from_millis(1500));
    out("END");
    std::mem::forget((st, rx));
}

/// AR4 (v2.0.80): « A switch ends everything the previous session started:
/// its node, its keys, its timers and background tasks (the hourly purge, the
/// personal-data sweep), and its pending proposals. » A primary session with
/// a fake model (the orchestrator `run_tauri` installs is replaced by one over
/// `Orchestrator::for_host_tests_with_model`, sovereign-ai's `test-support`)
/// proposes a rename and waits for its answer; then a switch to the duress
/// persona in the same process. After the switch, the primary's files are
/// snapshotted, the hourly purge's tick is forced (`doc_trash::purge_tick_in`),
/// the primary's pending proposal is approved as `approve_action` sends it,
/// and the files are snapshotted again.
///
/// **Extension point:** the personal-data sweep (ch. 09, C-SWEEP, v0.0.13)
/// joins here when it exists: its pass is forced next to the purge's tick.
fn child_ar4_switch_ends() {
    use sovereign_core::interfaces::OrchestratorEvent;
    let (mut st, rx) = boot();
    let answers: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<String>>> = Default::default();
    let a = answers.clone();
    let generate: std::sync::Arc<dyn Fn(&str, u32) -> anyhow::Result<String> + Send + Sync> =
        std::sync::Arc::new(move |_p: &str, _m: u32| Ok(a.lock().unwrap().pop_front().unwrap_or_default()));
    let paths = sovereign_ai::OrchestratorStatePaths::for_persona(&sovereign_core::sovereign_dir(), sovereign_core::auth::PersonaKind::Primary);
    let db: std::sync::Arc<dyn sovereign_db::GraphDB> = st.db.clone();
    let mut o = sovereign_ai::Orchestrator::for_host_tests_with_model(db, st.orch_tx.clone(), paths, 4096, generate);
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    o.set_decision_rx(drx);
    st.decision_tx = dtx;
    let orch = std::sync::Arc::new(o);
    st.orchestrator = Some(orch.clone());

    let l = login(&st, MAIN);
    out(&format!("KV\tprimary.login\t{}", shown(&l)));
    // A primary document, trashed; the primary's own purge tick runs at real
    // time (its unlock tick: it records the « trash since » floor); then the
    // service's clock moves 31 days on (`sovereign_core::clock::test_clock`), so the
    // document is due for purge — by the primary's session only.
    // The clock is this process's: the guard holds it until the child ends
    // (a child process runs this scenario alone).
    let clock = sovereign_core::clock::test_clock::hold();
    let trashed_versions = block(async {
        use sovereign_docs::Clock as _;
        let docs = st.docs().await.expect("the primary's document service");
        let thread = st
            .db
            .list_threads()
            .await
            .unwrap_or_default()
            .into_iter()
            .find(|t| t.name == PRIMARY_THREAD)
            .and_then(|t| t.id_string())
            .expect("the primary's lane");
        let d = sovereign_ai::docs_host::create(&docs, sovereign_ai::docs_host::new_user_document(TRASHED_DOC, thread, "trashed by the primary"))
            .await
            .expect("a primary document");
        docs.trash(&d.id).await.expect("trashed");
        sovereign_docs::purge_tick(&docs).await.expect("the primary's unlock tick");
        clock.advance(chrono::Duration::days(31));
        let now = sovereign_ai::docs_host::SystemClock.now();
        let cutoff = now - chrono::Duration::days(sovereign_docs::TRASH_RETENTION_DAYS);
        let at = docs.read(&d.id).await.ok().and_then(|v| v.trashed_at);
        let since = docs.trash_since().await.ok().flatten();
        let due = at.is_some_and(|t| t <= cutoff) && since.is_some_and(|t| t <= cutoff);
        out(&format!("KV\ttrashed.due\t{due}"));
        docs.versions(&d.id).await.map(|v| v.len()).unwrap_or(0)
    });
    out(&format!("KV\ttrashed.versions\t{trashed_versions}"));
    {
        let mut q = answers.lock().unwrap();
        q.push_back(
            r#"{"action":"rename_thread","target":"Primary lane to Renamed under duress","confidence":0.95}"#.into(),
        );
        q.push_back("Done.".into());
    }
    let o2 = orch.clone();
    tauri::async_runtime::spawn(async move {
        let _ = o2.handle_query("Rename the lane Primary lane to Renamed under duress").await;
    });
    let mut proposed = false;
    let end_at = std::time::Instant::now() + Duration::from_secs(30);
    while std::time::Instant::now() < end_at && !proposed {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(200)) {
            proposed = matches!(ev, OrchestratorEvent::ActionProposed { .. });
        }
    }
    out(&format!("KV\tprimary.proposal_pending\t{proposed}"));

    // The switch.
    let l = login(&st, DURESS);
    out(&format!("KV\tduress.login\t{}", shown(&l)));
    let root = sovereign_core::sovereign_dir();
    let primary: Vec<(String, bool)> = primary_area(&st.config).into_iter().map(|e| (e.rel, e.dir)).collect();
    let primary_files = |snap: &Snap| -> Snap {
        snap.iter().filter(|(k, _)| in_area(k, &primary)).map(|(k, v)| (k.clone(), v.clone())).collect()
    };
    let before = primary_files(&snapshot(&root));

    // The purge's tick, forced.
    let purge = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        block(crate::doc_trash::purge_tick_in(&st))
    }));
    out(&format!("KV\tpurge.result\t{}", match purge {
        Ok(Ok(r)) => format!("Ok: {} purged", r.documents.len()),
        Ok(Err(e)) => format!("Err: {e}"),
        Err(p) => format!("not known: {}", panic_text(&p)),
    }));
    // The personal-data sweep's pass, forced (C-SWEEP, added by the test author
    // of lot LS(a)): the routine the app runs (SW9), over the session open now.
    let sweep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        block(async {
            let docs = st.docs().await?;
            Ok::<_, String>(sovereign_ai::pii::sweep::run_sweep_pass(&docs, 10).await)
        })
    }));
    out(&format!("KV\tsweep.result\t{}", match sweep {
        Ok(Ok(r)) => format!("Ok: {} analysed", r.analysed.len()),
        Ok(Err(e)) => format!("Err: {e}"),
        Err(p) => format!("not known: {}", panic_text(&p)),
    }));

    // The primary's pending proposal, approved as `approve_action` sends it.
    let sent = st.decision_tx.try_send(sovereign_core::security::ActionDecision::Approve).is_ok();
    out(&format!("KV\tapprove.sent\t{sent}"));
    let end_at = std::time::Instant::now() + Duration::from_secs(8);
    while std::time::Instant::now() < end_at {
        if let Ok(ev) = rx.recv_timeout(Duration::from_millis(200)) {
            if let OrchestratorEvent::ActionExecuted { action, success, .. } = ev {
                out(&format!("KV\tafter_switch.executed\t{action} success={success}"));
            }
        }
    }
    std::thread::sleep(Duration::from_millis(1000));
    for (kind, rel) in diff(&before, &primary_files(&snapshot(&root))) {
        out(&format!("KV\tprimary.write\t{kind} {rel}"));
    }
    out("END");
    std::mem::forget((st, rx));
}

/// The primary's document trashed before the switch (AR4, a switch ends everything).
const TRASHED_DOC: &str = "Trashed by the primary";

/// AR3 and AR4 launches, once per test process, on copies of the walks' P0.
struct Ar {
    /// The primary session at P0 (its ids).
    primary: Run,
    incomplete: Run,
    switch: Run,
    /// `ar4_switch_ends`, then a primary session on the same profile.
    switch_ends: Run,
    switch_ends_primary: Run,
    store: Run,
    lists: Run,
    #[cfg(feature = "p2p")]
    ar3: Ar3,
}

#[cfg(feature = "p2p")]
struct Ar3 {
    duress: Run,
    after_hash: String,
    primary: Run,
    corrupt_before: Vec<u8>,
    corrupt_after: Option<Vec<u8>>,
    corrupt_primary: Run,
    /// `ar3_seams`: the duress session's pairing (the list's hash before and
    /// after), and the listing over the undecryptable list.
    pair_launch: Result<(), String>,
    pair: Run,
    pair_before: String,
    pair_after: String,
    list_launch: Result<(), String>,
    list: Run,
}

fn ar_runs() -> &'static Ar {
    static A: OnceLock<Result<Ar, String>> = OnceLock::new();
    match A.get_or_init(|| std::panic::catch_unwind(run_ar).map_err(|p| panic_text(&p))) {
        Ok(a) => a,
        Err(e) => panic!("the AR3/AR4 launches could not run: {e}"),
    }
}

fn run_ar() -> Ar {
    let w = walks();
    let base = &w.base;
    let fresh = base.join("ar4-incomplete");
    std::fs::create_dir_all(&fresh).unwrap();
    let o = base.join("ar4-incomplete.out");
    launch("ar4_incomplete", &fresh, &o);
    let incomplete = parse(&o);
    let one = |name: &str, scenario: &str| -> Run {
        let p = base.join(name);
        if !p.exists() {
            copy_dir(&base.join("p0"), &p);
        }
        let o = base.join(format!("{name}-{scenario}.out"));
        launch(scenario, &p, &o);
        parse(&o)
    };
    let switch = one("ar4-switch", "ar4_switch");
    let switch_ends = one("ar4-switch-ends", "ar4_switch_ends");
    let switch_ends_primary = one("ar4-switch-ends", "primary_lists");
    let store = one("ar4-store", "primary_store");
    let lists = one("ar4-store", "primary_lists");
    #[cfg(feature = "p2p")]
    let ar3 = {
        let rel = "crypto/paired_devices.json";
        let p = base.join("ar3");
        copy_dir(&base.join("p0"), &p);
        let o = base.join("ar3-duress.out");
        launch("ar3_duress", &p, &o);
        let duress = parse(&o);
        let after_hash = std::fs::read(p.join(rel)).map(|b| fnv(&b)).unwrap_or_else(|_| "<deleted>".into());
        let primary = one("ar3", "primary_lists");
        // The list made undecryptable, then a primary login.
        let c = base.join("ar3-corrupt");
        copy_dir(&p, &c);
        let mut bytes = std::fs::read(c.join(rel)).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if let Some(at) = ciphertext_at(&text) {
            bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
        }
        std::fs::write(c.join(rel), &bytes).unwrap();
        let corrupt_primary = one("ar3-corrupt", "primary_lists");
        let corrupt_after = std::fs::read(c.join(rel)).ok();
        let hash = |p: &Path| std::fs::read(p).map(|b| fnv(&b)).unwrap_or_else(|_| "<deleted>".into());
        let g = base.join("ar3-pair");
        copy_dir(&p, &g);
        let pair_before = hash(&g.join(rel));
        let pair_launch = launch_soft("ar3_pair", &g, &base.join("ar3-pair.out"));
        let pair = parse(&base.join("ar3-pair.out"));
        let pair_after = hash(&g.join(rel));
        let list_out = base.join("ar3-list.out");
        let list_launch = launch_soft("ar3_list", &c, &list_out);
        let list = parse(&list_out);
        Ar3 {
            duress,
            after_hash,
            primary,
            corrupt_before: bytes,
            corrupt_after,
            corrupt_primary,
            pair_launch,
            pair,
            pair_before,
            pair_after,
            list_launch,
            list,
        }
    };
    Ar {
        primary: parse(&base.join("primary.out")),
        incomplete,
        switch,
        switch_ends,
        switch_ends_primary,
        store,
        lists,
        #[cfg(feature = "p2p")]
        ar3,
    }
}

// ----------------------------------------------------------------- AR4

#[test]
fn ar4_app_after_an_incomplete_setup_a_duress_login_holds_no_primary_kek() {
    let _kept = Kept;
    let a = ar_runs();
    let r = &a.incomplete;
    assert!(r.complete, "the launch did not finish");
    assert_eq!(kv1(r, "setup.incomplete"), "true", "precondition: the setup ends « incomplete »");
    assert!(kv1(r, "duress.login").contains("duress"), "precondition: the duress login opens: {}", kv1(r, "duress.login"));
    let kek = kv1(r, "duress.kek");
    assert!(
        kek == "none" || kek == "duress",
        "C-AREA AR4 (phone): after an incomplete setup, the duress session holds the {kek} KEK (setup left the {} one)",
        kv1(r, "setup.kek")
    );
    assert_eq!(kv1(r, "duress.device_key"), "duress", "C-AREA AR4 (phone): the duress session's device key");
}

/// AR4 (v2.0.79, « Aucun moteur »): after a setup that ends « incomplete »,
/// a duress login stops the primary's node and runs none of its own (nothing
/// is announced, nothing can pair).
#[cfg(feature = "p2p")]
#[test]
fn ar4_app_after_an_incomplete_setup_a_duress_session_runs_no_node() {
    let _kept = Kept;
    let r = &ar_runs().incomplete;
    assert!(r.complete, "the launch did not finish");
    let node = kv1(r, "duress.node");
    let stopped = kv1(r, "duress.old_node_stopped");
    assert!(
        node == "none" && stopped == "true",
        "C-AREA AR4 (phone): after an incomplete setup and a duress login, the P2P node is {node:?} (none expected), \
         the primary's stopped: {stopped}"
    );
}

#[test]
fn ar4_app_the_kek_is_cleared_or_replaced_at_every_persona_switch() {
    let _kept = Kept;
    let r = &ar_runs().switch;
    assert!(r.complete, "the launch did not finish");
    let mut problems = Vec::new();
    for (phase, want) in [("primary", "primary"), ("duress", "duress"), ("primary_again", "primary")] {
        let kek = kv1(r, &format!("{phase}.kek"));
        let ok = kek == want || (want == "duress" && kek == "none");
        if !ok {
            problems.push(format!("after the {phase} login the session holds the {kek} KEK"));
        }
        let dk = kv1(r, &format!("{phase}.device_key"));
        if dk != want {
            problems.push(format!("after the {phase} login the session holds the {dk} device key"));
        }
    }
    assert!(problems.is_empty(), "C-AREA AR4 (phone):\n  {}", problems.join("\n  "));
}

/// AR4 (v2.0.79): at a switch to the duress persona the primary's node stops
/// and none runs; at the switch back, the primary's node starts under its own
/// device key (the identity key the state holds; the node is started from
/// it, `sync_startup::start_p2p_node`).
#[cfg(feature = "p2p")]
#[test]
fn ar4_app_at_every_switch_the_duress_session_runs_no_node_and_the_primary_its_own() {
    let _kept = Kept;
    let r = &ar_runs().switch;
    assert!(r.complete, "the launch did not finish");
    let mut problems = Vec::new();
    let (node, stopped) = (kv1(r, "duress.node"), kv1(r, "duress.old_node_stopped"));
    if node != "none" || stopped != "true" {
        problems.push(format!("at the duress login the node is {node:?} (none expected), the primary's stopped: {stopped}"));
    }
    let node = kv1(r, "primary_again.node");
    if node != "started" && node != "restarted" {
        problems.push(format!("back to the primary, its node is {node:?} (started expected)"));
    }
    let dk = kv1(r, "primary_again.device_key");
    if dk != "primary" {
        problems.push(format!("back to the primary, the session holds the {dk} device key"));
    }
    assert!(problems.is_empty(), "C-AREA AR4 (phone):\n  {}", problems.join("\n  "));
}

/// The one stated exception to AR4's « after the switch, no file of the primary's changes » (the C-AREA
/// test author, 2026-11-14, on the L1c implementer's measure, claude-coord from-windows/0263):
/// - **with the RocksDB engine (the desktop feature set),** SurrealDB 2.6.5 never releases a RocksDB
///   store inside the process. After an in-process switch, its own housekeeping (a node heartbeat
///   every 3 s, a compaction lease every 5 s) writes the primary store's files under
///   `data/sovereign.db/`. Closing the store would break the next primary login.
/// - **No shipped host switches in process on that engine:** the desktop Tauri app is frozen, and the
///   shell does not switch in process.
/// - **What stays strict:** every other file of the primary's; the phone's engine (SurrealKV), whose
///   store closes; and `ar4_app_the_primarys_store_is_not_written_during_a_duress_session`, the duress
///   login without a switch, on both engines.
/// - **The limit goes to BACKLOG §16** (the release owner records it), for the day a desktop host
///   switches in process (C-SESS).
#[cfg(feature = "rocksdb")]
const ENGINE_HOUSEKEEPING_AFTER_SWITCH: Option<&str> = Some("data/sovereign.db/");
#[cfg(not(feature = "rocksdb"))]
const ENGINE_HOUSEKEEPING_AFTER_SWITCH: Option<&str> = None;

/// AR4 (v2.0.80): after a switch to the duress persona, nothing the primary's
/// session started acts: the forced purge tick and the approval of the
/// primary's pending proposal change none of the primary's files, no action
/// runs, and a primary session afterwards finds its lane under its own name.
/// The primary has a document 31 days in the trash by the service's clock
/// (`sovereign_core::clock::test_clock`, held and moved after the primary's own unlock tick): the
/// tick forced after the switch must not purge it — the primary finds it
/// afterwards, with its versions.
/// **Reading:** the primary's files are snapshotted after the switch (the
/// switch itself writing the primary's store is
/// `ar4_app_the_primarys_store_is_not_written_during_a_duress_session`).
#[test]
fn ar4_app_a_switch_ends_everything_the_previous_session_started() {
    let _kept = Kept;
    let a = ar_runs();
    let (r, p) = (&a.switch_ends, &a.switch_ends_primary);
    assert!(r.complete && p.complete, "a launch did not finish");
    assert_eq!(kv1(r, "primary.proposal_pending"), "true", "precondition: the primary had a proposal pending");
    assert!(kv1(r, "duress.login").contains("duress"), "precondition: the switch to duress");
    let mut problems = Vec::new();
    assert_eq!(kv1(r, "trashed.due"), "true", "precondition: the primary's trashed document is due for purge by the service's clock");
    let want = format!("present, {} versions", kv1(r, "trashed.versions"));
    let found = kv(p, "primary.trashed_doc");
    if found.first() != Some(&want) {
        problems.push(format!(
            "the primary's document, due for purge, was purged after the switch (the primary finds: {found:?}; {want} expected)"
        ));
    }
    let purge = kv1(r, "purge.result");
    if !purge.starts_with("Ok") {
        problems.push(format!("the forced purge tick did not run: {purge}"));
    }
    // C-SWEEP (added by the test author of lot LS(a)): the sweep's pass, forced
    // after the switch, ran, and the primary's files did not change (below).
    let sweep = kv1(r, "sweep.result");
    if !sweep.starts_with("Ok") {
        problems.push(format!("the forced pass of the personal-data sweep did not run: {sweep}"));
    }
    for w in kv(r, "primary.write") {
        if let Some(prefix) = ENGINE_HOUSEKEEPING_AFTER_SWITCH {
            if w.split_once(' ').is_some_and(|(_, rel)| rel.starts_with(prefix)) {
                eprintln!("C-AREA AR4 (desktop features): excused, the RocksDB engine's own housekeeping: {w}");
                continue;
            }
        }
        problems.push(format!("after the switch, the primary's files changed: {w}"));
    }
    for e in kv(r, "after_switch.executed") {
        problems.push(format!("after the switch, the primary's proposal could still be confirmed: {e}"));
    }
    let threads = kv(p, "primary.thread");
    if threads.iter().any(|t| t == "Renamed under duress") {
        problems.push("the primary's lane was renamed by its proposal, approved after the switch".to_string());
    }
    if !threads.iter().any(|t| t == PRIMARY_THREAD) {
        problems.push(format!("(precondition) the primary's lane is missing afterwards: {threads:?}"));
    }
    assert!(problems.is_empty(), "C-AREA AR4 (phone):\n  {}", problems.join("\n  "));
}

/// AR4 (coordinator, 2026-11-14, from the walk's finding): the primary's raw
/// store is not written during a duress session — every file of
/// `data/sovereign.db/` is as it was before the duress login.
#[test]
fn ar4_app_the_primarys_store_is_not_written_during_a_duress_session() {
    let _kept = Kept;
    let r = &ar_runs().store;
    assert!(r.complete, "the launch did not finish");
    assert!(kv1(r, "duress.login").contains("duress"), "precondition: the duress login opens");
    let writes = kv(r, "primary_store.write");
    assert!(
        writes.is_empty(),
        "C-AREA AR4 (phone): the duress session wrote the primary's store:\n  {}",
        writes.join("\n  ")
    );
}

// -------------------------------------------------------- open vs write

#[test]
fn area_app_the_duress_session_never_lists_the_primarys_objects() {
    let _kept = Kept;
    let r = &ar_runs().store;
    assert!(r.complete, "the launch did not finish");
    let docs = kv(r, "duress.doc");
    let threads = kv(r, "duress.thread");
    let seen: Vec<String> = docs
        .iter()
        .filter(|d| d.as_str() == PRIMARY_DOC)
        .map(|d| format!("the document {d:?}"))
        .chain(threads.iter().filter(|t| t.as_str() == PRIMARY_THREAD).map(|t| format!("the thread {t:?}")))
        .collect();
    assert!(seen.is_empty(), "C-AREA (phone), open vs write: the duress session lists {seen:?}");
    assert_eq!(
        kv1(r, "duress.unreadable"),
        "0",
        "C-AREA (phone), open vs write: rows the duress session's store met and could not decrypt (a listing skips \
         them, so they would not show above)"
    );
    let primary_ids: BTreeSet<String> = kv(&ar_runs().primary, "primary.id").into_iter().collect();
    assert!(!primary_ids.is_empty(), "precondition: the primary session reported its ids");
    let by_id: Vec<String> = kv(r, "duress.seen_id").into_iter().filter(|id| primary_ids.contains(id)).collect();
    assert!(by_id.is_empty(), "C-AREA (phone), open vs write: the duress session lists the primary's records {by_id:?}");
}

#[test]
fn area_app_a_primary_session_finds_nothing_the_duress_session_created() {
    let _kept = Kept;
    let r = &ar_runs().lists;
    assert!(r.complete, "the launch did not finish");
    let docs = kv(r, "primary.doc");
    let threads = kv(r, "primary.thread");
    let created: BTreeSet<String> = kv(&ar_runs().store, "duress.created_id").into_iter().collect();
    assert_eq!(created.len(), 2, "precondition: the duress session reported what it created");
    let mut found: Vec<String> = docs
        .iter()
        .filter(|d| d.as_str() == DURESS_DOC || DECOY_TITLES.contains(&d.as_str()))
        .chain(threads.iter().filter(|t| t.as_str() == DURESS_THREAD))
        .map(|n| format!("{n:?}"))
        .collect();
    found.extend(kv(r, "primary.seen_id").into_iter().filter(|id| created.contains(id)).map(|id| format!("the record {id}")));
    let unreadable = kv1(r, "primary.unreadable");
    if unreadable != "0" {
        found.push(format!("rows the primary's store met and could not decrypt: {unreadable} (0 expected)"));
    }
    assert!(found.is_empty(), "C-AREA (phone), open vs write: the primary finds the duress session's {found:?}");
    assert!(docs.iter().any(|d| d == PRIMARY_DOC), "precondition: the primary lists its own document: {docs:?}");
}

// ----------------------------------------------------------------- AR3

#[cfg(feature = "p2p")]
#[test]
fn ar3_app_a_duress_login_keeps_the_primarys_paired_list() {
    let _kept = Kept;
    let a = &ar_runs().ar3;
    assert!(a.duress.complete && a.primary.complete, "a launch did not finish");
    assert!(kv1(&a.duress, "duress.login").contains("duress"), "precondition: the duress login opens");
    assert_eq!(
        a.after_hash,
        kv1(&a.duress, "fixture.hash"),
        "C-AREA AR3 (phone): a duress login changed the primary's paired list"
    );
    let paired = kv(&a.primary, "primary.paired");
    assert!(
        paired.iter().any(|n| n == "Primary's paired device"),
        "C-AREA AR3 (phone): after a duress login, the primary's list no longer holds its device: {paired:?}"
    );
}

#[cfg(feature = "p2p")]
#[test]
fn ar3_app_a_list_that_fails_to_decrypt_is_kept() {
    let _kept = Kept;
    let a = &ar_runs().ar3;
    assert!(a.corrupt_primary.complete, "the primary session did not finish");
    assert_eq!(
        a.corrupt_after.as_deref(),
        Some(a.corrupt_before.as_slice()),
        "C-AREA AR3 (phone): a list that fails to decrypt was replaced or deleted by a primary login"
    );
    // « and the failure is reported »: the phone reports through
    // `list_paired_devices`, a Webview command; see the gated test below.
}

/// AR3 on the phone, through the session-level functions under the pairing
/// commands (claude-coord from-windows/0241 (a), 0242): their signatures are
/// the test author's, their bodies the implementer's.
/// - `generate_pair_qr_in(&AppState)`: a duress session arms an offer, a
///   second node pairs, and the primary's list keeps its bytes;
/// - `list_paired_devices_in(&AppState)`: over a list that fails to decrypt,
///   the result reports the failure (an `Err`, or a result naming it), never
///   an empty list as if nothing were paired.
///
/// Their launches run in `run_ar`, and may fail (a body not written yet
/// panics): the tests say so.
#[cfg(feature = "p2p")]
mod ar3_seams {
    use super::*;

    /// AR3, AR4 (v2.0.80, « Le même leurre que les gardiens »): a duress
    /// session cannot pair, and its pairing screen is the usual decoy. Asked
    /// for an offer, `generate_pair_qr_in` answers one of the real form (a
    /// payload that decodes as a pairing offer, not naming the primary's peer
    /// id, and a PIN of a real code's form); nothing is armed, so a second
    /// device trying it is refused. The primary's list keeps its bytes.
    #[test]
    fn ar3_app_a_duress_session_cannot_pair() {
        let _kept = Kept;
        let a = &ar_runs().ar3;
        assert!(
            a.pair_launch.is_ok(),
            "C-AREA AR3 (phone): the duress session's pairing gesture did not run: {}",
            a.pair_launch.as_ref().err().map(String::as_str).unwrap_or("")
        );
        let mut problems = Vec::new();
        let result = kv1(&a.pair, "pair.result");
        if !result.starts_with("Ok") {
            problems.push(format!("no decoy: generate_pair_qr_in gave no offer in a duress session: {result}"));
        }
        for (key, want) in [("pair.screen", "offer"), ("pair.pin_form", "real"), ("pair.offer_is_primarys", "false")] {
            let v = kv1(&a.pair, key);
            if v != want {
                problems.push(format!("the decoy: {key} = {v} ({want} expected)"));
            }
        }
        let join = kv1(&a.pair, "pair.join");
        if join == "paired" {
            problems.push("a second device paired with the duress session: something was armed".to_string());
        }
        if a.pair_after != a.pair_before {
            problems.push("the duress session's pairing changed the primary's list".to_string());
        }
        assert!(problems.is_empty(), "C-AREA AR3 (phone):\n  {}", problems.join("\n  "));
    }

    pub(super) fn child_pair() {
        let (st, rx) = boot();
        let l = login(&st, DURESS);
        assert!(l.is_ok(), "the duress login: {:?}", l.err());
        let qr = block(crate::tauri_commands::pairing::generate_pair_qr_in(&st));
        out(&format!("KV\tpair.result\t{}", match &qr {
            Ok(_) => "Ok: an offer".to_string(),
            Err(e) => format!("Err: {e}"),
        }));
        let real = sovereign_crypto::pair_payload::generate_pairing_code();
        let shape = |s: &str| -> Vec<bool> { s.chars().map(|c| c.is_ascii_alphanumeric()).collect() };
        let primary_peer = sovereign_crypto::auth::AuthStore::load(&crate::setup::crypto_dir().join("auth.store"))
            .ok()
            .and_then(|s| s.authenticate(MAIN.as_bytes()).ok())
            .and_then(|a| sovereign_p2p::identity::derive_keypair(&a.device_key).ok())
            .map(|kp| kp.public().to_peer_id().to_string())
            .unwrap_or_default();
        if let Ok(qr) = &qr {
            let form = if qr.pin.len() == real.len() && shape(&qr.pin) == shape(&real) { "real" } else { "other" };
            out(&format!("KV	pair.pin_form	{form}"));
        }
        let join = match &qr {
            Err(_) => "no offer".to_string(),
            Ok(qr) => match sovereign_p2p::PairingOffer::decode(&qr.qr_payload_b64) {
                Err(_) => {
                    out("KV	pair.screen	not an offer");
                    "the payload is not an offer".to_string()
                }
                Ok(offer) => {
                    out("KV	pair.screen	offer");
                    out(&format!("KV	pair.offer_is_primarys	{}", !primary_peer.is_empty() && offer.source_peer_id == primary_peer));
                    let mk = sovereign_crypto::master_key::MasterKey::generate();
                    let dk = sovereign_crypto::device_key::DeviceKey::derive(&mk, "walk-second-device").expect("a device key");
                    let peer = sovereign_p2p::identity::derive_keypair(&dk).expect("a keypair").public().to_peer_id().to_string();
                    match block(sovereign_p2p::pairing_client::pair_with_source(
                        &offer,
                        &qr.pin,
                        "Walk second device",
                        |_| Ok(peer),
                        Duration::from_secs(15),
                    )) {
                        Ok(_) => "paired".to_string(),
                        Err(e) => format!("refused: {e}"),
                    }
                }
            },
        };
        out(&format!("KV\tpair.join\t{join}"));
        std::thread::sleep(Duration::from_millis(1500));
        out("END");
        std::mem::forget((st, rx));
    }

    #[test]
    fn ar3_app_a_list_that_fails_to_decrypt_is_reported() {
        let _kept = Kept;
        let a = &ar_runs().ar3;
        assert!(a.list_launch.is_ok(), "C-AREA AR3 (phone): the listing did not run: {}", a.list_launch.as_ref().err().map(String::as_str).unwrap_or(""));
        assert_eq!(
            kv1(&a.list, "list.reported"),
            "true",
            "C-AREA AR3 (phone): over a list that fails to decrypt, the listing says nothing of it: {:?}",
            kv(&a.list, "list.result")
        );
    }

    pub(super) fn child_list() {
        let (st, rx) = boot();
        let _ = login(&st, MAIN);
        let r = block(crate::tauri_commands::pairing::list_paired_devices_in(&st));
        out(&format!("KV\tlist.result\t{}", shown(&r)));
        let reported = match &r {
            Err(_) => true,
            Ok(v) => {
                let t = serde_json::to_string(v).unwrap_or_default().to_lowercase();
                t.contains("decrypt") || t.contains("unreadable") || t.contains("not be read")
            }
        };
        out(&format!("KV\tlist.reported\t{reported}"));
        out("END");
        std::mem::forget((st, rx));
    }
}

// ----------------------------------------------------------------- AR7

/// AR7 (phone): the duress walk's process writes nothing in its temporary
/// directory (each launch has its own: `TMP`/`TEMP`/`TMPDIR`). The phone has
/// no embedded browser as built (its browser commands are desktop only).
#[test]
fn ar7_app_the_duress_walk_writes_nothing_in_the_temporary_directory() {
    let _kept = Kept;
    let w = walks();
    assert!(w.walk.complete, "the walk did not finish");
    let writes: Vec<String> = w.walk.temp_writes.iter().map(|(k, r)| format!("{k} {r}")).collect();
    assert!(
        writes.is_empty(),
        "C-AREA AR7 (phone): the duress walk wrote {} path(s) in the temporary directory:\n  {}",
        writes.len(),
        writes.join("\n  ")
    );
}

/// The index of the first character of the paired list's ciphertext (the
/// file is pretty-printed JSON: `"ciphertext": "<base64>"`). Used by p2p builds.
#[cfg_attr(not(feature = "p2p"), allow(dead_code))]
fn ciphertext_at(text: &str) -> Option<usize> {
    let k = text.find("\"ciphertext\"")? + "\"ciphertext\"".len();
    let colon = k + text[k..].find(':')?;
    Some(colon + text[colon..].find('"')? + 1)
}

// =========================================================== l1c-tests-v7

/// **Seventh round (`l1c-tests-v7`, 2026-11-14; spec v2.0.87-88, ch. 07, AR2;
/// claude-coord from-fable-2nde-position/0037, 0038, 0039, from-windows/0267,
/// from-laptop/0261).**
///
/// - **Before any login, on a device that has an account** (v2.0.87-88: « no
///   command answers or writes a persona's identity, on either host. A
///   process with no session has no identity directory. … The commands that
///   create an account or answer its designation are refused once the device
///   has an account, inside a duress session too. A check that reads no file
///   and names no persona still answers: the new password's policy, which
///   recovery uses from the login screen. »). Each command runs in a fresh
///   process of its own, on a copy of the names sequence's profile (both
///   personas have names of their own); `get_profile` again after a failed
///   login, which opens no session.
/// - **What stays allowed:** `validate_password_policy` before any login;
///   `onboarding_designation` on a device with no account (the setup wizard).
/// - **An install set up before v0.0.13** (« the same draw happens at the
///   first duress login that finds no names of its own »): an account with
///   the shared `user_profile.json` as v0.0.12 wrote it (read at tag
///   `v0.0.12`: the phone kept one profile, in the data directory, for both
///   personas), the primary's assistant renamed, and no duress names. The
///   phone has no device name of a persona's own (it is the configuration's),
///   so only the profile's names are asserted here.
///
/// **Readings** (reported with the tests):
/// - « Refused » is an `Err`; « answers nothing of either persona » is an
///   answer that holds none of the two personas' values (designation,
///   nickname, profile id, display name).
/// - « Changes no file »: before any login, any file created, changed or
///   deleted, except AR2's exceptions that may change; inside a duress
///   session (whose own background work may write its own area), any write
///   outside the duress persona's area other than those exceptions, and any
///   `user_profile.json` anywhere.
/// - « After the interface's reset at login » (`resetSessionState`): the
///   reset runs once a login has opened a session, so the state it leaves is
///   a session, not « before any login »; the backend's second check is after
///   a failed login, which leaves the process with no session. The reset's
///   own test is the interface's (`c_area_reset.svelte.test.ts`).
/// - « Drawn at random »: on two installs set up the same way, the first
///   duress login's names differ. « The names » are those setup draws
///   (`create_duress_identity`: designation and nickname).
mod v7 {
    use super::*;

    const BEFORE_LOGIN: &str = "Before any login";
    /// A new password a recovery would check from the login screen.
    const NEW_PASSWORD: &str = "Recovery!NewPass2026";

    const OLD_PRIMARY_DESIGNATION: &str = "Ikshal-V012-Ω";
    const OLD_PRIMARY_NICKNAME: &str = "Old primary nickname";
    const OLD_PRIMARY_USER_ID: &str = "5f0c0012-0000-4000-8000-000000000012";

    /// `user_profile.json` as v0.0.12 wrote it in the phone's data directory,
    /// shared by the personas (`UserProfile::save`: `serde_json::to_string_pretty`
    /// of the same struct), after the primary named its assistant.
    const V0012_USER_PROFILE: &str = r#"{
  "user_id": "5f0c0012-0000-4000-8000-000000000012",
  "designation": "Ikshal-V012-Ω",
  "nickname": "Old primary nickname",
  "bubble_style": "wave",
  "display_name": null,
  "theme": "light",
  "created": "2026-10-20T08:00:00.000000+00:00",
  "last_updated": "2026-11-03T09:30:00.000000+00:00",
  "interaction_patterns": {
    "suggestion_receptiveness": 0.5,
    "command_verbosity": "detailed"
  },
  "skill_preferences": {},
  "suggestion_feedback": {}
}"#;

    /// The paired setup's input as the wizard sends it, every confirmation
    /// matching; the offer is not a real one (no source device in a test).
    fn paired_input() -> crate::tauri_commands::pairing::CompletePairedOnboardingInput {
        crate::tauri_commands::pairing::CompletePairedOnboardingInput {
            qr_payload_b64: "not-a-real-offer".into(),
            pin: "000000".into(),
            password: MAIN.into(),
            password_confirm: Some(MAIN.into()),
            duress_password: Some(DURESS.into()),
            duress_confirm: Some(DURESS.into()),
            nickname: Some("Tester".into()),
            bubble_style: None,
            canary_phrase: None,
            canary_confirm: None,
            seed_sample_data: false,
        }
    }

    /// Runs `f`, and reports its result (`{prefix}.{name}.result`) and every
    /// path it created, changed or deleted under the Sovereign directory
    /// (`{prefix}.{name}.write`).
    fn run_command<T: serde::Serialize>(prefix: &str, name: &str, root: &Path, f: impl FnOnce() -> Result<T, String>) {
        let before = snapshot(root);
        let r = f();
        std::thread::sleep(Duration::from_millis(300));
        out(&format!("KV\t{prefix}.{name}.result\t{}", shown(&r)));
        for (kind, rel) in diff(&before, &snapshot(root)) {
            out(&format!("KV\t{prefix}.{name}.write\t{kind} {rel}"));
        }
    }

    /// One command before any login, in a fresh process (no session has ever
    /// opened in it). `get_profile` is asked again after a failed login.
    pub(super) fn child_before_login(command: &str) {
        use crate::tauri_commands::{auth, pairing};
        let root = sovereign_core::sovereign_dir();
        let (st, rx) = boot();
        let s = || state(&st);
        match command {
            "get_profile" => {
                run_command("bl", "get_profile.fresh", &root, || block(auth::get_profile(s())));
                let failed = block(auth::validate_password(s(), "Not-the-password-0".into(), vec![]));
                out(&format!("KV\tbl.failed_login\t{}", shown(&failed)));
                run_command("bl", "get_profile.after_failed_login", &root, || block(auth::get_profile(s())));
            }
            "save_profile" => run_command("bl", "save_profile", &root, || {
                block(auth::save_profile(
                    s(),
                    crate::tauri_commands::SaveProfileDto {
                        nickname: Some(BEFORE_LOGIN.into()),
                        bubble_style: None,
                        display_name: Some(BEFORE_LOGIN.into()),
                    },
                ))
            }),
            "onboarding_designation" => run_command("bl", "onboarding_designation", &root, || block(auth::onboarding_designation(s()))),
            "validate_password_policy" => {
                run_command("bl", "validate_password_policy", &root, || block(auth::validate_password_policy(s(), NEW_PASSWORD.into())))
            }
            "complete_onboarding" => {
                run_command("bl", "complete_onboarding", &root, || block(auth::complete_onboarding(s(), data(MAIN, Some(DURESS), None))))
            }
            "complete_onboarding_paired" => run_command("bl", "complete_onboarding_paired", &root, || {
                block(pairing::complete_onboarding_paired(s(), paired_input()))
            }),
            other => panic!("unknown command {other}"),
        }
        out("END");
        std::mem::forget((st, rx));
    }

    /// The three commands that create an account or answer its designation,
    /// inside a duress session.
    pub(super) fn child_in_duress() {
        use crate::tauri_commands::{auth, pairing};
        let root = sovereign_core::sovereign_dir();
        let (st, rx) = boot();
        for e in duress_area(&st.config) {
            out(&format!("AREA\t{}\t{}\t{}", e.rel, e.dir, e.source));
        }
        let l = login(&st, DURESS);
        out(&format!("KV\tdd.login\t{}", shown(&l)));
        // The session's background work (the one-time migration) settles.
        std::thread::sleep(Duration::from_millis(2500));
        let s = || state(&st);
        run_command("dd", "onboarding_designation", &root, || block(auth::onboarding_designation(s())));
        run_command("dd", "complete_onboarding", &root, || block(auth::complete_onboarding(s(), data(MAIN, Some(DURESS), None))));
        run_command("dd", "complete_onboarding_paired", &root, || block(pairing::complete_onboarding_paired(s(), paired_input())));
        out("END");
        std::mem::forget((st, rx));
    }

    /// `onboarding_designation` on a device with no account: the setup wizard's.
    pub(super) fn child_no_account() {
        let root = sovereign_core::sovereign_dir();
        let (st, rx) = boot();
        run_command("na", "onboarding_designation", &root, || block(crate::tauri_commands::auth::onboarding_designation(state(&st))));
        out("END");
        std::mem::forget((st, rx));
    }

    /// An account as v0.0.12 left it on the phone: `auth.store` and the salt
    /// (`child_store_only`), the setup's marker, and the shared
    /// `user_profile.json` as v0.0.12 wrote it, the primary's names in it. No
    /// duress names: v0.0.12 drew none.
    pub(super) fn child_old_install() {
        child_store_only();
        let base = sovereign_core::sovereign_dir();
        std::fs::write(base.join("user_profile.json"), V0012_USER_PROFILE).expect("v0.0.12's shared profile");
        std::fs::write(base.join("onboarding_done"), "1").expect("the setup's marker");
        out("END");
    }

    /// v9 (DEL-0010): the duress persona's identity file made unreadable (not
    /// valid JSON) before the launch; a duress login; `get_profile` twice; the
    /// file's bytes before and after.
    pub(super) fn child_unreadable_duress() {
        let base = sovereign_core::sovereign_dir();
        let file = crate::tauri_state::persona_identity_dir(&base, Some(sovereign_core::auth::PersonaKind::Duress))
            .join("user_profile.json");
        out(&format!("KV\tud.file_existed\t{}", file.exists()));
        std::fs::write(&file, b"{ not a profile").expect("the duress profile, made unreadable");
        let before = std::fs::read(&file).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable: {e}"));
        let (st, rx) = boot();
        let l = login(&st, DURESS);
        out(&format!("KV\tud.login\t{}", shown(&l)));
        for n in 1..=2 {
            let r = block(crate::tauri_commands::auth::get_profile(state(&st)));
            out(&format!("KV\tud.get_profile{n}\t{}", shown(&r)));
        }
        let after = std::fs::read(&file).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable: {e}"));
        out(&format!("KV\tud.file_before\t{before}"));
        out(&format!("KV\tud.file_after\t{after}"));
        out("END");
        std::mem::forget((st, rx));
    }

    struct V7 {
        launches: BTreeMap<String, (Result<(), String>, Run)>,
    }

    const BEFORE_LOGIN_COMMANDS: [&str; 6] = [
        "get_profile",
        "save_profile",
        "onboarding_designation",
        "validate_password_policy",
        "complete_onboarding",
        "complete_onboarding_paired",
    ];

    fn runs() -> &'static V7 {
        static V: OnceLock<Result<V7, String>> = OnceLock::new();
        match V.get_or_init(|| std::panic::catch_unwind(run).map_err(|p| panic_text(&p))) {
            Ok(v) => v,
            Err(e) => panic!("the v7 launches could not run: {e}"),
        }
    }

    fn run() -> V7 {
        let w = walks();
        let base = w.base.clone();
        let mut launches = BTreeMap::new();
        let mut one = |profile: &Path, scenario: &str, tag: &str| {
            let o = base.join(format!("v7-{tag}.out"));
            let r = launch_soft(scenario, profile, &o);
            launches.insert(tag.to_string(), (r, parse(&o)));
        };
        for cmd in BEFORE_LOGIN_COMMANDS {
            let p = base.join(format!("v7-bl-{cmd}"));
            copy_dir(&base.join("names"), &p);
            one(&p, &format!("bl_{cmd}"), &format!("bl_{cmd}"));
        }
        let d = base.join("v7-in-duress");
        copy_dir(&base.join("names"), &d);
        one(&d, "in_duress", "in_duress");
        // v9 (DEL-0010): a duress profile made unreadable.
        let u = base.join("v9-unreadable-duress");
        copy_dir(&base.join("names"), &u);
        one(&u, "unreadable_duress", "unreadable_duress");
        // v10: the primary's real guardian offer (p2p builds), on a copy of P0.
        #[cfg(feature = "p2p")]
        {
            let g = base.join("v10-guardian-primary");
            copy_dir(&base.join("p0"), &g);
            one(&g, "guardian_primary", "guardian_primary");
        }
        let n = base.join("v7-no-account");
        std::fs::create_dir_all(&n).unwrap();
        one(&n, "no_account", "no_account");
        let old_a: &[(&str, &str)] = &[("old_install", "old_install_a"), ("old_d1", "o_d1"), ("old_d2", "o_d2"), ("old_p", "o_p")];
        let old_b: &[(&str, &str)] = &[("old_install", "old_install_b"), ("old_b", "o_b")];
        for (dir, steps) in [("v7-old-a", old_a), ("v7-old-b", old_b)] {
            let p = base.join(dir);
            std::fs::create_dir_all(&p).unwrap();
            for (scenario, tag) in steps {
                one(&p, scenario, tag);
            }
        }
        V7 { launches }
    }

    /// v10: the launch `tag`'s run, for another module of this file.
    #[cfg_attr(not(feature = "p2p"), allow(dead_code))]
    pub(super) fn launched_run(tag: &str) -> &'static Run {
        launched(tag)
    }

    /// The launch `tag`'s run; panics, with the child's output, if it did not run.
    fn launched(tag: &str) -> &'static Run {
        let (r, run) = runs().launches.get(tag).unwrap_or_else(|| panic!("no launch {tag}"));
        if let Err(e) = r {
            panic!("C-AREA (phone, v7): the launch `{tag}` did not run:\n{e}");
        }
        assert!(run.complete, "C-AREA (phone, v7): the launch `{tag}` did not finish");
        run
    }

    /// A persona's names, as its own sessions of the names sequence reported them.
    struct Names {
        designation: String,
        nickname: String,
        user_id: String,
        display_name: String,
    }

    /// The primary's and the duress persona's names at the end of the names
    /// sequence (`names_p2`; `names_d2`, after its rename).
    fn persona_names() -> (Names, Names) {
        let w = walks();
        let (d2, p2) = (&w.names[2], &w.names[3]);
        for r in [d2, p2] {
            assert!(r.complete, "precondition: the names sequence finished");
        }
        let primary = Names {
            designation: kv1(p2, "p2.designation"),
            nickname: kv1(p2, "p2.nickname"),
            user_id: kv1(p2, "p2.user_id"),
            display_name: kv1(p2, "p2.display_name"),
        };
        let duress = Names {
            designation: kv1(d2, "d2.designation"),
            nickname: kv1(d2, "d2.nickname_after"),
            user_id: kv1(d2, "d2.user_id"),
            display_name: kv1(d2, "d2.display_name"),
        };
        (primary, duress)
    }

    /// Whose values `shown` holds, if any.
    fn whose(shown: &str, p: &Names, d: &Names) -> Vec<String> {
        let mut out = Vec::new();
        for (who, n) in [("the primary's", p), ("the duress persona's", d)] {
            for (what, v) in [
                ("designation", n.designation.as_str()),
                ("nickname", n.nickname.as_str()),
                ("profile id", n.user_id.as_str()),
                ("display name", n.display_name.as_str()),
            ] {
                if !v.is_empty() && v != "<not reported>" && shown.contains(v) {
                    out.push(format!("{who} {what} {v:?}"));
                }
            }
        }
        out
    }

    fn refused(result: &str) -> bool {
        result.starts_with("Err")
    }

    /// The writes reported for `{key}` before any login: every one, but AR2's
    /// exceptions that may change.
    fn writes_before_login(r: &Run, key: &str) -> Vec<String> {
        kv(r, &format!("{key}.write"))
            .into_iter()
            .filter(|w| exception(w.split_once(' ').map(|(_, rel)| rel).unwrap_or("")) != Some(May::Change))
            .map(|w| {
                let rel = w.split_once(' ').map(|(_, rel)| rel.to_string()).unwrap_or_default();
                format!("{w}{}", undecided_note(&rel))
            })
            .collect()
    }

    /// The writes reported for `{key}` inside the duress session `r` that the
    /// command is answerable for: outside the duress area (but AR2's
    /// exceptions that may change), and any `user_profile.json`.
    fn writes_in_duress(r: &Run, key: &str) -> Vec<String> {
        let area = area_of(r);
        kv(r, &format!("{key}.write"))
            .into_iter()
            .filter(|w| {
                let rel = w.split_once(' ').map(|(_, rel)| rel).unwrap_or("");
                rel.ends_with("user_profile.json") || (!in_area(rel, &area) && exception(rel) != Some(May::Change))
            })
            .collect()
    }

    /// AR2 (v2.0.87-88): before any login, on a device that has an account,
    /// `get_profile` is refused, or answers nothing of either persona; from a
    /// fresh process, and again after a failed login. It writes nothing.
    #[test]
    fn ar2_app_before_any_login_get_profile_answers_nothing_of_either_persona() {
        let _kept = Kept;
        let r = launched("bl_get_profile");
        let (p, d) = persona_names();
        assert!(refused(&kv1(r, "bl.failed_login")), "precondition: the wrong password is refused: {}", kv1(r, "bl.failed_login"));
        let mut problems = Vec::new();
        for (phase, when) in [("get_profile.fresh", "in a fresh process"), ("get_profile.after_failed_login", "after a failed login")] {
            let result = kv1(r, &format!("bl.{phase}.result"));
            if !refused(&result) {
                let found = whose(&result, &p, &d);
                if !found.is_empty() {
                    problems.push(format!("{when}, get_profile answers {}: {result}", found.join(", ")));
                }
            }
            for w in writes_before_login(r, &format!("bl.{phase}")) {
                problems.push(format!("{when}, get_profile wrote: {w}"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone), before any login on a device that has an account (v2.0.87-88: « no command answers or \
             writes a persona's identity … A process with no session has no identity directory »):\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR2 (v2.0.87-88): before any login, on a device that has an account,
    /// `save_profile` (a nickname and a display name) changes no file of
    /// either persona.
    #[test]
    fn ar2_app_before_any_login_save_profile_changes_no_file_of_either_persona() {
        let _kept = Kept;
        let r = launched("bl_save_profile");
        let writes = writes_before_login(r, "bl.save_profile");
        assert!(
            writes.is_empty(),
            "C-AREA AR2 (phone): before any login, on a device that has an account, save_profile (answered {}) wrote:\n  {}",
            kv1(r, "bl.save_profile.result"),
            writes.join("\n  ")
        );
    }

    /// AR2 (v2.0.88, from-fable-2nde-position/0038): on a device that has an
    /// account, `onboarding_designation` is refused before any login and
    /// inside a duress session, and in neither case changes a file.
    #[test]
    fn ar2_app_onboarding_designation_is_refused_on_a_device_with_an_account() {
        let _kept = Kept;
        let (bl, dd) = (launched("bl_onboarding_designation"), launched("in_duress"));
        assert!(kv1(dd, "dd.login").contains("duress"), "precondition: the duress login opens: {}", kv1(dd, "dd.login"));
        let (p, d) = persona_names();
        let mut problems = Vec::new();
        for (when, result, writes) in [
            ("before any login", kv1(bl, "bl.onboarding_designation.result"), writes_before_login(bl, "bl.onboarding_designation")),
            ("inside a duress session", kv1(dd, "dd.onboarding_designation.result"), writes_in_duress(dd, "dd.onboarding_designation")),
        ] {
            if !refused(&result) {
                let found = whose(&result, &p, &d);
                let named = if found.is_empty() { String::new() } else { format!(" ({})", found.join(", ")) };
                problems.push(format!("{when}, onboarding_designation answers {result}{named}: it is not refused"));
            }
            for w in writes {
                problems.push(format!("{when}, onboarding_designation wrote: {w}"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone), v2.0.88 (« The commands that create an account or answer its designation are refused \
             once the device has an account, inside a duress session too »):\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR2 (v2.0.88): « Setup, on a device with no account yet, is the one case
    /// that writes an identity before any login »: there,
    /// `onboarding_designation` still answers a designation (the wizard shows
    /// it), so a fix cannot simply remove the command.
    #[test]
    fn ar2_app_onboarding_designation_still_answers_on_a_device_with_no_account() {
        let _kept = Kept;
        let r = launched("no_account");
        let result = kv1(r, "na.onboarding_designation.result");
        assert!(
            !refused(&result) && !result.trim_matches('"').trim().is_empty(),
            "C-AREA AR2 (phone): on a device with no account, onboarding_designation gives the setup wizard no \
             designation: {result}"
        );
    }

    /// AR2 (v2.0.88, from-fable-2nde-position/0039): « A check that reads no
    /// file and names no persona still answers: the new password's policy,
    /// which recovery uses from the login screen. » Before any login, on a
    /// device that has an account, `validate_password_policy` answers, and
    /// writes nothing.
    #[test]
    fn ar2_app_the_password_policy_check_still_answers_before_any_login() {
        let _kept = Kept;
        let r = launched("bl_validate_password_policy");
        let result = kv1(r, "bl.validate_password_policy.result");
        let mut problems = Vec::new();
        if refused(&result) {
            problems.push(format!("it is refused: {result}"));
        }
        for w in kv(r, "bl.validate_password_policy.write") {
            problems.push(format!("it wrote: {w}"));
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone): before any login, on a device that has an account, validate_password_policy (recovery's \
             check of the new password, from the login screen):\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR2 (v2.0.88): the two commands that create an account,
    /// `complete_onboarding` and `complete_onboarding_paired`, are refused
    /// once the device has an account, before any login and inside a duress
    /// session, and change no file.
    #[test]
    fn ar2_app_the_account_creating_commands_are_refused_once_the_device_has_an_account() {
        let _kept = Kept;
        let dd = launched("in_duress");
        assert!(kv1(dd, "dd.login").contains("duress"), "precondition: the duress login opens: {}", kv1(dd, "dd.login"));
        let mut problems = Vec::new();
        for cmd in ["complete_onboarding", "complete_onboarding_paired"] {
            let bl = launched(&format!("bl_{cmd}"));
            for (when, result, writes) in [
                ("before any login", kv1(bl, &format!("bl.{cmd}.result")), writes_before_login(bl, &format!("bl.{cmd}"))),
                ("inside a duress session", kv1(dd, &format!("dd.{cmd}.result")), writes_in_duress(dd, &format!("dd.{cmd}"))),
            ] {
                if !refused(&result) {
                    problems.push(format!("{when}, {cmd} is not refused: {result}"));
                }
                for w in writes {
                    problems.push(format!("{when}, {cmd} wrote: {w}"));
                }
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone), v2.0.88 (« The commands that create an account … are refused once the device has an \
             account, inside a duress session too »):\n  {}",
            problems.join("\n  ")
        );
    }

    /// v9, DEL-0010 (the Fable maille, by delegation of Céline, 2026-11-14:
    /// « (not set) », as the primary's; the file left as it is), on the phone,
    /// where `identity_profile` returns the read error instead. The duress
    /// persona's identity file is made unreadable; in a duress session,
    /// `get_profile` twice: neither answer holds anything of the primary's,
    /// the two answers are the same (no designation drawn at each reading),
    /// and the file is byte for byte as it was.
    #[test]
    fn del0010_app_an_unreadable_duress_profile_answers_nothing_of_the_primarys_and_is_left_as_it_is() {
        let _kept = Kept;
        let r = launched("unreadable_duress");
        assert_eq!(kv1(r, "ud.file_existed"), "true", "precondition: the duress persona had an identity file of its own");
        assert!(kv1(r, "ud.login").contains("duress"), "precondition: the duress login opens: {}", kv1(r, "ud.login"));
        let (p, _) = persona_names();
        let nobody = Names { designation: String::new(), nickname: String::new(), user_id: String::new(), display_name: String::new() };
        let mut problems = Vec::new();
        let (a1, a2) = (kv1(r, "ud.get_profile1"), kv1(r, "ud.get_profile2"));
        for (n, a) in [(1, &a1), (2, &a2)] {
            let found = whose(a, &p, &nobody);
            if !found.is_empty() {
                problems.push(format!("get_profile, reading {n}, answers {}: {a}", found.join(", ")));
            }
        }
        if a1 != a2 {
            problems.push(format!("the two readings differ (a profile drawn at each reading?): {a1} / {a2}"));
        }
        let (before, after) = (kv1(r, "ud.file_before"), kv1(r, "ud.file_after"));
        if before != after {
            problems.push(format!("the unreadable identity file changed: {before} -> {after}"));
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone), DEL-0010: a duress profile that cannot be read:\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR2 (v2.0.87: « An install set up before v0.0.13 never passes through
    /// setup again: there, the same draw happens at the first duress login
    /// that finds no names of its own »), on the phone. Two installs as
    /// v0.0.12 left them (`old_install`). On the first: two duress logins,
    /// then a primary one; on the second, one duress login. The first duress
    /// login's designation and nickname are drawn (present, and different on
    /// the two installs), differ from the primary's and copy nothing of it
    /// (its profile id neither); the second duress login keeps them; the
    /// primary's stay as v0.0.12 left them.
    #[test]
    fn ar2_app_an_install_set_up_before_v0013_draws_the_duress_names_at_its_first_duress_login() {
        let _kept = Kept;
        for t in ["old_install_a", "old_install_b"] {
            launched(t);
        }
        let (a1, a2, ap, b1) = (launched("o_d1"), launched("o_d2"), launched("o_p"), launched("o_b"));
        let get = |r: &Run, tag: &str, key: &str| kv1(r, &format!("{tag}.{key}"));
        let mut problems = Vec::new();
        let old = [
            ("designation", OLD_PRIMARY_DESIGNATION),
            ("nickname", OLD_PRIMARY_NICKNAME),
            ("user_id", OLD_PRIMARY_USER_ID),
        ];
        for (key, want) in old {
            let v = get(ap, "o_p", key);
            if v != want {
                problems.push(format!("the primary's {key} after the duress logins is {v:?} ({want:?}, as v0.0.12 left it, expected)"));
            }
        }
        for key in ["designation", "nickname"] {
            let v = get(a1, "o_d1", key);
            if v.is_empty() {
                problems.push(format!("the first duress login drew no {key}"));
                continue;
            }
            let other = get(b1, "o_b", key);
            if v == other {
                problems.push(format!(
                    "the duress {key} is {v:?} on two installs set up apart: not drawn at random (a default, or a value both share)"
                ));
            }
        }
        for (key, primary_old) in old {
            let v = get(a1, "o_d1", key);
            if !v.is_empty() && (v == primary_old || v == get(ap, "o_p", key)) {
                problems.push(format!("the duress {key} is the primary's: {v:?}"));
            }
            let again = get(a2, "o_d2", key);
            if again != v {
                problems.push(format!("a second duress login changed the duress {key}: {v:?} -> {again:?}"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (phone), an install set up before v0.0.13, at its first duress login:\n  {}",
            problems.join("\n  ")
        );
    }
}

// ========================================================== l1c-tests-v10

/// **Tenth round (`l1c-tests-v10`, 2026-11-15; claude-coord
/// from-fable-2nde-position/0062, from-windows/0302, from-laptop/0285).**
/// Ch. 07, *After login* (Céline, 2026-10-17): in a duress session, guardian
/// enrollment « appears to work, waits for the guardian, and never
/// completes »; AR4: « Guardian enrollment keeps its decoy waiting screen. »
/// Neither names a host. On the phone, `begin_guardian_enrollment` had no
/// duress branch: in a duress session it stopped with an error.
///
/// The walk drives `guardian::begin_guardian_enrollment_in` (the function
/// under the command; signature by the test author, body the implementer's)
/// in the duress session it already opens, inside its snapshot, in builds
/// with `p2p` (the phone's); a launch of its own opens a primary session's
/// real offer on a copy of P0.
///
/// **Readings:**
/// - « An offer of the real form »: `Ok`, a payload that decodes as a
///   `GuardianEnrollOffer` not naming the primary's peer id, a code of a real
///   code's form, a shard id, and `total` the roster's size; the same fields a
///   real enrollment fills.
/// - « Nothing is armed »: no node runs in the session that answered (no
///   offer can be handed to one), and nothing under `recovery/` is written
///   (AR2: the primary's only); AR1's snapshot checks every other path.
/// - The stretch, held by structure: the count of the one stretch both paths
///   share (`guardian::stretch_enroll_key`, counted by
///   `guardian::enroll_key_stretches`), around each offer.
#[cfg(feature = "p2p")]
mod v10 {
    use super::*;

    fn count() -> Result<usize, String> {
        std::panic::catch_unwind(crate::tauri_commands::guardian::enroll_key_stretches).map_err(|p| panic_text(&p))
    }

    /// Asks `st`'s session for a guardian-enrollment offer, as the Recovery
    /// tab does, and reports under `prefix` what came back, its form, the
    /// stretches run, and whether a node runs. Returns what the session shows
    /// (its form, stable from one run to the next, for AR6) and whether it
    /// answered an offer.
    pub(super) fn drive_guardian(st: &AppState, prefix: &str) -> (String, Result<(), String>) {
        let before = count();
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block(crate::tauri_commands::guardian::begin_guardian_enrollment_in(st))
        }))
        .map_err(|p| format!("not known: {}", panic_text(&p)))
        .and_then(|r| r.map_err(|e| format!("Err: {e}")));
        let stretches = match (before, count()) {
            (Ok(a), Ok(b)) => b.saturating_sub(a).to_string(),
            (Err(e), _) | (_, Err(e)) => format!("not known: {e}"),
        };
        out(&format!("KV\t{prefix}.stretches\t{stretches}"));
        out(&format!("KV\t{prefix}.node_running\t{}", node_tx(st).is_some()));
        let primary_peer = sovereign_crypto::auth::AuthStore::load(&crate::setup::crypto_dir().join("auth.store"))
            .ok()
            .and_then(|s| s.authenticate(MAIN.as_bytes()).ok())
            .and_then(|a| sovereign_p2p::identity::derive_keypair(&a.device_key).ok())
            .map(|kp| kp.public().to_peer_id().to_string())
            .unwrap_or_default();
        match r {
            Err(e) => {
                out(&format!("KV\t{prefix}.result\t{e}"));
                (e.clone(), Err(e))
            }
            Ok(b) => {
                out(&format!("KV\t{prefix}.result\tOk"));
                let offer = sovereign_p2p::guardian_enroll::GuardianEnrollOffer::decode(&b.qr_payload_b64);
                let real = sovereign_crypto::pair_payload::generate_pairing_code();
                let shape = |s: &str| -> Vec<bool> { s.chars().map(|c| c.is_ascii_alphanumeric()).collect() };
                let form = if b.code.len() == real.len() && shape(&b.code) == shape(&real) { "real" } else { "other" };
                out(&format!("KV\t{prefix}.payload_decodes\t{}", offer.is_ok()));
                out(&format!(
                    "KV\t{prefix}.offer_is_primarys\t{}",
                    offer.as_ref().map(|o| !primary_peer.is_empty() && o.owner_peer_id == primary_peer).unwrap_or(false)
                ));
                out(&format!("KV\t{prefix}.code_form\t{form}"));
                out(&format!("KV\t{prefix}.shard_id_present\t{}", !b.shard_id.is_empty()));
                out(&format!("KV\t{prefix}.enrolled_count\t{}", b.enrolled_count));
                // v11: what the answer holds, to look for the primary's card in it.
                out(&format!("KV\t{prefix}.shard_id\t{}", b.shard_id));
                out(&format!(
                    "KV\t{prefix}.answer_text\t{} {} {}",
                    b.shard_id,
                    b.code,
                    offer.as_ref().map(|o| format!("{o:?}")).unwrap_or_default().replace(['\t', '\n'], " ")
                ));
                out(&format!("KV\t{prefix}.total\t{}", b.total));
                (
                    format!(
                        "an offer: decodes {}, code {form}, shard {}, {} of {}",
                        offer.is_ok(),
                        !b.shard_id.is_empty(),
                        b.enrolled_count,
                        b.total
                    ),
                    Ok(()),
                )
            }
        }
    }

    /// v11: the owner tag and the enrolled guardian of the primary's card.
    const CARD_OWNER_TAG: &str = "walk-v11-owner-tag-of-the-primary";
    const CARD_GUARDIAN: &str = "12D3KooWWalkV11PrimarysEnrolledGuardianPeer";

    /// v11 (from-fable-2nde-position/0063, from-windows/0303,
    /// from-laptop/0288): the run's primary holds a recovery card naming one
    /// enrolled guardian, in the form and place the real store writes it
    /// (`RecoveryStore::write_recovery_card`: `serde_json::to_vec_pretty` of a
    /// `RecoveryCard`, at `crypto/recovery_card.json`; plain JSON, read before
    /// login, ch. 06), so that a decoy that read it can be told apart from an
    /// empty primary.
    pub(super) fn write_primary_recovery_card(p0: &Path) {
        let card = sovereign_crypto::recovery_store::RecoveryCard {
            owner_tag: CARD_OWNER_TAG.into(),
            epoch: 1,
            threshold: sovereign_crypto::recovery_roster::GUARDIAN_THRESHOLD,
            guardian_peer_ids: vec![CARD_GUARDIAN.into()],
            relays: Vec::new(),
        };
        let json = serde_json::to_vec_pretty(&card).expect("the card serializes");
        std::fs::write(p0.join("crypto").join("recovery_card.json"), json).expect("the primary's recovery card");
    }

    /// v11, AR4 and AR2 (ch. 07: guardian recovery is the primary's, « a
    /// duress session never reads or writes them »; the table's `recovery/`,
    /// kept in `crypto/` on the phone): with the primary's recovery card
    /// naming one enrolled guardian, the duress session's decoy answers as a
    /// persona with no roster: `enrolled_count` 0, a shard id that is none of
    /// the card's values, nothing of the card's owner tag or peer id; the same
    /// count when the card is stripped (AR6's second run); and no file of the
    /// primary's recovery store written.
    #[test]
    fn ar4_app_the_guardian_decoy_answers_as_a_persona_with_no_roster_whatever_the_primarys_card_says() {
        let _kept = Kept;
        let w = walks();
        let card = "crypto/recovery_card.json";
        assert!(w.p0.contains_key(card), "precondition: the run's primary holds a recovery card");
        assert!(w.walk.before.contains_key(card), "precondition: the card is in the duress walk's profile (its « before »)");
        let mut problems = Vec::new();
        let result = kv1(&w.walk, "guardian.result");
        if result != "Ok" {
            problems.push(format!("the decoy did not answer: {result}"));
        }
        let count = kv1(&w.walk, "guardian.enrolled_count");
        if count != "0" {
            problems.push(format!(
                "the decoy answers enrolled_count {count} (0 expected: a persona with no roster), while the primary's \
                 card names one enrolled guardian"
            ));
        }
        let shard = kv1(&w.walk, "guardian.shard_id");
        let text = kv1(&w.walk, "guardian.answer_text");
        for (what, v) in [("owner tag", CARD_OWNER_TAG), ("enrolled guardian's peer id", CARD_GUARDIAN)] {
            if shard == v {
                problems.push(format!("the decoy's shard id is the card's {what}: {v:?}"));
            }
            if text.contains(v) {
                problems.push(format!("the decoy's answer holds the card's {what} {v:?}: {text}"));
            }
        }
        // AR6's second run: the card stripped.
        if !w.removed.iter().any(|r| r == card) {
            problems.push("(precondition) AR6's stripped run still holds the card".to_string());
        }
        let stripped = kv1(&w.stripped, "guardian.enrolled_count");
        if stripped != count {
            problems.push(format!("the decoy's count is {count} with the primary's card, {stripped} without it"));
        }
        for (kind, rel) in &w.walk.writes {
            if matches!(verdict(rel), Some((Verdict::PrimaryOnly, _))) {
                problems.push(format!("the duress session {kind} {rel} (the primary's recovery store)"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR4, AR2 (phone), the guardian decoy and the primary's recovery card:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The primary's real guardian offer: a primary login (its node starts),
    /// then the same request.
    pub(super) fn child_guardian_primary() {
        let (st, rx) = boot();
        let l = login(&st, MAIN);
        out(&format!("KV\tgp.login\t{}", shown(&l)));
        let _ = drive_guardian(&st, "gp");
        out("END");
        std::mem::forget((st, rx));
    }

    /// The `guardian_primary` launch (see `v7`'s runner).
    fn primary_run() -> &'static Run {
        v7::launched_run("guardian_primary")
    }

    /// The form a real enrollment's answer has, checked on `r`'s report under
    /// `prefix`; `own` is whether the offer must name the primary's peer id.
    fn form_problems(r: &Run, prefix: &str, primarys: bool) -> Vec<String> {
        let mut problems = Vec::new();
        let result = kv1(r, &format!("{prefix}.result"));
        if result != "Ok" {
            problems.push(format!("no offer: {result}"));
            return problems;
        }
        let total = sovereign_crypto::recovery_roster::GUARDIAN_TOTAL.to_string();
        for (key, want) in [
            ("payload_decodes", "true".to_string()),
            ("code_form", "real".to_string()),
            ("shard_id_present", "true".to_string()),
            ("total", total.clone()),
            ("offer_is_primarys", primarys.to_string()),
        ] {
            let v = kv1(r, &format!("{prefix}.{key}"));
            if v != want {
                problems.push(format!("{key} = {v} ({want} expected)"));
            }
        }
        let enrolled = kv1(r, &format!("{prefix}.enrolled_count"));
        if !enrolled.parse::<usize>().is_ok_and(|n| n < sovereign_crypto::recovery_roster::GUARDIAN_TOTAL) {
            problems.push(format!("enrolled_count = {enrolled} (fewer than {total} expected: an offer hands out a share)"));
        }
        problems
    }

    /// AR4 (« Guardian enrollment keeps its decoy waiting screen »), ch. 07
    /// (Céline, 2026-10-17: « appears to work, waits for the guardian, and
    /// never completes »): in a duress session, the phone's guardian
    /// enrollment answers an offer of the real form, with the fields a real
    /// enrollment fills, and no error.
    #[test]
    fn ar4_app_in_a_duress_session_guardian_enrollment_answers_an_offer_of_the_real_form() {
        let _kept = Kept;
        let w = walks();
        assert!(w.walk.complete, "the walk did not finish");
        let problems = form_problems(&w.walk, "guardian", false);
        assert!(
            problems.is_empty(),
            "C-AREA AR4 (phone): in a duress session, guardian enrollment (`begin_guardian_enrollment_in`) does not \
             answer the decoy, an offer of the real form:\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR4, the decoy's key stretch, held by structure, not by timing (the
    /// phone's form of the shell's v9 test): the real offer runs the shared
    /// stretch, and the decoy as many times.
    #[test]
    fn ar4_app_the_guardian_decoy_goes_through_the_same_key_stretch_as_the_real_offer() {
        let _kept = Kept;
        let w = walks();
        let real = kv1(primary_run(), "gp.stretches");
        let decoy = kv1(&w.walk, "guardian.stretches");
        let mut problems = Vec::new();
        if !real.parse::<usize>().is_ok_and(|n| n >= 1) {
            problems.push(format!(
                "the real guardian offer (a primary session's): runs of the shared stretch \
                 `guardian::stretch_enroll_key` counted: {real} (one or more expected)"
            ));
        }
        if decoy != real {
            problems.push(format!(
                "runs of the shared enrollment stretch counted: {decoy} around the duress session's decoy, {real} around \
                 the real offer"
            ));
        }
        assert!(problems.is_empty(), "C-AREA AR4 (phone), the guardian enrollment's decoy:\n  {}", problems.join("\n  "));
    }

    /// AR4 and AR2: the decoy arms nothing (no node runs in the session that
    /// answered it) and leaves the primary's roster and recovery store as
    /// they were: nothing under `recovery/` is created, changed or deleted
    /// by the duress walk (whose snapshot holds the request).
    #[test]
    fn ar4_app_the_guardian_decoy_arms_nothing_and_leaves_the_primarys_recovery_store() {
        let _kept = Kept;
        let w = walks();
        assert!(w.walk.complete, "the walk did not finish");
        let mut problems = Vec::new();
        let result = kv1(&w.walk, "guardian.result");
        if result != "Ok" {
            problems.push(format!("the decoy did not answer (nothing to check it armed nothing): {result}"));
        }
        let node = kv1(&w.walk, "guardian.node_running");
        if node != "false" {
            problems.push(format!("a node runs in the session that answered the offer: {node}"));
        }
        for (kind, rel) in &w.walk.writes {
            if rel == "recovery/" || rel.starts_with("recovery/") {
                problems.push(format!("the duress session {kind} {rel} (the primary's recovery store)"));
            }
        }
        assert!(problems.is_empty(), "C-AREA AR4, AR2 (phone), the guardian enrollment's decoy:\n  {}", problems.join("\n  "));
    }

    /// In a primary session, the real guardian offer still opens: an offer of
    /// the real form, naming the primary's own peer id.
    #[test]
    fn ar4_app_in_a_primary_session_the_real_guardian_offer_still_opens() {
        let _kept = Kept;
        let r = primary_run();
        assert!(kv1(r, "gp.login").contains("primary"), "precondition: the primary login opens: {}", kv1(r, "gp.login"));
        assert_eq!(kv1(r, "gp.node_running"), "true", "precondition: the primary's node runs");
        let problems = form_problems(r, "gp", true);
        assert!(
            problems.is_empty(),
            "C-AREA AR4 (phone): in a primary session, the real guardian offer does not open:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The enrollment stretch both paths share is the guardian handshake's key
    /// derivation itself (`derive_enroll_key`), so the count above counts the
    /// real work.
    #[test]
    fn ar4_app_the_shared_enrollment_stretch_is_the_enroll_key_derivation() {
        let offer = sovereign_p2p::guardian_enroll::GuardianEnrollOffer::new(
            "12D3KooWEnrollSeamFixturePhone".into(),
            "Walk device".into(),
            Vec::new(),
            sovereign_p2p::guardian_enroll::GUARDIAN_OFFER_TTL_SECONDS,
        );
        let code = "ABCDE-FGHJK";
        let want = sovereign_p2p::guardian_enroll::derive_enroll_key(code, &offer).expect("the reference derivation");
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            block(crate::tauri_commands::guardian::stretch_enroll_key(&offer, code))
        }))
        .map_err(|p| panic_text(&p));
        assert!(
            matches!(&got, Ok(Ok(k)) if *k == want),
            "C-AREA AR4 (phone): `guardian::stretch_enroll_key` is not the enrollment key derivation: {:?}",
            got.map(|r| r.map(|_| "a different key"))
        );
    }
}
