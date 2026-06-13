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
}
