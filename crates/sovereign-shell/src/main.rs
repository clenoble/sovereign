//! Sovereign native shell — spatial-canvas foundation (native-UI migration).
//!
//! See doc/plans/native-ui-migration.md. The REAL shell foundation (promoted
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
mod camera;
mod canvas;
mod clip;
mod comms;
mod crypto;
mod onboarding;
mod p2p;
mod panels;
mod text;
mod theme;

use winit::event_loop::{ControlFlow, EventLoop};

use crate::app::{run_auth_probe, run_chat_probe, run_shot, App};

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
    // Harden the DLL search order first, before winit/wgpu/wry load anything.
    #[cfg(windows)]
    harden_dll_search();

    // Surface backend/P2P logs (sync engine, mDNS, pairing). Default shows
    // warnings + sovereign_p2p at debug for diagnosing sync; override via RUST_LOG.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn,sovereign_p2p=debug")),
        )
        .try_init();

    if std::env::var("SHELL_AUTH_PROBE").is_ok() {
        run_auth_probe();
        return;
    }
    if let Ok(msg) = std::env::var("SHELL_CHAT_PROBE") {
        run_chat_probe(msg);
        return;
    }
    if let Ok(path) = std::env::var("SHELL_SHOT") {
        run_shot(path);
        return;
    }
    let event_loop = EventLoop::new().unwrap();
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
    event_loop.run_app(&mut app).unwrap();
}
