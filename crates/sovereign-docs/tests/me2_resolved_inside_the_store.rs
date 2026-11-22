//! ME2 — media are resolved only inside the media store.
//!
//! Spec ch. 03, *Received content is hostile — contract (C-MEDIA)*: "**ME2 —
//! Media are resolved only inside the media store.** A hash is computed only
//! over what the media store holds, named by the store. It is never computed
//! from a path that content supplies.
//!   - **Lexical refusals come first, before any file-system call:** an
//!     absolute path, a network path, a device name, `..`, and on Windows the
//!     `\\?\` prefix and alternate data streams.
//!   - **Then a resolution inside the store** opens nothing unless the resolved
//!     path is inside it. A link or junction that leaves the store is refused
//!     there."
//!
//! The service side of ME2 (no hash from a content path) is in
//! `me4_every_caller.rs` and `media_store_copy.rs`. Here: the store's own
//! resolution, over real files in a temporary directory (never the user's
//! data: no `SOVEREIGN_DATA_DIR` is involved, the store root is a fresh temp
//! folder).

use std::path::{Path, PathBuf};

use sovereign_docs::{check_media_name, resolve_in_store, sha256_in_store, MediaError, MEDIA_SIZE_CAP};

/// sha256("test").
const SHA_TEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let d = std::env::temp_dir().join(format!("sov-me2-{}-{nanos}-{tag}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Removes its folders when the test ends, passed or not; a link first, so a
/// junction is removed as a link and never followed.
struct Cleanup(Vec<PathBuf>);
impl Drop for Cleanup {
    fn drop(&mut self) {
        for p in &self.0 {
            if std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false) || p.ends_with("escape") {
                let _ = std::fs::remove_dir(p).or_else(|_| std::fs::remove_file(p));
            } else {
                let _ = std::fs::remove_dir_all(p);
            }
        }
    }
}

fn assert_refused(name: &str) {
    match check_media_name(name) {
        Err(MediaError::Refused(_)) => {}
        other => panic!("{name:?} must be refused from its name alone, got {other:?}"),
    }
}

// ME2, lexical: an absolute path is refused.
#[test]
fn me2_an_absolute_path_is_refused() {
    for n in ["/etc/passwd", "/dev/zero", "/x.png"] {
        assert_refused(n);
    }
    #[cfg(windows)]
    for n in ["C:/Windows/win.ini", r"C:\Windows\win.ini", r"\Windows\win.ini"] {
        assert_refused(n);
    }
}

// ME2, lexical: a network path is refused.
#[test]
fn me2_a_network_path_is_refused() {
    for n in [r"\\server\share\x.png", "//server/share/x.png"] {
        assert_refused(n);
    }
}

// ME2, lexical: a device name is refused.
#[test]
fn me2_a_device_name_is_refused() {
    for n in ["CON", "nul", "PRN", "AUX", "COM1", "LPT1", "nul.png", "con.txt", "a/NUL", "a/com3.jpg"] {
        assert_refused(n);
    }
    #[cfg(windows)]
    assert_refused(r"\\.\PhysicalDrive0");
}

// ME2, lexical: `..` is refused.
#[test]
fn me2_a_parent_component_is_refused() {
    for n in ["..", "../x.png", "a/../../x.png", "a/.."] {
        assert_refused(n);
    }
    #[cfg(windows)]
    for n in [r"..\x.png", r"a\..\..\x.png"] {
        assert_refused(n);
    }
}

// ME2, lexical, on Windows: the `\\?\` prefix is refused.
#[cfg(windows)]
#[test]
fn me2_the_verbatim_prefix_is_refused_on_windows() {
    for n in [r"\\?\C:\x.png", r"\\?\UNC\server\share\x.png", "//?/C:/x.png"] {
        assert_refused(n);
    }
}

// ME2, lexical, on Windows: an alternate data stream is refused.
#[cfg(windows)]
#[test]
fn me2_an_alternate_data_stream_is_refused_on_windows() {
    for n in ["file.jpg:stream", "file.jpg:stream:$DATA", "file.jpg::$DATA", "a/file.jpg:x"] {
        assert_refused(n);
    }
}

// ME2: a plain name inside the store passes the lexical check (a check that refuses everything is not ME2).
#[test]
fn me2_a_plain_store_name_passes_the_lexical_check() {
    for n in ["a.png", "ab/cd.png", "media-0f3a.bin", "a.b.c.jpg"] {
        assert_eq!(check_media_name(n), Ok(()), "{n:?} names something inside the store");
    }
}

// ME2: the lexical refusal comes before any file-system call — the target exists, or the host does not, and the answer is still a refusal.
#[test]
fn me2_lexical_refusal_comes_before_any_file_system_call() {
    let root = temp_dir("lex-root");
    let outside = temp_dir("lex-outside");
    let secret = outside.join("secret.png");
    std::fs::write(&secret, b"test").unwrap();
    let _clean = Cleanup(vec![root.clone(), outside.clone()]);
    let absolute = secret.to_string_lossy().to_string();
    // An existing file, named absolutely: a refusal, not a read.
    for n in [absolute.as_str(), "//l1b-test-host.invalid/share/x.png"] {
        match resolve_in_store(&root, n) {
            Err(MediaError::Refused(_)) => {}
            other => panic!("{n:?}: refused from its name before any file-system call, got {other:?}"),
        }
        match sha256_in_store(&root, n, MEDIA_SIZE_CAP) {
            Err(MediaError::Refused(_)) => {}
            other => panic!("{n:?}: hashing refused from its name before any file-system call, got {other:?}"),
        }
    }
    // A store root that does not exist yet still refuses by name, not by I/O.
    let missing = root.join("not-there");
    assert!(matches!(resolve_in_store(&missing, "../x.png"), Err(MediaError::Refused(_))));
}

// ME2: a name inside the store resolves inside it, and the hash is over what the store holds.
#[test]
fn me2_a_name_inside_the_store_resolves_inside_and_hashes_what_it_holds() {
    let root = temp_dir("inside");
    let _clean = Cleanup(vec![root.clone()]);
    std::fs::create_dir_all(root.join("ab")).unwrap();
    std::fs::write(root.join("ab").join("cd.png"), b"test").unwrap();
    let p = resolve_in_store(&root, "ab/cd.png").expect("inside the store");
    let canon_root = std::fs::canonicalize(&root).unwrap();
    let canon_p = std::fs::canonicalize(&p).unwrap_or(p.clone());
    assert!(canon_p.starts_with(&canon_root), "{canon_p:?} is inside {canon_root:?}");
    assert_eq!(sha256_in_store(&root, "ab/cd.png", MEDIA_SIZE_CAP), Ok(SHA_TEST.to_string()));
}

/// Makes `link` a directory link to `target`: a symbolic link where the
/// environment allows one, else (Windows) a junction, which needs no
/// privilege. `None` if neither can be made here.
fn dir_link(target: &Path, link: &Path) -> Option<&'static str> {
    #[cfg(unix)]
    {
        return std::os::unix::fs::symlink(target, link).ok().map(|_| "symlink");
    }
    #[cfg(windows)]
    {
        if std::os::windows::fs::symlink_dir(target, link).is_ok() {
            return Some("symlink");
        }
        let ok = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        return ok.then_some("junction");
    }
    #[allow(unreachable_code)]
    None
}

// ME2: a link or junction that leaves the store is refused at resolution.
#[test]
fn me2_a_link_that_leaves_the_store_is_refused() {
    let root = temp_dir("link-root");
    let outside = temp_dir("link-outside");
    std::fs::write(outside.join("secret.png"), b"test").unwrap();
    let link = root.join("escape");
    let _clean = Cleanup(vec![link.clone(), root.clone(), outside.clone()]);
    let kind = dir_link(&outside, &link).expect(
        "this environment can make neither a symbolic link nor a junction: the link case of ME2 is untested here",
    );
    for n in ["escape/secret.png", "escape"] {
        assert_eq!(resolve_in_store(&root, n), Err(MediaError::OutsideStore), "{kind} {n:?} leaves the store");
    }
    assert_eq!(sha256_in_store(&root, "escape/secret.png", MEDIA_SIZE_CAP), Err(MediaError::OutsideStore), "{kind}");
}
