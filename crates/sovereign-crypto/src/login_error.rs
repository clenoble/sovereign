//! What a failed login says (spec ch. 07, « Login errors say what failed —
//! contract (LE) », LE2, LE3 and LE5).
//!
//! The desktop shell and the app show the same three sentences, so they are
//! written once, here, beside the error they come from
//! ([`AuthStore::authenticate`](crate::auth::AuthStore::authenticate)):
//! - a wrong password: « Invalid password »;
//! - the key derivation failed: « Your password could not be checked: {cause}.
//!   This is not a wrong password. »;
//! - a probe opened, then the keys' unwrapping or the session's installation
//!   failed: « Your password is right, but your account could not be opened:
//!   {cause}. »
//!
//! `{cause}` is the underlying error's own text, with every path and file name
//! of the data directory replaced by plain words ([`scrub_paths`]): a path can
//! name a persona, and the same failure must read the same for either persona
//! (LE5). Nothing here logs: the full cause is written nowhere new.

use std::path::Path;

use crate::error::CryptoError;

/// The one sentence for a wrong password (LE2). The app's login screen counts
/// this exact text, and only it, toward the lockout (LE4).
pub const WRONG_PASSWORD: &str = "Invalid password";

/// Why a login failed, with its cause already cleared of the data
/// directory's paths. `Display` gives the sentence the user reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginFailure {
    /// The derivation succeeded and no entry's probe opened (LE1).
    WrongPassword,
    /// The key derivation failed, before any probe was tried.
    NotChecked { cause: String },
    /// A probe opened (the password is right), then the keys' unwrapping or
    /// the session's installation failed.
    NotOpened { cause: String },
}

impl LoginFailure {
    /// The failure an error of [`AuthStore::authenticate`](crate::auth::AuthStore::authenticate)
    /// stands for. `data_dirs` are the folders whose paths the cause must not
    /// name (see [`scrub_paths`]).
    pub fn from_authenticate(e: &CryptoError, data_dirs: &[&Path]) -> Self {
        match e {
            CryptoError::WrongPassphrase => LoginFailure::WrongPassword,
            // `authenticate` derives every key before its first probe, so a
            // derivation error is always one from before any probe.
            CryptoError::DerivationFailed(_) => LoginFailure::NotChecked { cause: cause_of(e, data_dirs) },
            other => LoginFailure::NotOpened { cause: cause_of(other, data_dirs) },
        }
    }

    /// A failure of the session's installation, after the password matched:
    /// opening the persona's database, its key databases, and the rest.
    pub fn not_opened(cause: impl std::fmt::Display, data_dirs: &[&Path]) -> Self {
        LoginFailure::NotOpened { cause: cause_of(cause, data_dirs) }
    }

    pub fn is_wrong_password(&self) -> bool {
        matches!(self, LoginFailure::WrongPassword)
    }

    /// The cause alone, for a sentence of its own (the desktop's end of a first
    /// setup, « Your account was created, but it could not be opened: {error} »).
    /// A wrong password reads « Invalid password », which is exact there too.
    pub fn cause_alone(&self) -> &str {
        match self {
            LoginFailure::WrongPassword => WRONG_PASSWORD,
            LoginFailure::NotChecked { cause } | LoginFailure::NotOpened { cause } => cause,
        }
    }
}

impl std::fmt::Display for LoginFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginFailure::WrongPassword => f.write_str(WRONG_PASSWORD),
            LoginFailure::NotChecked { cause } => {
                write!(f, "Your password could not be checked: {cause}. This is not a wrong password.")
            }
            LoginFailure::NotOpened { cause } => {
                write!(f, "Your password is right, but your account could not be opened: {cause}.")
            }
        }
    }
}

impl std::error::Error for LoginFailure {}

/// The cause's own text, cleared of paths, on one line (a system message can
/// end with a line break), without the full stop it may end with (the sentence
/// puts its own). Never empty: an error with no text says so.
fn cause_of(e: impl std::fmt::Display, data_dirs: &[&Path]) -> String {
    let text = scrub_paths(&e.to_string(), data_dirs);
    let text = if text.contains(['\n', '\r']) {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        text
    };
    let text = text.trim().trim_end_matches('.').trim_end();
    if text.is_empty() {
        "the error gave no reason".to_string()
    } else {
        text.to_string()
    }
}

// ── Paths in plain words (LE3) ───────────────────────────────────────

/// `text` with every path or file name of the data directory replaced by
/// plain words (« the database », « a key database »…); the rest of the text
/// is kept as it is.
///
/// Two passes:
/// 1. each spelling of each folder of `data_dirs` (as given, with either kind
///    of slash, and as the system resolves it), with what follows it up to the
///    end of the path, becomes the words for what it names;
/// 2. any remaining word that is a path or a file name of the profile (a
///    persona's database, a key database, a key file, `auth.store`, RocksDB's
///    `LOCK`, anything naming the duress persona, or the folder's own name)
///    becomes the same words. This catches a relative path, or a folder spelled
///    in a way pass 1 does not know.
///
/// Matching ignores ASCII case: Windows paths can come back in either.
pub fn scrub_paths(text: &str, data_dirs: &[&Path]) -> String {
    let mut spellings: Vec<String> = Vec::new();
    let mut folder_names: Vec<String> = Vec::new();
    for dir in data_dirs {
        spellings.extend(spellings_of(dir));
        if let Some(name) = dir.file_name().map(|n| n.to_string_lossy().into_owned()) {
            if name.len() > 3 {
                folder_names.push(name);
            }
        }
    }
    // Longest first, so a longer spelling (the `\\?\` form) wins over its tail.
    spellings.sort_by_key(|s| std::cmp::Reverse(s.len()));
    spellings.dedup();

    let mut out = text.to_string();
    for spelling in &spellings {
        out = replace_rooted(&out, spelling);
    }
    replace_tokens(&out, &folder_names)
}

/// The ways a folder can be spelled in an error's text. Spellings shorter than
/// four characters are left out: a root such as `/` would match every path.
fn spellings_of(dir: &Path) -> Vec<String> {
    let mut forms = Vec::new();
    let mut push = |s: String| {
        let s = s.trim_end_matches(['/', '\\']).to_string();
        if s.len() >= 4 {
            forms.push(s.replace('\\', "/"));
            forms.push(s.replace('/', "\\"));
            forms.push(s);
        }
    };
    push(dir.to_string_lossy().into_owned());
    if let Ok(resolved) = std::fs::canonicalize(dir) {
        let resolved = resolved.to_string_lossy().into_owned();
        if let Some(plain) = resolved.strip_prefix(r"\\?\") {
            push(plain.to_string());
        }
        push(resolved);
    }
    forms
}

/// A character that can continue a path after its folder: our file names hold
/// no space, so a path ends at the first space, quote or bracket.
fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '/' | '\\' | '~' | '+' | '@' | '$' | '#' | ':')
}

/// Where a path that starts at `start` ends: after the last path character,
/// less the punctuation that ends a sentence or introduces what follows.
fn path_end(text: &str, start: usize) -> usize {
    let mut end = start;
    for (i, c) in text[start..].char_indices() {
        if !is_path_char(c) {
            break;
        }
        end = start + i + c.len_utf8();
    }
    while end > start && text[..end].ends_with(['.', ':']) {
        end -= 1;
    }
    end
}

/// The byte offset of `needle` in `hay` at or after `from`, ignoring ASCII case.
fn find_ignoring_case(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || h.len() < n.len() {
        return None;
    }
    (from..=h.len() - n.len())
        .find(|&i| hay.is_char_boundary(i) && h[i..i + n.len()].eq_ignore_ascii_case(n))
}

/// Pass 1: each occurrence of `root`, with the rest of its path, in words.
fn replace_rooted(text: &str, root: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = 0;
    let mut from = 0;
    while let Some(at) = find_ignoring_case(text, root, from) {
        // A long-path prefix just before the folder goes with it.
        let start = [r"\\?\", "//?/"]
            .iter()
            .find(|p| text[..at].ends_with(**p))
            .map_or(at, |p| at - p.len());
        let end = path_end(text, at + root.len()).max(at + root.len());
        out.push_str(&text[rest..start]);
        if end == at + root.len() {
            out.push_str("your data folder");
        } else {
            out.push_str(words_for(&text[at..end]));
        }
        rest = end;
        from = end;
    }
    out.push_str(&text[rest..]);
    out
}

/// Pass 2: each word that is a path or a file name of the profile, in words.
fn replace_tokens(text: &str, folder_names: &[String]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().expect("a char at a boundary");
        if !is_path_char(c) {
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let end = path_end(text, i);
        if end == i {
            // Only punctuation here (a lone `.` or `:`): kept as it is.
            out.push(c);
            i += c.len_utf8();
            continue;
        }
        let token = &text[i..end];
        if names_the_profile(token, folder_names) {
            out.push_str(words_for(token));
        } else {
            out.push_str(token);
        }
        i = end;
    }
    out
}

/// The components of a path, split on either slash.
fn components(path: &str) -> impl Iterator<Item = &str> {
    path.split(['/', '\\']).filter(|c| !c.is_empty())
}

fn is_key_database(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("keys") && n.ends_with(".db")
}

fn is_key_file(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("index") && n.ends_with(".key")
}

/// Whether a word outside any known folder still names a file of the profile.
fn names_the_profile(token: &str, folder_names: &[String]) -> bool {
    let lower = token.to_ascii_lowercase();
    let has_separator = token.contains(['/', '\\']);
    lower.contains("duress")
        || folder_names.iter().any(|n| find_ignoring_case(token, n, 0).is_some())
        || components(token).any(|c| {
            let cl = c.to_ascii_lowercase();
            c == "LOCK"
                || is_key_database(c)
                || is_key_file(c)
                || cl == "auth.store"
                || cl.ends_with(".db")
                || (has_separator && (cl == "data" || cl == "crypto"))
        })
}

/// The plain words for a path of the profile: what it names, never its name.
fn words_for(path: &str) -> &'static str {
    let parts: Vec<&str> = components(path).collect();
    if parts.iter().any(|c| *c == "LOCK") {
        "the database's lock file"
    } else if parts.iter().any(|c| is_key_database(c)) {
        "a key database"
    } else if parts.iter().any(|c| is_key_file(c)) {
        "a key file"
    } else if parts.iter().any(|c| c.eq_ignore_ascii_case("auth.store")) {
        "the password store"
    } else if parts.iter().any(|c| c.to_ascii_lowercase().ends_with(".db"))
        || parts.iter().any(|c| c.to_ascii_lowercase().contains("duress"))
        || parts.iter().any(|c| c.eq_ignore_ascii_case("data"))
    {
        "the database"
    } else {
        "a file of your data folder"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_lock_error_names_no_file() {
        let dir = Path::new(r"C:\Users\Some One\AppData\Local\Temp\.tmpAb12Cd");
        let text = r"IO error: Failed to create lock file: C:\Users\Some One\AppData\Local\Temp\.tmpAb12Cd\data\sovereign-duress.db/LOCK: The process cannot access the file.";
        assert_eq!(
            scrub_paths(text, &[dir]),
            "IO error: Failed to create lock file: the database's lock file: The process cannot access the file."
        );
    }

    #[test]
    fn the_two_personas_read_the_same() {
        let dir = Path::new("/home/someone/.sovereign");
        let a = scrub_paths("open /home/someone/.sovereign/data/sovereign.db: Not a directory", &[dir]);
        let b = scrub_paths("open /home/someone/.sovereign/data/sovereign-duress.db: Not a directory", &[dir]);
        assert_eq!(a, b);
        assert_eq!(a, "open the database: Not a directory");
    }

    #[test]
    fn relative_and_bare_names_are_replaced_too() {
        let text = "data/sovereign.db is locked; keys.duress.db and index.key unreadable; auth.store ok";
        assert_eq!(
            scrub_paths(text, &[]),
            "the database is locked; a key database and a key file unreadable; the password store ok"
        );
    }

    #[test]
    fn the_rest_of_the_text_is_kept() {
        let text = "argon2 params: memory cost is too small";
        assert_eq!(scrub_paths(text, &[Path::new("/home/someone/.sovereign")]), text);
    }

    #[test]
    fn the_sentences() {
        let dirs: &[&Path] = &[];
        assert_eq!(LoginFailure::from_authenticate(&CryptoError::WrongPassphrase, dirs).to_string(), "Invalid password");
        assert_eq!(
            LoginFailure::from_authenticate(&CryptoError::DerivationFailed("argon2 params: bad".into()), dirs)
                .to_string(),
            "Your password could not be checked: Key derivation failed: argon2 params: bad. This is not a wrong password."
        );
        assert_eq!(
            LoginFailure::not_opened("the store is locked.", dirs).to_string(),
            "Your password is right, but your account could not be opened: the store is locked."
        );
        assert_eq!(LoginFailure::not_opened("", dirs).cause_alone(), "the error gave no reason");
    }
}
