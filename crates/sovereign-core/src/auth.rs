use std::path::{Path, PathBuf};

/// Which persona a passphrase unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonaKind {
    Primary,
    Duress,
}

/// Where a persona's orchestrator state (session log, user profile, trust
/// state) lives under the data directory `base`. The primary keeps the
/// historical `orchestrator` directory; the duress persona gets a sibling of
/// its own, so neither session can read, verify or overwrite the other's
/// state (spec ch. 07, *After login — every persona's state stays its own*).
pub fn orchestrator_state_dir(base: &Path, persona: PersonaKind) -> PathBuf {
    match persona {
        PersonaKind::Primary => base.join("orchestrator"),
        PersonaKind::Duress => base.join("orchestrator-duress"),
    }
}

/// Path of a persona's session-log beacon. It lives in the crypto directory,
/// outside the persona's log directory, so wiping the log directory does not
/// remove the proof that the log had content. The primary keeps the
/// historical name; the duress persona gets its own.
pub fn session_log_beacon_path(base: &Path, persona: PersonaKind) -> PathBuf {
    let name = match persona {
        PersonaKind::Primary => "session_log.beacon",
        PersonaKind::Duress => "session_log.duress.beacon",
    };
    base.join("crypto").join(name)
}

/// Directory of a persona's media store (contract C-MEDIA, spec ch. 03, *The
/// media store*): Sovereign's encrypted copies of the images and videos its
/// documents hold. It lies **inside the persona's own state directory**
/// ([`orchestrator_state_dir`]), so it is in that persona's area as C-AREA
/// names it (spec ch. 07): a duress session's store is inside the duress
/// area, and nothing synced or backed up reads this directory. (The record of
/// what the migration pass neutralised is a sealed record of the persona's
/// own database, not a file here.)
pub fn media_store_dir(base: &Path, persona: PersonaKind) -> PathBuf {
    media_store_dir_in(&orchestrator_state_dir(base, persona))
}

/// The media store's directory inside a persona's state directory
/// (`state_dir`, as [`orchestrator_state_dir`] gives it): for a host that
/// already holds that directory, so it computes no new path under the
/// Sovereign directory (C-AREA AR8).
pub fn media_store_dir_in(state_dir: &Path) -> PathBuf {
    state_dir.join("media")
}

/// Directory of a persona's identity profile (`user_profile.json`: display
/// name, nickname, designation), read and written only after login. The
/// primary keeps the historical location, `base` itself, so at migration the
/// existing identity is the primary's; the duress persona gets a directory of
/// its own, apart from the orchestrator state. Device preferences read before
/// login (theme, bubble style) stay in the profile file at `base`, shared
/// (spec ch. 07, *After login — every persona's state stays its own*).
pub fn identity_profile_dir(base: &Path, persona: PersonaKind) -> PathBuf {
    match persona {
        PersonaKind::Primary => base.to_path_buf(),
        PersonaKind::Duress => base.join("profile-duress"),
    }
}

/// Events emitted by the authentication system for the UI to handle.
#[derive(Debug, Clone)]
pub enum AuthEvent {
    LoginSuccess(PersonaKind),
    LoginFailed { attempts_remaining: u32 },
    AccountLocked { until_secs: u64 },
    ReauthRequired,
    CanaryTriggered,
    LockdownComplete,
}

#[cfg(test)]
#[path = "persona_state_dir_tests.rs"]
mod persona_state_dir_tests;
