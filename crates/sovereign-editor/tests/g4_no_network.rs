//! Guarantee 4 — "No network. The editor's crate depends on no networking
//! crate (checked in CI)." (spec ch. 12, C-TEXT)
//!
//! Plus the static half of guarantee 3 ("No plaintext leaves memory except
//! through the host's save") and of "It owns no storage, opens no connection
//! and writes no file" (C-TEXT, *What the editor is*): no file-system, temp or
//! crash-reporting crate and no such API in the editor's own source.
//!
//! These checks pass against the stubs, by design: they constrain what the
//! editor may become, and they keep running once it exists.

mod common;
use common::deps::*;

/// Crates that open connections, resolve names, speak network protocols, or
/// embed something that does (a web engine, a runtime with sockets, the
/// project's own networked crates).
const NETWORK_CRATES: &[&str] = &[
    // sockets / runtimes / event loops
    "mio",
    "socket2",
    "tokio",
    "tokio-*",
    "async-std",
    "async-io",
    "async-net",
    "smol",
    "polling",
    "if-addrs",
    "if-watch",
    "netlink-*",
    "local-ip-address",
    "get_if_addrs",
    // HTTP and friends
    "reqwest",
    "reqwest-*",
    "hyper",
    "hyper-*",
    "h2",
    "h3",
    "h3-*",
    "http-body",
    "ureq",
    "isahc",
    "curl",
    "curl-sys",
    "surf",
    "attohttpc",
    "minreq",
    "websocket",
    "tungstenite",
    "tokio-tungstenite",
    "async-tungstenite",
    "ws",
    // QUIC / TLS / DNS
    "quinn",
    "quinn-*",
    "quiche",
    "rustls",
    "rustls-*",
    "tokio-rustls",
    "native-tls",
    "openssl",
    "openssl-sys",
    "webpki",
    "webpki-*",
    "trust-dns-*",
    "hickory-*",
    "mdns",
    "mdns-sd",
    "libmdns",
    "igd",
    "igd-next",
    // peer-to-peer and messaging
    "libp2p",
    "libp2p-*",
    "lettre",
    "imap",
    "async-imap",
    "async-native-tls",
    "presage",
    "libsignal-*",
    // web engines (they do their own networking — see spike K2)
    "wry",
    "webview2-com",
    "webview2-com-sys",
    "webkit2gtk",
    "webkit2gtk-sys",
    "tauri",
    "tauri-*",
    // the project's networked / storage crates
    "sovereign-p2p",
    "sovereign-comms",
    "sovereign-relay",
    "sovereign-guardian",
    "sovereign-db",
    "sovereign-ai",
    "sovereign-app",
    "surrealdb",
    "surrealdb-*",
];

/// Crates whose job is to put bytes on disk or ship crash data.
const DISK_AND_CRASH_CRATES: &[&str] = &[
    "tempfile",
    "tempdir",
    "memmap",
    "memmap2",
    "rocksdb",
    "librocksdb-sys",
    "sled",
    "rusqlite",
    "libsqlite3-sys",
    "redb",
    "sentry",
    "sentry-*",
    "crash-handler",
    "crash-context",
    "minidumper",
    "minidump-writer",
    "breakpad-*",
    "human-panic",
    "tracing-appender",
    "log4rs",
    "fern",
    "flexi_logger",
    "simplelog",
];

// "The editor's crate depends on no networking crate"
#[test]
fn g4_no_networking_crate_is_reachable_from_the_editor() {
    let reachable = reachable_packages();
    assert!(reachable.contains("accesskit"), "sanity: the walk sees the editor's own dependencies");
    let bad: Vec<_> = reachable.iter().filter(|n| matches_any(n, NETWORK_CRATES)).collect();
    assert!(bad.is_empty(), "networking crates reachable from sovereign-editor: {bad:?}");
}

// The walk is not vacuous: from the shell (which embeds a browser and syncs),
// it finds networking crates.
#[test]
fn g4_dependency_walk_finds_networking_crates_where_they_are() {
    let shell = reachable_from("sovereign-shell");
    for expected in ["tokio", "wry", "sovereign-p2p"] {
        assert!(shell.contains(expected), "walk from sovereign-shell misses {expected}");
    }
    assert!(shell.iter().any(|n| matches_any(n, NETWORK_CRATES)));
}

// The source scan ignores prose in comments but not code.
#[test]
fn g4_source_scan_ignores_comments_only() {
    let s = strip_comments("/// uses std::net\nlet a = 1; // std::fs\n/* std::process */ use std::net::TcpStream;\n");
    assert!(!s.contains("std::fs") && !s.contains("std::process"));
    assert!(s.contains("use std::net::TcpStream"));
}

// "opens no connection" — not through std either.
#[test]
fn g4_editor_source_uses_no_network_or_process_api() {
    let hits = source_uses(&[
        "std::net",
        "TcpStream",
        "TcpListener",
        "UdpSocket",
        "ToSocketAddrs",
        "std::process",
        "Command::new",
    ]);
    assert!(hits.is_empty(), "{hits:?}");
}

// "No temp file, no swap file, … no crash report written by Sovereign containing
// it" / "writes no file"
#[test]
fn g3_no_disk_or_crash_reporting_crate_is_reachable_from_the_editor() {
    let bad = disk_violations("sovereign-editor");
    assert!(bad.is_empty(), "disk / crash-report crates reachable from sovereign-editor: {bad:?}");
}

/// Decided by Céline 2026-10-10: `memmap2` is allowed when EVERY path to it
/// goes through `fontique` (parley's font library maps font files read-only;
/// the editor is built on parley). Any other path to it still fails.
const ALLOWED_ONLY_THROUGH: &[(&str, &str)] = &[("memmap2", "fontique")];

/// Disk / crash-report crates reachable from `root`, except those reachable
/// only through their allowed intermediary.
fn disk_violations(root: &str) -> Vec<String> {
    let reachable = reachable_from(root);
    reachable
        .iter()
        .filter(|n| matches_any(n, DISK_AND_CRASH_CRATES))
        .filter(|n| match ALLOWED_ONLY_THROUGH.iter().find(|(c, _)| c == n) {
            // still reachable once the intermediary is cut off: another path exists
            Some((c, via)) => reachable_from_avoiding(root, &[*via]).contains(*c),
            None => true,
        })
        .cloned()
        .collect()
}

// The exception is not vacuous and not a blanket pass: parley reaches memmap2
// only through fontique (allowed); the shell also reaches it through winit /
// smithay-client-toolkit (not allowed) and is flagged.
#[test]
fn g3_memmap2_is_allowed_only_through_fontique() {
    assert!(reachable_from("parley").contains("memmap2"), "sanity: parley reaches memmap2");
    assert!(!disk_violations("parley").contains(&"memmap2".to_string()), "parley's only path is fontique");
    assert!(
        disk_violations("sovereign-shell").contains(&"memmap2".to_string()),
        "the shell reaches memmap2 by other paths too, and must be flagged"
    );
}

// "writes no file"; "no crash report written by Sovereign containing it"
#[test]
fn g3_editor_source_uses_no_file_or_panic_hook_api() {
    let hits = source_uses(&[
        "std::fs",
        "File::create",
        "File::open",
        "OpenOptions",
        "temp_dir",
        "set_hook",
        "take_hook",
        // stdout / stderr are captured into logs and crash reports by hosts
        "eprintln!",
        "println!",
        "print!(",
        "dbg!",
    ]);
    assert!(hits.is_empty(), "{hits:?}");
}
