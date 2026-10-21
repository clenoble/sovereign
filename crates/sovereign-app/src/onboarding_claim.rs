//! Account creation is **single-flight** (spec ch. 07, *Auth store*, "Created
//! once, never replaced by account creation", v2.0.46 as extended).
//!
//! Every command that writes the first `auth.store` of a profile — fresh
//! onboarding, paired onboarding, backup recovery — takes the claim of that
//! profile directory first, before any key derivation and before any file
//! write, and holds the guard until `auth.store` is written. A request that
//! arrives while a claim is held is refused [`ClaimRefused::InProgress`]; one
//! that arrives once `auth.store` exists is refused
//! [`ClaimRefused::AlreadyOnboarded`]. A refusal changes no file.
//!
//! The claim has no kind: it is keyed by the profile directory alone, so an
//! onboarding and a backup recovery on one profile exclude each other.
//!
//! Scope: one process. Two processes on one profile directory are not
//! excluded by this claim; the exclusive write of `auth.store`
//! (`AuthStore::save_new`) still refuses the second one's first write.

use std::collections::BTreeSet;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// Why a claim was refused. `Display` gives the spec's two sentences, which
/// the commands return as their error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClaimRefused {
    /// Another account creation or recovery holds the claim on this profile.
    InProgress,
    /// `crypto/auth.store` already exists: the account is created.
    AlreadyOnboarded,
}

impl fmt::Display for ClaimRefused {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ClaimRefused::InProgress => "Setup is already in progress",
            ClaimRefused::AlreadyOnboarded => "Already onboarded — log in instead",
        })
    }
}

impl std::error::Error for ClaimRefused {}

/// The command error for a failed first write of `auth.store`
/// (`AuthStore::save_new`): a refusal because the file exists says
/// "Already onboarded — log in instead"; any other failure says what it was.
#[cfg(feature = "encryption")]
pub(crate) fn save_new_error(e: sovereign_crypto::CryptoError) -> String {
    match e {
        sovereign_crypto::CryptoError::AlreadyExists(_) => ClaimRefused::AlreadyOnboarded.to_string(),
        other => other.to_string(),
    }
}

/// The held claim on one profile directory. Dropping it releases the claim.
/// `Send`, so it can be held across `.await` in a Tauri async command.
#[must_use = "the claim is released as soon as the guard is dropped; bind it to a named variable"]
#[derive(Debug)]
pub(crate) struct OnboardingClaim {
    key: PathBuf,
}

impl Drop for OnboardingClaim {
    fn drop(&mut self) {
        held().remove(&self.key);
    }
}

/// The claimed profile directories of this process.
static HELD: Mutex<BTreeSet<PathBuf>> = Mutex::new(BTreeSet::new());

/// A poisoned lock is recovered: the set is only ever inserted into or
/// removed from, one call at a time, so it stays consistent.
fn held() -> MutexGuard<'static, BTreeSet<PathBuf>> {
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// One spelling per directory, so `a/./b`, `a/b/` and a relative path name
/// the same claim: the canonical path when the directory exists, else the
/// absolute path, else the path as given.
fn key_of(profile_dir: &Path) -> PathBuf {
    std::fs::canonicalize(profile_dir)
        .or_else(|_| std::path::absolute(profile_dir))
        .unwrap_or_else(|_| profile_dir.to_path_buf())
}

/// Claim account creation on `profile_dir`.
///
/// Refused `AlreadyOnboarded` if `profile_dir/crypto/auth.store` exists,
/// `InProgress` if another claim on the same directory is held; otherwise
/// returns the guard. The existence check and the insertion happen under one
/// lock, and a holder writes `auth.store` before it releases, so a claim taken
/// after a completed creation always sees the store.
pub(crate) fn claim(profile_dir: &Path) -> Result<OnboardingClaim, ClaimRefused> {
    let key = key_of(profile_dir);
    let mut held = held();
    if held.contains(&key) {
        return Err(ClaimRefused::InProgress);
    }
    if profile_dir.join("crypto").join("auth.store").exists() {
        return Err(ClaimRefused::AlreadyOnboarded);
    }
    held.insert(key.clone());
    Ok(OnboardingClaim { key })
}
