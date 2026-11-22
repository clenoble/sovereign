//! This device's name (v0.0.12 item 19, `doc/plans/V0012-items14-19-devices.md`,
//! N1, N1b, N2).
//!
//! - **N1** — one name per workspace: a file of its own in the data directory
//!   (`sovereign_dir()`), read at start, overriding `[p2p] device_name`.
//!   **Since v0.0.13, one per persona** (C-AREA AR2, spec ch. 07: « it is what
//!   the persona's paired devices are shown; it differs per persona »): the
//!   primary keeps the data directory's file, so an existing name is the
//!   primary's; the duress persona's is in its own identity directory. Before
//!   login the primary's is read; at login, the persona's.
//! - **N1b** — the name is not the document identity: since v0.0.12 item 22
//!   the document service stamps with this persona's libp2p peer id, derived
//!   from its device key (`sovereign_crypto::peer_identity`); this module
//!   never touches it.
//! - **N2** — a name as typed: trimmed, at most [`MAX_CLUSTERS`] grapheme
//!   clusters, never cut inside one; a blank entry means the default.

use std::path::PathBuf;

use sovereign_core::auth::PersonaKind as CorePersona;

use unicode_segmentation::UnicodeSegmentation;

/// The longest name kept, in grapheme clusters (N2).
pub(crate) const MAX_CLUSTERS: usize = 64;

/// The setup field's label, and the rename field's name for this device (N2, N6).
pub(crate) const FIELD_LABEL: &str = "This device's name";

/// The plain words under the setup field (N2).
pub(crate) const SEEN_BY: &str = "Your other devices and your guardians see this name.";

/// Under this device's name in Settings → Devices (N4).
pub(crate) const KEEP_LEARNED: &str =
    "Devices already paired with this one keep the name they learned until you pair them again.";

/// The file holding this device's name, in the workspace (N1).
const FILE_NAME: &str = "device_name.txt";

/// A name as typed (N2): trimmed, each line break a space, at most
/// [`MAX_CLUSTERS`] grapheme clusters, never cut inside one. `None` for a
/// blank or whitespace-only entry, which means the default.
pub(crate) fn clean(input: &str) -> Option<String> {
    let one_line = input.replace("\r\n", " ").replace(['\n', '\r'], " ");
    let trimmed = one_line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let kept: String = trimmed.graphemes(true).take(MAX_CLUSTERS).collect();
    // A cut can leave a space at the end: the name is trimmed after it too.
    let kept = kept.trim_end().to_string();
    (!kept.is_empty()).then_some(kept)
}

/// Where the primary's name for this device is kept (N1): the data
/// directory's file, as before v0.0.13.
pub(crate) fn path() -> PathBuf {
    sovereign_core::sovereign_dir().join(FILE_NAME)
}

/// Where `persona`'s name for this device is kept (C-AREA AR2): the
/// primary's is [`path`]; the duress persona's is in its identity directory,
/// never the primary's file. **No persona, no place** (v2.0.87: « A process
/// with no session has no identity directory »): before any login nothing of
/// a persona's name is read or written.
pub(crate) fn path_of(persona: Option<CorePersona>) -> Option<PathBuf> {
    match persona {
        Some(CorePersona::Duress) => Some(
            sovereign_core::auth::identity_profile_dir(&sovereign_core::sovereign_dir(), CorePersona::Duress)
                .join(FILE_NAME),
        ),
        Some(CorePersona::Primary) => Some(path()),
        None => None,
    }
}

/// `persona`'s name kept in the workspace, if any (N1). An unreadable or
/// blank file is no name; with no persona there is none.
pub(crate) fn load(persona: Option<CorePersona>) -> Option<String> {
    std::fs::read_to_string(path_of(persona)?).ok().and_then(|s| clean(&s))
}

/// `persona`'s name for this device (N1): its own, else the config's, else
/// "Sovereign Device". `None` (before any login): the config's, else the
/// default, never a persona's.
pub(crate) fn resolve(persona: Option<CorePersona>, config_name: &str) -> String {
    load(persona)
        .or_else(|| clean(config_name))
        .unwrap_or_else(|| "Sovereign Device".to_string())
}

/// Keeps `name` (already cleaned) as `persona`'s name for this device, in its
/// own file (N1). The config file is never written. With no persona open
/// (before any login) it is refused, and nothing is written.
pub(crate) fn save(persona: Option<CorePersona>, name: &str) -> std::io::Result<()> {
    let p = path_of(persona).ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::PermissionDenied, "the workspace is locked: log in to rename this device")
    })?;
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    sovereign_crypto::fs_private::write_private(&p, name.as_bytes())
}

/// A device name drawn at random, of the form an operating system gives a
/// computer it names itself (`DESKTOP-` and seven letters or digits, as
/// Windows does): the duress persona's at setup (C-AREA AR2, Céline
/// 2026-11-13: « des noms d'appareil … random générés automatiquement »),
/// never copied from the primary's.
pub(crate) fn generated() -> String {
    // Seven characters of a fresh pairing code: random, letters and digits.
    let tail: String = sovereign_crypto::pair_payload::generate_pairing_code()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(7)
        .collect::<String>()
        .to_ascii_uppercase();
    format!("DESKTOP-{tail}")
}

/// The computer's name (the OS host name), the setup fields' prefill (N2).
/// Empty when it cannot be read.
pub(crate) fn host_name() -> String {
    let from_env = |k: &str| std::env::var(k).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    if cfg!(windows) {
        return from_env("COMPUTERNAME").unwrap_or_default();
    }
    if let Some(h) = std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return h;
    }
    if let Some(h) = from_env("HOSTNAME") {
        return h;
    }
    std::process::Command::new("hostname")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}
