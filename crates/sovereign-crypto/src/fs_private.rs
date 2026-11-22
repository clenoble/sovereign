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

/// Test-only fault injection for the two write paths of this module, so that
/// a test can stand a crash or a power loss at any point of a write (spec
/// ch. 07, *Never a torn store*, AS1-AS3).
///
/// **Every** filesystem operation [`write_private`] and [`write_private_new`]
/// perform (an open or create, a write, a sync, a hard link, a rename, a
/// removal, a directory sync) goes through this plan, in the order the write
/// performs them. Nothing here is compiled outside tests.
#[cfg(test)]
pub(crate) mod fault {
    /// What a test asks of this process's writes. Installed once, at the start
    /// of a child process that a test launched for this purpose.
    #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
    pub(crate) struct Plan {
        /// Hard links fail, with `PermissionDenied`, as they do in the app's
        /// data directory on Céline's phone (measured 2026-10-21).
        pub refuse_hard_links: bool,
        /// The process aborts (`std::process::abort`: no unwinding, no drop, no
        /// cleanup) just before its operation number `n`, counted from 0 over
        /// every operation of this module's writes in this process.
        pub abort_before: Option<usize>,
        /// When the operation the process aborts before is a write of bytes, half
        /// of those bytes reach the file first, then the process aborts.
        pub half_write: bool,
    }

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::OnceLock;

    static PLAN: OnceLock<Plan> = OnceLock::new();
    static OPS: AtomicUsize = AtomicUsize::new(0);

    /// Installs `plan` for every later write of this process.
    pub(crate) fn install(plan: Plan) {
        OPS.store(0, Ordering::SeqCst);
        let _ = PLAN.set(plan);
    }

    /// The number of filesystem operations this module's writes have performed
    /// in this process since the plan was installed.
    pub(crate) fn ops() -> usize {
        OPS.load(Ordering::SeqCst)
    }

    /// Counts one operation about to be performed; aborts the process if the
    /// plan says to crash before it. No plan installed: nothing counted.
    pub(super) fn step() {
        let Some(plan) = PLAN.get() else { return };
        let n = OPS.fetch_add(1, Ordering::SeqCst);
        if plan.abort_before == Some(n) {
            std::process::abort();
        }
    }

    /// As [`step`], for a write of `bytes` to `file`: with `half_write`, half
    /// of them reach the file before the abort.
    pub(super) fn step_write(file: &mut std::fs::File, bytes: &[u8]) {
        let Some(plan) = PLAN.get() else { return };
        let n = OPS.fetch_add(1, Ordering::SeqCst);
        if plan.abort_before == Some(n) {
            if plan.half_write {
                use std::io::Write;
                let _ = file.write_all(&bytes[..bytes.len() / 2]);
            }
            std::process::abort();
        }
    }

    /// Whether this process's plan refuses hard links.
    pub(super) fn refuses_hard_links() -> bool {
        PLAN.get().is_some_and(|p| p.refuse_hard_links)
    }
}

// ---- every filesystem operation of the two writes, one per function, so the
// fault plan sees each of them in tests; outside tests these are the plain
// calls.

fn op_open(options: &std::fs::OpenOptions, path: &Path) -> io::Result<std::fs::File> {
    #[cfg(test)]
    fault::step();
    options.open(path)
}

fn op_write(file: &mut std::fs::File, bytes: &[u8]) -> io::Result<()> {
    #[cfg(test)]
    fault::step_write(file, bytes);
    file.write_all(bytes)
}

fn op_sync(file: &std::fs::File) -> io::Result<()> {
    #[cfg(test)]
    fault::step();
    file.sync_all()
}

fn op_rename(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    fault::step();
    std::fs::rename(from, to)
}

fn op_remove(path: &Path) -> io::Result<()> {
    #[cfg(test)]
    fault::step();
    std::fs::remove_file(path)
}

fn op_hard_link(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    {
        fault::step();
        if fault::refuses_hard_links() {
            return Err(io::Error::new(io::ErrorKind::PermissionDenied, "hard links refused (test plan)"));
        }
    }
    std::fs::hard_link(from, to)
}

/// Syncs `path`'s directory, so a rename or a new entry survives a power
/// loss. Best effort: a directory that cannot be opened is left as it is.
fn op_sync_dir(path: &Path) {
    #[cfg(test)]
    fault::step();
    #[cfg(unix)]
    if let Some(dir) = path.parent() {
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
    }
    #[cfg(not(unix))]
    let _ = path;
}

/// Renames `from` to `to` only if `to` does not exist, atomically at the
/// directory entry: `renameat2(RENAME_NOREPLACE)` on Linux and Android, where
/// an app's data directory may refuse hard links. Elsewhere, or where the
/// filesystem does not support the flag, a check then a rename: atomic for the
/// file's content, no-replace only against a single writer.
fn op_rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(test)]
    fault::step();
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;
        let c = |p: &Path| {
            CString::new(p.as_os_str().as_bytes()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a path holds a NUL byte"))
        };
        let (f, t) = (c(from)?, c(to)?);
        // SAFETY: two valid NUL-terminated paths, resolved against the
        // current directory (AT_FDCWD), and a flag the kernel checks.
        let r = unsafe {
            libc::syscall(
                libc::SYS_renameat2,
                libc::AT_FDCWD,
                f.as_ptr(),
                libc::AT_FDCWD,
                t.as_ptr(),
                libc::RENAME_NOREPLACE as libc::c_uint,
            )
        };
        if r == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        match e.raw_os_error() {
            Some(libc::EEXIST) => return Err(already_exists(to)),
            // The kernel or the filesystem does not know the flag.
            Some(libc::EINVAL) | Some(libc::ENOSYS) => {}
            _ => return Err(e),
        }
    }
    if std::fs::symlink_metadata(to).is_ok() {
        return Err(already_exists(to));
    }
    std::fs::rename(from, to)
}

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
        let mut file = op_open(&options, &tmp)?;
        op_write(&mut file, bytes.as_ref())?;
        op_sync(&file)?;
        drop(file);
        op_rename(&tmp, path)
    })();

    if result.is_err() {
        let _ = op_remove(&tmp);
        return result;
    }

    // Make the rename itself durable across power loss.
    op_sync_dir(path);
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
/// other than `AlreadyExists`, as in the app's data directory on Android), the
/// temp sibling is renamed into place without replacing
/// (`renameat2(RENAME_NOREPLACE)` on Linux and Android). Either way a crash
/// leaves no `path`, or the whole file, never a torn one (spec ch. 07, AS1).
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
        let mut file = op_open(&options, &tmp)?;
        op_write(&mut file, bytes)?;
        op_sync(&file)
    })();
    if let Err(e) = written {
        let _ = op_remove(&tmp);
        return Err(e);
    }

    let mode = match op_hard_link(&tmp, path) {
        Ok(()) => {
            let _ = op_remove(&tmp);
            NewWrite::Linked
        }
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let _ = op_remove(&tmp);
            return Err(already_exists(path));
        }
        // Hard links refused here: the temp file itself is moved into place,
        // never replacing, so the store is still never written in place.
        Err(_) => match op_rename_noreplace(&tmp, path) {
            Ok(()) => NewWrite::InPlace,
            Err(e) => {
                let _ = op_remove(&tmp);
                return Err(e);
            }
        },
    };

    // Make the new directory entry durable across power loss.
    op_sync_dir(path);
    Ok(mode)
}

/// How [`write_private_new`] published the new file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewWrite {
    /// Temp sibling + no-replace hard link: a crash cannot leave a torn file.
    Linked,
    /// Hard links refused here. Since v0.0.13 the temp sibling is renamed
    /// into place, never replacing ([`op_rename_noreplace`]), so a crash
    /// cannot leave a torn file here either (spec ch. 07, AS1). The name is
    /// kept from when this path wrote the file in place: the log reads it, and
    /// a test pins it.
    InPlace,
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
