//! Feature 1 — owner-side recovery-setup persistence + enrollment reconciliation.
//!
//! Wraps [`crate::recovery_roster::RecoverySetup`] with on-disk storage:
//!   - `recovery_roster.enc` — the roster (incl. not-yet-distributed shares),
//!     AEAD-encrypted under the content KEK. Owner-side, login-only.
//!   - `recovery.bundle` — the bare [`RecoveryBundle`], self-encrypted under
//!     the (guardian-held) Recovery Key. Readable **pre-login** by the recovery
//!     flow.
//!
//! Enrollment confirmations arrive as `P2pEvent::GuardianEnrolled` in the p2p
//! event translator, which cannot hold the unlocked KEK. So it appends a
//! non-secret record (`guardian_enrolled.pending`, JSONL: shard_id + guardian
//! peer id/label — never key material) and the owner-side callers — which DO
//! have the KEK — reconcile those into the encrypted roster via
//! [`RecoveryStore::reconcile_pending`].
//!
//! # Why this lives in `sovereign-crypto` (moved 2026-07-16)
//!
//! It began as `sovereign-app::recovery_setup`, a private module. Every one of
//! its dependencies already lived here, and the native shell (`sovereign-shell`)
//! needs the same logic but cannot depend on `sovereign-app`. Duplicating it
//! into the shell would have created two copies of key-handling logic free to
//! diverge — the drift family that cost two failed recoveries during the F1
//! live run. One implementation, two faces: each caller passes its own
//! directory (both resolve to `sovereign_dir()/crypto`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::account_key::AccountKey;
use crate::aead::{self, NONCE_SIZE};
use crate::auth::AuthStore;
use crate::kek::Kek;
use crate::key_db::KeyDatabase;
use crate::recovery_key::RecoveryBundle;
use crate::recovery_roster::{RecoverySetup, GUARDIAN_THRESHOLD};

/// The primary persona's content-key stores. Guardian **Access** Recovery must
/// be able to open every one of these under the recovered KEK before it commits
/// (the duress decoy is re-seeded fresh, so only the primary set matters).
/// Primary-persona filenames are the bare bases (the duress variant appends
/// `.duress.db`), so these are usable as-is.
pub const CONTENT_KEY_STORES: [&str; 6] = [
    "keys.db",
    "keys.messages.db",
    "keys.threads.db",
    "keys.conversations.db",
    "keys.contacts.db",
    "keys.share_records.db",
];

/// Verify-before-commit for Guardian Access Recovery (spec §Guardian Social
/// Recovery, finalize step).
///
/// Proves the recovered KEK actually opens every on-disk content-key store
/// **before** the caller overwrites `auth.store`. A store the KEK cannot open
/// (e.g. one still sealed under a pre-migration Device Key whose passphrase is
/// gone) means this recovery cannot decrypt the data — so the caller must abort
/// and leave the prior `auth.store` intact rather than brick an otherwise
/// recoverable account.
///
/// This lives here, **shared and singular, on purpose.** Its *absence* is what
/// let the first live run overwrite the store before an at-rest failure, and
/// 168 green crypto tests never caught it because every test sealed *and* opened
/// with the same key — none exercised "restore the KEK but not the DeviceKey."
/// Two copies of this check could drift apart exactly where it matters; there
/// must be one, and it must have the test the original lacked (below).
///
/// `Ok(())` if every *present* store opens under `kek`. A store that does not
/// exist yet (a fresh account may not have written it) is skipped. `Err` names
/// the first store that fails — never echoes key material.
pub fn verify_content_key_stores(crypto_dir: &Path, kek: &Kek) -> Result<(), String> {
    for base in CONTENT_KEY_STORES {
        let path = crypto_dir.join(base);
        if path.exists() {
            KeyDatabase::load(&path, kek).map_err(|e| {
                format!("recovery cannot decrypt at-rest content (store {base}): {e}")
            })?;
        }
    }
    Ok(())
}

/// The security-critical core of Guardian Access Recovery finalize (spec
/// §Guardian Social Recovery, step 9): re-create `auth.store` under a NEW
/// passphrase, wrapping the **recovered** secrets — but only after proving they
/// can actually decrypt the on-disk content.
///
/// Ordering is the whole point, and it is why this is one shared function
/// rather than a copy per face:
///
/// 1. [`verify_content_key_stores`] runs **first**. If the recovered KEK cannot
///    open the content-key stores, this returns `Err` **without having written
///    anything** — the prior `auth.store` is left intact, so a failed recovery
///    can never brick an otherwise-recoverable account. This is the exact
///    ordering whose absence let the first live run overwrite the store before
///    an at-rest failure (0050 #4).
/// 2. Only then: a **fresh** salt + device id (the recovered KEK is reused, but
///    the DeviceKey is new — new passphrase, new salt, new device id ⇒ a
///    different DeviceKey; this is why the content chain must be rooted at the
///    KEK, not the DeviceKey), a random unreachable duress decoy, and
///    [`AuthStore::create_with_secrets`] wrapping the recovered KEK + AccountKey
///    under the new passphrase.
/// 3. Persist `auth.store` + the `salt` file, and return the store. The caller
///    installs the session (builds the EncryptedGraphDB) with its own
///    face-specific wiring — synced content now decrypts under the recovered
///    KEK.
///
/// Returns the new [`AuthStore`] on success (the caller then authenticates it
/// under `new_passphrase` to install the session).
pub fn install_recovered_auth_store(
    crypto_dir: &Path,
    kek: &Kek,
    account_key: &AccountKey,
    new_passphrase: &[u8],
) -> Result<AuthStore, String> {
    // (1) Verify BEFORE touching auth.store. Do not reorder.
    verify_content_key_stores(crypto_dir, kek)?;

    // (2) Fresh DeviceKey inputs; the KEK is the recovered one.
    let salt: [u8; 32] = rand::random();
    let device_id = crate::random_hex_32();
    // A skipped/absent duress must never be an empty password (that would open
    // the decoy with an empty string). Random ⇒ unreachable decoy.
    let random_duress = crate::random_hex_32();
    let store = AuthStore::create_with_secrets(
        new_passphrase,
        random_duress.as_bytes(),
        &salt,
        &device_id,
        account_key,
        kek,
    )
    .map_err(|e| format!("create recovered auth store: {e}"))?;

    // (3) Persist. Order store-then-salt to mirror the app finalize exactly.
    std::fs::create_dir_all(crypto_dir).map_err(|e| format!("mkdir crypto dir: {e}"))?;
    store
        .save(&crypto_dir.join("auth.store"))
        .map_err(|e| format!("save auth.store: {e}"))?;
    crate::fs_private::write_private(&crypto_dir.join("salt"), &salt)
        .map_err(|e| format!("write salt: {e}"))?;
    Ok(store)
}

/// Non-secret pointer the recovering (pre-login) device reads to reach the
/// guardians and identify the backup: the guardian tag `T_g`, the enrolled
/// guardian peer ids, and the seed relays to build circuit addresses. Holds
/// no key material — a leak still needs >=3 guardian approvals + the
/// out-of-band human proof. Written whenever the roster changes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryCard {
    pub owner_tag: String,
    pub epoch: u32,
    pub threshold: u8,
    pub guardian_peer_ids: Vec<String>,
    pub relays: Vec<String>,
}

/// One enrollment confirmation from the p2p layer. Non-secret roster metadata
/// only — the share itself already went to the guardian, not into this record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingEnrollment {
    pub shard_id: String,
    pub guardian_peer_id: String,
    pub guardian_label: String,
    pub enrolled_at: String,
}

/// Outcome of [`RecoveryStore::reconcile_pending`].
///
/// Exists so the caller — not this crate — decides what to say about entries
/// that could not be applied. Previously those went to a `tracing` log and the
/// queue was deleted unconditionally, so a malformed record was destroyed
/// while the caller still got `Ok`: a torn append could leave a guardian
/// holding their shard and believing they were enrolled while the owner's
/// roster never marked the slot — discovered only at recovery time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Confirmations folded into the roster.
    pub applied: usize,
    /// Human-readable reasons for entries that were not applied.
    ///
    /// **Never quotes the raw bytes of an unparseable line.** A malformed line
    /// is arbitrary content by definition; echoing it into a string the caller
    /// will log is the credential-leak shape. Only the line number and the
    /// parse error are described.
    ///
    /// Precise bound (so the "never quotes" claim isn't over-read): the parse
    /// error `{e}` is interpolated. For a **syntax** error — the realistic
    /// corruption case, a torn write — serde reports position only ("expected
    /// value at line 1 column 1"), so no content leaks. For a **type-mismatch**
    /// error (valid JSON, wrong shape) serde can quote the offending *parsed*
    /// value — but that is a [`PendingEnrollment`] field
    /// (`shard_id`/`guardian_peer_id`/`guardian_label`/`enrolled_at`), which
    /// carries **no key material**; the worst case echoes a label. Raw
    /// unparsed bytes are never surfaced either way. (Bound noted by
    /// claude-laptop's seam-2 review, coord 0060.)
    pub skipped: Vec<String>,
    /// Unparseable lines left in the queue rather than deleted, so they keep
    /// resurfacing until a human resolves them. "Surfaced, then destroyed" is
    /// still destroyed.
    pub retained: usize,
}

impl Reconciled {
    /// True if something was kept back for a human to look at.
    pub fn needs_attention(&self) -> bool {
        self.retained > 0
    }
}

/// Owner-side recovery state on disk, rooted at a caller-supplied directory.
///
/// The directory is injected rather than resolved from a process-global so
/// that both faces (`sovereign-app`'s `setup::crypto_dir()` and
/// `sovereign-shell`'s `crypto::crypto_dir()`) share one implementation — and
/// so this module is testable at all: as a global it could not be pointed at a
/// tempdir, which is why it carried no tests before the move.
pub struct RecoveryStore {
    dir: PathBuf,
}

impl RecoveryStore {
    /// Root the store at `dir` (callers pass `sovereign_dir()/crypto`).
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// The directory this store reads and writes.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn roster_path(&self) -> PathBuf {
        self.dir.join("recovery_roster.enc")
    }
    fn bundle_path(&self) -> PathBuf {
        self.dir.join("recovery.bundle")
    }
    fn card_path(&self) -> PathBuf {
        self.dir.join("recovery_card.json")
    }
    fn pending_path(&self) -> PathBuf {
        self.dir.join("guardian_enrolled.pending")
    }

    /// Write/refresh the recovery card from the current roster.
    pub fn write_recovery_card(
        &self,
        setup: &RecoverySetup,
        owner_tag: &str,
        relays: &[String],
    ) -> Result<(), String> {
        let card = RecoveryCard {
            owner_tag: owner_tag.to_string(),
            epoch: setup.epoch,
            threshold: GUARDIAN_THRESHOLD,
            guardian_peer_ids: setup
                .slots
                .iter()
                .filter_map(|s| s.guardian_peer_id.clone())
                .collect(),
            relays: relays.to_vec(),
        };
        let json = serde_json::to_vec_pretty(&card).map_err(|e| format!("serialize card: {e}"))?;
        crate::fs_private::write_private(&self.card_path(), &json)
            .map_err(|e| format!("write card: {e}"))
    }

    /// Read the recovery card (pre-login). `None` if recovery was never set up.
    pub fn read_recovery_card(&self) -> Result<Option<RecoveryCard>, String> {
        match std::fs::read(self.card_path()) {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| format!("parse card: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read card: {e}")),
        }
    }

    /// Read the bare recovery bundle (pre-login).
    pub fn read_bundle(&self) -> Result<Option<RecoveryBundle>, String> {
        match std::fs::read(self.bundle_path()) {
            Ok(b) => serde_json::from_slice(&b)
                .map(Some)
                .map_err(|e| format!("parse bundle: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("read bundle: {e}")),
        }
    }

    /// Append a guardian-enrollment confirmation for later reconciliation.
    /// Called from the p2p event translator (no KEK there). Best-effort JSONL
    /// append.
    pub fn append_pending(&self, rec: &PendingEnrollment) -> Result<(), String> {
        use std::io::Write;
        let line = serde_json::to_string(rec).map_err(|e| format!("serialize pending: {e}"))?;
        let path = self.pending_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("mkdir crypto dir: {e}"))?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("open pending: {e}"))?;
        writeln!(f, "{line}").map_err(|e| format!("append pending: {e}"))?;
        Ok(())
    }

    /// Load the owner's recovery roster, decrypting under the KEK. `None` if
    /// setup hasn't run yet.
    pub fn load(&self, kek: &Kek) -> Result<Option<RecoverySetup>, String> {
        let bytes = match std::fs::read(self.roster_path()) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("read recovery roster: {e}")),
        };
        if bytes.len() < NONCE_SIZE {
            return Err("recovery roster truncated".into());
        }
        let (nonce_b, ct) = bytes.split_at(NONCE_SIZE);
        let mut nonce = [0u8; NONCE_SIZE];
        nonce.copy_from_slice(nonce_b);
        let plain =
            aead::decrypt(ct, &nonce, kek.as_bytes()).map_err(|e| format!("decrypt roster: {e}"))?;
        let setup = serde_json::from_slice(&plain).map_err(|e| format!("parse roster: {e}"))?;
        Ok(Some(setup))
    }

    /// Persist the roster (encrypted under the KEK, file = nonce || ciphertext)
    /// and the bare bundle (self-encrypted; readable pre-login by recovery).
    pub fn save(&self, setup: &RecoverySetup, kek: &Kek) -> Result<(), String> {
        let json = serde_json::to_vec(setup).map_err(|e| format!("serialize roster: {e}"))?;
        let (ct, nonce) =
            aead::encrypt(&json, kek.as_bytes()).map_err(|e| format!("encrypt roster: {e}"))?;
        let mut file = Vec::with_capacity(NONCE_SIZE + ct.len());
        file.extend_from_slice(&nonce);
        file.extend_from_slice(&ct);
        crate::fs_private::write_private(&self.roster_path(), &file)
            .map_err(|e| format!("write roster: {e}"))?;

        let bundle_json =
            serde_json::to_vec(&setup.bundle).map_err(|e| format!("serialize bundle: {e}"))?;
        crate::fs_private::write_private(&self.bundle_path(), &bundle_json)
            .map_err(|e| format!("write bundle: {e}"))?;
        Ok(())
    }

    /// Load existing setup, or create it (generate Recovery Key + bundle + 5
    /// pending shares) and persist. Requires the unlocked session's secrets.
    pub fn load_or_create(
        &self,
        kek: &Kek,
        account_key: &AccountKey,
    ) -> Result<RecoverySetup, String> {
        if let Some(existing) = self.load(kek)? {
            return Ok(existing);
        }
        let setup =
            RecoverySetup::new(kek, account_key, 1).map_err(|e| format!("recovery setup: {e}"))?;
        self.save(&setup, kek)?;
        Ok(setup)
    }

    /// Fold any queued `GuardianEnrolled` confirmations into `setup` (mark each
    /// slot enrolled, clearing the distributed share) and persist.
    ///
    /// Queue disposition — **consume what we understood, keep what we didn't**:
    ///
    /// - *Applied* records are consumed: they are now in the roster.
    /// - *Unknown/duplicate slot* records are consumed: genuinely idempotent,
    ///   they will never become valid later.
    /// - *Unparseable* lines are **retained** and counted in
    ///   [`Reconciled::retained`]. A corrupt line is not a duplicate — a torn
    ///   append can make a real enrollment unreadable, and deleting it destroys
    ///   the only evidence that a guardian who believes they are enrolled is
    ///   missing from the roster.
    pub fn reconcile_pending(
        &self,
        setup: &mut RecoverySetup,
        kek: &Kek,
    ) -> Result<Reconciled, String> {
        let path = self.pending_path();
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Reconciled::default()),
            Err(e) => return Err(format!("read pending: {e}")),
        };

        let mut out = Reconciled::default();
        let mut keep: Vec<&str> = Vec::new();

        for (idx, line) in content
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
        {
            let rec: PendingEnrollment = match serde_json::from_str(line) {
                Ok(r) => r,
                Err(e) => {
                    // Describe, never quote: the line is arbitrary bytes.
                    out.skipped
                        .push(format!("line {}: unparseable, kept for review ({e})", idx + 1));
                    keep.push(line);
                    continue;
                }
            };
            match setup.mark_enrolled(
                &rec.shard_id,
                &rec.guardian_peer_id,
                &rec.guardian_label,
                &rec.enrolled_at,
            ) {
                Ok(()) => out.applied += 1,
                // Already-enrolled / unknown slot: idempotent, not an error.
                Err(e) => out
                    .skipped
                    .push(format!("line {}: not applied ({e})", idx + 1)),
            }
        }

        if out.applied > 0 {
            self.save(setup, kek)?;
        }

        out.retained = keep.len();
        if keep.is_empty() {
            let _ = std::fs::remove_file(&path);
        } else {
            let mut rest = keep.join("\n");
            rest.push('\n');
            crate::fs_private::write_private(&path, &rest)
                .map_err(|e| format!("rewrite pending: {e}"))?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secrets() -> (Kek, AccountKey) {
        (
            Kek::from_bytes([0x33; 32]),
            AccountKey::from_bytes([0x44; 32]),
        )
    }

    fn store() -> (RecoveryStore, tempfile::TempDir) {
        let td = tempfile::tempdir().unwrap();
        (RecoveryStore::new(td.path()), td)
    }

    fn pending(shard_id: &str) -> PendingEnrollment {
        PendingEnrollment {
            shard_id: shard_id.to_string(),
            guardian_peer_id: "peer-1".into(),
            guardian_label: "Alice".into(),
            enrolled_at: "2026-07-16T00:00:00Z".into(),
        }
    }

    #[test]
    fn load_is_none_before_setup() {
        let (s, _td) = store();
        let (kek, _ak) = secrets();
        assert!(s.load(&kek).unwrap().is_none());
        assert!(s.read_bundle().unwrap().is_none());
        assert!(s.read_recovery_card().unwrap().is_none());
    }

    #[test]
    fn save_load_roundtrip_under_kek() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let setup = s.load_or_create(&kek, &ak).unwrap();
        let back = s.load(&kek).unwrap().expect("roster persisted");
        assert_eq!(back.epoch, setup.epoch);
        assert_eq!(back.slots.len(), setup.slots.len());
        // The bare bundle is readable pre-login (no KEK needed to read the file).
        assert!(s.read_bundle().unwrap().is_some());
    }

    #[test]
    fn load_or_create_is_idempotent() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let a = s.load_or_create(&kek, &ak).unwrap();
        let b = s.load_or_create(&kek, &ak).unwrap();
        // The second call must return the SAME setup, not mint a new Recovery
        // Key — that would orphan every already-distributed shard.
        assert_eq!(a.epoch, b.epoch);
        assert_eq!(
            serde_json::to_vec(&a.bundle).unwrap(),
            serde_json::to_vec(&b.bundle).unwrap()
        );
    }

    #[test]
    fn wrong_kek_fails_closed() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        s.load_or_create(&kek, &ak).unwrap();
        // Must be an error, never Ok(None): a silent "no recovery set up" for
        // an account that has one is exactly the failure this guards.
        assert!(s.load(&Kek::from_bytes([0x99; 32])).is_err());
    }

    #[test]
    fn reconcile_applies_pending_and_consumes_queue() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let mut setup = s.load_or_create(&kek, &ak).unwrap();
        let shard = setup.slots[0].shard_id.clone();

        s.append_pending(&pending(&shard)).unwrap();

        let r = s.reconcile_pending(&mut setup, &kek).unwrap();
        assert_eq!(r.applied, 1);
        assert_eq!(r.retained, 0);
        assert!(!r.needs_attention());
        assert_eq!(setup.enrolled_count(), 1);
        // Persisted, not merely in-memory.
        assert_eq!(s.load(&kek).unwrap().unwrap().enrolled_count(), 1);
        // A fully-understood queue is consumed.
        assert!(!s.pending_path().exists());
    }

    #[test]
    fn reconcile_is_idempotent_across_double_apply() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let mut setup = s.load_or_create(&kek, &ak).unwrap();
        let shard = setup.slots[0].shard_id.clone();
        // The same confirmation queued twice must not double-count.
        s.append_pending(&pending(&shard)).unwrap();
        s.append_pending(&pending(&shard)).unwrap();
        let r = s.reconcile_pending(&mut setup, &kek).unwrap();
        assert_eq!(r.applied, 1);
        assert_eq!(r.skipped.len(), 1, "the duplicate is reported, not hidden");
        assert_eq!(r.retained, 0, "a duplicate is understood — consume it");
        assert_eq!(setup.enrolled_count(), 1);
    }

    #[test]
    fn malformed_line_is_surfaced_and_retained_not_destroyed() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let mut setup = s.load_or_create(&kek, &ak).unwrap();
        let shard = setup.slots[0].shard_id.clone();

        // A torn append: one good record, one corrupt line.
        s.append_pending(&pending(&shard)).unwrap();
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .append(true)
                .open(s.pending_path())
                .unwrap();
            writeln!(f, "{{\"shard_id\":\"torn").unwrap();
        }

        let r = s.reconcile_pending(&mut setup, &kek).unwrap();
        assert_eq!(r.applied, 1, "the good record still applies");
        assert_eq!(r.retained, 1, "the corrupt line is kept, not deleted");
        assert!(r.needs_attention());
        assert_eq!(r.skipped.len(), 1);
        // The reason must not echo the raw bytes back at the caller.
        assert!(
            !r.skipped[0].contains("torn"),
            "raw payload leaked into the reason string"
        );

        // The queue survives, so the problem resurfaces instead of vanishing.
        let left = std::fs::read_to_string(s.pending_path()).unwrap();
        assert!(left.contains("torn"), "corrupt line must remain queued");
        // ...and re-reconciling does not re-apply the good record.
        let r2 = s.reconcile_pending(&mut setup, &kek).unwrap();
        assert_eq!(r2.applied, 0);
        assert_eq!(r2.retained, 1);
    }

    #[test]
    fn verify_content_key_stores_rejects_a_kek_that_cannot_open_them() {
        // The test the first live run did NOT have: seal a store under one KEK,
        // then verify under a DIFFERENT one. A recovery that reconstructs the
        // KEK but faces stores sealed under a now-unreachable key must FAIL
        // here, before auth.store is touched — not report success and brick the
        // account. (Stand-in for "restore the KEK but not the DeviceKey".)
        let td = tempfile::tempdir().unwrap();
        let dir = td.path();
        let good = Kek::from_bytes([0x11; 32]);
        KeyDatabase::new(dir.join("keys.db")).save(&good).unwrap();
        KeyDatabase::new(dir.join("keys.messages.db")).save(&good).unwrap();

        // The real recovered KEK opens them → commit is safe.
        assert!(verify_content_key_stores(dir, &good).is_ok());

        // A wrong KEK must fail closed, naming the store, never echoing a key.
        let wrong = Kek::from_bytes([0x22; 32]);
        let err = verify_content_key_stores(dir, &wrong).expect_err("wrong KEK must abort");
        assert!(err.contains("keys.db"), "{err}");
        assert!(!err.contains("11") && !err.contains("22"), "key bytes leaked: {err}");
    }

    #[test]
    fn verify_content_key_stores_passes_when_no_stores_exist_yet() {
        // A fresh account may not have written any content store. Nothing to
        // verify is not a failure — it must not block recovery.
        let td = tempfile::tempdir().unwrap();
        assert!(verify_content_key_stores(td.path(), &Kek::from_bytes([0x11; 32])).is_ok());
    }

    #[test]
    fn install_recovered_leaves_auth_store_intact_when_verify_fails() {
        // THE property the first live run's 168 green tests could not catch: a
        // recovery that cannot decrypt the content must abort WITHOUT touching
        // auth.store, so it never bricks an otherwise-recoverable account.
        let td = tempfile::tempdir().unwrap();
        let dir = td.path();
        // Content sealed under the real KEK...
        KeyDatabase::new(dir.join("keys.db")).save(&Kek::from_bytes([0x11; 32])).unwrap();
        // ...an existing auth.store we will prove is NOT overwritten...
        std::fs::write(dir.join("auth.store"), b"OLD-STORE-SENTINEL").unwrap();
        // ...and a recovery presenting the WRONG KEK.
        let wrong = Kek::from_bytes([0x22; 32]);
        let ak = AccountKey::from_bytes([0x44; 32]);

        // `.err()` (not `expect_err`) — AuthStore isn't Debug.
        let err = install_recovered_auth_store(dir, &wrong, &ak, b"NewPass!1234")
            .err()
            .expect("wrong KEK must abort finalize");
        assert!(err.contains("keys.db"), "{err}");
        // The old store survives byte-for-byte — the whole point.
        assert_eq!(std::fs::read(dir.join("auth.store")).unwrap(), b"OLD-STORE-SENTINEL");
    }

    #[test]
    fn install_recovered_writes_a_store_the_new_passphrase_opens_to_the_same_secrets() {
        let td = tempfile::tempdir().unwrap();
        let dir = td.path();
        let kek = Kek::from_bytes([0x11; 32]);
        let ak = AccountKey::from_bytes([0x44; 32]);
        // Content the recovered KEK can open ⇒ verify passes ⇒ commit.
        KeyDatabase::new(dir.join("keys.db")).save(&kek).unwrap();

        let store = install_recovered_auth_store(dir, &kek, &ak, b"NewPass!1234").unwrap();
        // The new passphrase authenticates and yields the SAME recovered secrets
        // — so synced content stays decryptable after recovery.
        let ok = store.authenticate(b"NewPass!1234").expect("new passphrase opens the store");
        assert_eq!(ok.kek.as_bytes(), kek.as_bytes(), "recovered KEK must survive re-wrap");
        assert_eq!(ok.account_key.as_bytes(), ak.as_bytes(), "recovered AccountKey must survive");
        assert!(dir.join("auth.store").exists() && dir.join("salt").exists());
        // The forgotten passphrase is gone; a wrong one must not open it.
        assert!(store.authenticate(b"the-forgotten-one").is_err());
    }

    #[test]
    fn recovery_card_roundtrips_and_lists_enrolled_guardians() {
        let (s, _td) = store();
        let (kek, ak) = secrets();
        let mut setup = s.load_or_create(&kek, &ak).unwrap();
        let shard = setup.slots[0].shard_id.clone();
        setup
            .mark_enrolled(&shard, "peer-1", "Alice", "2026-07-16T00:00:00Z")
            .unwrap();

        s.write_recovery_card(&setup, "tag-g", &["relay-a".into()])
            .unwrap();
        let card = s.read_recovery_card().unwrap().expect("card written");
        assert_eq!(card.owner_tag, "tag-g");
        assert_eq!(card.threshold, GUARDIAN_THRESHOLD);
        assert_eq!(card.guardian_peer_ids, vec!["peer-1".to_string()]);
        assert_eq!(card.relays, vec!["relay-a".to_string()]);
    }
}
