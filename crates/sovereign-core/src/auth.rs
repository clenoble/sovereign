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
