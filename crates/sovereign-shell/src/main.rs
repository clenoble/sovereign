//! Sovereign native shell — spatial-canvas foundation (native-UI migration).
//!
//! See doc/plans/archive/plans/native-ui-migration.md. The REAL shell foundation (promoted
//! from the proven spike): a winit + wgpu + Vello window rendering the spatial
//! timeline canvas — time (X) × thread-lane (Y), 4 LOD tiers, real shaped
//! titles (parley), per-thread colors, cross-thread link edges, an adaptive
//! time axis, a minimap with a live viewport box, and the owned-vs-external
//! provenance shape language (rounded rect vs parallelogram).
//!
//! Phase 0a (here): synthetic data, proving the render foundation in-tree.
//! Phase 0b (next): wire the backend crates IN-PROCESS (no Tauri IPC) so the
//! canvas shows the real workspace (documents/threads/relationships from
//! sovereign-db; provenance from ownership; titles decrypted post-login).
//!
//! Controls: drag = pan, scroll = zoom (toward cursor). FPS / visible-cards /
//! visible-links / zoom in the window title.

mod app;
mod approval;
// v0.0.12 item 18: the locked screens' key derivations, run on a worker
// thread (`doc/plans/V0012-items18-20-first-run.md`, B1–B4).
mod auth_work;
mod camera;
mod canvas;
// C-DOC adoption step 5: the card menu by document id (S5-4) and the trash
// view (S5-5..S5-7).
mod card_menu;
// v0.0.12 item 21 (`doc/plans/V0012-item21-composer.md`): the chat composer
// and the search query, on the editor's single-line field.
mod chat_field;
mod clip;
mod comms;
mod crypto;
// S5-8: the one-click delete's notice and Undo.
mod delete_notice;
mod dev;
mod display_check;
// v0.0.12 item 19 (`doc/plans/V0012-items14-19-devices.md`): this device's
// name (N1, N2) and the Rename field of Settings → Devices (N3, N5, N6).
mod device_name;
mod device_rename;
mod doc_writes;
// v0.0.12, no OpenGL backend on Windows (`doc/plans/V0012-no-gl-windows.md`):
// the one place the shell builds its wgpu instance (W4).
mod gpu;
// Renaming a thread in its lane label (contract R1–R6, `doc/plans/THREAD-name.md`):
// the label's field, its hit-testing and its save; the seam to the one stamped
// thread-write path (THREAD-sync S1).
mod lane_rename;
// v0.0.12 item 14: History names the device (R1) on one line (R2).
mod history_device;
mod thread_writes;
// Naming a new thread (contract N1–N6, `doc/plans/THREAD-name.md`): the prompt
// and the creation path.
mod new_thread;
mod onboarding;
mod p2p;
mod panels;
mod same_name;
mod log_window;
mod panic_hook;
mod recovery;
mod text;
// C-LOG's log window: the seams its scrolling test needs.
mod theme;
// The trash (Settings → Trash) and its drawing.
mod trash;
mod trash_ui;
// Documents whose thread was deleted (S5-12), in the Alerts window, and its
// two dialogs (the thread to move into, the new thread's name).
mod orphans;
mod orphans_ui;
// The S5-12 tests (W3, W5), written before the code; the implementer does not
// edit them.
#[cfg(test)]
mod orphans_tests;
// Test helper: the name Narrator receives for a node (v0.0.12 rc2,
// `doc/plans/V0012-rc2-labels-spoken.md`), shared by the tests that read it.
#[cfg(test)]
mod a11y_spoken;
// winit keys / input methods / clicks → the editor component's input events,
// shared by the document editor and the new-thread prompt's field.
mod winit_input;
// The step-5 tests (S5-4 to S5-7), written before the code; kept in their own
// file so whoever implements step 5 does not edit them.
#[cfg(test)]
mod step5_tests;
// Title editing in the document editor's window (contract T1–T7,
// `doc/plans/TITLE-edit.md`): the shell's tests, written before the code; the
// implementer does not edit them. Compiled in every test build, release
// included: the editor is in every build (contract E1).
#[cfg(test)]
mod title_tests;
// Naming a new thread (contract N1–N6, `doc/plans/THREAD-name.md`): the tests,
// written before the code; the implementer does not edit them.
#[cfg(test)]
mod thread_name_tests;
// Renaming a thread in its lane label (contract R1–R6, `doc/plans/THREAD-name.md`):
// the tests, written before the code; the implementer does not edit them.
#[cfg(test)]
mod thread_rename_tests;
// C-HOST AT8 (spec ch. 08, v2.0.78): a lane's rename, the Skills menu and
// document windows go by id, through the shared functions; written by the LH
// test author, the implementer does not edit them.
#[cfg(test)]
mod at8_identity_tests;
// C-HOST AT6 in the shell (spec ch. 08; Fable 0050): the modal counts down to
// the deadline the proposal carries, and the answer is bound to its request id;
// written by the LH test author, the implementer does not edit them.
#[cfg(test)]
mod at6_shell_tests;
// rc2: the auth gate's and the wizard's text fields are readable in both themes
// (spec ch. 12, *Onboarding and the auth gate*): the tests, written before the
// code; the implementer does not edit them.
#[cfg(test)]
mod field_contrast_tests;
// v0.0.13 rc2: a focused text field's border is seen in both themes (spec
// ch. 12, *Theme*, TC3): the tests, written before the code; the implementer
// does not edit them.
#[cfg(test)]
mod focused_field_border_tests;
// v0.0.13 rc2: text on the accent and the gravity word are read in both themes
// (spec ch. 12, *Theme*, TC1, TC2, TC4): the tests, written before the code;
// the implementer does not edit them.
#[cfg(test)]
mod text_on_fill_tests;
// v0.0.13 rc2: a chosen row's mark is seen in both themes (spec ch. 12,
// *Theme*, TC6): the tests, written before the code; the implementer does not
// edit them.
#[cfg(test)]
mod chosen_row_outline_tests;
// rc2: the desktop's first writes of auth.store are exclusive (spec ch. 07,
// *Auth store*, as extended): the tests, written before the code; the
// implementer does not edit them.
#[cfg(test)]
mod auth_store_write_tests;
// rc2 Q2 (laptop's second position): the salt restored from auth.store, through
// `crypto::load_or_create_salt_in`. The tests, written before the code; the
// implementer does not edit them.
#[cfg(test)]
mod salt_restore_tests;
// v0.0.12 item 21 (`doc/plans/V0012-item21-composer.md`): K3b through the
// rename and new-thread seams. The tests, written before the code; the
// implementer does not edit them.
#[cfg(test)]
mod field_enter_tests;
// v0.0.12 item 21: the composer and search field (`crate::chat_field`), K1–K5.
// The tests, written before the code; the implementer does not edit them.
#[cfg(test)]
mod chat_field_tests;
// v0.0.12 item 7 V2b, B1 (`doc/plans/V0012-item7-v2b-app-list.md`): the
// Alerts window's list takes its wording from `sovereign-docs`. Source scans,
// written before the code; the implementer does not edit them.
#[cfg(test)]
mod item7_v2b_scan_tests;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::App;

/// INSTALLER-002: restrict the runtime DLL search path to System32 + the
/// application directory before any DLL is loaded. Without it, a user-writable
/// DLL dropped beside `sovereign.exe` whose name matches a runtime-loaded
/// library (cudart/cublas on `--features cuda`, or WebView2Loader.dll used by
/// the embedded browser) loads ahead of the legitimate one with process
/// privileges (search-order / preload hijack). `SetDefaultDllDirectories` is a
/// kernel32 export linked by default on the MSVC target.
#[cfg(windows)]
fn harden_dll_search() {
    const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;
    const LOAD_LIBRARY_SEARCH_APPLICATION_DIR: u32 = 0x0000_0200;
    extern "system" {
        fn SetDefaultDllDirectories(directory_flags: u32) -> i32;
    }
    // Safe: one kernel32 call with constant flags, no pointers.
    unsafe {
        SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_SYSTEM32 | LOAD_LIBRARY_SEARCH_APPLICATION_DIR);
    }
}

fn main() {
    // Crash messages (spec ch. 12): a panic never writes its payload — it can
    // quote document text. Every build, before anything else can panic.
    panic_hook::install();

    // Harden the DLL search order first, before winit/wgpu/wry load anything.
    #[cfg(windows)]
    harden_dll_search();

    // Surface backend/P2P logs (sync engine, mDNS, pairing). Default shows
    // warnings, plus sovereign_p2p at debug in development builds only
    // (`dev::default_log_filter`); override via RUST_LOG.
    // `icu_provider=error`: parley finds word boundaries in Han text without
    // ICU's Chinese/Japanese dictionary (parley's `complex-scripts` feature is
    // off), and ICU logs a warning ("No segmentation model for language: ja"
    // — "ja" is its name for that dictionary, not a language choice) for every
    // Han run laid out. Harmless: line breaks between Han characters come from
    // UAX #14, unaffected, and the editor finds words itself. The filter only
    // reaches it because Cargo.toml turns on icu_provider's `logging` feature:
    // without it a debug build of icu_provider writes the warning with
    // `eprintln!`, past any subscriber (a release build drops it).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(dev::default_log_filter())),
        )
        .try_init();

    // Lot 0.5: the probes and the headless shot are development switches —
    // compiled only into debug builds or with `--features dev-switches` (see
    // dev.rs). A release binary has no code that reads these variables.
    #[cfg(any(debug_assertions, feature = "dev-switches"))]
    {
        if dev::flag("SHELL_AUTH_PROBE") {
            app::run_auth_probe();
            return;
        }
        if let Some(msg) = dev::var("SHELL_CHAT_PROBE") {
            app::run_chat_probe(msg);
            return;
        }
        if let Some(path) = dev::var("SHELL_SHOT") {
            app::run_shot(path);
            return;
        }
    }
    if let Some(sentence) = display_check::refusal_before_event_loop() {
        eprintln!("{sentence}");
        std::process::exit(1);
    }
    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("{}", display_check::event_loop_failure_here(&e));
            std::process::exit(1);
        }
    };
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    let run = event_loop.run_app(&mut app);
    // C6: the shell releases its database before main returns, within a bound,
    // on every return of the event loop, an error included (the error is then
    // raised once the close is done).
    app.close_before_exit(app::EXIT_CLOSE_BOUND);
    run.unwrap();
}
