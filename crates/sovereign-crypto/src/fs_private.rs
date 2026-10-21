//! Owner-only, atomic file writes for key material.
//!
//! Plain `std::fs::write` inherits the process umask (typically 0644 on
//! Unix), leaving wrapped keys, auth stores, and salts readable by every
//! local user — and it truncates the destination in place, so a crash
//! mid-write destroys the only copy of the key DB (audit CRYPTO-005).
//! All key-material writes go through here instead: content lands in a
//! temp sibling first, is fsynced, then renamed over the target so the
//! file is always either the old version or the new one, never a torn
//! half-write.

use std::io::{self, Write};
use std::path::Path;

/// Atomically write `bytes` to `path`, restricted to the owner (0600) on
/// Unix. On Windows the per-user profile ACL already scopes access.
pub fn write_private(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<()> {
    let tmp = tmp_sibling(path)?;

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let result = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes.as_ref())?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&tmp, path)
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return result;
    }

    // Make the rename itself durable across power loss.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }

    Ok(())
}

/// Write `bytes` to `path` only if `path` does not exist yet — an exclusive
/// create, never a replace (spec ch. 07, *Auth store*, "Created once").
/// Same owner-only permissions as [`write_private`].
///
/// If `path` exists (whatever it holds), returns an `AlreadyExists` error and
/// changes no file: the existing file stays byte-identical and no temp file is
/// left beside it.
///
/// How: the content is written and fsynced to a uniquely named temp sibling,
/// then published with `std::fs::hard_link(tmp, path)`, which fails if `path`
/// exists (no-replace, atomic at the directory entry), and the temp name is
/// removed. A crash mid-write leaves at most a stray temp file, never a
/// half-written `path` — a torn `auth.store` would block onboarding for good,
/// since its mere existence refuses the next one.
///
/// Fallback: on a filesystem or sandbox that refuses hard links (any error
/// other than `AlreadyExists`), the target is opened with `create_new(true)`
/// (still exclusive) and written in place; only on that path can a crash
/// mid-write leave a torn file.
///
/// Returns which of the two published the file ([`NewWrite`]), so a caller
/// can log it (whether a platform's sandbox allows hard links is a fact to
/// observe, not to assume).
pub fn write_private_new(path: &Path, bytes: impl AsRef<[u8]>) -> io::Result<NewWrite> {
    let bytes = bytes.as_ref();
    if std::fs::symlink_metadata(path).is_ok() {
        return Err(already_exists(path));
    }
    let tmp = unique_tmp_sibling(path)?;

    // The temp name is unique, so `create_new` never touches another writer's file.
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    let written = (|| {
        let mut file = options.open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }

    let linked = std::fs::hard_link(&tmp, path);
    let _ = std::fs::remove_file(&tmp);
    let mode = match linked {
        Ok(()) => NewWrite::Linked,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(already_exists(path)),
        Err(_) => {
            write_in_place_exclusive(path, bytes)?;
            NewWrite::InPlace
        }
    };

    // Make the new directory entry durable across power loss.
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    Ok(mode)
}

/// How [`write_private_new`] published the new file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewWrite {
    /// Temp sibling + no-replace hard link: a crash cannot leave a torn file.
    Linked,
    /// Hard links refused here: exclusive in-place write (`create_new`), which
    /// a crash mid-write can leave torn.
    InPlace,
}

/// The fallback of [`write_private_new`] when hard links are refused: an
/// exclusive open of the target itself. If the write fails after the create,
/// the file this call created is removed.
fn write_in_place_exclusive(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = match options.open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => return Err(already_exists(path)),
        Err(e) => return Err(e),
    };
    let result = file.write_all(bytes).and_then(|()| file.sync_all());
    if result.is_err() {
        drop(file);
        let _ = std::fs::remove_file(path);
    }
    result
}

fn already_exists(path: &Path) -> io::Error {
    io::Error::new(
        io::ErrorKind::AlreadyExists,
        format!("refusing to replace an existing file: {}", path.display()),
    )
}

/// A temp sibling no other writer uses: `.<name>.<pid>.<random>.tmp`.
fn unique_tmp_sibling(path: &Path) -> io::Result<std::path::PathBuf> {
    use rand::RngExt;
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("write_private_new: path has no file name: {}", path.display()),
        )
    })?;
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(".{}.{:016x}.tmp", std::process::id(), rand::rng().random::<u64>()));
    Ok(path.with_file_name(tmp_name))
}

fn tmp_sibling(path: &Path) -> io::Result<std::path::PathBuf> {
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("write_private: path has no file name: {}", path.display()),
        )
    })?;
    let mut tmp_name = file_name.to_os_string();
    tmp_name.push(".tmp");
    Ok(path.with_file_name(tmp_name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_content() {
        let path = std::env::temp_dir().join("sovereign-write-private-test");
        write_private(&path, b"secret").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"secret");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn overwrites_existing_and_leaves_no_temp() {
        let path = std::env::temp_dir().join("sovereign-write-private-overwrite-test");
        write_private(&path, b"first").unwrap();
        write_private(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        assert!(!tmp_sibling(&path).unwrap().exists());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn rejects_path_without_file_name() {
        let err = write_private(Path::new("/"), b"x").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[cfg(unix)]
    #[test]
    fn sets_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;
        let path = std::env::temp_dir().join("sovereign-write-private-mode-test");
        write_private(&path, b"secret").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let _ = std::fs::remove_file(&path);
    }

    #[cfg(unix)]
    #[test]
    fn write_private_new_sets_owner_only_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("sovereign-write-private-new-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("auth.store");
        let _ = std::fs::remove_file(&path);
        write_private_new(&path, b"secret").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temp file left");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
