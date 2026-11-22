//! C-AREA in the native shell — **AR1** (the walk), **AR2** (shared files are
//! named, with their reason), **AR5** (the walk grows with each release) and
//! **AR6** (reads count too: a differential walk). Spec ch. 07, *One persona,
//! one area — contract (C-AREA)*. AR3 and AR4 are not here (their tests are
//! written separately); AR7 neither; AR8 is
//! `sovereign-core/tests/c_area_ar8_shared_paths.rs`.
//!
//! Written before the code, by another author than whoever moves the shared
//! files: whoever implements L1c makes these pass **without editing this
//! file**; a test that looks wrong is reported, not worked around. Several
//! fail on today's code, and are meant to: each failure names the file or the
//! feature.
//!
//! **How it runs.** A child module of `app` (declared in `app.rs`, like
//! `first_run_tests`), so the walk drives `App`'s own paths on a real,
//! headless `App` over a real profile directory. `SOVEREIGN_DATA_DIR` is read
//! by the whole process, so each launch is a **child process** of this test
//! binary on its own temporary profile (as `first_run_tests` does): the
//! parent runs `child_entry` with `C_AREA_WALK_CHILD=<scenario>`,
//! `SOVEREIGN_DATA_DIR=<profile>` and `C_AREA_WALK_OUT=<file>`; the child
//! writes what it saw to that file, outside the profile. The launches:
//! 1. `setup` — a real first-device setup through the wizard, with a primary
//!    and a duress password (`first_run_tests::fill_wizard`, `press_finish`).
//! 2. `primary` — a primary session that leaves primary files behind: a
//!    document in a starter lane, typed and saved.
//! 3. The profile is copied three times: `walk`, `control` and `stripped`.
//!    In `stripped`, every file that is neither the duress persona's own
//!    area nor a named exception is deleted (AR6's second run: « the
//!    primary's files and the unnamed shared files … absent »).
//! 4. `walk` runs on each copy: the duress login, then every feature
//!    ([`FEATURES`]), then quit. The child snapshots the whole profile (each
//!    file's existence and content hash, and each directory) right before
//!    the login and right after the quit, and records what the session shows
//!    after each feature: the accessibility tree (role, spoken name, value of
//!    every node, in document order), the status bar's text and the notice.
//!
//! The three walks run once per test process (`walks()`), and the tests read
//! them:
//! - `ar1_shell_*_drives_every_feature` — a feature the walk could not drive
//!   is a failure, with its reason;
//! - `ar1_shell_*_writes_only_its_own_area` — every file or directory
//!   created, changed or deleted by the walk is in the duress area, or a named
//!   exception that may change;
//! - `ar2_shell_*` — each named exception is a real shared file of the
//!   profile, and one that may only be read is not changed;
//! - `ar6_shell_*` — the `walk` and `stripped` runs show and write the same.
//!   The `control` run (a second copy of the same full profile) is the noise
//!   floor: when `walk` and `control` already differ, the comparison is not
//!   stable, and the test says so instead of blaming a read.
//! - `ar5_shell_*` — every window kind and every canvas-menu action the shell
//!   has is named by a feature of the walk (a new one fails to compile, or
//!   fails this test, until it is).
//!
//! **Second round (2026-11-14, claude-coord from-windows/0241 and
//! from-fable-2nde-position/0016):**
//! - AR1's features test is a ratchet: the features not driven yet are named
//!   in [`NOT_DRIVEN_YET`]; a new one, or one driven and still listed, fails.
//! - The primary session at P0 makes a lane and a document of its own, and
//!   pairs a device (« Primary's paired device »). After the walk, a
//!   `primary_check` launch logs in as the primary and reports its lanes,
//!   cards and paired devices.
//! - **AR3:** the duress walk's pairing leaves the primary's list as it was,
//!   and the primary still lists its own device; a list made undecryptable
//!   (one ciphertext character changed) is kept as it is by a primary
//!   session that pairs, and the failure is shown (`primary_pair` launch).
//! - **Open vs write:** the duress session never lists the primary's lane or
//!   document; the primary never finds what the duress session created.
//! - **AR7:** each launch has a temporary directory of its own; the walk
//!   writes nothing there. The browser's profile is read from the source
//!   (the WebView cannot be built without a window), with a behaviour test
//!   gated on the function it names.
//! - **The assistant's actions:** the walk carries a fake model
//!   (`fake_model`), over `Orchestrator::for_host_tests_with_model`
//!   (sovereign-ai's `test-support`).
//!
//! **Fifth round (2026-11-14, spec v2.0.79, Céline: « Aucun moteur »):** a
//! duress session runs no P2P node. In the duress walk, pairing, sync, sync
//! now, rename and forget a paired device are gestures that must come to
//! nothing: they count as driven, and the walk records whether a node runs
//! and whether a second device can pair (AR3, AR4). AR2 follows v2.0.79's
//! table (« written once, by whichever persona logs in first »).
//!
//! **Fourth round (2026-11-14, from-fable-2nde-position/0019 §2, spec
//! v2.0.78):** AR2 follows the spec's verdict table ([`VERDICTS`]): a shared
//! file not in it fails, a « read only » or « the primary's only » file a
//! duress session writes fails. A second order of setup: a fresh account
//! whose first login is the duress one (`store_only`, then the walk), for
//! AR1 and AR2.
//!
//! **Third round (2026-11-14, from-windows/0242, from-fable-2nde-position
//! /0017):** the seams exist as signatures (« types and signatures »
//! commit), so the gated tests compile and fail until their bodies are
//! written; open vs write also requires that, in each direction, the store
//! met no row it could not decrypt (`EncryptedGraphDB::undecryptable_rows_met`);
//! the temporary profiles are deleted when every walk test of the process
//! passed, and kept, their path printed, when one failed.
//!
//! **Readings chosen** (the contract leaves them open; reported with the
//! tests):
//! - **The duress persona's own area** is derived from the code's
//!   persona-path functions, listed in [`duress_area`] with their source
//!   lines. A directory entry covers everything below it; a file entry covers
//!   that file and its write-temporaries (`<file>.tmp`, `<file>.*.tmp`); a
//!   directory that holds an entry (`data/`, `crypto/`) is not counted when
//!   created, and is kept when the profile is stripped for AR6.
//! - **The snapshot** starts after the app is launched and before the duress
//!   login: what `App::new` writes before any login is not the session's.
//! - **« Shows »** is the accessibility tree (what a screen reader receives),
//!   the status bar and the notice, after each feature. Text that differs
//!   from one run to the next by nature is masked before comparing: the
//!   pairing code and PIN, the guardian code, the second device's peer id,
//!   the duress persona's designation as its own profile file holds it
//!   (all random per run), and any token holding digits with `:`, `-` or `/`
//!   (times, dates, record ids).
//! - **« Writes »** for AR6 is the set of paths created, changed and deleted,
//!   not their bytes: the content is encrypted under fresh nonces, so two
//!   identical sessions never write the same bytes. The digit runs of a
//!   path's last segment are masked there (RocksDB's numbered files and
//!   timestamped log rotations); AR1 reads the paths unmasked.
//! - **Logout:** the shell has no logout (no lock, no persona switch): quit
//!   is the end of its session, and the walk ends with it. Reported.
//! - **Models:** the walk points the session's model directory at a
//!   directory of its own (empty, then one fake `.gguf` that « delete »
//!   removes), so no real model is loaded or deleted, and the walk is the
//!   same on every machine. The chat therefore runs without a model.
//! - **Email** (`comms::EMAIL_ENABLED` is false): the shell offers no email
//!   as built, so the walk does not count it as a feature. Reported; it joins
//!   the walk the release that turns it on (AR5).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use sovereign_core::auth::PersonaKind as CorePersona;
use sovereign_editor::input::{Command as EdCommand, Modifiers};
use sovereign_editor::{Position, Selection};
use winit::keyboard::Key;

use super::editor_v2::route_key;
use super::first_run_tests::{
    fill_wizard, first_frame, frames_until, login_settled, press_finish, read_tree, setup_settled, shut_runtime,
    submit_login, DURESS, H, PRIMARY, W,
};
use super::{App, CtxAction, WindowContent, WindowKind};

#[path = "../../sovereign-p2p/tests/doc_harness/mod.rs"]
#[allow(dead_code, clippy::all)]
mod doc_harness;

const CHILD_VAR: &str = "C_AREA_WALK_CHILD";
const OUT_VAR: &str = "C_AREA_WALK_OUT";

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
    // v12 (from-fable-2nde-position/0066): the recovery store is the four
    // `crypto/` files of `sovereign_crypto::recovery_store` on both hosts
    // (the shell's `crypto::recovery_store`, `RecoveryStore::new(crypto_dir())`).
    ("crypto/recovery_roster.enc", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/recovery.bundle", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/recovery_card.json", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
    ("crypto/guardian_enrolled.pending", Verdict::PrimaryOnly, "the recovery store: guardian recovery belongs to the primary; a duress session neither reads nor writes it, and its decoy answers as a persona with no roster"),
];

fn verdict(rel: &str) -> Option<(Verdict, &'static str)> {
    VERDICTS
        .iter()
        .find(|(p, _, _)| rel == *p || (p.ends_with('/') && (rel.starts_with(p) || rel == p.trim_end_matches('/'))))
        .map(|(_, v, why)| (*v, *why))
}

/// One entry of the duress persona's own area: a path relative to the
/// Sovereign directory, whether it is a directory, and the code it comes from.
struct AreaEntry {
    rel: String,
    dir: bool,
    source: &'static str,
}

fn rel_to_data_dir(p: &Path) -> String {
    let base = sovereign_core::sovereign_dir();
    p.strip_prefix(&base)
        .unwrap_or_else(|_| panic!("{} is not under the data directory {}", p.display(), base.display()))
        .to_string_lossy()
        .replace('\\', "/")
}

/// The duress persona's own area **in the shell**, derived from the code's
/// persona-path functions (each entry with its source line, at `ba1dc18`).
/// Any path the shell gives the duress persona and that is not here is a
/// guess this list did not make: reported, not added.
fn duress_area() -> Vec<AreaEntry> {
    persona_area(CorePersona::Duress)
}

/// The primary's own area, by the same functions (its identity profile is the
/// data directory itself, so that entry is left out: `user_profile.json` has
/// its verdict in the table).
fn primary_area() -> Vec<AreaEntry> {
    persona_area(CorePersona::Primary).into_iter().filter(|e| !e.rel.is_empty()).collect()
}

fn persona_area(d: CorePersona) -> Vec<AreaEntry> {
    let base = sovereign_core::sovereign_dir();
    let crypto = rel_to_data_dir(&crate::crypto::crypto_dir());
    let mut v = vec![
        AreaEntry {
            rel: rel_to_data_dir(&crate::crypto::persona_raw_db_path(d)),
            dir: true,
            source: "sovereign-shell/src/crypto.rs l. 138, persona_raw_db_path (the persona's raw database, RocksDB)",
        },
        AreaEntry {
            rel: rel_to_data_dir(&sovereign_core::auth::orchestrator_state_dir(&base, d)),
            dir: true,
            source: "sovereign-core/src/auth.rs l. 18, orchestrator_state_dir (session log, trust state; via OrchestratorStatePaths::for_persona, crypto.rs l. 74)",
        },
        AreaEntry {
            rel: rel_to_data_dir(&sovereign_core::auth::session_log_beacon_path(&base, d)),
            dir: false,
            source: "sovereign-core/src/auth.rs l. 29, session_log_beacon_path",
        },
        AreaEntry {
            rel: rel_to_data_dir(&sovereign_core::auth::identity_profile_dir(&base, d)),
            dir: true,
            source: "sovereign-core/src/auth.rs l. 44, identity_profile_dir (the persona's identity profile; app.rs identity_profile)",
        },
        AreaEntry {
            rel: rel_to_data_dir(&crate::crypto::persona_model_tofu_path(d)),
            dir: false,
            source: "sovereign-shell/src/crypto.rs l. 167, persona_model_tofu_path",
        },
        AreaEntry {
            rel: format!("{crypto}/{}", crate::crypto::persona_index_filename(d)),
            dir: false,
            source: "sovereign-shell/src/crypto.rs l. 150, persona_index_filename (the blind-index key)",
        },
    ];
    for kdb in ["keys.db", "keys.messages.db", "keys.threads.db", "keys.conversations.db", "keys.contacts.db", "keys.share_records.db"] {
        v.push(AreaEntry {
            rel: format!("{crypto}/{}", crate::crypto::persona_key_db_filename(d, kdb)),
            dir: false,
            source: "sovereign-shell/src/crypto.rs l. 144, persona_key_db_filename (build_encrypted_db l. 265-270)",
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

/// Every file (`rel` → content hash) and directory (`rel/` → "dir") under `root`.
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
                // A file held open exclusively (a RocksDB LOCK on Windows)
                // reads as unreadable: its existence still counts.
                let h = std::fs::read(&p).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable:{}", e.kind()));
                out.insert(rel, h);
            }
        }
    }
    let mut out = Snap::new();
    walk(root, root, &mut out);
    out
}

/// (kind, rel) for every path created, changed or deleted between two snapshots.
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

// =========================================================== what is shown

/// Masks what differs between two runs by nature (see the header).
fn mask(line: &str, volatile: &[String]) -> String {
    let mut s = line.to_string();
    for v in volatile {
        if v.len() >= 4 {
            s = s.replace(v.as_str(), "<volatile>");
        }
    }
    s.split(' ')
        .map(|tok| {
            let has_digit = tok.chars().any(|c| c.is_ascii_digit());
            if has_digit && (tok.contains(':') || tok.contains('-') || tok.contains('/')) {
                "<t>".to_string()
            } else {
                tok.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// ================================================================ the walk

/// The result of one feature: driven, or why it could not be.
type Step = Result<(), String>;

struct Walk {
    app: App,
    /// Strings random per run, masked in what is shown.
    volatile: Vec<String>,
    /// The thread of the lane the walk creates, and the document it creates.
    lane_thread: Option<String>,
    second_thread: Option<String>,
    doc: Option<String>,
    second: Option<SecondDevice>,
    /// (step, lines shown after it).
    shown: Vec<(String, Vec<String>)>,
    /// A model directory of the walk's own (never the repository's).
    model_dir: PathBuf,
    /// The name the second device pairs under.
    second_name: &'static str,
    /// The fake model's answers, in order, when one is installed
    /// ([`install_fake_model`]); `None` while the seam it needs is missing.
    model_answers: Option<Arc<std::sync::Mutex<std::collections::VecDeque<String>>>>,
    /// Open vs write: what of the primary's the duress session showed.
    primary_seen: Vec<String>,
    /// Open vs write, by record id (ids are stored in clear, so a store
    /// opened by the wrong persona shows its rows' ids even when it cannot
    /// decrypt their names): every thread and document id the session listed.
    seen_ids: BTreeSet<String>,
}

impl Walk {
    fn new(app: App, model_dir: PathBuf, second_name: &'static str) -> Walk {
        Walk {
            app,
            volatile: Vec::new(),
            lane_thread: None,
            second_thread: None,
            doc: None,
            second: None,
            shown: Vec::new(),
            model_dir,
            second_name,
            model_answers: None,
            primary_seen: Vec::new(),
            seen_ids: BTreeSet::new(),
        }
    }
}

/// The primary's own objects, left at P0 (open vs write).
const PRIMARY_LANE: &str = "Primary-only lane";
const PRIMARY_TEXT: &str = "Primary only, never in a duress session";
/// What the duress walk creates, which a primary session must never find.
const DURESS_LANES: [&str; 4] = ["Walk lane", "Walk lane two", "Walk lane renamed", "Assistant lane"];
const DURESS_TEXT: &str = "Walk text";

/// A feature of the walk: its name, and how it is driven.
type Feature = (&'static str, fn(&mut Walk) -> Step);

/// **AR1, AR5: every feature the shell offers, as built, in the order the
/// walk drives them.** A release adds a store, a window or an action by
/// adding one entry here (AR5).
///
/// *v0.0.13:* the personal-data sweep's review queue (ch. 09, C-SWEEP SW4: its
/// findings, dismissals and the count seen at the last login) joins here as
/// `("personal-data sweep: review queue", f_pii_sweep)`, added by the test
/// author of lot LS(a).
const FEATURES: &[Feature] = &[
    ("canvas: the workspace loads", f_canvas),
    ("threads: create (the + menu's New lane, named)", f_thread_create),
    ("threads: rename (the lane label)", f_thread_rename),
    ("documents: create (the lane menu's New document)", f_doc_create),
    ("documents: edit and save (the editor)", f_doc_edit),
    ("documents: open from the card menu", f_doc_open_menu),
    ("documents: pin", f_doc_pin),
    ("documents: move to another thread (drop on a lane)", f_doc_move),
    ("documents: run a skill (the card menu's skills)", f_doc_skill),
    ("documents: history and restore a version", f_doc_history),
    ("documents: delete, then Undo", f_doc_delete_undo),
    ("trash: restore from Settings → Trash", f_trash_restore),
    ("trash: empty (purge)", f_trash_empty),
    ("search", f_search),
    ("chat", f_chat),
    ("the assistant's actions (approve or reject a proposed action)", f_assistant_actions),
    ("the embedded browser", f_browser),
    ("settings: open", f_settings),
    ("settings: theme", f_theme),
    ("settings: bubble style", f_bubble),
    ("settings: this device's name", f_device_name),
    ("models: open, assign, delete", f_models),
    ("personal data: open and review a record", f_pii),
    ("personal-data sweep: review queue", f_pii_sweep),
    ("inbox", f_inbox),
    ("devices: open", f_devices),
    ("pairing (a second node in this process)", f_pairing),
    ("sync (with the paired second node)", f_sync),
    ("sync now", f_sync_now),
    ("paired device: rename on this device", f_paired_rename),
    ("paired device: forget", f_paired_forget),
    ("guardian enrollment", f_guardian),
    ("alerts (changes from paired devices)", f_alerts),
    ("previous values", f_previous_values),
    ("quit", f_quit),
];

/// AR5: the feature of the walk that covers each window kind. Exhaustive: a
/// new kind does not compile until it is named.
fn window_feature(k: WindowKind) -> &'static str {
    match k {
        WindowKind::Doc => "documents: open from the card menu",
        WindowKind::Inbox => "inbox",
        WindowKind::Chat => "chat",
        WindowKind::Settings => "settings: open",
        WindowKind::Search => "search",
        WindowKind::History => "documents: history and restore a version",
        WindowKind::Models => "models: open, assign, delete",
        WindowKind::Pii => "personal data: open and review a record",
        WindowKind::Browser => "the embedded browser",
        WindowKind::Devices => "devices: open",
        WindowKind::Alerts => "alerts (changes from paired devices)",
        WindowKind::PreviousValues => "previous values",
    }
}

/// AR5: the feature of the walk that covers each canvas-menu action.
fn menu_feature(a: &CtxAction) -> &'static str {
    match a {
        CtxAction::Open => "documents: open from the card menu",
        CtxAction::TogglePin => "documents: pin",
        CtxAction::Delete => "documents: delete, then Undo",
        CtxAction::OpenSkills | CtxAction::RunSkill { .. } => "documents: run a skill (the card menu's skills)",
        CtxAction::NewThread => "threads: create (the + menu's New lane, named)",
        CtxAction::NewDocument(_) => "documents: create (the lane menu's New document)",
    }
}

fn frames(app: &mut App, n: usize) {
    for _ in 0..n {
        app.build_scene(W, H);
        app.v2_frame();
    }
}

fn notice(app: &App) -> Option<String> {
    app.notice.as_ref().map(|n| format!("{} — {}", n.said.0, n.said.1))
}

fn record_shown(w: &mut Walk, step: &str) {
    frames(&mut w.app, 2);
    // The duress persona's own designation is drawn at random when its
    // identity profile is first made (`App::identity_profile`): random per
    // run, so masked — but only that one, read from the persona's own file;
    // the primary's designation, if shown, stays visible.
    let own = sovereign_core::auth::identity_profile_dir(&sovereign_core::sovereign_dir(), CorePersona::Duress);
    if own.join("user_profile.json").exists() {
        if let Ok(p) = sovereign_core::profile::UserProfile::load(&own) {
            if !w.volatile.contains(&p.designation) {
                w.volatile.push(p.designation);
            }
        }
    }
    // Open vs write: the duress session never lists the primary's objects.
    for l in &w.app.lane_labels {
        w.seen_ids.insert(l.thread_id.clone());
    }
    for c in &w.app.cards {
        w.seen_ids.insert(c.id.clone());
    }
    for l in &w.app.lane_labels {
        if l.name == PRIMARY_LANE {
            w.primary_seen.push(format!("after « {step} »: the lane {:?}", l.name));
        }
    }
    for c in &w.app.cards {
        if c.body.contains(PRIMARY_TEXT) || c.title.contains(PRIMARY_TEXT) {
            w.primary_seen.push(format!("after « {step} »: a card with the primary's text ({:?})", c.title));
        }
    }
    let t = read_tree(&mut w.app);
    let mut lines: Vec<String> = t.order.iter().map(|s| format!("{:?} {:?} = {:?}", s.role, s.name(), s.value)).collect();
    lines.push(format!("status: {}", w.app.status_text()));
    lines.push(format!("notice: {:?}", notice(&w.app)));
    let lines = lines.iter().map(|l| mask(l, &w.volatile)).collect();
    w.shown.push((step.to_string(), lines));
}

fn lane_of(app: &App, thread: &str) -> Option<usize> {
    app.lane_labels.iter().find(|l| l.thread_id == thread).map(|l| l.lane)
}

fn card_of(app: &App, id: &str) -> Option<usize> {
    app.cards.iter().position(|c| c.id == id)
}

fn doc_window(app: &App, id: &str) -> Option<usize> {
    app.windows.iter().position(|w| matches!(&w.content, WindowContent::Doc(d) if d.doc_id == id))
}

fn with_doc(w: &Walk) -> Result<String, String> {
    w.doc.clone().ok_or_else(|| "no document: « documents: create » did not run".to_string())
}

fn f_canvas(w: &mut Walk) -> Step {
    w.app.load_workspace_now();
    Ok(())
}

fn create_thread(app: &mut App, name: &str) -> Result<String, String> {
    app.open_new_thread_prompt();
    if app.new_thread_prompt.is_none() {
        return Err(format!("the name prompt did not open; notice {:?}", notice(app)));
    }
    app.prompt_step(crate::new_thread::PromptStep::Create(name.into()));
    app.lane_labels
        .iter()
        .find(|l| l.name == name)
        .map(|l| l.thread_id.clone())
        .ok_or_else(|| format!("no lane named {name:?} after Create; notice {:?}", notice(app)))
}

fn f_thread_create(w: &mut Walk) -> Step {
    w.lane_thread = Some(create_thread(&mut w.app, "Walk lane")?);
    w.second_thread = Some(create_thread(&mut w.app, "Walk lane two")?);
    Ok(())
}

fn f_thread_rename(w: &mut Walk) -> Step {
    let t = w.lane_thread.clone().ok_or("no lane: « threads: create » did not run")?;
    w.app.save_lane_rename(&t, "Walk lane renamed");
    if w.app.lane_labels.iter().any(|l| l.thread_id == t && l.name == "Walk lane renamed") {
        Ok(())
    } else {
        Err(format!("the lane was not renamed; notice {:?}", notice(&w.app)))
    }
}

fn f_doc_create(w: &mut Walk) -> Step {
    let t = w.lane_thread.clone().ok_or("no lane: « threads: create » did not run")?;
    let lane = lane_of(&w.app, &t).ok_or("the walk's lane is not on the canvas")?;
    let before: BTreeSet<String> = w.app.cards.iter().map(|c| c.id.clone()).collect();
    w.app.new_document(lane);
    let new: Vec<String> = w.app.cards.iter().map(|c| c.id.clone()).filter(|id| !before.contains(id)).collect();
    match new.as_slice() {
        [id] => {
            w.doc = Some(id.clone());
            Ok(())
        }
        other => Err(format!("{} new cards after New document; notice {:?}", other.len(), notice(&w.app))),
    }
}

/// Types `s` at the end of the body of the open document `id` and saves.
fn type_and_save(app: &mut App, id: &str, s: &str) -> Step {
    let wi = doc_window(app, id).ok_or("the document's window is not open")?;
    let WindowContent::Doc(d) = &mut app.windows[wi].content else { unreachable!() };
    let p = d.editor_pane_mut().ok_or("the window holds no editor")?;
    p.set_focus(true);
    let b = &p.editor.document().blocks()[0];
    let at = Position { block: b.id(), offset: b.text().unwrap_or("").len() };
    p.editor.set_selection(Selection::caret(at));
    for c in s.chars() {
        let t = c.to_string();
        route_key(&mut p.editor, &Key::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
    }
    p.editor.command(EdCommand::Save);
    frames(app, 5);
    Ok(())
}

fn stored_text(app: &App, id: &str) -> Result<String, String> {
    let docs = app.docs.clone().ok_or("no document service")?;
    app.rt.block_on(docs.read(id)).map(|d| d.plain_text()).map_err(|e| format!("{e:?}"))
}

fn f_doc_edit(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    type_and_save(&mut w.app, &id, "Walk text, call alice.walk@example.com")?;
    let t = stored_text(&w.app, &id)?;
    if t.contains("Walk text") {
        Ok(())
    } else {
        Err(format!("the saved text is not stored: {t:?}; notice {:?}", notice(&w.app)))
    }
}

fn f_doc_open_menu(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    // Closed first, then opened as the card menu's Open does.
    if let Some(wi) = doc_window(&w.app, &id) {
        w.app.request_close_window(wi);
    }
    let ci = card_of(&w.app, &id).ok_or("the document has no card")?;
    w.app.open_doc_window(ci);
    doc_window(&w.app, &id).map(|_| ()).ok_or_else(|| format!("no window opened; notice {:?}", notice(&w.app)))
}

fn f_doc_pin(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    w.app.toggle_pin(&id);
    let ci = card_of(&w.app, &id).ok_or("the document has no card")?;
    if w.app.cards[ci].pinned {
        Ok(())
    } else {
        Err(format!("the card is not pinned; notice {:?}", notice(&w.app)))
    }
}

fn f_doc_move(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    let to = w.second_thread.clone().ok_or("no second lane")?;
    let lane = lane_of(&w.app, &to).ok_or("the second lane is not on the canvas")?;
    w.app.test_view = Some((W, H));
    // The screen y of the middle of that lane, as `lane_at_y` reads it.
    let y = (lane as f64 + 0.5) * crate::canvas::LANE_H * w.app.cam.zoom + w.app.cam.offset_y;
    if w.app.drop_target_thread(y).as_deref() != Some(to.as_str()) {
        return Err(format!("no drop target at the second lane's y ({y}); the walk could not aim the drop"));
    }
    w.app.drop_card_on_lane(&id, y);
    let ci = card_of(&w.app, &id).ok_or("the document has no card after the move")?;
    if lane_of(&w.app, &to) == Some(w.app.cards[ci].lane) {
        Ok(())
    } else {
        Err(format!("the card is not on the second lane; notice {:?}", notice(&w.app)))
    }
}

fn f_doc_skill(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    let si = w
        .app
        .skills
        .all_skills()
        .iter()
        .position(|s| s.name().to_lowercase().contains("word"))
        .ok_or("no word-count skill in the registry")?;
    w.app.run_skill(&id, si);
    frames(&mut w.app, 3);
    if w.app.notice.is_some() {
        Ok(())
    } else {
        Err("the skill showed no result".into())
    }
}

fn f_doc_history(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    let title = w.app.cards[card_of(&w.app, &id).ok_or("no card")?].title.clone();
    w.app.open_history(&id, &title);
    let oldest = w
        .app
        .windows
        .iter()
        .find_map(|win| match &win.content {
            WindowContent::History(hp) if hp.doc_id == id => Some(hp.commits.last().map(|c| c.commit_id.clone())),
            _ => None,
        })
        .ok_or_else(|| format!("History did not open; notice {:?}", notice(&w.app)))?
        .ok_or("History shows no version")?;
    w.app.restore_commit(&id, &oldest);
    frames(&mut w.app, 3);
    Ok(())
}

fn f_doc_delete_undo(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    w.app.trash_document(&id);
    if card_of(&w.app, &id).is_some() {
        return Err(format!("the card is still on the canvas after Delete; notice {:?}", notice(&w.app)));
    }
    w.app.undo_delete();
    card_of(&w.app, &id).map(|_| ()).ok_or_else(|| format!("Undo did not bring it back; notice {:?}", notice(&w.app)))
}

fn open_settings(app: &mut App) {
    if app.window_index_of(WindowKind::Settings).is_none() {
        app.focus_or_toggle(WindowKind::Settings);
    }
}

fn f_trash_restore(w: &mut Walk) -> Step {
    let id = with_doc(w)?;
    w.app.trash_document(&id);
    w.app.delete_notice = None;
    open_settings(&mut w.app);
    w.app.refresh_trash();
    w.app.trash_row_action(crate::trash_ui::TrashRowAction::RestoreOne(id.clone()));
    card_of(&w.app, &id).map(|_| ()).ok_or_else(|| format!("not restored; notice {:?}", notice(&w.app)))
}

fn f_trash_empty(w: &mut Walk) -> Step {
    // A second document, trashed and purged; the walk's document stays.
    let t = w.second_thread.clone().ok_or("no second lane")?;
    let lane = lane_of(&w.app, &t).ok_or("the second lane is not on the canvas")?;
    let before: BTreeSet<String> = w.app.cards.iter().map(|c| c.id.clone()).collect();
    w.app.new_document(lane);
    let id = w.app.cards.iter().map(|c| c.id.clone()).find(|i| !before.contains(i)).ok_or("no document to purge")?;
    if let Some(wi) = doc_window(&w.app, &id) {
        w.app.windows.remove(wi);
    }
    w.app.trash_document(&id);
    w.app.delete_notice = None;
    open_settings(&mut w.app);
    w.app.refresh_trash();
    w.app.trash_row_action(crate::trash_ui::TrashRowAction::EmptyTrash);
    if !matches!(w.app.trash_modal, Some(crate::trash_ui::TrashModal::ConfirmEmpty { .. })) {
        return Err("Empty trash asked no confirmation".into());
    }
    // The confirmation's click arm (`handle_trash_modal_click`, Confirm):
    // a click needs a window, so the arm is reproduced.
    w.app.trash_modal = None;
    let docs = w.app.docs.clone().ok_or("no document service")?;
    let mut view = w.app.trash.take().ok_or("no trash view")?;
    let r = w.app.rt.block_on(view.confirm_empty_report(&docs));
    w.app.trash = Some(view);
    match r {
        Ok(e) => w.app.set_notice("Trash emptied", &crate::trash::emptied_message(e)),
        Err(msg) => return Err(format!("not emptied: {msg}")),
    }
    w.app.rebuild_settings_windows();
    Ok(())
}

fn f_search(w: &mut Walk) -> Step {
    w.app.focus_or_toggle(WindowKind::Search);
    let wi = w.app.window_index_of(WindowKind::Search).ok_or("the Search window did not open")?;
    if let WindowContent::Search(sp) = &mut w.app.windows[wi].content {
        for c in "untitled".chars() {
            let t = c.to_string();
            sp.query.key(&Key::Character(t.as_str().into()), Some(&t), Modifiers::NONE);
        }
    }
    w.app.update_search_window(wi);
    Ok(())
}

fn f_chat(w: &mut Walk) -> Step {
    w.app.focus_or_toggle(WindowKind::Chat);
    let wi = w.app.window_index_of(WindowKind::Chat).ok_or("the Chat window did not open")?;
    let before = match &w.app.windows[wi].content {
        WindowContent::Chat(c) => c.msgs.len(),
        _ => 0,
    };
    if let Some(q) = &w.model_answers {
        let mut q = q.lock().unwrap();
        q.push_back(r#"{"action":"chat","target":null,"confidence":0.95}"#.into());
        q.push_back("Your workspace has a few lanes.".into());
    }
    w.app.dispatch_chat("What is in my workspace?".into());
    let replied = frames_until(&mut w.app, 120, |a| {
        a.window_index_of(WindowKind::Chat)
            .and_then(|i| match &a.windows[i].content {
                WindowContent::Chat(c) => Some(c.msgs.len() > before),
                _ => None,
            })
            .unwrap_or(false)
    });
    if replied {
        Ok(())
    } else {
        Err("no reply reached the chat window in 120 s".into())
    }
}

fn f_assistant_actions(w: &mut Walk) -> Step {
    // An action is proposed only by the model's answer: the fake model
    // classifies the request as a rename, the shell shows the proposal, the
    // walk approves it as the button does, and the lane is renamed.
    let Some(q) = w.model_answers.clone() else {
        return Err("no model: the fake model could not be installed (no session database)".into());
    };
    {
        let mut q = q.lock().unwrap();
        q.push_back(
            r#"{"action":"rename_thread","target":"Walk lane renamed to Assistant lane","confidence":0.95}"#.into(),
        );
        q.push_back("Done.".into());
    }
    w.app.notice = None; // the notice left by the trash, closed as Esc does
    w.app.dispatch_chat("Rename the lane Walk lane renamed to Assistant lane".into());
    if !frames_until(&mut w.app, 60, |a| a.pending_action.is_some()) {
        return Err(format!("no action was proposed in 60 s; notice {:?}", notice(&w.app)));
    }
    record_shown(w, "the assistant's actions: the proposal");
    // Approve, as the button does: the shell drops an answer to a prompt shown
    // less than `approval::MIN_VISIBLE` ago, so frames are drawn first, and the
    // answer is given again until the prompt closes.
    let end_at = Instant::now() + Duration::from_secs(10);
    while w.app.pending_action.is_some() && Instant::now() < end_at {
        frames(&mut w.app, 1);
        std::thread::sleep(crate::approval::MIN_VISIBLE / 4);
        w.app.decide_action(sovereign_core::security::ActionDecision::Approve);
    }
    if frames_until(&mut w.app, 60, |a| a.lane_labels.iter().any(|l| l.name == "Assistant lane")) {
        Ok(())
    } else {
        Err(format!("approved, and the lane was not renamed; notice {:?}", notice(&w.app)))
    }
}

/// The fake model, installed in the duress session as the orchestrator the
/// shell would build lazily (`dispatch_chat`), over the persona's own state
/// paths and wired as the shell wires its own (`wire_new_orchestrator`). It
/// answers by the queue the walk fills before each request. Its seam,
/// `sovereign_ai::Orchestrator::for_host_tests_with_model`, is behind
/// sovereign-ai's `test-support` feature (this crate's dev-dependencies).
mod fake_model {
    use super::*;

    pub(super) fn install(w: &mut Walk) -> bool {
        let Some(db) = w.app.db.clone() else { return false };
        let answers: Arc<std::sync::Mutex<std::collections::VecDeque<String>>> = Arc::default();
        let a = answers.clone();
        let generate: Arc<dyn Fn(&str, u32) -> anyhow::Result<String> + Send + Sync> =
            Arc::new(move |_prompt: &str, _max: u32| Ok(a.lock().unwrap().pop_front().unwrap_or_default()));
        let paths = crate::crypto::orchestrator_state_paths(w.app.persona);
        let mut o =
            sovereign_ai::Orchestrator::for_host_tests_with_model(db, w.app.orch_tx.clone(), paths, 4096, generate);
        let (orch, dc, ic, ak, docs) = (
            w.app.orch.clone(),
            w.app.decision_rx_cell.clone(),
            w.app.injection_decision_rx_cell.clone(),
            w.app.account_key.clone(),
            w.app.docs.clone(),
        );
        w.app.rt.block_on(async move {
            super::super::wire_new_orchestrator(&mut o, &dc, &ic, &ak, &docs).await;
            *orch.lock().await = Some(Arc::new(o));
        });
        w.model_answers = Some(answers);
        true
    }
}

/// Installs the fake model; false when the session has no database.
fn install_fake_model(w: &mut Walk) -> bool {
    fake_model::install(w)
}

fn f_browser(_w: &mut Walk) -> Step {
    Err("the embedded browser is a wry WebView overlaid on the winit window: a headless App has no window to \
         host it, so no page loads and nothing it would write (its profile) is reached"
        .into())
}

fn f_settings(w: &mut Walk) -> Step {
    open_settings(&mut w.app);
    w.app.window_index_of(WindowKind::Settings).map(|_| ()).ok_or_else(|| "Settings did not open".into())
}

fn f_theme(w: &mut Walk) -> Step {
    let before = w.app.theme_name.clone();
    w.app.toggle_theme();
    if w.app.theme_name != before {
        Ok(())
    } else {
        Err("the theme did not change".into())
    }
}

fn f_bubble(w: &mut Walk) -> Step {
    use sovereign_core::profile::BubbleStyle;
    let s = if w.app.bubble_style == BubbleStyle::Wave { BubbleStyle::Rings } else { BubbleStyle::Wave };
    w.app.select_bubble_style(s);
    if w.app.bubble_style == s {
        Ok(())
    } else {
        Err("the bubble style did not change".into())
    }
}

fn f_device_name(w: &mut Walk) -> Step {
    w.app.set_this_device_name("Walk device");
    if w.app.device_name == "Walk device" {
        Ok(())
    } else {
        Err(format!("not renamed; notice {:?}", notice(&w.app)))
    }
}

fn f_models(w: &mut Walk) -> Step {
    // A model directory of the walk's own, with one fake model: deleting a
    // model must never touch the repository's real files.
    std::fs::create_dir_all(&w.model_dir).map_err(|e| e.to_string())?;
    std::fs::write(w.model_dir.join("walk-fake.gguf"), b"not a model").map_err(|e| e.to_string())?;
    w.app.focus_or_toggle(WindowKind::Models);
    w.app.window_index_of(WindowKind::Models).ok_or("the Models window did not open")?;
    w.app.assign_model("walk-fake.gguf", true);
    w.app.assign_model("walk-other.gguf", true);
    w.app.delete_model("walk-fake.gguf");
    if w.model_dir.join("walk-fake.gguf").exists() {
        Err(format!("the fake model was not deleted; notice {:?}", notice(&w.app)))
    } else {
        Ok(())
    }
}

fn f_pii(w: &mut Walk) -> Step {
    w.app.focus_or_toggle(WindowKind::Pii);
    let wi = w.app.window_index_of(WindowKind::Pii).ok_or("the personal-data window did not open")?;
    let first = match &w.app.windows[wi].content {
        WindowContent::Pii(p) => p.rows.first().map(|r| r.id.clone()),
        _ => None,
    };
    let id = first.ok_or(
        "no personal-data record to review: the e-mail address typed in the walk's document made none \
         (the review path was not reached)",
    )?;
    w.app.review_pii(&id, sovereign_db::schema::ReviewState::Confirmed);
    Ok(())
}

/// C-SWEEP SW4 (added by the test author of lot LS(a)): the sweep's stores in
/// the session's persona: a pass of the sweep over the session's documents,
/// the review window's groups, a dismissal and its take-back (her review's
/// versions and the dismissal store), a confirmation taken back and given
/// again, and the login's signal (the count seen at the last login).
fn f_pii_sweep(w: &mut Walk) -> Step {
    use sovereign_ai::pii::review::PiiReview;
    let docs = w.app.docs.clone().ok_or("no document service in the session")?;
    let key = w.app.account_key.clone().ok_or("no account key in the session")?;
    let db = w.app.db.clone().ok_or("no store in the session")?;
    let rt = &w.app.rt;
    let report = rt.block_on(sovereign_ai::pii::sweep::run_sweep_pass(&docs, 10));
    if !report.failed.is_empty() {
        return Err(format!("the sweep could not analyse {} document(s)", report.failed.len()));
    }
    let review = PiiReview::new(docs.clone(), key);
    let groups = rt.block_on(review.groups()).map_err(|e| format!("the review window: {e}"))?;
    let id = groups
        .iter()
        .flat_map(|g| g.rows.iter())
        .next()
        .map(|r| r.record_id.clone())
        .ok_or("no record in the review window: the walk's document made none")?;
    let one = [id];
    rt.block_on(review.dismiss(&one)).map_err(|e| format!("dismiss: {e}"))?;
    rt.block_on(review.take_back_dismissal(&one)).map_err(|e| format!("take back the dismissal: {e}"))?;
    rt.block_on(review.take_back_confirmation(&one)).map_err(|e| format!("take back the confirmation: {e}"))?;
    rt.block_on(review.confirm(&one)).map_err(|e| format!("confirm: {e}"))?;
    rt.block_on(sovereign_ai::pii::signal::login_signal(db.as_ref())).map_err(|e| format!("the login's signal: {e}"))?;
    Ok(())
}

fn f_inbox(w: &mut Walk) -> Step {
    // With no contacts the inbox stays closed and says so on stdout: that is
    // the feature as built for an empty persona.
    w.app.focus_or_toggle(WindowKind::Inbox);
    Ok(())
}

fn f_devices(w: &mut Walk) -> Step {
    w.app.focus_or_toggle(WindowKind::Devices);
    w.app.window_index_of(WindowKind::Devices).map(|_| ()).ok_or_else(|| "the Devices window did not open".into())
}

/// The second device: its own runtime, keys and in-memory store; its files
/// (key databases of the harness) go to the system temporary directory,
/// never to the profile.
struct SecondDevice {
    rt: tokio::runtime::Runtime,
    peer_id: String,
    shell_peer: String,
    account: Option<[u8; 32]>,
    keypair: sovereign_p2p::libp2p::identity::Keypair,
    addrs: Vec<String>,
    events: Option<tokio::sync::mpsc::Receiver<sovereign_p2p::P2pEvent>>,
    cmd: Option<tokio::sync::mpsc::Sender<sovereign_p2p::P2pCommand>>,
}

fn is_duress(w: &Walk) -> bool {
    w.app.persona == Some(CorePersona::Duress)
}

fn kv_out(key: &str, value: &str) {
    out(&format!("KV\t{key}\t{}", value.replace('\t', " ")));
}

/// Pairing. In a duress session (AR3, AR4, v2.0.79: « Aucun moteur ») the
/// gesture is made and nothing may come of it: the walk records whether a
/// node runs, and, if the screen shows a code that decodes as an offer,
/// whether a second device can pair with it. What the screen shows is an
/// open question to Céline: it is not asserted. The gesture counts as
/// driven; AR3 and AR4 read what was recorded. In a primary session, a real
/// pairing with a second node.
fn f_pairing(w: &mut Walk) -> Step {
    if is_duress(w) {
        return duress_pairing(w);
    }
    pair_for_real(w)
}

fn duress_pairing(w: &mut Walk) -> Step {
    kv_out("pairing.node", &w.app.p2p.is_some().to_string());
    let stretches = stretch_count();
    w.app.open_pairing();
    frames(&mut w.app, 2);
    // v7: how many pairing stretches the decoy ran (AR4, the same decoy).
    kv_out("pairing.stretches", &stretch_delta(stretches));
    let shown = w.app.pairing_modal.as_ref().map(|m| (m.code.clone(), m.pin.clone()));
    let real = sovereign_crypto::pair_payload::generate_pairing_code();
    let shape = |s: &str| -> Vec<bool> { s.chars().map(|c| c.is_ascii_alphanumeric()).collect() };
    let primary_peer = sovereign_crypto::auth::AuthStore::load(&crate::crypto::auth_store_path())
        .ok()
        .and_then(|s| s.authenticate(PRIMARY.as_bytes()).ok())
        .and_then(|a| sovereign_p2p::identity::derive_keypair(&a.device_key).ok())
        .map(|kp| kp.public().to_peer_id().to_string())
        .unwrap_or_default();
    let join = match shown {
        None => {
            kv_out("pairing.screen", "none");
            "no offer shown".to_string()
        }
        Some((code, pin)) => {
            w.volatile.push(code.clone());
            w.volatile.push(pin.clone());
            kv_out("pairing.pin_form", if pin.len() == real.len() && shape(&pin) == shape(&real) { "real" } else { "other" });
            match sovereign_p2p::PairingOffer::decode(&code) {
                Err(_) => {
                    kv_out("pairing.screen", "not an offer");
                    "the code shown is not an offer".to_string()
                }
                Ok(offer) => {
                    kv_out("pairing.screen", "offer");
                    kv_out("pairing.offer_is_primarys", &(!primary_peer.is_empty() && offer.source_peer_id == primary_peer).to_string());
                    try_join(w, offer, pin)
                }
            }
        }
    };
    kv_out("pairing.join", &join);
    w.app.pairing_modal = None;
    Ok(())
}

/// A second device tries to pair with `offer` (15 s): « paired », or why not.
fn try_join(w: &mut Walk, offer: sovereign_p2p::PairingOffer, pin: String) -> String {
    let rt = match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return format!("no runtime for the second device: {e}"),
    };
    let mk = sovereign_crypto::master_key::MasterKey::generate();
    let peer = sovereign_crypto::device_key::DeviceKey::derive(&mk, "walk-joiner")
        .ok()
        .and_then(|dk| sovereign_p2p::identity::derive_keypair(&dk).ok())
        .map(|kp| kp.public().to_peer_id().to_string())
        .unwrap_or_default();
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = rt.handle().clone();
    std::thread::spawn(move || {
        let r = handle.block_on(sovereign_p2p::pairing_client::pair_with_source(
            &offer,
            &pin,
            "Walk joiner",
            |_| Ok(peer),
            Duration::from_secs(15),
        ));
        let _ = tx.send(r.map(|_| ()).map_err(|e| e.to_string()));
    });
    let end_at = Instant::now() + Duration::from_secs(25);
    while Instant::now() < end_at {
        frames(&mut w.app, 1);
        if let Ok(r) = rx.try_recv() {
            return match r {
                Ok(()) => "paired".to_string(),
                Err(e) => format!("refused: {e}"),
            };
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    "no answer in 25 s".to_string()
}

fn pair_for_real(w: &mut Walk) -> Step {
    if w.app.p2p.is_none() {
        return Err("the P2P node is not running after the login".into());
    }
    let stretches = stretch_count();
    w.app.open_pairing();
    // v7: how many pairing stretches the real offer ran.
    kv_out("pairing.stretches", &stretch_delta(stretches));
    let (code, pin) = match &w.app.pairing_modal {
        Some(m) => (m.code.clone(), m.pin.clone()),
        None => return Err(format!("no pairing offer; notice {:?}", notice(&w.app))),
    };
    w.volatile.push(code.clone());
    w.volatile.push(pin.clone());
    record_shown(w, "pairing: the offer");
    let offer = sovereign_p2p::PairingOffer::decode(&code).map_err(|e| format!("the offer does not decode: {e}"))?;
    let mk = sovereign_crypto::master_key::MasterKey::generate();
    let dk = sovereign_crypto::device_key::DeviceKey::derive(&mk, "walk-second-device").map_err(|e| e.to_string())?;
    let keypair = sovereign_p2p::identity::derive_keypair(&dk).map_err(|e| e.to_string())?;
    let peer_id = keypair.public().to_peer_id().to_string();
    // The second device's peer id is random per run; the shell's own
    // (`offer.source_peer_id`) is the profile's, and is not masked.
    w.volatile.push(peer_id.clone());
    let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().map_err(|e| e.to_string())?;
    // The joiner runs on its own runtime, on another thread; this thread
    // keeps drawing frames so the shell handles what its node reports.
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = rt.handle().clone();
    let (offer2, pin2, final_peer) = (offer.clone(), pin.clone(), peer_id.clone());
    let second_name = w.second_name;
    std::thread::spawn(move || {
        let r = handle.block_on(sovereign_p2p::pairing_client::pair_with_source(
            &offer2,
            &pin2,
            second_name,
            |_| Ok(final_peer),
            Duration::from_secs(60),
        ));
        let _ = tx.send(r.map(|o| o.secrets.account_key_bytes).map_err(|e| e.to_string()));
    });
    let mut result = None;
    let end = Instant::now() + Duration::from_secs(90);
    while Instant::now() < end {
        frames(&mut w.app, 1);
        if let Ok(r) = rx.try_recv() {
            result = Some(r);
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let account = match result {
        Some(Ok(a)) => a,
        Some(Err(e)) => return Err(format!("the second node's pairing failed: {e} (offer dial hints {:?})", offer.addrs)),
        None => return Err("the pairing did not end in 90 s".into()),
    };
    let paired = frames_until(&mut w.app, 30, |a| notice(a).is_some_and(|n| n.contains("Paired with")));
    // Done, as its button does (`handle_click`'s pairing-modal arm).
    w.app.pairing_modal = None;
    w.second = Some(SecondDevice {
        rt,
        peer_id,
        shell_peer: offer.source_peer_id.clone(),
        account: Some(account),
        keypair,
        addrs: offer.addrs.clone(),
        events: None,
        cmd: None,
    });
    if paired {
        Ok(())
    } else {
        Err(format!("the shell never said it paired; notice {:?}", notice(&w.app)))
    }
}

/// Sync. In a duress session: the walk records whether a node runs (none
/// may, AR4); the gesture counts as driven. In a primary session, a real
/// sync with the paired second node.
fn f_sync(w: &mut Walk) -> Step {
    if is_duress(w) {
        kv_out("sync.node", &w.app.p2p.is_some().to_string());
        return Ok(());
    }
    sync_for_real(w)
}

fn sync_for_real(w: &mut Walk) -> Step {
    use sovereign_p2p::{P2pCommand, P2pConfig, P2pEvent, SovereignNode, SyncService};
    let s = w.second.as_mut().ok_or("no paired second node: « pairing » did not complete")?;
    let account = sovereign_crypto::account_key::AccountKey::from_bytes(s.account.ok_or("no account key")?);
    let kp = s.keypair.clone();
    let (peer, shell_peer, addrs) = (s.peer_id.clone(), s.shell_peer.clone(), s.addrs.clone());
    let started: Result<_, String> = s.rt.block_on(async {
        let (db, docs) = doc_harness::encrypted_store("walk-second").await;
        let svc = Arc::new(
            SyncService::new(
                db as Arc<dyn sovereign_db::GraphDB>,
                peer.clone(),
                account.derive_transport_key(),
                kp.clone(),
                sovereign_p2p::VersionStore::ephemeral(),
            )
            .with_documents(docs),
        );
        let mut keys = std::collections::HashMap::new();
        keys.insert(shell_peer.clone(), account.derive_pair_key(&shell_peer, &peer));
        svc.set_pair_keys(keys);
        let (event_tx, event_rx) = tokio::sync::mpsc::channel::<P2pEvent>(64);
        let (cmd_tx, cmd_rx) = tokio::sync::mpsc::channel::<P2pCommand>(64);
        let cfg = P2pConfig {
            enabled: true,
            listen_port: 0,
            rendezvous_server: None,
            device_name: "Walk second device".into(),
            enable_mdns: false,
            wifi_only: false,
            seed_relays: Vec::new(),
        };
        let mut node = SovereignNode::new(&cfg, kp, event_tx, cmd_rx, svc, None).map_err(|e| e.to_string())?;
        node.listen(&cfg).map_err(|e| e.to_string())?;
        tokio::spawn(async move { node.run().await });
        cmd_tx.send(P2pCommand::UpdatePairedPeers { peer_ids: vec![shell_peer.clone()] }).await.map_err(|e| e.to_string())?;
        for a in &addrs {
            let _ = cmd_tx.send(P2pCommand::Dial { address: format!("{a}/p2p/{shell_peer}") }).await;
        }
        tokio::time::sleep(Duration::from_millis(800)).await;
        cmd_tx.send(P2pCommand::StartSync { peer_id: shell_peer.clone() }).await.map_err(|e| e.to_string())?;
        Ok((event_rx, cmd_tx))
    });
    let (mut events, cmd) = started.map_err(|e| format!("the second node did not start: {e}"))?;
    let end = Instant::now() + Duration::from_secs(60);
    let mut seen = Vec::new();
    let mut synced = None;
    while Instant::now() < end && synced.is_none() {
        frames(&mut w.app, 1);
        while let Ok(ev) = events.try_recv() {
            match &ev {
                P2pEvent::SyncCompleted { .. } => synced = Some(format!("{ev:?}")),
                _ => seen.push(format!("{ev:?}").chars().take(80).collect::<String>()),
            }
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let s = w.second.as_mut().unwrap();
    s.events = Some(events);
    s.cmd = Some(cmd);
    frames(&mut w.app, 10);
    match synced {
        Some(_) => Ok(()),
        None => Err(format!("no SyncCompleted on the second node in 60 s; its events: {:?}", seen.iter().take(8).collect::<Vec<_>>())),
    }
}

fn f_sync_now(w: &mut Walk) -> Step {
    if is_duress(w) {
        // The gesture (Settings → Sync now); its notice is the screen's, not asserted.
        w.app.p2p_sync_now();
        frames(&mut w.app, 10);
        kv_out("sync_now.node", &w.app.p2p.is_some().to_string());
        return Ok(());
    }
    if w.app.p2p.is_none() {
        return Err("the P2P node is not running".into());
    }
    w.app.p2p_sync_now();
    frames(&mut w.app, 10);
    Ok(())
}

/// A device a duress session has never paired (it cannot pair, AR3).
const NO_SUCH_PEER: &str = "12D3KooWDuressWalkNoSuchPairedDevice";

fn f_paired_rename(w: &mut Walk) -> Step {
    if is_duress(w) {
        w.app.rename_paired_device(NO_SUCH_PEER, "Walk renamed device");
        frames(&mut w.app, 2);
        return Ok(());
    }
    let peer = w.second.as_ref().map(|s| s.peer_id.clone()).ok_or("no paired device: « pairing » did not complete")?;
    w.app.rename_paired_device(&peer, "Walk renamed device");
    match &w.app.p2p {
        Some(h) if h.paired_devices(&w.app.rt).iter().any(|(p, n)| *p == peer && n == "Walk renamed device") => Ok(()),
        _ => Err(format!("not renamed; notice {:?}", notice(&w.app))),
    }
}

fn f_paired_forget(w: &mut Walk) -> Step {
    if is_duress(w) {
        w.app.p2p_forget(NO_SUCH_PEER);
        frames(&mut w.app, 2);
        return Ok(());
    }
    let peer = w.second.as_ref().map(|s| s.peer_id.clone()).ok_or("no paired device: « pairing » did not complete")?;
    w.app.p2p_forget(&peer);
    match &w.app.p2p {
        Some(h) if !h.paired_devices(&w.app.rt).iter().any(|(p, _)| *p == peer) => Ok(()),
        _ => Err("the device is still paired".into()),
    }
}

fn f_guardian(w: &mut Walk) -> Step {
    let stretches = enroll_stretch_count();
    // A notice an earlier step left open, closed as Esc does: a notice is above the enrollment (SQ11a), which
    // is read on top.
    w.app.notice = None;
    w.app.open_guardian_enroll();
    // v9: how many enrollment stretches the offer ran (the decoy, in a duress session).
    kv_out("guardian.stretches", &enroll_stretch_delta(stretches));
    match w.app.guardian_modal.as_ref().map(|m| m.code.clone()) {
        Some(code) => {
            w.volatile.push(code);
            record_shown(w, "guardian enrollment: the screen");
            // Done, as its button does (`handle_click`'s guardian-modal arm).
            w.app.guardian_modal = None;
            Ok(())
        }
        None => Err(format!("the enrollment screen did not open; notice {:?}", notice(&w.app))),
    }
}

fn f_alerts(w: &mut Walk) -> Step {
    w.app.refresh_alerts();
    w.app.focus_or_toggle(WindowKind::Alerts);
    Ok(())
}

fn f_previous_values(w: &mut Walk) -> Step {
    w.app.focus_or_toggle(WindowKind::PreviousValues);
    w.app.window_index_of(WindowKind::PreviousValues).map(|_| ()).ok_or_else(|| "the window did not open".into())
}

fn f_quit(w: &mut Walk) -> Step {
    if w.app.request_quit() {
        Ok(())
    } else {
        Err(format!("the app did not quit; prompt {:?}", w.app.unsaved_prompt.is_some()))
    }
}

// ============================================================ the launches

fn out(line: &str) {
    let path = std::env::var_os(OUT_VAR).expect("the output file");
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path).expect("open the output file");
    writeln!(f, "{}", line.replace('\n', "\\n")).expect("write the output file");
}

fn module() -> &'static str {
    module_path!()
}

#[test]
#[ignore = "a launch, run by its parent test in a child process on a profile of its own"]
fn child_entry() {
    let Ok(scenario) = std::env::var(CHILD_VAR) else { return };
    match scenario.as_str() {
        "setup" => child_setup(),
        "primary" => child_primary(),
        "walk" => child_walk(),
        "primary_check" => child_primary_check(),
        "primary_pair" => child_primary_pair(),
        "store_only" => child_store_only(),
        "names_d1" => child_names(DURESS, "d1", None),
        "names_p1" => child_names(PRIMARY, "p1", Some("Primary renamed device")),
        "names_d2" => child_names(DURESS, "d2", Some("Duress renamed device")),
        "names_p2" => child_names(PRIMARY, "p2", None),
        "before_login" => v7::child_before_login(),
        "old_install" => v7::child_old_install(),
        "old_d1" => child_names(DURESS, "o_d1", None),
        "old_d2" => child_names(DURESS, "o_d2", None),
        "old_p" => child_names(PRIMARY, "o_p", None),
        "old_b" => child_names(DURESS, "o_b", None),
        "old_install_nonick" => v7::child_old_install_with_duress_profile(None),
        "old_install_emptynick" => v7::child_old_install_with_duress_profile(Some("")),
        "c_d1" => child_names(DURESS, "c_d1", None),
        "c_d2" => child_names(DURESS, "c_d2", None),
        "e_d1" => child_names(DURESS, "e_d1", None),
        "e_d2" => child_names(DURESS, "e_d2", None),
        "guardian_real" => v7::child_guardian_real(),
        "unreadable_duress" => v7::child_unreadable_duress(),
        other => panic!("unknown scenario {other}"),
    }
}

fn end(mut app: App) {
    shut_runtime(&mut app);
    std::mem::forget(app);
}

fn child_setup() {
    let mut app = App::new();
    first_frame(&mut app);
    fill_wizard(&mut app, PRIMARY, DURESS);
    press_finish(&mut app);
    assert!(frames_until(&mut app, 240, setup_settled), "the setup ends");
    assert!(!app.locked, "setup unlocks: {:?}", app.wizard.as_ref().and_then(|w| w.error.clone()));
    frames(&mut app, 3);
    let _ = app.request_quit();
    end(app);
}

fn child_primary() {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, PRIMARY);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert_eq!(app.persona, Some(CorePersona::Primary), "the primary opens: {:?}", app.auth_form.error);
    let thread = create_thread(&mut app, PRIMARY_LANE).expect("the primary's lane");
    let lane = lane_of(&app, &thread).expect("the primary's lane on the canvas");
    let before: BTreeSet<String> = app.cards.iter().map(|c| c.id.clone()).collect();
    app.new_document(lane);
    let id = app.cards.iter().map(|c| c.id.clone()).find(|i| !before.contains(i)).expect("the primary's document");
    type_and_save(&mut app, &id, PRIMARY_TEXT).expect("typed and saved");
    frames(&mut app, 5);
    for l in &app.lane_labels {
        out(&format!("PRIMARY_ID\t{}", l.thread_id));
    }
    for c in &app.cards {
        out(&format!("PRIMARY_ID\t{}", c.id));
    }
    // AR3: the primary has a paired device of its own.
    let models = PathBuf::from(std::env::var_os(OUT_VAR).unwrap()).with_extension("models");
    let mut w = Walk::new(app, models, "Primary's paired device");
    f_pairing(&mut w).expect("the primary pairs a device");
    let Walk { mut app, second, .. } = w;
    assert!(app.request_quit());
    end(app);
    drop(second);
}

/// The second order's account (from-fable-2nde-position/0019 §2): what the
/// wizard writes before its first session — the setup-started record, then
/// `auth.store` (with the salt and the device id) — and nothing else: a
/// setup interrupted right after the store. Its first login is the walk's,
/// with the duress password.
/// AR2 (v2.0.78-79, Céline: « la persona de contrainte a des noms d'appareil
/// et nom de profil random générés automatiquement ») — one session of the
/// names sequence: it logs in with `password`, reports this session's device
/// name and identity profile (designation, nickname), and, given `rename`,
/// renames this device as Settings does (`set_this_device_name`) and reports
/// the name again.
fn child_names(password: &str, tag: &str, rename: Option<&str>) {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, password);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert!(!app.locked, "the account opens: {:?}", app.auth_form.error);
    frames(&mut app, 3);
    kv_out(&format!("{tag}.device"), &app.device_name);
    let profile = app.identity_profile();
    kv_out(&format!("{tag}.designation"), &profile.as_ref().map(|p| p.designation.clone()).unwrap_or_default());
    kv_out(&format!("{tag}.nickname"), &profile.as_ref().and_then(|p| p.nickname.clone()).unwrap_or_default());
    // v8: the nickname as held (absent, or present and maybe empty).
    kv_out(&format!("{tag}.nickname_raw"), &format!("{:?}", profile.as_ref().map(|p| p.nickname.clone())));
    // v7: the profile's id, and the browser profile this session would get.
    kv_out(&format!("{tag}.user_id"), &profile.as_ref().map(|p| p.user_id.clone()).unwrap_or_default());
    kv_out(&format!("{tag}.browser"), &v7::browser_of(&app));
    if let Some(name) = rename {
        app.set_this_device_name(name);
        frames(&mut app, 2);
        kv_out(&format!("{tag}.device_after"), &app.device_name);
    }
    out("END");
    let _ = app.request_quit();
    end(app);
}

fn child_store_only() {
    crate::crypto::mark_setup_started().expect("the setup-started record");
    crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("auth.store");
}

/// After a walk: a primary session, and what it finds (AR3, open vs write).
fn child_primary_check() {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, PRIMARY);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert_eq!(app.persona, Some(CorePersona::Primary), "the primary opens: {:?}", app.auth_form.error);
    frames(&mut app, 10);
    for l in &app.lane_labels {
        out(&format!("LANE\t{}", l.name));
        out(&format!("SEEN_ID\t{}", l.thread_id));
    }
    for c in &app.cards {
        out(&format!("SEEN_ID\t{}", c.id));
    }
    for c in &app.cards {
        out(&format!("CARD\t{}\t{}", c.title.replace('\t', " "), c.body.replace('\t', " ")));
    }
    let paired = app.p2p.as_ref().map(|h| h.paired_devices(&app.rt)).unwrap_or_default();
    for (_, name) in &paired {
        out(&format!("PAIRED\t{name}"));
    }
    out(&format!("UNREADABLE\t{}", unreadable(&app)));
    // AR4: back to the primary, its node runs under its own device key.
    let own_peer = sovereign_crypto::auth::AuthStore::load(&crate::crypto::auth_store_path())
        .ok()
        .and_then(|s| s.authenticate(PRIMARY.as_bytes()).ok())
        .and_then(|a| sovereign_p2p::identity::derive_keypair(&a.device_key).ok())
        .map(|kp| kp.public().to_peer_id().to_string())
        .unwrap_or_default();
    kv_out("primary.node", &app.p2p.is_some().to_string());
    kv_out(
        "primary.node_is_own",
        &app.p2p.as_ref().map_or(false, |h| !own_peer.is_empty() && h.local_peer_id == own_peer).to_string(),
    );
    out("END");
    end(app);
}

/// AR3's second half: a primary session over a paired list that does not
/// decrypt, which then pairs a device. What it shows is recorded: every
/// notice, and the Devices window.
fn child_primary_pair() {
    let mut app = App::new();
    first_frame(&mut app);
    submit_login(&mut app, PRIMARY);
    assert!(frames_until(&mut app, 240, login_settled), "the login ends");
    assert_eq!(app.persona, Some(CorePersona::Primary), "the primary opens: {:?}", app.auth_form.error);
    let mut notices = Vec::new();
    let end_at = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end_at {
        frames(&mut app, 1);
        if let Some(n) = notice(&app) {
            if !notices.contains(&n) {
                notices.push(n);
            }
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    app.focus_or_toggle(WindowKind::Devices);
    frames(&mut app, 2);
    let tree = read_tree(&mut app);
    for s in &tree.order {
        out(&format!("SHOWN_DEVICES\t{}", s.name()));
    }
    for n in &notices {
        out(&format!("NOTICE\t{n}"));
    }
    let models = PathBuf::from(std::env::var_os(OUT_VAR).unwrap()).with_extension("models");
    let mut w = Walk::new(app, models, "Walk third device");
    let r = f_pairing(&mut w);
    out(&format!("PAIRING\t{}", fmt_step(&r)));
    let Walk { mut app, second, .. } = w;
    let _ = app.request_quit();
    end(app);
    drop(second);
    out("END");
}

fn child_walk() {
    let root = sovereign_core::sovereign_dir();
    for e in duress_area() {
        out(&format!("AREA\t{}\t{}\t{}", e.rel, e.dir, e.source));
    }
    for e in primary_area() {
        out(&format!("PAREA\t{}\t{}", e.rel, e.dir));
    }
    let mut app = App::new();
    first_frame(&mut app);
    let before = snapshot(&root);
    // AR7: the temporary directory of this process (the parent gives each
    // launch one of its own, `TMP`/`TEMP`/`TMPDIR`).
    let temp = std::env::temp_dir();
    let temp_before = snapshot(&temp);
    let model_dir = PathBuf::from(std::env::var_os(OUT_VAR).unwrap()).with_extension("models");
    let mut w = Walk::new(app, model_dir, "Walk second device");
    // The login: the walk's first feature.
    submit_login(&mut w.app, DURESS);
    let opened = frames_until(&mut w.app, 240, login_settled);
    let login: Step = if !opened {
        Err("the login did not end in 240 s".into())
    } else if w.app.persona != Some(CorePersona::Duress) {
        Err(format!("the duress password did not open the duress persona: {:?} {:?}", w.app.persona, w.app.auth_form.error))
    } else {
        Ok(())
    };
    out(&format!("FEATURE\tlogin (the duress password)\t{}", fmt_step(&login)));
    if login.is_ok() {
        // The walk's own model directory, for every feature (chat, models):
        // the same on every machine, and never the repository's real models.
        let _ = std::fs::create_dir_all(&w.model_dir);
        w.app.ai_config.model_dir = w.model_dir.to_string_lossy().into_owned();
        kv_out("login.node", &w.app.p2p.is_some().to_string());
        let fake = install_fake_model(&mut w);
        out(&format!("FAKE_MODEL\t{fake}"));
        frames(&mut w.app, 5);
        record_shown(&mut w, "login");
        for (name, f) in FEATURES {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&mut w)))
                .unwrap_or_else(|p| Err(format!("panicked: {}", panic_text(&p))));
            out(&format!("FEATURE\t{name}\t{}", fmt_step(&r)));
            record_shown(&mut w, name);
        }
    } else {
        for (name, _) in FEATURES {
            out(&format!("FEATURE\t{name}\tFAIL\tnot reached: the duress login failed"));
        }
    }
    frames(&mut w.app, 3);
    out(&format!("UNREADABLE\t{}", unreadable(&w.app)));
    let Walk { app, second, shown, primary_seen, seen_ids, .. } = w;
    // The session ends: its runtime stops (nodes, tasks), then the snapshot.
    end(app);
    drop(second);
    std::thread::sleep(Duration::from_millis(500));
    let after = snapshot(&root);
    for (kind, rel) in diff(&temp_before, &snapshot(&temp)) {
        // The second device's key databases (`doc_harness`, the test's own
        // device) are the only files the test itself puts there.
        if !rel.starts_with("sovereign-e2e-") {
            out(&format!("TEMPWRITE\t{kind}\t{rel}"));
        }
    }
    for id in seen_ids {
        out(&format!("SEEN_ID\t{id}"));
    }
    for s in primary_seen {
        out(&format!("SEEN_PRIMARY\t{s}"));
    }
    for (k, v) in &before {
        out(&format!("BEFORE\t{k}\t{v}"));
    }
    for (kind, rel) in diff(&before, &after) {
        out(&format!("WRITE\t{kind}\t{rel}"));
    }
    for (step, lines) in shown {
        for l in lines {
            out(&format!("SHOWN\t{step}\t{l}"));
        }
    }
    out("END");
}

/// How many rows the session's store met and could not decrypt (open vs
/// write, from-fable-2nde-position/0017 §1), or why it is not known.
fn unreadable(app: &App) -> String {
    let Some(docs) = app.docs.clone() else { return "no document service".into() };
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| docs.store().undecryptable_rows_met())) {
        Ok(n) => n.to_string(),
        Err(p) => format!("not known: {}", panic_text(&p)),
    }
}

fn fmt_step(s: &Step) -> String {
    match s {
        Ok(()) => "OK\t".into(),
        Err(e) => format!("FAIL\t{e}"),
    }
}

fn panic_text(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default()
}

fn tail(s: &str) -> &str {
    let mut at = s.len().saturating_sub(6000);
    while !s.is_char_boundary(at) {
        at += 1;
    }
    &s[at..]
}

/// Runs `scenario` in a child process on `profile`; panics unless it passed.
fn launch(scenario: &str, profile: &Path, out_file: &Path) {
    let rel = module().split_once("::").map(|(_, r)| r).unwrap_or(module());
    let exe = std::env::current_exe().expect("the test binary");
    let temp = out_file.with_extension("tmpdir");
    std::fs::create_dir_all(&temp).unwrap();
    let o = Command::new(exe)
        .args(["--exact", &format!("{rel}::child_entry"), "--ignored", "--nocapture", "--test-threads=1"])
        .env(CHILD_VAR, scenario)
        .env(OUT_VAR, out_file)
        .env("SOVEREIGN_DATA_DIR", profile)
        .env("TMP", &temp)
        .env("TEMP", &temp)
        .env("TMPDIR", &temp)
        .env_remove("SHELL_NO_AUTH")
        .env_remove("SHELL_AUTH_AUTO")
        .env_remove("SHELL_OPEN_WIZARD")
        .env_remove("SHELL_OPEN_JOIN")
        .env_remove("SHELL_SEED_PII_TEST")
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

/// One walk, as the child reported it.
#[derive(Default)]
struct Run {
    area: Vec<(String, bool, String)>,
    primary_area: Vec<(String, bool)>,
    features: Vec<(String, Step)>,
    before: Snap,
    writes: Vec<(String, String)>,
    shown: Vec<(String, Vec<String>)>,
    temp_writes: Vec<(String, String)>,
    seen_primary: Vec<String>,
    seen_ids: BTreeSet<String>,
    primary_ids: BTreeSet<String>,
    unreadable: Option<String>,
    fake_model: bool,
    /// `primary_check` / `primary_pair` launches.
    lanes: Vec<String>,
    cards: Vec<String>,
    paired: Vec<String>,
    notices: Vec<String>,
    shown_devices: Vec<String>,
    pairing: Option<Step>,
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
            ["FEATURE", name, "OK", _] => r.features.push((name.to_string(), Ok(()))),
            ["FEATURE", name, "FAIL", why] => r.features.push((name.to_string(), Err(why.replace("\\n", "\n")))),
            ["BEFORE", k, v] => {
                r.before.insert(k.to_string(), v.to_string());
            }
            ["WRITE", kind, rel] => r.writes.push((kind.to_string(), rel.to_string())),
            ["SHOWN", step, l] => match r.shown.last_mut() {
                Some((s, ls)) if s == step => ls.push(l.to_string()),
                _ => r.shown.push((step.to_string(), vec![l.to_string()])),
            },
            ["TEMPWRITE", kind, rel] => r.temp_writes.push((kind.to_string(), rel.to_string())),
            ["SEEN_PRIMARY", s] => r.seen_primary.push(s.to_string()),
            ["UNREADABLE", n] => r.unreadable = Some(n.to_string()),
            ["KV", k, v] => r.kv.push((k.to_string(), v.to_string())),
            ["SEEN_ID", id] => {
                r.seen_ids.insert(id.to_string());
            }
            ["PRIMARY_ID", id] => {
                r.primary_ids.insert(id.to_string());
            }
            ["FAKE_MODEL", b] => r.fake_model = *b == "true",
            ["LANE", n] => r.lanes.push(n.to_string()),
            ["CARD", t, b] => r.cards.push(format!("{t} | {b}")),
            ["CARD", t] => r.cards.push(t.to_string()),
            ["PAIRED", n] => r.paired.push(n.to_string()),
            ["NOTICE", n] => r.notices.push(n.to_string()),
            ["SHOWN_DEVICES", n] => r.shown_devices.push(n.to_string()),
            ["PAIRING", "OK", _] => r.pairing = Some(Ok(())),
            ["PAIRING", "FAIL", why] => r.pairing = Some(Err(why.to_string())),
            ["END"] => r.complete = true,
            _ => {}
        }
    }
    r
}

struct Walks {
    /// The profile as setup and the primary session left it (a snapshot of P0).
    p0: Snap,
    walk: Run,
    control: Run,
    stripped: Run,
    /// What `stripped` lost before its walk.
    removed: Vec<String>,
    /// A primary session after the walk, on the walk's profile.
    after_walk: Run,
    /// The second order: the duress login first, on a fresh account.
    duress_first: Run,
    /// The primary session at P0 (its ids).
    primary: Run,
    /// The paired list's bytes at P0, before the walk.
    paired_before: Option<Vec<u8>>,
    /// AR3's second half: the corrupted list's bytes before and after a
    /// primary session that pairs a device, and that session's launch.
    corrupt_before: Vec<u8>,
    corrupt_after: Option<Vec<u8>>,
    corrupt: Run,
    /// AR2's names sequence: duress, primary (renames), duress (renames), primary.
    names: Vec<Run>,
}

fn walks() -> &'static Result<Walks, String> {
    static W: OnceLock<Result<Walks, String>> = OnceLock::new();
    W.get_or_init(|| std::panic::catch_unwind(run_walks).map_err(|p| panic_text(&p)))
}

fn run_walks() -> Walks {
    let base = tempfile::Builder::new().prefix("sov-c-area-shell-").tempdir().expect("a temporary base").keep();
    let _ = BASE.set(base.clone());
    let p0 = base.join("p0");
    std::fs::create_dir_all(&p0).unwrap();
    launch("setup", &p0, &base.join("setup.out"));
    launch("primary", &p0, &base.join("primary.out"));
    // v12: the primary's recovery card, naming one enrolled guardian.
    v7::write_primary_recovery_card(&p0);
    let p0_snap = snapshot(&p0);
    let mut runs = BTreeMap::new();
    let mut removed = Vec::new();
    for name in ["walk", "control", "stripped"] {
        let p = base.join(name);
        copy_dir(&p0, &p);
        if name == "stripped" {
            removed = strip_profile(&p);
        }
        let o = base.join(format!("{name}.out"));
        launch("walk", &p, &o);
        runs.insert(name, parse(&o));
    }
    let paired_rel = "crypto/paired_devices.json";
    let paired_before = std::fs::read(p0.join(paired_rel)).ok();
    let after_walk_out = base.join("after-walk.out");
    launch("primary_check", &base.join("walk"), &after_walk_out);
    // AR3's second half: the primary's list made undecryptable (one character
    // of its ciphertext changed), then a primary session that pairs.
    let corrupt = base.join("corrupt");
    copy_dir(&p0, &corrupt);
    let mut bytes = std::fs::read(corrupt.join(paired_rel)).expect("P0 has the primary's paired list");
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let at = ciphertext_at(&text).expect("an encrypted list");
    bytes[at] = if bytes[at] == b'A' { b'B' } else { b'A' };
    std::fs::write(corrupt.join(paired_rel), &bytes).unwrap();
    let corrupt_out = base.join("corrupt.out");
    launch("primary_pair", &corrupt, &corrupt_out);
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
        p0: p0_snap,
        walk: runs.remove("walk").unwrap(),
        control: runs.remove("control").unwrap(),
        stripped: runs.remove("stripped").unwrap(),
        removed,
        after_walk: parse(&after_walk_out),
        duress_first: parse(&df_out),
        primary: parse(&base.join("primary.out")),
        paired_before,
        corrupt_before: bytes,
        corrupt_after: std::fs::read(corrupt.join(paired_rel)).ok(),
        corrupt: parse(&corrupt_out),
        names,
    }
}

/// AR6's second profile: everything that is neither the duress area nor a
/// named exception is deleted. The area is the code's (computed here with
/// the same functions, relative to the data directory).
fn strip_profile(p: &Path) -> Vec<String> {
    let area: Vec<(String, bool)> = duress_area().into_iter().map(|e| (e.rel, e.dir)).collect();
    let snap = snapshot(p);
    let mut removed = Vec::new();
    // Files first, then directories left empty, deepest first.
    for rel in snap.keys().filter(|k| !k.ends_with('/')) {
        if in_area(rel, &area) || exception(rel).is_some() {
            continue;
        }
        std::fs::remove_file(p.join(rel)).unwrap_or_else(|e| panic!("remove {rel}: {e}"));
        removed.push(rel.clone());
    }
    let mut dirs: Vec<&String> = snap.keys().filter(|k| k.ends_with('/')).collect();
    dirs.sort_by_key(|d| std::cmp::Reverse(d.len()));
    for d in dirs {
        let rel = d.trim_end_matches('/');
        if in_area(d, &area) {
            continue;
        }
        if std::fs::read_dir(p.join(rel)).map(|mut r| r.next().is_none()).unwrap_or(false) {
            let _ = std::fs::remove_dir(p.join(rel));
            removed.push(d.clone());
        }
    }
    removed
}

/// The temporary base of the walks, set by `run_walks`.
static BASE: OnceLock<PathBuf> = OnceLock::new();
static FINISHED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static FAILED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The tests of this file that use the walks (each holds a [`Kept`]).
const WALK_TESTS: usize = 22;

/// Held by each test that uses the walks. A failing test keeps the
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
                eprintln!("C-AREA (shell): the temporary profiles are kept for inspection: {}", b.display());
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

fn the_walks() -> &'static Walks {
    match walks() {
        Ok(w) => w,
        Err(e) => panic!("the walks could not run: {e}"),
    }
}

fn kv1(r: &Run, key: &str) -> String {
    r.kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_else(|| "<not reported>".into())
}

fn area_of(r: &Run) -> Vec<(String, bool)> {
    r.area.iter().map(|(a, d, _)| (a.clone(), *d)).collect()
}

fn area_listing(r: &Run) -> String {
    r.area.iter().map(|(a, d, s)| format!("  {a}{} — {s}", if *d { "/" } else { "" })).collect::<Vec<_>>().join("\n")
}

fn undecided_note(rel: &str) -> String {
    match verdict(rel) {
        Some((v, why)) => format!(" [verdict: {v:?} — {why}]"),
        None => " [no verdict: not in the table]".to_string(),
    }
}

// =================================================================== tests

/// AR1's undriven features, **named** (from-fable-2nde-position/0016, the
/// ratchet form of AR8): a feature the walk cannot drive yet is on this list;
/// the test fails when a feature fails that is not on it, and when one on it
/// is driven, or is no longer in the walk (the list must shrink with it).
/// Order is not the test author's to decide; the list is only made explicit.
const NOT_DRIVEN_YET: &[(&str, &str)] =
    &[("the embedded browser", "a wry WebView needs a window; a headless App has none")];

fn not_driven_ratchet(features: &[(String, Step)], listed: &[(&str, &str)], host: &str) -> Vec<String> {
    let mut problems = Vec::new();
    for (n, r) in features {
        let on_list = listed.iter().any(|(l, _)| l == n);
        match (r, on_list) {
            (Err(e), false) => problems.push(format!("not driven, and not on the list (new): {n}: {e}")),
            (Ok(()), true) => problems.push(format!("driven now, still on the list (remove it): {n}")),
            _ => {}
        }
    }
    for (l, _) in listed {
        if !features.iter().any(|(n, _)| n == l) {
            problems.push(format!("on the list, and no longer in the walk: {l}"));
        }
    }
    println!("C-AREA AR1 ({host}): {} not driven yet, named: {:?}", listed.len(), listed.iter().map(|(l, _)| *l).collect::<Vec<_>>());
    problems
}

#[test]
fn ar1_shell_duress_walk_drives_every_feature() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.walk.complete, "the walk did not finish (its output has no END)");
    let problems = not_driven_ratchet(&w.walk.features, NOT_DRIVEN_YET, "shell");
    let driven = w.walk.features.iter().filter(|(_, r)| r.is_ok()).count();
    assert!(
        problems.is_empty(),
        "C-AREA AR1 (shell): {driven} of {} features driven; the list of those not driven yet is not exact:\n  {}",
        w.walk.features.len(),
        problems.join("\n  ")
    );
}

// ===================================================================== AR3

/// AR3 (v2.0.79): « A duress session never changes the primary's list. It has
/// no pairing of its own either (AR4). » The duress walk makes the pairing
/// gesture: no device can pair (if the screen shows a code that decodes as
/// an offer, a second device tries it and is refused), the primary's list is
/// as P0 left it, and a primary session still lists its own device.
/// **The decoy** (v2.0.80, Céline: « Le même leurre que les gardiens »): the
/// usual screen, a code of the real form and a QR that decodes as a pairing
/// offer (not naming the primary's peer id), and it waits forever: nothing
/// is armed, so the second device is refused.
#[test]
fn ar3_shell_a_duress_session_cannot_pair_and_leaves_the_primarys_list() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.walk.complete && w.after_walk.complete, "the walk or the primary session after it did not finish");
    let mut problems = Vec::new();
    let rel = "crypto/paired_devices.json";
    if w.paired_before.is_none() {
        problems.push(format!("precondition: P0 has no {rel} (the primary's pairing did not save)"));
    }
    for (order, r) in [("primary first", &w.walk), ("duress first", &w.duress_first)] {
        let join = kv1(r, "pairing.join");
        if join == "paired" {
            problems.push(format!("a second device paired with the duress session ({order})"));
        }
        if kv1(r, "pairing.node") != "false" {
            problems.push(format!("a node runs when the duress session opens pairing ({order}): an offer can be armed"));
        }
        let screen = kv1(r, "pairing.screen");
        if screen != "offer" {
            problems.push(format!("the duress pairing screen shows no decoy of the real form ({order}): {screen}"));
        }
        if screen == "offer" && kv1(r, "pairing.pin_form") != "real" {
            problems.push(format!("the decoy's code does not have the form of a real pairing code ({order})"));
        }
        if kv1(r, "pairing.offer_is_primarys") == "true" {
            problems.push(format!("the duress pairing screen names the primary's peer id ({order})"));
        }
        for (kind, f) in &r.writes {
            if f == rel {
                problems.push(format!("the duress session ({order}) {kind} {rel}"));
            }
        }
    }
    if !w.after_walk.paired.iter().any(|n| n == "Primary's paired device") {
        problems.push(format!("a primary session after the walk does not list its own device: {:?}", w.after_walk.paired));
    }
    if w.after_walk.paired.iter().any(|n| n.starts_with("Walk")) {
        problems.push(format!("a primary session after the walk lists a device of the duress session: {:?}", w.after_walk.paired));
    }
    assert!(problems.is_empty(), "C-AREA AR3 (shell):\n  {}", problems.join("\n  "));
}

/// AR4 (v2.0.79, « Aucun moteur »): a duress session runs no P2P node —
/// none after its login, none when pairing or sync is asked for — so nothing
/// is announced on the network (no node, no mDNS: the absence of a node is
/// what the test observes), in both setup orders. The shell has no persona
/// switch within a process (no logout): « back to the primary » is the next
/// launch, whose node must run under the primary's own device key.
/// For the same reason, v2.0.80's « a switch ends everything the previous
/// session started » has no shell test: there is no switch to make. It is
/// tested on the phone (`ar4_app_a_switch_ends_everything_…`).
#[test]
fn ar4_shell_a_duress_session_runs_no_node_and_the_primary_runs_its_own() {
    let _kept = Kept;
    let w = the_walks();
    let mut problems = Vec::new();
    for (order, r) in [("primary first", &w.walk), ("duress first", &w.duress_first)] {
        assert!(r.complete, "the {order} walk did not finish");
        for key in ["login.node", "pairing.node", "sync.node", "sync_now.node"] {
            let v = kv1(r, key);
            if v != "false" {
                problems.push(format!("{order}: {key} = {v} (a node runs in the duress session)"));
            }
        }
    }
    for key in ["primary.node", "primary.node_is_own"] {
        let v = kv1(&w.after_walk, key);
        if v != "true" {
            problems.push(format!("the primary session after the walk: {key} = {v}"));
        }
    }
    assert!(problems.is_empty(), "C-AREA AR4 (shell):\n  {}", problems.join("\n  "));
}

/// AR3: « A list that fails to decrypt is never replaced by an empty one. It
/// is kept as it is, and the failure is reported. » One character of the
/// primary's list changed; a primary session opens and pairs a device.
/// **Reading:** « reported » is read as « shown to the user »: a notice, or a
/// line of the Devices window, that names the paired devices and says they
/// could not be read (one of: decrypt, unreadable, be read, not read,
/// couldn't read, kept as it is); and the Devices window does not say « No
/// paired devices yet » over a list it could not read.
#[test]
fn ar3_shell_a_list_that_fails_to_decrypt_is_kept_and_reported() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.corrupt.complete, "the session over the corrupted list did not finish");
    let mut problems = Vec::new();
    match &w.corrupt_after {
        Some(after) if *after == w.corrupt_before => {}
        Some(_) => problems.push("the list that failed to decrypt was replaced (its bytes changed)".into()),
        None => problems.push("the list that failed to decrypt was deleted".into()),
    }
    let names_it = |s: &String| {
        let l = s.to_lowercase();
        l.contains("paired")
            && ["decrypt", "unreadable", "be read", "not read", "couldn't read", "kept as it is"].iter().any(|w| l.contains(w))
    };
    // An empty list shown over one that exists is the list « replaced by an
    // empty one » on screen.
    if w.corrupt.shown_devices.iter().any(|s| s == "No paired devices yet") {
        problems.push("the Devices window says « No paired devices yet » over a list it could not read".into());
    }
    if !w.corrupt.notices.iter().any(names_it) && !w.corrupt.shown_devices.iter().any(names_it) {
        problems.push(format!(
            "the failure is not reported: notices {:?}; Devices window {:?}",
            w.corrupt.notices, w.corrupt.shown_devices
        ));
    }
    println!("C-AREA AR3 (shell): the pairing after the failure: {:?}", w.corrupt.pairing);
    assert!(problems.is_empty(), "C-AREA AR3 (shell):\n  {}", problems.join("\n  "));
}

// ============================================================ open vs write

/// Open vs write (from-fable-2nde-position/0016): a primary session after the
/// duress walk finds none of the objects the duress session created.
#[test]
fn area_shell_a_primary_session_finds_nothing_the_duress_session_created() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.after_walk.complete, "the primary session after the walk did not finish");
    let mut found = Vec::new();
    for l in &w.after_walk.lanes {
        if DURESS_LANES.contains(&l.as_str()) {
            found.push(format!("the lane {l:?}"));
        }
    }
    for c in &w.after_walk.cards {
        if c.contains(DURESS_TEXT) {
            found.push(format!("the card {c:?}"));
        }
    }
    let primary_known: BTreeSet<&String> = w.primary.primary_ids.iter().collect();
    for id in w.walk.seen_ids.iter().filter(|id| !primary_known.contains(id)) {
        if w.after_walk.seen_ids.contains(id) {
            found.push(format!("the record {id} the duress session listed"));
        }
    }
    match w.after_walk.unreadable.as_deref() {
        Some("0") => {}
        other => found.push(format!("rows the primary's store met and could not decrypt: {other:?} (0 expected)")),
    }
    if !w.after_walk.lanes.iter().any(|l| l == PRIMARY_LANE) {
        found.push(format!("(precondition) the primary's own lane is missing: {:?}", w.after_walk.lanes));
    }
    assert!(found.is_empty(), "C-AREA (shell), open vs write: the primary finds the duress session's objects:\n  {}", found.join("\n  "));
}

/// Open vs write: the duress session's lists never contained the primary's
/// lane or document (checked after every feature of the walk).
#[test]
fn area_shell_the_duress_session_never_lists_the_primarys_objects() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.walk.complete, "the walk did not finish");
    assert!(!w.primary.primary_ids.is_empty(), "precondition: the primary session reported its ids");
    assert_eq!(
        w.walk.unreadable.as_deref(),
        Some("0"),
        "C-AREA (shell), open vs write: rows the duress session's store met and could not decrypt (a listing skips \
         them, so they would not show above)"
    );
    let by_id: Vec<String> =
        w.walk.seen_ids.intersection(&w.primary.primary_ids).map(|id| format!("the primary's record {id}")).collect();
    assert!(
        by_id.is_empty(),
        "C-AREA (shell), open vs write: the duress session listed the primary's records (by id):\n  {}",
        by_id.join("\n  ")
    );
    assert!(
        w.walk.seen_primary.is_empty(),
        "C-AREA (shell), open vs write: the duress session listed the primary's objects:\n  {}",
        w.walk.seen_primary.join("\n  ")
    );
}

// ===================================================================== AR7

/// AR7: « Nothing is written outside the Sovereign directory »: the duress
/// walk's process writes nothing in its temporary directory (each launch has
/// its own, `TMP`/`TEMP`/`TMPDIR`). The second device's key databases, the
/// test's own, are left out by their prefix (`sovereign-e2e-`).
/// **Not covered:** the logs (the shell writes none: stdout and stderr only)
/// and places a host could reach by an absolute path (the user's home).
#[test]
fn ar7_shell_the_duress_walk_writes_nothing_in_the_temporary_directory() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.walk.complete, "the walk did not finish");
    let writes: Vec<String> = w.walk.temp_writes.iter().map(|(k, r)| format!("{k} {r}")).collect();
    assert!(
        writes.is_empty(),
        "C-AREA AR7 (shell): the duress walk wrote {} path(s) in the temporary directory:\n  {}",
        writes.len(),
        writes.join("\n  ")
    );
}

/// The body of `fn name` in `src` (its first `{` to the matching `}`).
fn fn_body<'a>(src: &'a str, name: &str) -> &'a str {
    let at = src.find(&format!("fn {name}(")).unwrap_or_else(|| panic!("fn {name} not found"));
    let open = at + src[at..].find('{').unwrap();
    let mut depth = 0usize;
    for (i, c) in src[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &src[open..=open + i];
                }
            }
            _ => {}
        }
    }
    &src[open..]
}

/// AR7: « The browser keeps one profile per persona, in that persona's
/// area. » A WebView cannot be built without a window, so the place the
/// shell hands wry is read from the source: where `sync_browser_webview`
/// builds the WebView, a `WebContext` with a data directory must be given,
/// and that directory must come from the persona. Today none is (« the
/// shell's webview uses the platform's default profile »): red. The
/// behaviour test is `ar7_shell_the_browser_profile_is_in_the_personas_area`
/// (gated on the function it names).
#[test]
fn ar7_shell_the_browser_gets_a_profile_of_the_persona() {
    let src = include_str!("app.rs");
    let body = fn_body(src, "sync_browser_webview");
    let mut problems = Vec::new();
    if !(body.contains("with_web_context") || body.contains("new_with_web_context")) {
        problems.push("the WebView is built with no WebContext: the platform's default profile, one for both personas");
    }
    if !(body.contains("WebContext::new(Some(") || body.contains("data_directory")) {
        problems.push("no data directory is given to the WebView");
    }
    if !body.contains("persona") {
        problems.push("nothing in the WebView's construction depends on the persona");
    }
    assert!(problems.is_empty(), "C-AREA AR7 (shell), the browser's profile:\n  {}", problems.join("\n  "));
}

/// AR7, behaviour: the browser profile directory each persona gets is in its
/// own area, and the two differ: `crate::crypto::persona_browser_profile_dir`
/// (its signature is the test author's, its body the implementer's).
#[test]
fn ar7_shell_the_browser_profile_is_in_the_personas_area() {
    let duress = crate::crypto::persona_browser_profile_dir(CorePersona::Duress);
    let primary = crate::crypto::persona_browser_profile_dir(CorePersona::Primary);
    assert_ne!(duress, primary, "one profile per persona");
    let area: Vec<(String, bool)> = duress_area().into_iter().map(|e| (e.rel, e.dir)).collect();
    let rel = rel_to_data_dir(&duress);
    assert!(in_area(&format!("{rel}/x"), &area), "the duress profile {rel} is in the duress area");
    assert!(!in_area(&format!("{}/x", rel_to_data_dir(&primary)), &area), "the primary's is not");
}

#[test]
fn ar1_shell_duress_walk_writes_only_its_own_area() {
    let _kept = Kept;
    let w = the_walks();
    assert!(w.walk.complete, "the walk did not finish (its output has no END)");
    let area = area_of(&w.walk);
    let outside: Vec<String> = w
        .walk
        .writes
        .iter()
        .filter(|(_, rel)| !in_area(rel, &area) && exception(rel) != Some(May::Change))
        .map(|(kind, rel)| format!("{kind} {rel}{}", undecided_note(rel)))
        .collect();
    assert!(
        outside.is_empty(),
        "C-AREA AR1 (shell): the duress walk created, changed or deleted {} path(s) outside its own area:\n  {}\n\
         The duress area (derived from the code):\n{}",
        outside.len(),
        outside.join("\n  "),
        area_listing(&w.walk)
    );
}

#[test]
fn ar2_shell_every_shared_file_has_its_verdict_and_keeps_it() {
    let _kept = Kept;
    let w = the_walks();
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
    assert!(problems.is_empty(), "C-AREA AR2 (shell):\n  {}", problems.join("\n  "));
}

/// AR1 in the second order (from-fable-2nde-position/0019 §2): a fresh
/// profile whose first login is the duress one, with no primary session
/// before it (the account as a setup interrupted right after the store
/// leaves it). What the first login writes is caught here, not in the
/// primary-first walk.
#[test]
fn ar1_shell_duress_first_walk_writes_only_its_own_area() {
    let _kept = Kept;
    let w = the_walks();
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
        "C-AREA AR1 (shell, duress first): the duress walk created, changed or deleted {} path(s) outside its own area:\n  {}",
        outside.len(),
        outside.join("\n  ")
    );
}

/// AR2 (v2.0.78-79): the duress persona's own names. After setup, its
/// device name and its profile's names (designation, nickname) differ from
/// the primary's and are not copied from them; a later change of the
/// primary's device name does not reach the duress session; the duress
/// session renames its own device and the primary's name stays. What a
/// generated name looks like is not asserted (a risk Céline accepted). The
/// shell has no gesture to change a profile name after setup, so only the
/// device name is renamed.
#[test]
fn ar2_shell_the_duress_session_has_names_of_its_own() {
    let _kept = Kept;
    let w = the_walks();
    let [d1, p1, d2, p2] = [&w.names[0], &w.names[1], &w.names[2], &w.names[3]];
    for r in [d1, p1, d2, p2] {
        assert!(r.complete, "a session of the names sequence did not finish");
    }
    let mut problems = Vec::new();
    for key in ["device", "designation", "nickname"] {
        let (d, p) = (kv1(d1, &format!("d1.{key}")), kv1(p1, &format!("p1.{key}")));
        if d == p {
            problems.push(format!("after setup, the duress session's {key} is the primary's: {d:?}"));
        }
    }
    let renamed = kv1(p1, "p1.device_after");
    if renamed != "Primary renamed device" {
        problems.push(format!("(precondition) the primary did not rename its device: {renamed:?}"));
    }
    for key in ["device", "designation", "nickname"] {
        let (before, after) = (kv1(d1, &format!("d1.{key}")), kv1(d2, &format!("d2.{key}")));
        if before != after {
            problems.push(format!("the duress session's {key} changed between its sessions, after a primary rename: {before:?} -> {after:?}"));
        }
    }
    let own = kv1(d2, "d2.device_after");
    if own != "Duress renamed device" {
        problems.push(format!("the duress session could not rename its own device: {own:?}"));
    }
    let primary_after = kv1(p2, "p2.device");
    if primary_after != "Primary renamed device" {
        problems.push(format!("the duress session's rename reached the primary's device name: {primary_after:?}"));
    }
    assert!(problems.is_empty(), "C-AREA AR2 (shell), the duress persona's names:\n  {}", problems.join("\n  "));
}

#[test]
fn ar6_shell_duress_walk_shows_and_writes_the_same_without_the_primarys_files() {
    let _kept = Kept;
    let w = the_walks();
    for (n, r) in [("walk", &w.walk), ("control", &w.control), ("stripped", &w.stripped)] {
        assert!(r.complete, "the {n} run did not finish");
    }
    let shown_diff = |a: &Run, b: &Run| -> Vec<String> {
        let mut out = Vec::new();
        let steps: BTreeSet<&String> = a.shown.iter().map(|(s, _)| s).chain(b.shown.iter().map(|(s, _)| s)).collect();
        for s in steps {
            let la = a.shown.iter().find(|(x, _)| x == s).map(|(_, l)| l.clone()).unwrap_or_default();
            let lb = b.shown.iter().find(|(x, _)| x == s).map(|(_, l)| l.clone()).unwrap_or_default();
            if la != lb {
                let only_a: Vec<&String> = la.iter().filter(|l| !lb.contains(l)).take(4).collect();
                let only_b: Vec<&String> = lb.iter().filter(|l| !la.contains(l)).take(4).collect();
                out.push(format!("after « {s} »: with the files {only_a:?}; without {only_b:?}"));
            }
        }
        out
    };
    let writes = |r: &Run| -> BTreeSet<(String, String)> {
        r.writes.iter().map(|(k, p)| (k.clone(), numbered(p))).collect()
    };
    let features = |r: &Run| -> Vec<(String, bool)> { r.features.iter().map(|(n, s)| (n.clone(), s.is_ok())).collect() };

    // The noise floor: two runs on the same full profile.
    let noise_shown = shown_diff(&w.walk, &w.control);
    let noise_writes: Vec<_> = writes(&w.walk).symmetric_difference(&writes(&w.control)).cloned().collect();
    assert!(
        noise_shown.is_empty() && noise_writes.is_empty() && features(&w.walk) == features(&w.control),
        "C-AREA AR6 (shell): the comparison is not stable — two walks on the same full profile already differ, \
         so a difference without the primary's files would prove nothing. Shown:\n  {}\nWritten: {:?}",
        noise_shown.join("\n  "),
        noise_writes
    );

    let mut problems = Vec::new();
    for (n, ok) in features(&w.walk) {
        let other = features(&w.stripped).into_iter().find(|(m, _)| *m == n).map(|(_, o)| o);
        if other != Some(ok) {
            problems.push(format!("« {n} »: driven {ok} with the files, {other:?} without"));
        }
    }
    problems.extend(shown_diff(&w.walk, &w.stripped));
    let (a, b) = (writes(&w.walk), writes(&w.stripped));
    for x in a.difference(&b) {
        problems.push(format!("written with the files only: {} {}{}", x.0, x.1, undecided_note(&x.1)));
    }
    for x in b.difference(&a) {
        problems.push(format!("written without the files only: {} {}{}", x.0, x.1, undecided_note(&x.1)));
    }
    assert!(
        problems.is_empty(),
        "C-AREA AR6 (shell): the duress session shows or writes differently when the primary's files and the \
         unnamed shared files are absent ({} removed: {:?}):\n  {}",
        w.removed.len(),
        w.removed,
        problems.join("\n  ")
    );
}

#[test]
fn ar5_shell_walk_names_every_window_and_menu_action() {
    let names: BTreeSet<&str> = FEATURES.iter().map(|(n, _)| *n).collect();
    let kinds = [
        WindowKind::Doc,
        WindowKind::Inbox,
        WindowKind::Chat,
        WindowKind::Settings,
        WindowKind::Search,
        WindowKind::History,
        WindowKind::Models,
        WindowKind::Pii,
        WindowKind::Browser,
        WindowKind::Devices,
        WindowKind::Alerts,
        WindowKind::PreviousValues,
    ];
    let actions = [
        CtxAction::Open,
        CtxAction::TogglePin,
        CtxAction::Delete,
        CtxAction::OpenSkills,
        CtxAction::RunSkill { skill: Default::default() },
        CtxAction::NewThread,
        CtxAction::NewDocument(0),
    ];
    let mut missing = Vec::new();
    for k in kinds {
        if !names.contains(window_feature(k)) {
            missing.push(format!("a window kind → {:?}", window_feature(k)));
        }
    }
    for a in &actions {
        if !names.contains(menu_feature(a)) {
            missing.push(format!("{:?}", menu_feature(a)));
        }
    }
    assert!(missing.is_empty(), "C-AREA AR5 (shell): named but not in the walk: {missing:?}");
}

#[test]
fn the_area_matcher_reads_entries_as_documented() {
    let area = vec![("data/sovereign-duress.db".to_string(), true), ("crypto/keys.duress.db".to_string(), false)];
    assert!(in_area("data/sovereign-duress.db/", &area));
    assert!(in_area("data/sovereign-duress.db/000004.log", &area));
    assert!(in_area("crypto/keys.duress.db", &area));
    assert!(in_area("crypto/keys.duress.db.tmp", &area));
    assert!(!in_area("data/sovereign.db/000004.log", &area));
    assert!(!in_area("crypto/keys.db", &area));
    assert!(!in_area("crypto/keys.duress.db.bak", &area));
    assert!(in_area("data/", &area), "the parent directory of an entry");
    assert!(!in_area("data/other", &area));
    assert_eq!(mask("Today 14:03 thread:abc1 3 documents", &[]), "Today <t> <t> 3 documents");
    assert_eq!(numbered("data/sovereign-duress.db/LOG.old.1794595573956124"), "data/sovereign-duress.db/LOG.old.#");
    assert_eq!(numbered("data/x.db/000017.log"), "data/x.db/#.log");
}

/// The index of the first character of the paired list's ciphertext (the
/// file is pretty-printed JSON: `"ciphertext": "<base64>"`).
fn ciphertext_at(text: &str) -> Option<usize> {
    let k = text.find("\"ciphertext\"")? + "\"ciphertext\"".len();
    let colon = k + text[k..].find(':')?;
    Some(colon + text[colon..].find('"')? + 1)
}

/// How many pairing stretches this process has run (`p2p::pairing_key_stretches`,
/// a seam whose body is the implementer's), or why it is not known.
fn stretch_count() -> Result<usize, String> {
    std::panic::catch_unwind(crate::p2p::pairing_key_stretches).map_err(|p| panic_text(&p))
}

/// How many guardian-enrollment stretches this process has run
/// (`p2p::enroll_key_stretches`, a seam whose body is the implementer's), or
/// why it is not known (`l1c-tests-v9`).
fn enroll_stretch_count() -> Result<usize, String> {
    std::panic::catch_unwind(crate::p2p::enroll_key_stretches).map_err(|p| panic_text(&p))
}

/// The enrollment stretches run since `before`, or why it is not known.
fn enroll_stretch_delta(before: Result<usize, String>) -> String {
    match (before, enroll_stretch_count()) {
        (Ok(a), Ok(b)) => b.saturating_sub(a).to_string(),
        (Err(e), _) | (_, Err(e)) => format!("not known: {e}"),
    }
}

/// The stretches run since `before` (`stretch_count`), or why it is not known.
fn stretch_delta(before: Result<usize, String>) -> String {
    match (before, stretch_count()) {
        (Ok(a), Ok(b)) => b.saturating_sub(a).to_string(),
        (Err(e), _) | (_, Err(e)) => format!("not known: {e}"),
    }
}

// =========================================================== l1c-tests-v7

/// **Seventh round (`l1c-tests-v7`, 2026-11-14; spec v2.0.87-88, ch. 07,
/// AR2 and AR4; claude-coord from-fable-2nde-position/0037, 0038, 0039,
/// from-windows/0267, from-laptop/0261).**
///
/// - **Before any login, on a device that has an account** (v2.0.87-88: « no
///   command answers or writes a persona's identity, on either host. A
///   process with no session has no identity directory. »): the shell's four
///   places that fall back to the primary's when no persona is open — this
///   device's name (`device_name::path_of`, read at start by `App::new`), the
///   identity (`App::identity_profile`), the mail settings
///   (`comms::comms_config_path_of`) and the browser profile
///   (`sync_browser_webview`) — read and, for the two that can be written,
///   written; and the wizard's designation (`App::onboard_designation`),
///   which `ensure_wizard`'s development path opens on such a device.
/// - **An install set up before v0.0.13** (« the same draw happens at the
///   first duress login that finds no names of its own »): an account with
///   the shared `device_name.txt` and `user_profile.json` as v0.0.12 wrote
///   them (read at tag `v0.0.12`: `device_name::save`, `UserProfile::save`),
///   the primary renamed, and no duress names.
/// - **The pairing decoy's key stretch** (AR4, « the same decoy »;
///   SIDECHANNEL-001's rule): held by a count of the one stretch both paths
///   share, never by timing.
///
/// **Seams** (« types and signatures », their bodies the implementer's):
/// `App::browser_profile_dir`, `p2p::stretch_pairing_key`,
/// `p2p::pairing_key_stretches` (tests only).
///
/// **Readings** (reported with the tests):
/// - « Answers a persona's identity » is a value one of the two personas
///   holds (its device name, designation, nickname, profile id, mail
///   address): a default, a fresh value or nothing pass.
/// - « Writes » before any login: any file created, changed or deleted,
///   except AR2's exceptions that may change.
/// - « Drawn at random »: on two installs set up the same way, the first
///   duress login's names differ; a default or a constant would be the same.
///   « The names » are those setup draws (device name, designation,
///   nickname); what a drawn name looks like is not asserted.
/// - The browser's place is read through the seam: a WebView cannot be built
///   without a window, so `sync_browser_webview` is read from the source too.
///
/// **Eighth round (`l1c-tests-v8`; DEL-0006, by delegation of Céline,
/// from-fable-2nde-position/0048, from-laptop/0270):** at the first duress
/// login on an older install, the names are drawn name by name. Two more
/// older installs: a duress profile as v0.0.12 wrote it when a duress session
/// opened Settings (a random designation, no nickname; no duress device
/// name), and one whose nickname is present and empty. « Kept » is the value
/// the session holds after the login, the designation byte for byte and the
/// empty nickname as present (`Some("")`); « drawn » is present and not the
/// value another install's first duress login drew.
mod v7 {
    use super::*;

    const PRIMARY_MAIL: &str = "primary-mail@walk.invalid";
    const DURESS_MAIL: &str = "duress-mail@walk.invalid";
    const BEFORE_LOGIN_MAIL: &str = "before-login@walk.invalid";
    const BEFORE_LOGIN_DEVICE: &str = "Renamed before any login";

    /// v0.0.12's install: the primary renamed its device and its assistant.
    const OLD_PRIMARY_DEVICE: &str = "Old primary device";
    const OLD_PRIMARY_DESIGNATION: &str = "Ikshal-V012-Ω";
    const OLD_PRIMARY_NICKNAME: &str = "Old primary nickname";
    const OLD_PRIMARY_USER_ID: &str = "5f0c0012-0000-4000-8000-000000000012";

    /// `user_profile.json` as v0.0.12 wrote it in the data directory, shared
    /// by the personas (`UserProfile::save`: `serde_json::to_string_pretty`
    /// of the same struct, theme and bubble style still in it), after the
    /// primary named its assistant.
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

    fn mail(user: &str) -> sovereign_comms::config::EmailAccountConfig {
        sovereign_comms::config::EmailAccountConfig {
            imap_host: "imap.walk.invalid".into(),
            imap_port: 993,
            smtp_host: "smtp.walk.invalid".into(),
            smtp_port: 587,
            username: user.into(),
            display_name: None,
        }
    }

    /// The browser profile `app` gives the open session
    /// (`App::browser_profile_dir`), relative to the data directory: `none`,
    /// a path, or why it is not known.
    pub(super) fn browser_of(app: &App) -> String {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.browser_profile_dir())) {
            Ok(None) => "none".into(),
            Ok(Some(p)) => match p.strip_prefix(sovereign_core::sovereign_dir()) {
                Ok(r) => r.to_string_lossy().replace('\\', "/"),
                Err(_) => format!("outside the data directory: {}", p.display()),
            },
            Err(p) => format!("not known: {}", panic_text(&p)),
        }
    }

    /// Before any login, on a device with an account whose two personas have
    /// names of their own (a copy of the names sequence's profile), with each
    /// persona's mail settings written where the code keeps each one's: what
    /// the shell holds and answers with no persona open, and what renaming the
    /// device and saving mail settings write.
    pub(super) fn child_before_login() {
        crate::comms::save_email_config(Some(CorePersona::Primary), &mail(PRIMARY_MAIL)).expect("the primary's mail settings");
        crate::comms::save_email_config(Some(CorePersona::Duress), &mail(DURESS_MAIL)).expect("the duress persona's mail settings");
        let root = sovereign_core::sovereign_dir();
        let mut app = App::new();
        first_frame(&mut app);
        kv_out("bl.locked", &app.locked.to_string());
        kv_out("bl.persona", &format!("{:?}", app.persona));
        // The places, read.
        kv_out("bl.device", &app.device_name);
        match app.identity_profile() {
            None => kv_out("bl.identity", "none"),
            Some(p) => kv_out("bl.identity", &format!("{} | {} | {}", p.designation, p.nickname.unwrap_or_default(), p.user_id)),
        }
        kv_out("bl.mail", &crate::comms::load_email_config(app.persona).map(|c| c.username).unwrap_or_else(|| "none".into()));
        kv_out("bl.browser", &browser_of(&app));
        // The wizard, through the development switch's path (`SHELL_OPEN_WIZARD`).
        app.debug_wizard = Some(0);
        app.ensure_wizard();
        kv_out("bl.wizard", &app.wizard.as_ref().map(|w| w.designation.clone()).unwrap_or_else(|| "none".into()));
        app.wizard = None;
        // The two that can be written.
        let before = snapshot(&root);
        app.set_this_device_name(BEFORE_LOGIN_DEVICE);
        frames(&mut app, 2);
        let mid = snapshot(&root);
        for (k, rel) in diff(&before, &mid) {
            kv_out("bl.device_write", &format!("{k} {rel}"));
        }
        let r = crate::comms::save_email_config(app.persona, &mail(BEFORE_LOGIN_MAIL));
        kv_out("bl.mail_save", &match &r {
            Ok(()) => "Ok".to_string(),
            Err(e) => format!("Err: {e}"),
        });
        for (k, rel) in diff(&mid, &snapshot(&root)) {
            kv_out("bl.mail_write", &format!("{k} {rel}"));
        }
        out("END");
        let _ = app.request_quit();
        end(app);
    }

    /// An account as v0.0.12 left it: the setup's records and `auth.store`
    /// (with the salt and the device id), then the shared `user_profile.json`
    /// and `device_name.txt` written as v0.0.12 wrote them, the primary's
    /// names in them. No duress names: v0.0.12 drew none.
    pub(super) fn child_old_install() {
        crate::crypto::mark_setup_started().expect("the setup-started record");
        crate::crypto::create_auth_store(PRIMARY.as_bytes(), DURESS.as_bytes()).expect("auth.store");
        crate::crypto::mark_setup_complete().expect("the setup-complete record");
        let base = sovereign_core::sovereign_dir();
        std::fs::write(base.join("user_profile.json"), V0012_USER_PROFILE).expect("v0.0.12's shared profile");
        std::fs::write(base.join("device_name.txt"), OLD_PRIMARY_DEVICE).expect("v0.0.12's shared device name");
        out("END");
    }

    /// v8 (DEL-0006): the designation of the duress profile v0.0.12 left.
    const OLD_DURESS_DESIGNATION: &str = "Ikshal-D012-Ж";

    /// The duress persona's `user_profile.json` as v0.0.12 wrote it the first
    /// time a duress session read its identity (Settings): `identity_profile`
    /// (v0.0.12 `app.rs` l. 7401-7414) saved `UserProfile::default_new()`
    /// (`profile.rs` l. 234-252: a random designation, no nickname, the default
    /// bubble style and theme) in `identity_profile_dir(Duress)`, which was
    /// `profile-duress/` (v0.0.12 `sovereign-core/src/auth.rs` l. 41-46).
    /// `nickname` is the JSON of the nickname: `null` as v0.0.12 wrote it, or
    /// `""` for v8's second case.
    fn v0012_duress_profile(nickname: &str) -> String {
        format!(
            r#"{{
  "user_id": "d0e50012-0000-4000-8000-0000000000d1",
  "designation": "{OLD_DURESS_DESIGNATION}",
  "nickname": {nickname},
  "bubble_style": "icon",
  "display_name": null,
  "theme": "dark",
  "created": "2026-10-25T18:00:00.000000+00:00",
  "last_updated": "2026-10-25T18:00:00.000000+00:00",
  "interaction_patterns": {{
    "suggestion_receptiveness": 0.5,
    "command_verbosity": "detailed"
  }},
  "skill_preferences": {{}},
  "suggestion_feedback": {{}}
}}"#
        )
    }

    /// v8 (DEL-0006): an install as v0.0.12 left it after a duress session
    /// opened Settings: [`child_old_install`], then the duress persona's own
    /// profile in v0.0.12's place, with no nickname (`None`) or a nickname
    /// present and equal to `nickname`. No duress device name: v0.0.12 kept
    /// one file, shared (`device_name.rs` l. 49-51).
    pub(super) fn child_old_install_with_duress_profile(nickname: Option<&str>) {
        child_old_install();
        let dir = sovereign_core::sovereign_dir().join("profile-duress");
        std::fs::create_dir_all(&dir).expect("v0.0.12's duress profile directory");
        let json = match nickname {
            None => "null".to_string(),
            Some(n) => format!("{n:?}"),
        };
        std::fs::write(dir.join("user_profile.json"), v0012_duress_profile(&json)).expect("v0.0.12's duress profile");
    }

    fn launch_soft(scenario: &str, profile: &Path, out_file: &Path) -> Result<(), String> {
        std::panic::catch_unwind(|| launch(scenario, profile, out_file)).map_err(|p| panic_text(&p))
    }

    struct V7 {
        launches: BTreeMap<String, (Result<(), String>, Run)>,
    }

    fn runs() -> &'static V7 {
        static V: OnceLock<Result<V7, String>> = OnceLock::new();
        match V.get_or_init(|| std::panic::catch_unwind(run).map_err(|p| panic_text(&p))) {
            Ok(v) => v,
            Err(e) => panic!("the v7 launches could not run: {e}"),
        }
    }

    fn run() -> V7 {
        let _ = the_walks();
        let base = BASE.get().expect("the walks' base").clone();
        let mut launches = BTreeMap::new();
        let mut one = |profile: &Path, scenario: &str, tag: &str| {
            let o = base.join(format!("v7-{tag}.out"));
            let r = launch_soft(scenario, profile, &o);
            launches.insert(tag.to_string(), (r, parse(&o)));
        };
        let bl = base.join("v7-before-login");
        copy_dir(&base.join("names"), &bl);
        one(&bl, "before_login", "before_login");
        let old_a: &[(&str, &str)] = &[("old_install", "old_install_a"), ("old_d1", "o_d1"), ("old_d2", "o_d2"), ("old_p", "o_p")];
        let old_b: &[(&str, &str)] = &[("old_install", "old_install_b"), ("old_b", "o_b")];
        // v8 (DEL-0006): a duress profile of its own as v0.0.12 wrote it, with
        // no nickname; and one whose nickname is present and empty.
        let old_c: &[(&str, &str)] = &[("old_install_nonick", "old_install_c"), ("c_d1", "c_d1"), ("c_d2", "c_d2")];
        let old_e: &[(&str, &str)] = &[("old_install_emptynick", "old_install_e"), ("e_d1", "e_d1"), ("e_d2", "e_d2")];
        // v9: the real guardian offer, in a primary session on a copy of P0.
        let g = base.join("v9-guardian-real");
        copy_dir(&base.join("p0"), &g);
        one(&g, "guardian_real", "guardian_real");
        // v9 (DEL-0010): a duress profile made unreadable, on a copy of the
        // names sequence's profile.
        let u = base.join("v9-unreadable-duress");
        copy_dir(&base.join("names"), &u);
        one(&u, "unreadable_duress", "unreadable_duress");
        for (dir, steps) in [("v7-old-a", old_a), ("v7-old-b", old_b), ("v8-old-c", old_c), ("v8-old-e", old_e)] {
            let p = base.join(dir);
            std::fs::create_dir_all(&p).unwrap();
            for (scenario, tag) in steps {
                one(&p, scenario, tag);
            }
        }
        V7 { launches }
    }

    /// The launch `tag`'s run; panics, with the child's output, if it did not run.
    fn launched(tag: &str) -> &'static Run {
        let (r, run) = runs().launches.get(tag).unwrap_or_else(|| panic!("no launch {tag}"));
        if let Err(e) = r {
            panic!("C-AREA (shell, v7): the launch `{tag}` did not run:\n{e}");
        }
        assert!(run.complete, "C-AREA (shell, v7): the launch `{tag}` did not finish");
        run
    }

    fn kv_all(r: &Run, key: &str) -> Vec<String> {
        r.kv.iter().filter(|(k, _)| k == key).map(|(_, v)| v.clone()).collect()
    }

    /// A persona's names, as its own sessions of the names sequence reported them.
    struct Names {
        device: String,
        designation: String,
        nickname: String,
        user_id: String,
        mail: &'static str,
    }

    /// The primary's and the duress persona's names at the end of the names
    /// sequence (`names_p2`; `names_d2`, after its rename).
    fn persona_names(w: &Walks) -> (Names, Names) {
        let (d2, p2) = (&w.names[2], &w.names[3]);
        for r in [d2, p2] {
            assert!(r.complete, "precondition: the names sequence finished");
        }
        let primary = Names {
            device: kv1(p2, "p2.device"),
            designation: kv1(p2, "p2.designation"),
            nickname: kv1(p2, "p2.nickname"),
            user_id: kv1(p2, "p2.user_id"),
            mail: PRIMARY_MAIL,
        };
        let duress = Names {
            device: kv1(d2, "d2.device_after"),
            designation: kv1(d2, "d2.designation"),
            nickname: kv1(d2, "d2.nickname"),
            user_id: kv1(d2, "d2.user_id"),
            mail: DURESS_MAIL,
        };
        (primary, duress)
    }

    /// Whose value `shown` holds (`the primary's …` / `the duress persona's …`), if any.
    fn whose(shown: &str, p: &Names, d: &Names) -> Vec<String> {
        let mut out = Vec::new();
        for (who, n) in [("the primary's", p), ("the duress persona's", d)] {
            for (what, v) in [
                ("device name", n.device.as_str()),
                ("designation", n.designation.as_str()),
                ("nickname", n.nickname.as_str()),
                ("profile id", n.user_id.as_str()),
                ("mail address", n.mail),
            ] {
                if !v.is_empty() && v != "<not reported>" && shown.contains(v) {
                    out.push(format!("{who} {what} {v:?}"));
                }
            }
        }
        out
    }

    /// Writes reported under `key` that are not AR2's exceptions that may change.
    fn shared_writes(r: &Run, key: &str) -> Vec<String> {
        kv_all(r, key)
            .into_iter()
            .filter(|w| {
                let rel = w.split_once(' ').map(|(_, r)| r).unwrap_or("");
                exception(rel) != Some(May::Change)
            })
            .map(|w| {
                let rel = w.split_once(' ').map(|(_, r)| r.to_string()).unwrap_or_default();
                format!("{w}{}", undecided_note(&rel))
            })
            .collect()
    }

    /// AR2, v2.0.87-88, the shell: « On a device that has an account, before
    /// any login, no command answers or writes a persona's identity … A
    /// process with no session has no identity directory. » The four places
    /// that fall back to the primary's when no persona is open: none answers
    /// a persona's name, address or directory, and renaming the device or
    /// saving mail settings writes no file of either persona.
    #[test]
    fn ar2_shell_before_any_login_no_fallback_resolves_to_a_personas_directory() {
        let _kept = Kept;
        let w = the_walks();
        let r = launched("before_login");
        assert_eq!(kv1(r, "bl.locked"), "true", "precondition: the shell is at its login screen");
        assert_eq!(kv1(r, "bl.persona"), "None", "precondition: no persona is open");
        let (p, d) = persona_names(w);
        let mut problems = Vec::new();
        for (key, place) in [
            ("bl.device", "this device's name, read at start (`device_name::resolve(None, …)`: `path_of(None)` is the primary's file)"),
            ("bl.identity", "the identity profile (`App::identity_profile`: no persona is the primary's directory)"),
            ("bl.mail", "the mail settings (`comms::load_email_config(None)`: `comms_config_path_of(None)` is the primary's file)"),
        ] {
            let shown = kv1(r, key);
            for found in whose(&shown, &p, &d) {
                problems.push(format!("{place} answers {found}: {shown:?}"));
            }
        }
        let browser = kv1(r, "bl.browser");
        let (duress_area, primary_area) = (area_of(&w.walk), w.walk.primary_area.clone());
        if browser.starts_with("not known") {
            problems.push(format!("the browser profile with no persona open is not known (`App::browser_profile_dir`): {browser}"));
        } else if browser != "none" {
            for (who, area) in [("the duress persona's", &duress_area), ("the primary's", &primary_area)] {
                if in_area(&format!("{browser}/x"), area) {
                    problems.push(format!("the browser profile with no persona open is in {who} area: {browser}"));
                }
            }
        }
        for (key, gesture) in [("bl.device_write", "renaming this device"), ("bl.mail_write", "saving mail settings")] {
            for wr in shared_writes(r, key) {
                problems.push(format!("before any login, {gesture} wrote: {wr}"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (shell), before any login on a device that has an account (v2.0.87-88: « no command answers or \
             writes a persona's identity … A process with no session has no identity directory »):\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR2, v2.0.88 (from-fable-2nde-position/0038): the commands that answer
    /// an account's designation are refused once the device has one. In the
    /// shell that is `App::onboard_designation`, which the wizard shows; on a
    /// device with an account, before any login, `ensure_wizard`'s
    /// development path opens the wizard. It is refused there (no wizard) or
    /// shows a designation that is neither persona's.
    #[test]
    fn ar2_shell_the_wizards_designation_is_no_personas_on_a_device_with_an_account() {
        let _kept = Kept;
        let w = the_walks();
        let r = launched("before_login");
        let (p, d) = persona_names(w);
        let wizard = kv1(r, "bl.wizard");
        let found = if wizard == "none" { Vec::new() } else { whose(&wizard, &p, &d) };
        assert!(
            found.is_empty(),
            "C-AREA AR2 (shell): on a device that has an account, before any login, the wizard opened by \
             `ensure_wizard`'s development path shows {} (`App::onboard_designation` reads the data directory's profile); \
             it must be refused there, or answer no persona's",
            found.join(", ")
        );
    }

    /// AR2 (v2.0.87: « An install set up before v0.0.13 never passes through
    /// setup again: there, the same draw happens at the first duress login
    /// that finds no names of its own »). Two installs as v0.0.12 left them
    /// (`old_install`). On the first: two duress logins, then a primary one;
    /// on the second, one duress login. The first duress login's device name,
    /// designation and nickname are drawn (present, and different on the two
    /// installs), differ from the primary's and copy nothing of it (its
    /// profile id neither); the second duress login keeps them; the primary's
    /// stay as v0.0.12 left them.
    #[test]
    fn ar2_shell_an_install_set_up_before_v0013_draws_the_duress_names_at_its_first_duress_login() {
        let _kept = Kept;
        let _ = the_walks();
        for t in ["old_install_a", "old_install_b"] {
            let (r, _) = runs().launches.get(t).expect("the fixture's launch");
            if let Err(e) = r {
                panic!("C-AREA (shell, v7): the v0.0.12 fixture `{t}` could not be written:\n{e}");
            }
        }
        let (a1, a2, ap, b1) = (launched("o_d1"), launched("o_d2"), launched("o_p"), launched("o_b"));
        let get = |r: &Run, tag: &str, key: &str| kv1(r, &format!("{tag}.{key}"));
        let mut problems = Vec::new();
        let old = [
            ("device", OLD_PRIMARY_DEVICE),
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
        for key in ["device", "designation", "nickname"] {
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
            "C-AREA AR2 (shell), an install set up before v0.0.13, at its first duress login:\n  {}",
            problems.join("\n  ")
        );
    }

    /// v9: the real guardian-enrollment offer in a primary session (its
    /// recovery roster is created there, on a copy of P0): how many
    /// enrollment stretches it ran.
    pub(super) fn child_guardian_real() {
        let mut app = App::new();
        first_frame(&mut app);
        submit_login(&mut app, PRIMARY);
        assert!(frames_until(&mut app, 240, login_settled), "the login ends");
        assert_eq!(app.persona, Some(CorePersona::Primary), "the primary opens: {:?}", app.auth_form.error);
        frames(&mut app, 3);
        kv_out("guardian_real.node", &app.p2p.is_some().to_string());
        let stretches = enroll_stretch_count();
        app.open_guardian_enroll();
        kv_out("guardian.stretches", &enroll_stretch_delta(stretches));
        kv_out("guardian_real.screen", &app.guardian_modal.is_some().to_string());
        kv_out("guardian_real.notice", &format!("{:?}", notice(&app)));
        app.guardian_modal = None;
        out("END");
        let _ = app.request_quit();
        end(app);
    }

    /// The identity rows Settings shows (`build_settings`): display name,
    /// nickname, designation.
    fn settings_identity(app: &App) -> Vec<(String, String)> {
        let panel = app.build_settings();
        let mut rows = Vec::new();
        for tab in &panel.tab_rows {
            for (header, k, v) in tab {
                if !*header && ["Display name", "Nickname", "Designation"].contains(&k.as_str()) {
                    rows.push((k.clone(), v.clone()));
                }
            }
        }
        rows
    }

    /// v9 (DEL-0010): the duress persona's profile file, made unreadable (not
    /// valid JSON) before the launch; a duress login; Settings built twice
    /// and the identity read twice; the file's bytes before and after.
    pub(super) fn child_unreadable_duress() {
        let base = sovereign_core::sovereign_dir();
        let file = sovereign_core::auth::identity_profile_dir(&base, CorePersona::Duress).join("user_profile.json");
        kv_out("ud.file_existed", &file.exists().to_string());
        std::fs::write(&file, b"{ not a profile").expect("the duress profile, made unreadable");
        let before = std::fs::read(&file).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable: {e}"));
        let mut app = App::new();
        first_frame(&mut app);
        submit_login(&mut app, DURESS);
        assert!(frames_until(&mut app, 240, login_settled), "the login ends");
        kv_out("ud.persona", &format!("{:?}", app.persona));
        frames(&mut app, 3);
        for n in 1..=2 {
            for (k, v) in settings_identity(&app) {
                kv_out(&format!("ud.settings{n}.{k}"), &v);
            }
            let id = app.identity_profile().map(|p| format!("{} | {:?}", p.designation, p.nickname));
            kv_out(&format!("ud.identity{n}"), &id.unwrap_or_else(|| "none".into()));
        }
        let after = std::fs::read(&file).map(|b| fnv(&b)).unwrap_or_else(|e| format!("unreadable: {e}"));
        kv_out("ud.file_before", &before);
        kv_out("ud.file_after", &after);
        out("END");
        let _ = app.request_quit();
        end(app);
    }

    /// v9, AR4 (the guardian enrollment's decoy; claude-coord
    /// from-windows/0286, from-laptop/0275): the duress session's guardian
    /// decoy goes through the same key stretch as the real offer, held by
    /// structure, not by timing, in the form of the pairing decoy's test. The
    /// walk counts the runs of the one stretch both paths share
    /// (`p2p::stretch_enroll_key`, counted by `p2p::enroll_key_stretches`)
    /// around a primary session's real enrollment (`guardian_real`) and
    /// around the duress session's (`walk`): the real offer runs it, and the
    /// decoy as many times.
    #[test]
    fn ar4_shell_the_guardian_decoy_goes_through_the_same_key_stretch_as_the_real_offer() {
        let _kept = Kept;
        let w = the_walks();
        assert!(w.walk.complete, "the walk did not finish");
        let r = launched("guardian_real");
        assert_eq!(kv1(r, "guardian_real.node"), "true", "precondition: the primary's node runs");
        let real = kv1(r, "guardian.stretches");
        let decoy = kv1(&w.walk, "guardian.stretches");
        let mut problems = Vec::new();
        if kv1(r, "guardian_real.screen") != "true" {
            problems.push(format!(
                "(precondition) the primary's real enrollment screen did not open: {}",
                kv1(r, "guardian_real.notice")
            ));
        }
        if !real.parse::<usize>().is_ok_and(|n| n >= 1) {
            problems.push(format!(
                "the real guardian offer (a primary session's enrollment): runs of the shared stretch \
                 `p2p::stretch_enroll_key` counted: {real} (one or more expected)"
            ));
        }
        if decoy != real {
            problems.push(format!(
                "runs of the shared enrollment stretch counted: {decoy} around the duress session's decoy, {real} around \
                 the real offer: the decoy skips the key stretch the real offer waits on (`p2p::decoy_guardian_offer`)"
            ));
        }
        assert!(problems.is_empty(), "C-AREA AR4 (shell), the guardian enrollment's decoy:\n  {}", problems.join("\n  "));
    }

    /// The enrollment stretch both paths share is the guardian handshake's
    /// key derivation itself (`sovereign_p2p::guardian_enroll::derive_enroll_key`),
    /// so the count above counts the real work.
    #[test]
    fn ar4_shell_the_shared_enrollment_stretch_is_the_enroll_key_derivation() {
        let offer = sovereign_p2p::guardian_enroll::GuardianEnrollOffer::new(
            "12D3KooWEnrollSeamFixture".into(),
            "Walk device".into(),
            Vec::new(),
            sovereign_p2p::guardian_enroll::GUARDIAN_OFFER_TTL_SECONDS,
        );
        let code = "ABCDE-FGHJK";
        let want = sovereign_p2p::guardian_enroll::derive_enroll_key(code, &offer).expect("the reference derivation");
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::p2p::stretch_enroll_key(code, &offer)))
            .map_err(|p| panic_text(&p));
        assert!(
            matches!(&got, Ok(Ok(k)) if *k == want),
            "C-AREA AR4 (shell): `p2p::stretch_enroll_key` is not the enrollment key derivation: {:?}",
            got.map(|r| r.map(|_| "a different key"))
        );
    }

    /// v9, DEL-0010 (the Fable maille, by delegation of Céline, 2026-11-14): a
    /// duress profile that cannot be read shows « (not set) », as the
    /// primary's does, and the file is left as it is. The duress persona's
    /// profile file is made unreadable (not valid JSON); in a duress session,
    /// Settings is built twice and the identity read twice: « (not set) » for
    /// the display name, the nickname and the designation both times, no
    /// designation drawn, nothing of the primary's, and the file byte for
    /// byte as it was.
    #[test]
    fn del0010_shell_an_unreadable_duress_profile_shows_not_set_and_is_left_as_it_is() {
        let _kept = Kept;
        let w = the_walks();
        let r = launched("unreadable_duress");
        assert_eq!(kv1(r, "ud.file_existed"), "true", "precondition: the duress persona had a profile file of its own");
        assert_eq!(kv1(r, "ud.persona"), "Some(Duress)", "precondition: the duress session opens");
        let primary_designation = kv1(&w.names[3], "p2.designation");
        let mut problems = Vec::new();
        for n in 1..=2 {
            for key in ["Display name", "Nickname", "Designation"] {
                let v = kv1(r, &format!("ud.settings{n}.{key}"));
                if v != "(not set)" {
                    problems.push(format!("Settings, opening {n}: {key} shows {v:?} (« (not set) » expected)"));
                }
                if !primary_designation.is_empty() && v == primary_designation {
                    problems.push(format!("Settings, opening {n}: {key} shows the primary's designation {v:?}"));
                }
            }
            let id = kv1(r, &format!("ud.identity{n}"));
            if id != "none" {
                problems.push(format!("reading {n}: the identity answers a profile ({id}): a designation drawn, or read from elsewhere"));
            }
        }
        let (before, after) = (kv1(r, "ud.file_before"), kv1(r, "ud.file_after"));
        if before != after {
            problems.push(format!("the unreadable profile file changed: {before} -> {after}"));
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (shell), DEL-0010: a duress profile that cannot be read:\n  {}",
            problems.join("\n  ")
        );
    }

    /// v12: the owner tag and the enrolled guardian of the primary's card.
    const CARD_OWNER_TAG: &str = "walk-v12-owner-tag-of-the-primary";
    const CARD_GUARDIAN: &str = "12D3KooWWalkV12PrimarysEnrolledGuardianPeer";

    /// v12 (from-fable-2nde-position/0066, the shell's form of v11): the
    /// run's primary holds a recovery card naming one enrolled guardian, in
    /// the form and place the real store writes it
    /// (`RecoveryStore::write_recovery_card`: `serde_json::to_vec_pretty` of
    /// a `RecoveryCard`, at `crypto/recovery_card.json`), so that a decoy that
    /// read it can be told apart from an empty primary.
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

    /// v12, AR4 and AR2 (spec v2.0.103: a duress session's guardian decoy
    /// « answers as a persona with no roster »): with the primary's recovery
    /// card naming one enrolled guardian in the duress walk's profile, the
    /// shell's enrollment screen in a duress session is that of a persona
    /// with no roster: « Enroll guardian 1 of 5 »; nothing it shows holds the
    /// card's owner tag or guardian peer id; AR6's stripped run, without the
    /// card, shows the same; no file of the primary's recovery store is
    /// written.
    #[test]
    fn ar4_shell_the_guardian_decoy_answers_as_a_persona_with_no_roster_whatever_the_primarys_card_says() {
        let _kept = Kept;
        let w = the_walks();
        let card = "crypto/recovery_card.json";
        assert!(w.p0.contains_key(card), "precondition: the run's primary holds a recovery card");
        assert!(w.walk.before.contains_key(card), "precondition: the card is in the duress walk's profile (its « before »)");
        let step = "guardian enrollment: the screen";
        let lines = |r: &Run| -> Vec<String> {
            r.shown.iter().find(|(s, _)| s == step).map(|(_, l)| l.clone()).unwrap_or_default()
        };
        let titles = |r: &Run| -> Vec<String> { lines(r).into_iter().filter(|l| l.contains("Enroll guardian")).collect() };
        let mut problems = Vec::new();
        let (with, without) = (titles(&w.walk), titles(&w.stripped));
        if !with.iter().any(|l| l.contains("Enroll guardian 1 of 5")) {
            problems.push(format!(
                "the decoy's screen is not that of a persona with no roster (« Enroll guardian 1 of 5 » expected), while \
                 the primary's card names one enrolled guardian: {with:?}"
            ));
        }
        for l in lines(&w.walk) {
            for (what, v) in [("owner tag", CARD_OWNER_TAG), ("enrolled guardian's peer id", CARD_GUARDIAN)] {
                if l.contains(v) {
                    problems.push(format!("the decoy's screen shows the card's {what}: {l}"));
                }
            }
        }
        if !w.removed.iter().any(|r| r == card) {
            problems.push("(precondition) AR6's stripped run still holds the card".to_string());
        }
        if with != without {
            problems.push(format!("the decoy's screen differs with the primary's card {with:?} and without it {without:?}"));
        }
        for (kind, rel) in &w.walk.writes {
            if matches!(verdict(rel), Some((Verdict::PrimaryOnly, _))) {
                problems.push(format!("the duress session {kind} {rel} (the primary's)"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR4, AR2 (shell), the guardian decoy and the primary's recovery card:\n  {}",
            problems.join("\n  ")
        );
    }

    /// The device name and nickname another install's first duress login drew
    /// (`o_b`, the v0.0.12 install with no duress names), and the primary's.
    fn other_draws_and_primary() -> (Vec<(&'static str, String)>, [(&'static str, &'static str); 3]) {
        let b1 = launched("o_b");
        let draws = vec![("device", kv1(b1, "o_b.device")), ("nickname", kv1(b1, "o_b.nickname"))];
        let primary = [("device", OLD_PRIMARY_DEVICE), ("designation", OLD_PRIMARY_DESIGNATION), ("nickname", OLD_PRIMARY_NICKNAME)];
        (draws, primary)
    }

    /// v8, AR2 and DEL-0006 (by delegation of Céline, 2026-11-14: « name by
    /// name »): on an install as v0.0.12 left it after a duress session opened
    /// Settings — a duress profile of its own with a random designation and no
    /// nickname, and no duress device name — the first duress login draws the
    /// nickname and the device name, keeps the designation byte for byte, and
    /// none of them is the primary's. A second duress login keeps them.
    /// « Drawn »: present, and not the value another install's first duress
    /// login drew.
    #[test]
    fn ar2_shell_an_older_install_with_a_duress_profile_and_no_nickname_draws_the_missing_names() {
        let _kept = Kept;
        let _ = the_walks();
        let (r, _) = runs().launches.get("old_install_c").expect("the fixture's launch");
        if let Err(e) = r {
            panic!("C-AREA (shell, v8): the v0.0.12 fixture could not be written:\n{e}");
        }
        let (c1, c2) = (launched("c_d1"), launched("c_d2"));
        let (draws, primary) = other_draws_and_primary();
        let get = |r: &Run, tag: &str, key: &str| kv1(r, &format!("{tag}.{key}"));
        let mut problems = Vec::new();
        let designation = get(c1, "c_d1", "designation");
        if designation != OLD_DURESS_DESIGNATION {
            problems.push(format!(
                "the duress designation v0.0.12 left, {OLD_DURESS_DESIGNATION:?}, is not kept: the session holds {designation:?}"
            ));
        }
        for (key, other) in &draws {
            let v = get(c1, "c_d1", key);
            if v.is_empty() {
                problems.push(format!("the first duress login drew no {key}"));
            } else if v == *other {
                problems.push(format!(
                    "the duress {key} is {v:?}, the value another install's first duress login has: not drawn at random \
                     (a default, or a value both share)"
                ));
            }
        }
        for (key, p) in primary {
            let v = get(c1, "c_d1", key);
            if v == p {
                problems.push(format!("the duress {key} is the primary's: {v:?}"));
            }
        }
        for key in ["device", "designation", "nickname"] {
            let (v, again) = (get(c1, "c_d1", key), get(c2, "c_d2", key));
            if again != v {
                problems.push(format!("a second duress login changed the duress {key}: {v:?} -> {again:?}"));
            }
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (shell), DEL-0006: an install set up before v0.0.13 whose duress persona has a profile and no \
             nickname, nor device name, at its first duress login:\n  {}",
            problems.join("\n  ")
        );
    }

    /// v8, AR2 and DEL-0006: « A nickname that is there, even empty, is the
    /// user's and is kept. » On an install as v0.0.12 left it, with a duress
    /// profile whose nickname is present and empty and no duress device name:
    /// after the first duress login (and a second) the nickname is still
    /// present and empty; the designation is kept; the device name is drawn,
    /// and is not the primary's.
    #[test]
    fn ar2_shell_an_older_install_keeps_a_duress_nickname_that_is_there_even_empty() {
        let _kept = Kept;
        let _ = the_walks();
        let (r, _) = runs().launches.get("old_install_e").expect("the fixture's launch");
        if let Err(e) = r {
            panic!("C-AREA (shell, v8): the v0.0.12 fixture could not be written:\n{e}");
        }
        let (e1, e2) = (launched("e_d1"), launched("e_d2"));
        let (draws, _) = other_draws_and_primary();
        let get = |r: &Run, tag: &str, key: &str| kv1(r, &format!("{tag}.{key}"));
        let mut problems = Vec::new();
        for (r, tag, when) in [(e1, "e_d1", "the first duress login"), (e2, "e_d2", "a second duress login")] {
            let raw = get(r, tag, "nickname_raw");
            if raw != r#"Some(Some(""))"# {
                problems.push(format!("after {when}, the nickname that was there and empty is {raw} (Some(Some(\"\")) expected)"));
            }
            let designation = get(r, tag, "designation");
            if designation != OLD_DURESS_DESIGNATION {
                problems.push(format!("after {when}, the designation {OLD_DURESS_DESIGNATION:?} is not kept: {designation:?}"));
            }
        }
        let device = get(e1, "e_d1", "device");
        let other = draws.iter().find(|(k, _)| *k == "device").map(|(_, v)| v.clone()).unwrap_or_default();
        if device.is_empty() {
            problems.push("the first duress login drew no device name".to_string());
        } else if device == other {
            problems.push(format!(
                "the duress device is {device:?}, the value another install's first duress login has: not drawn at random"
            ));
        }
        if device == OLD_PRIMARY_DEVICE {
            problems.push(format!("the duress device is the primary's: {device:?}"));
        }
        let again = get(e2, "e_d2", "device");
        if again != device {
            problems.push(format!("a second duress login changed the duress device: {device:?} -> {again:?}"));
        }
        assert!(
            problems.is_empty(),
            "C-AREA AR2 (shell), DEL-0006: an install set up before v0.0.13 whose duress profile has a nickname present \
             and empty, at its first duress login:\n  {}",
            problems.join("\n  ")
        );
    }

    /// AR4 (« Pairing shows the same decoy »; from-fable-2nde-position/0037
    /// §2): the duress session's pairing decoy goes through the same key
    /// stretch as the real offer, held by structure, not by timing. The walk
    /// counts the runs of the one stretch both paths share
    /// (`p2p::stretch_pairing_key`, counted by `p2p::pairing_key_stretches`)
    /// around a primary session's « Pair a device » (`primary` launch) and
    /// around the duress session's (`walk`): the real offer runs it, and the
    /// decoy as many times.
    #[test]
    fn ar4_shell_the_pairing_decoy_goes_through_the_same_key_stretch_as_the_real_offer() {
        let _kept = Kept;
        let w = the_walks();
        assert!(w.walk.complete, "the walk did not finish");
        let real = kv1(&w.primary, "pairing.stretches");
        let decoy = kv1(&w.walk, "pairing.stretches");
        let mut problems = Vec::new();
        if !real.parse::<usize>().is_ok_and(|n| n >= 1) {
            problems.push(format!(
                "the real offer (a primary session's « Pair a device »): runs of the shared stretch \
                 `p2p::stretch_pairing_key` counted: {real} (one or more expected)"
            ));
        }
        if decoy != real {
            problems.push(format!(
                "runs of the shared stretch counted: {decoy} around the duress session's decoy, {real} around the real \
                 offer: the decoy skips the key stretch the real offer waits on (`p2p::decoy_pairing_offer`)"
            ));
        }
        assert!(problems.is_empty(), "C-AREA AR4 (shell), the pairing decoy:\n  {}", problems.join("\n  "));
    }

    /// The stretch both paths share is the pairing handshake's key derivation
    /// itself (`sovereign_p2p::pairing_offer::derive_handshake_key`), so the
    /// count above counts the real work.
    #[test]
    fn ar4_shell_the_shared_pairing_stretch_is_the_handshake_key_derivation() {
        let offer = sovereign_p2p::PairingOffer::new("12D3KooWStretchSeamFixture".into(), "Walk device".into(), Vec::new(), 600);
        let code = "ABCDE-FGHJK";
        let want = sovereign_p2p::pairing_offer::derive_handshake_key(code, &offer).expect("the reference derivation");
        let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| crate::p2p::stretch_pairing_key(code, &offer)))
            .map_err(|p| panic_text(&p));
        assert!(
            matches!(&got, Ok(Ok(k)) if *k == want),
            "C-AREA AR4 (shell): `p2p::stretch_pairing_key` is not the handshake key derivation: {:?}",
            got.map(|r| r.map(|_| "a different key"))
        );
    }

    /// AR7 and AR2 (v2.0.87): the browser's profile comes from the open
    /// session, never from a fallback. `sync_browser_webview` takes it from
    /// `App::browser_profile_dir` (read from the source: a WebView cannot be
    /// built without a window), with no `unwrap_or(CorePersona::Primary)`;
    /// in a duress session it is in the duress persona's area, in a primary
    /// session in the primary's (the names sequence's first two sessions).
    #[test]
    fn ar7_shell_the_browser_profile_comes_from_the_open_session() {
        let _kept = Kept;
        let w = the_walks();
        let mut problems = Vec::new();
        let body = fn_body(include_str!("app.rs"), "sync_browser_webview");
        if !body.contains("browser_profile_dir()") {
            problems.push("`sync_browser_webview` does not take the WebView's profile from `App::browser_profile_dir()`".to_string());
        }
        if body.contains("unwrap_or(CorePersona::Primary)") {
            problems.push(
                "`sync_browser_webview` falls back to the primary's profile when no persona is open \
                 (`unwrap_or(CorePersona::Primary)`)"
                    .to_string(),
            );
        }
        let (duress, primary) = (area_of(&w.walk), w.walk.primary_area.clone());
        for (tag, run, area, who) in [("d1", &w.names[0], &duress, "the duress persona's"), ("p1", &w.names[1], &primary, "the primary's")] {
            let b = kv1(run, &format!("{tag}.browser"));
            if !in_area(&format!("{b}/x"), area) {
                problems.push(format!("in {who} session, the browser profile is {b:?}, not in {who} area"));
            }
        }
        assert!(problems.is_empty(), "C-AREA AR7 (shell), the browser's profile:\n  {}", problems.join("\n  "));
    }
}
