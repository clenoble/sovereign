/// Which persona a passphrase unlocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PersonaKind {
    Primary,
    Duress,
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
