//! rc2 — the laptop's second position on `2e8e8ff` (claude-coord
//! `from-laptop/0108-rc2-second-position.md`), Q1 and Q2, as settled by the
//! coordinator (2026-10-19). Source scans; the salt's behaviour is tested in
//! `salt_restore_tests.rs` (a child of `setup`).
//!
//! Written before the code, by another author: whoever implements it makes
//! these pass without editing this file.
//!
//! **Q1 — how `save_new` published.** `AuthStore::save_new` returns the mode
//! (`NewWrite::Linked` / `NewWrite::InPlace`). Every caller in the three phone
//! commands binds it to a name and logs it with `tracing` (any level, the
//! value formatted with `?` / `{:?}`), so Céline's phone test shows in logcat
//! whether the app sandbox allows hard links.
//!
//! **Q2 — the salt file.** The salt is written through
//! `sovereign_crypto::fs_private::write_private` (atomic, owner-only), never
//! `std::fs::write`, in the three phone commands and the shell's paired
//! onboarding (`p2p.rs`); and no other salt writer in `setup.rs` or the shell's
//! `crypto.rs` uses `std::fs::write` either.

use crate::onboarding_order_tests::{blank_comments, call_args, fn_body, line_of, read};

const PHONE_COMMANDS: &[&str] = &[
    "src/tauri_commands/auth.rs",
    "src/tauri_commands/pairing.rs",
    "src/tauri_commands/recovery.rs",
];

/// The name of the function whose body contains `pos` (the last `fn <name>(`
/// before it).
fn enclosing_fn(code: &str, pos: usize) -> String {
    let start = code[..pos].rfind("fn ").expect("a function before the call") + 3;
    code[start..].split('(').next().unwrap().trim().to_string()
}

/// Index just past the `;` ending the statement that starts at or before
/// `from` (depth-aware: parens, brackets, braces and string literals).
fn statement_end(body: &str, from: usize) -> usize {
    let b = body.as_bytes();
    let (mut depth, mut i, mut in_str) = (0i32, from, false);
    while i < b.len() {
        match b[i] {
            b'\\' if in_str => i += 1,
            b'"' => in_str = !in_str,
            b'(' | b'[' | b'{' if !in_str => depth += 1,
            b')' | b']' | b'}' if !in_str => {
                depth -= 1;
                if depth < 0 {
                    return i;
                }
            }
            b';' if !in_str && depth == 0 => return i + 1,
            _ => {}
        }
        i += 1;
    }
    b.len()
}

fn has_word(hay: &str, word: &str) -> bool {
    hay.match_indices(word).any(|(p, _)| {
        let before = hay[..p].chars().next_back();
        let after = hay[p + word.len()..].chars().next();
        let id = |c: Option<char>| c.map_or(false, |c| c.is_alphanumeric() || c == '_');
        !id(before) && !id(after)
    })
}

const TRACING_MACROS: &[&str] = &["trace!(", "debug!(", "info!(", "warn!(", "error!("];

fn check_save_new_logged(file: &str) -> Vec<String> {
    let code = blank_comments(&read(file));
    let mut problems = Vec::new();
    let calls: Vec<usize> = code.match_indices(".save_new(").map(|(p, _)| p).collect();
    if calls.is_empty() {
        problems.push(format!("{file}: no `.save_new(` call — the scan has nothing to check"));
    }
    for p in calls {
        let name = enclosing_fn(&code, p);
        let (offset, body) = fn_body(&code, &name, file);
        let local = p - offset;
        let stmt_start = body[..local].rfind(|c| c == ';' || c == '{' || c == '}').map(|q| q + 1).unwrap_or(0);
        let head = body[stmt_start..local].trim_start();
        let binding = head
            .strip_prefix("let ")
            .map(|rest| {
                rest.trim_start_matches("mut ")
                    .split(|c: char| c == '=' || c == ':' || c.is_whitespace())
                    .next()
                    .unwrap_or("")
            })
            .unwrap_or("");
        let at = format!("{file}:{}", line_of(&code, p));
        if binding.is_empty() || binding == "_" {
            problems.push(format!(
                "{name} ({at}): the mode `save_new` returns is not bound to a name (`let mode = ….save_new(…)…?;`) — it cannot be logged"
            ));
            continue;
        }
        let rest = &body[statement_end(body, local)..];
        let logged = TRACING_MACROS.iter().any(|m| {
            rest.match_indices(m).any(|(q, _)| {
                let args = call_args(rest, q + m.len() - 1);
                has_word(args, binding) && args.contains('?')
            })
        });
        if !logged {
            problems.push(format!(
                "{name} ({at}): `{binding}` (the mode `save_new` returned) is not logged with `tracing` (`tracing::info!(\"… {{{binding}:?}}\")`) after the write"
            ));
        }
    }
    problems
}

#[test]
fn q1_every_save_new_in_the_phone_commands_logs_its_mode() {
    let problems: Vec<String> = PHONE_COMMANDS.iter().flat_map(|f| check_save_new_logged(f)).collect();
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// Calls of `std::fs::write(` / `fs::write(` whose arguments name the salt.
fn plain_salt_writes(file: &str) -> Vec<String> {
    let code = blank_comments(&read(file));
    let mut found = Vec::new();
    for (p, _) in code.match_indices("fs::write(") {
        let args = call_args(&code, p + "fs::write".len());
        if args.contains("\"salt\"") || has_word(args, "salt_path") {
            found.push(format!(
                "{file}:{}: `fs::write({})` writes the salt — use `sovereign_crypto::fs_private::write_private` (atomic, owner-only)",
                line_of(&code, p),
                args.trim()
            ));
        }
    }
    found
}

fn writes_salt_privately(file: &str) -> bool {
    let code = blank_comments(&read(file));
    code.match_indices("write_private(").any(|(p, _)| {
        let args = call_args(&code, p + "write_private".len());
        args.contains("\"salt\"") || has_word(args, "salt_path")
    })
}

const SHELL_P2P: &str = "../sovereign-shell/src/p2p.rs";
const SHELL_CRYPTO: &str = "../sovereign-shell/src/crypto.rs";

#[test]
fn q2_the_salt_is_written_with_write_private_not_std_fs_write() {
    let mut problems = Vec::new();
    for f in PHONE_COMMANDS.iter().copied().chain([SHELL_P2P]) {
        problems.extend(plain_salt_writes(f));
        if !writes_salt_privately(f) {
            problems.push(format!("{f}: the salt is not written with `write_private(` (an argument naming \"salt\")"));
        }
    }
    for f in ["src/setup.rs", SHELL_CRYPTO] {
        problems.extend(plain_salt_writes(f));
    }
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// The scanners on known bodies, so a green result means something.
#[test]
fn the_q1_q2_scanners_tell_bad_from_good() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, src: &str| {
        let p = dir.path().join(name);
        std::fs::write(&p, src).unwrap();
        p.to_str().unwrap().to_string()
    };
    let dropped = write("a.rs", "fn f() -> R { s.save_new(&p).map_err(e)?; Ok(()) }");
    let unlogged = write("b.rs", "fn f() -> R { let mode = s.save_new(&p).map_err(e)?; tracing::info!(\"wrote\"); Ok(()) }");
    let logged = write(
        "c.rs",
        "fn f() -> R { let mode = s\n  .save_new(&crypto_dir.join(\"auth.store\"))\n  .map_err(|e| { let x = e; x })?;\n  tracing::info!(\"auth.store published: {mode:?}\"); Ok(()) }",
    );
    assert!(check_save_new_logged(&dropped).iter().any(|m| m.contains("not bound")));
    assert!(check_save_new_logged(&unlogged).iter().any(|m| m.contains("not logged")));
    assert!(check_save_new_logged(&logged).is_empty(), "{:?}", check_save_new_logged(&logged));

    let plain = write("d.rs", "fn f() { std::fs::write(crypto_dir.join(\"salt\"), &salt).unwrap(); }");
    let private = write("e.rs", "fn f() { sovereign_crypto::fs_private::write_private(&crypto_dir.join(\"salt\"), &salt).unwrap(); }");
    assert_eq!(plain_salt_writes(&plain).len(), 1);
    assert!(!writes_salt_privately(&plain));
    assert!(plain_salt_writes(&private).is_empty());
    assert!(writes_salt_privately(&private));
}
