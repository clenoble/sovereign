//! rc2 — the onboarding commands take the single-flight claim **before** the
//! key derivation and write `auth.store` with an exclusive create (spec ch. 07,
//! *Auth store*, "Created once, never replaced by account creation", v2.0.46):
//! "The claim is taken **before** the key derivation, so the check and the
//! write cannot be separated by it", "The first write of `auth.store` refuses
//! an existing file", and "a refused request changes no file".
//!
//! Written before the code, by another author: whoever implements rc2 makes
//! these pass without editing this file.
//!
//! The two commands need a Tauri `State`, so they are checked by reading
//! their source: each function body is extracted by brace matching (comments
//! blanked, string and char literals skipped), and the positions of the calls
//! are compared. What the scan requires, per command:
//! 1. `onboarding_claim::claim(` is called, bound to a named guard (not `_`,
//!    not a bare statement — either drops the guard at once), and not
//!    `drop(…)`ped before `auth.store` is written;
//! 2. the claim comes before the first key derivation (`AuthStore::create`,
//!    `create_with_imported_account_key`, `MasterKey::derive`, and the pairing
//!    handshake that derives inside its callback, `pair_with_source`) and
//!    before the first file write (`std::fs::write`, `create_dir_all`,
//!    `.save(` / `.save_new(`, `load_or_create_device_id`) — so a refused
//!    request has written nothing;
//! 3. `auth.store` is written with `.save_new(` and never with `.save(`.
//!
//! Extended the same day (spec v2.0.46 as extended by Céline, `7506579`):
//! "**Every** first write of `auth.store` refuses an existing file …: account
//! creation fresh or paired, on the phone and on the desktop, and the backup
//! recovery that rebuilds an account (ch. 06). Account creation and backup
//! recovery are **single-flight** on the phone, one claim excluding the other."
//! So `recovery_finalize` (`src/tauri_commands/recovery.rs`) is held to the same
//! three rules, its passphrase check (`passphrase_matches`, which derives a key)
//! counts as a derivation, and in all three commands the claim is taken on the
//! profile directory (its argument names `profile_dir`), so it is one claim.
//! The access recovery (`access_recovery_finalize`) is not a first write — it
//! replaces the forgotten `auth.store` by design (ch. 06, Feature 1) — and is
//! not checked here.

const AUTH_RS: &str = "src/tauri_commands/auth.rs";
const PAIRING_RS: &str = "src/tauri_commands/pairing.rs";
const RECOVERY_RS: &str = "src/tauri_commands/recovery.rs";

pub(crate) fn read(rel: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Same length as `src`, with every comment replaced by spaces (newlines kept),
/// so byte positions still line up with the original.
pub(crate) fn blank_comments(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    out[i] = b' ';
                    i += 1;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < b.len() {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        depth += 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        depth -= 1;
                        out[i] = b' ';
                        out[i + 1] = b' ';
                        i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if b[i] != b'\n' {
                            out[i] = b' ';
                        }
                        i += 1;
                    }
                }
            }
            b'"' => i = skip_string(b, i),
            b'r' if is_raw_string_start(b, i) => i = skip_raw_string(b, i),
            b'\'' => i = skip_char_or_lifetime(b, i),
            _ => i += 1,
        }
    }
    // Only ASCII bytes were replaced (by ASCII spaces), so this stays UTF-8.
    String::from_utf8(out).expect("blanking comments keeps UTF-8")
}

/// `i` at an opening `"`; returns the index just past the closing `"`.
fn skip_string(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn is_raw_string_start(b: &[u8], i: usize) -> bool {
    // `r"…"` or `r#"…"#`, not part of an identifier (e.g. `for"` cannot occur).
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return false;
    }
    let mut j = i + 1;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    b.get(j) == Some(&b'"')
}

fn skip_raw_string(b: &[u8], i: usize) -> usize {
    let mut j = i + 1;
    let mut hashes = 0;
    while b.get(j) == Some(&b'#') {
        hashes += 1;
        j += 1;
    }
    j += 1; // opening quote
    while j < b.len() {
        if b[j] == b'"' && (0..hashes).all(|k| b.get(j + 1 + k) == Some(&b'#')) {
            return j + 1 + hashes;
        }
        j += 1;
    }
    j
}

/// `i` at a `'`: a char literal (`'a'`, `'\n'`, `'{'`) is skipped whole; a
/// lifetime (`'_`, `'a`) only its quote.
fn skip_char_or_lifetime(b: &[u8], i: usize) -> usize {
    if b.get(i + 1) == Some(&b'\\') {
        let mut j = i + 2;
        while j < b.len() && b[j] != b'\'' {
            j += 1;
        }
        return j + 1;
    }
    // A one-character literal: `'x'` (possibly multi-byte UTF-8).
    let mut j = i + 1;
    if j < b.len() {
        j += 1;
        while j < b.len() && (b[j] & 0xC0) == 0x80 {
            j += 1;
        }
        if b.get(j) == Some(&b'\'') {
            return j + 1;
        }
    }
    i + 1
}

/// The body (from its `{` to its matching `}`) of `fn <name>(` in `code`
/// (comments already blanked), and its byte offset in `code`.
pub(crate) fn fn_body<'a>(code: &'a str, name: &str, file: &str) -> (usize, &'a str) {
    let needle = format!("fn {name}(");
    let start = code
        .find(&needle)
        .unwrap_or_else(|| panic!("{file}: `fn {name}(` not found"));
    let b = code.as_bytes();
    // The signature holds no braces; the first `{` opens the body.
    let open = start + code[start..].find('{').unwrap_or_else(|| panic!("{file}: no body for {name}"));
    let mut depth = 0usize;
    let mut i = open;
    while i < b.len() {
        match b[i] {
            b'{' => {
                depth += 1;
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return (open, &code[open..i]);
                }
            }
            b'"' => i = skip_string(b, i),
            b'r' if is_raw_string_start(b, i) => i = skip_raw_string(b, i),
            b'\'' => i = skip_char_or_lifetime(b, i),
            _ => i += 1,
        }
    }
    panic!("{file}: unbalanced braces in the body of {name}");
}

/// The text between the `(` that ends at `open_paren` and its matching `)`.
pub(crate) fn call_args(body: &str, open_paren: usize) -> &str {
    let b = body.as_bytes();
    let mut depth = 0usize;
    let mut i = open_paren;
    while i < b.len() {
        match b[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &body[open_paren + 1..i];
                }
                i += 1;
            }
            b'"' => i = skip_string(b, i),
            _ => i += 1,
        }
    }
    &body[open_paren + 1..]
}

/// The identifier just before `.method(` at `dot` (whitespace and newlines
/// between the receiver and the dot are allowed, as in a method chain).
fn receiver_before(body: &str, dot: usize) -> &str {
    let before = body[..dot].trim_end();
    let end = before.len();
    let start = before
        .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
        .map(|p| p + 1)
        .unwrap_or(0);
    &before[start..end]
}

pub(crate) fn line_of(code: &str, pos: usize) -> usize {
    code[..pos].matches('\n').count() + 1
}

/// The names an `AuthStore` is bound to in `body`: `auth_store`, plus every
/// `let <name> = …AuthStore::create…` binding.
fn store_bindings(body: &str) -> Vec<String> {
    let mut names = vec!["auth_store".to_string()];
    for (p, _) in body.match_indices("AuthStore::create") {
        let stmt_start = body[..p].rfind(|c| c == ';' || c == '{' || c == '}').map(|q| q + 1).unwrap_or(0);
        let head = body[stmt_start..p].trim();
        if let Some(rest) = head.strip_prefix("let ") {
            let name = rest
                .trim_start_matches("mut ")
                .split(|c: char| c == '=' || c == ':' || c.is_whitespace())
                .next()
                .unwrap_or("");
            if !name.is_empty() && name != "_" {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Markers that derive a key or touch the disk; the claim must come first.
const MUST_FOLLOW_THE_CLAIM: &[&str] = &[
    "AuthStore::create",
    "create_with_imported_account_key(",
    "MasterKey::derive(",
    "pair_with_source(",
    "passphrase_matches(",
    "std::fs::write(",
    "create_dir_all(",
    ".save(",
    ".save_new(",
    "load_or_create_device_id(",
];

fn check_onboarding_fn(file: &str, name: &str) -> Vec<String> {
    let code = blank_comments(&read(file));
    let (offset, body) = fn_body(&code, name, file);
    let at = |pos: usize| format!("{file}:{}", line_of(&code, offset + pos));
    let mut problems = Vec::new();

    // 1. The claim, bound to a named guard.
    let mut binding = "";
    match body.find("onboarding_claim::claim(") {
        None => problems.push(format!(
            "{name}: no call to `onboarding_claim::claim(` — account creation must take the single-flight claim"
        )),
        Some(claim_pos) => {
            let stmt_start =
                body[..claim_pos].rfind(|c| c == ';' || c == '{' || c == '}').map(|p| p + 1).unwrap_or(0);
            let stmt_head = body[stmt_start..claim_pos].trim();
            binding = stmt_head
                .strip_prefix("let ")
                .map(|rest| {
                    rest.trim_start_matches("mut ")
                        .split(|c: char| c == '=' || c == ':' || c.is_whitespace())
                        .next()
                        .unwrap_or("")
                })
                .unwrap_or("");
            if binding.is_empty() || binding == "_" {
                problems.push(format!(
                    "{name} ({}): the claim must be bound to a named guard held to the end (`let _claim = …`); found `{} onboarding_claim::claim(…` — `let _ =` or a bare statement drops it at once",
                    at(claim_pos),
                    stmt_head
                ));
            }
            // One claim for onboarding and recovery: keyed by the profile directory.
            let arg = call_args(body, claim_pos + "onboarding_claim::claim".len());
            if !arg.contains("profile_dir") {
                problems.push(format!(
                    "{name} ({}): the claim is taken on `{}`, not on the profile directory — onboarding and backup recovery must share one claim per profile",
                    at(claim_pos),
                    arg.trim()
                ));
            }

            // 2. Before any key derivation or file write.
            for marker in MUST_FOLLOW_THE_CLAIM {
                if let Some(p) = body.find(marker) {
                    if p < claim_pos {
                        problems.push(format!(
                            "{name}: `{marker}` at {} comes before the claim at {} — the claim must be taken before the key derivation and before any file is written",
                            at(p),
                            at(claim_pos)
                        ));
                    }
                }
            }
        }
    }

    // 3. auth.store written with save_new, never with save.
    let stores = store_bindings(body);
    let is_store = |recv: &str| stores.iter().any(|s| s == recv);
    let mut save_new_of_auth_store = None;
    for (p, _) in body.match_indices(".save_new(") {
        let args = call_args(body, p + ".save_new".len());
        let recv = receiver_before(body, p);
        if args.contains("auth.store") || is_store(recv) {
            save_new_of_auth_store.get_or_insert(p);
        }
    }
    for (p, _) in body.match_indices(".save(") {
        let args = call_args(body, p + ".save".len());
        let recv = receiver_before(body, p);
        if args.contains("auth.store") || is_store(recv) {
            problems.push(format!(
                "{name} ({}): `{recv}.save({})` replaces auth.store — the first write must be `.save_new(` (exclusive create)",
                at(p),
                args.trim()
            ));
        }
    }
    match save_new_of_auth_store {
        None => problems.push(format!(
            "{name}: auth.store is not written with `.save_new(` (on the AuthStore, or with an `auth.store` path)"
        )),
        Some(p) => {
            if !binding.is_empty() && binding != "_" {
                let drop_call = format!("drop({binding})");
                if let Some(d) = body.find(&drop_call) {
                    if d < p {
                        problems.push(format!(
                            "{name} ({}): `{drop_call}` releases the claim before auth.store is written at {}",
                            at(d),
                            at(p)
                        ));
                    }
                }
            }
        }
    }
    problems
}

#[test]
fn complete_onboarding_claims_before_deriving_and_writes_auth_store_exclusively() {
    let problems = check_onboarding_fn(AUTH_RS, "complete_onboarding");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

#[test]
fn complete_onboarding_paired_claims_before_deriving_and_writes_auth_store_exclusively() {
    let problems = check_onboarding_fn(PAIRING_RS, "complete_onboarding_paired");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// Backup recovery rebuilds an account on the phone: same claim, same
/// exclusive write (spec ch. 07 as extended, `7506579`).
#[test]
fn recovery_finalize_claims_before_deriving_and_writes_auth_store_exclusively() {
    let problems = check_onboarding_fn(RECOVERY_RS, "recovery_finalize");
    assert!(problems.is_empty(), "\n{}\n", problems.join("\n"));
}

/// The scanner itself: on a known-bad and a known-good body it finds what it
/// should, so a green result above is not a scanner that sees nothing.
#[test]
fn the_scanner_tells_a_bad_body_from_a_good_one() {
    let bad = "fn f(s: State<'_, A>) -> R { if p.join(\"crypto\").join(\"auth.store\").exists() { return Err(\"x\".into()); } \
               let a = AuthStore::create(b\"p\", b\"d\", &salt, \"{id}\")?; a.save(&dir.join(\"auth.store\"))?; let _ = onboarding_claim::claim(p); }";
    // A store bound to another name, saved through a path variable: still caught.
    let bad_name = "fn f(s: State<'_, A>) -> R { let _claim = onboarding_claim::claim(&s.profile_dir)?; \
                    let store = sovereign_crypto::auth::AuthStore::create(b\"p\", b\"d\", &salt, &id)?; store.save(&path)?; }";
    let good = "fn f(s: State<'_, A>) -> R { let _claim = crate::onboarding_claim::claim(&s.profile_dir).map_err(|e| e.to_string())?; \
                std::fs::create_dir_all(&d)?; let auth_store = AuthStore::create(b\"p\", b\"d\", &salt, \"{id}\")?; \
                auth_store\n    .save_new(&d.join(\"auth.store\"))?; }";
    let scan = |src: &str| {
        let dir = tempfile::tempdir().unwrap();
        let rel = dir.path().join("x.rs");
        std::fs::write(&rel, src).unwrap();
        let code = blank_comments(src);
        let (_, body) = fn_body(&code, "f", "x.rs");
        assert!(body.ends_with('}'));
        // Reuse the checker on an absolute path: CARGO_MANIFEST_DIR.join(abs) == abs.
        check_onboarding_fn(rel.to_str().unwrap(), "f")
    };
    let bad_problems = scan(bad);
    assert!(
        bad_problems.iter().any(|p| p.contains("comes before the claim")),
        "the scanner must flag a derivation before the claim: {bad_problems:?}"
    );
    assert!(
        bad_problems.iter().any(|p| p.contains("replaces auth.store")),
        "the scanner must flag `.save(` of auth.store: {bad_problems:?}"
    );
    assert!(
        bad_problems.iter().any(|p| p.contains("named guard")),
        "the scanner must flag `let _ =` on the claim: {bad_problems:?}"
    );
    assert!(
        bad_problems.iter().any(|p| p.contains("not on the profile directory")),
        "the scanner must flag a claim not keyed by the profile directory: {bad_problems:?}"
    );
    let named = scan(bad_name);
    assert!(
        named.iter().any(|p| p.contains("replaces auth.store")),
        "the scanner must flag `.save(` on a store bound to another name: {named:?}"
    );
    let good_problems = scan(good);
    assert!(good_problems.is_empty(), "the scanner must pass a good body: {good_problems:?}");
}
