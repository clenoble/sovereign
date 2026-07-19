//! F1 Surface 2 — pre-login Guardian **Access** Recovery driver (native shell).
//!
//! The user forgot their passphrase. This device already holds the pre-login
//! `recovery_card.json` + `recovery.bundle` (written during enrollment), so
//! recovery needs no card entry: build one dialable circuit address per
//! guardian from the card, then poll the guardians until ≥threshold release
//! their Recovery-Key shares (72h each). Once enough are in, the owner sets a
//! NEW passphrase and the account is re-wrapped + unlocked
//! ([`crate::crypto::recover_and_install`]).
//!
//! Thin wrappers over the public `sovereign_p2p::access_recovery::AccessRecovery`
//! — the same driver the Tauri owner's commands use, so the two faces behave
//! identically. State lives on disk, so a recovery resumes across restarts.
//! No node is required: `poll_round` dials the guardians itself, which is why
//! this works pre-login (nothing is unlocked yet).

use std::path::PathBuf;
use std::time::Duration;

use sovereign_p2p::access_recovery::{AccessRecovery, AccessRecoveryPhase, GuardianRequest};

/// Where the in-progress recovery is persisted. Mirrors the app's `access_dir`.
pub(crate) fn access_dir() -> PathBuf {
    sovereign_core::sovereign_dir().join("recovery")
}

/// A UI snapshot of an in-progress recovery. Phase is the enum (not a string)
/// so the wizard matches on it directly.
#[derive(Debug, Clone)]
pub(crate) struct RecoveryStatus {
    pub(crate) phase: AccessRecoveryPhase,
    pub(crate) shares_collected: u8,
    pub(crate) threshold: u8,
    /// (guardian id, released) per guardian, in roster order.
    pub(crate) guardians: Vec<(String, bool)>,
    pub(crate) error: Option<String>,
}

impl RecoveryStatus {
    fn of(rec: &AccessRecovery) -> Self {
        Self {
            phase: rec.phase,
            shares_collected: rec.shares_collected() as u8,
            threshold: rec.threshold,
            guardians: rec
                .guardians
                .iter()
                .map(|g: &GuardianRequest| (g.guardian_id.clone(), g.released))
                .collect(),
            error: rec.error.clone(),
        }
    }

    /// Enough shares are in — the owner can set a new passphrase.
    pub(crate) fn ready_to_finalize(&self) -> bool {
        self.shares_collected >= self.threshold && self.threshold > 0
    }

    /// Nothing left to poll.
    pub(crate) fn terminal(&self) -> bool {
        matches!(
            self.phase,
            AccessRecoveryPhase::Installed | AccessRecoveryPhase::Failed
        )
    }
}

/// Can this device do guardian access recovery? True when a recovery card +
/// bundle are present. Gates the login screen's "Recover with guardians" entry.
pub(crate) fn available() -> bool {
    let store = crate::crypto::recovery_store();
    matches!(store.read_recovery_card(), Ok(Some(_)))
        && matches!(store.read_bundle(), Ok(Some(_)))
}

/// Current status without polling (cheap; for mount/resume). `None` if no
/// recovery is in progress.
pub(crate) fn status() -> Option<RecoveryStatus> {
    AccessRecovery::load(&access_dir())
        .as_ref()
        .map(RecoveryStatus::of)
}

/// Derive the at-rest sealing key from the new passphrase + the per-recovery
/// salt (Argon2id, in `sovereign-crypto` — the KDF stays in the crypto crate).
fn seal_key_for(passphrase: &str, salt: &[u8]) -> Result<[u8; 32], String> {
    sovereign_crypto::recovery_seal::derive_seal_key(passphrase.as_bytes(), salt)
        .map_err(|e| format!("derive seal key: {e}"))
}

/// Begin recovery from this device's own card + bundle, sealing the collected
/// shares at rest under a key derived from `new_passphrase` (RECOVERY-001 /
/// SEAM A): a fresh random salt + Argon2id → seal key, so `access_recovery.json`
/// is never a passphrase-free path to the account. Builds one relay-circuit dial
/// address per guardian. Mirrors the Tauri `start_access_recovery` exactly.
pub(crate) fn start(new_passphrase: &str) -> Result<RecoveryStatus, String> {
    let store = crate::crypto::recovery_store();
    let card = store
        .read_recovery_card()?
        .ok_or_else(|| "no recovery card on this device".to_string())?;
    let bundle = store
        .read_bundle()?
        .ok_or_else(|| "no recovery bundle on this device".to_string())?;
    if card.relays.is_empty() {
        return Err("recovery card has no relay to reach guardians".into());
    }
    // <relay .../p2p/RELAY>/p2p-circuit/p2p/<guardian> — one per guardian.
    let relay = &card.relays[0];
    let guardians: Vec<(String, String)> = card
        .guardian_peer_ids
        .iter()
        .map(|gid| (gid.clone(), format!("{relay}/p2p-circuit/p2p/{gid}")))
        .collect();
    if guardians.len() < card.threshold as usize {
        return Err(format!(
            "recovery card lists {} guardian(s) but {} are needed",
            guardians.len(),
            card.threshold
        ));
    }

    // recovery_id: a monotonic-ish label. No wall clock available on this path
    // beyond SystemTime; mirror the app's `acc-<secs>` form.
    let recovery_id = format!(
        "acc-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    let mut rec = AccessRecovery::new(
        recovery_id,
        card.owner_tag,
        card.epoch,
        card.threshold,
        guardians,
        bundle,
    );
    // Seal from the start: fresh salt (non-secret, stored for resume) → seal key
    // → save the (empty-so-far) share set sealed + a probe. random_hex_32 is
    // 64 bytes, safely over Argon2id's ≥8-byte minimum.
    let salt = sovereign_crypto::random_hex_32().into_bytes();
    let seal_key = seal_key_for(new_passphrase, &salt)?;
    rec.set_seal_salt(salt);
    rec.save(&access_dir(), &seal_key)
        .map_err(|e| format!("save recovery: {e}"))?;
    Ok(RecoveryStatus::of(&rec))
}

/// Resume an in-progress recovery (relaunch mid-recovery): re-derive the sealing
/// key from `passphrase` + the stored salt and check the probe. A wrong
/// passphrase returns the frozen sentinel `Err("wrong-passphrase")` (the wizard
/// re-prompts); a match returns the current status so the wait resumes.
pub(crate) fn resume(passphrase: &str) -> Result<RecoveryStatus, String> {
    let rec = AccessRecovery::load(&access_dir())
        .ok_or_else(|| "no recovery in progress".to_string())?;
    let seal_key = seal_key_for(passphrase, rec.seal_salt())?;
    if !rec.verify_probe(&seal_key) {
        return Err("wrong-passphrase".into());
    }
    Ok(RecoveryStatus::of(&rec))
}

/// One network round: unseal the collected shares, poll guardians for newly
/// released ones, re-seal, persist. `passphrase` re-derives the seal key (shares
/// live in memory only; on disk they're always sealed). `None` if no recovery is
/// in progress. Async — the caller `block_on`s it.
pub(crate) async fn poll(passphrase: &str) -> Result<Option<RecoveryStatus>, String> {
    let dir = access_dir();
    let Some(mut rec) = AccessRecovery::load(&dir) else {
        return Ok(None);
    };
    let seal_key = seal_key_for(passphrase, rec.seal_salt())?;
    // Decrypt the shares collected so far into memory before the round appends.
    rec.unseal(&seal_key).map_err(|e| format!("unseal shares: {e}"))?;
    rec.poll_round(Duration::from_secs(20)).await;
    // Re-seal (now including any share this round added) before it touches disk.
    rec.save(&dir, &seal_key).map_err(|e| format!("save recovery: {e}"))?;
    Ok(Some(RecoveryStatus::of(&rec)))
}

/// Abandon an in-progress recovery (local only).
pub(crate) fn cancel() {
    AccessRecovery::cancel(&access_dir());
}
