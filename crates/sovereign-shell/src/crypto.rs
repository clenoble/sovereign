//! Auth + at-rest encryption (Phase 3).
//! Mirrors sovereign-app's setup.rs + install_session: derive the persona's key
//! stack from the passphrase, build the EncryptedGraphDB decorator (fail-CLOSED),
//! and isolate the duress persona onto a separate DB + key files (CRYPTO-001).

use std::path::PathBuf;
use std::sync::Arc;

use sovereign_db::traits::GraphDB;
use sovereign_core::auth::PersonaKind as CorePersona;
use sovereign_crypto::auth::{AuthStore, PersonaKind as CryptoPersona};

use crate::canvas::open_db_at;

pub(crate) fn crypto_dir() -> PathBuf {
    sovereign_core::sovereign_dir().join("crypto")
}

/// This shell's face onto the shared F1 recovery store.
///
/// The implementation lives in `sovereign-crypto` precisely so this crate and
/// `sovereign-app` run the *same* code: the shell cannot depend on the app, and
/// duplicating key-handling logic is the drift family that cost two failed
/// recoveries during the F1 live run. The only difference between the two faces
/// is which directory they pass — and both resolve to `sovereign_dir()/crypto`,
/// so both read the same roster.
pub(crate) fn recovery_store() -> sovereign_crypto::recovery_store::RecoveryStore {
    sovereign_crypto::recovery_store::RecoveryStore::new(crypto_dir())
}
pub(crate) fn auth_store_path() -> PathBuf {
    crypto_dir().join("auth.store")
}
pub(crate) fn auth_store_exists() -> bool {
    auth_store_path().exists()
}

pub(crate) fn map_persona(p: CryptoPersona) -> CorePersona {
    match p {
        CryptoPersona::Primary => CorePersona::Primary,
        CryptoPersona::Duress => CorePersona::Duress,
    }
}

/// Persona-isolated RAW DB path: duress gets a physically separate database so a
/// coerced login can never reach the primary persona's rows.
pub(crate) fn persona_raw_db_path(persona: CorePersona) -> PathBuf {
    let data = sovereign_core::sovereign_dir().join("data");
    match persona {
        CorePersona::Primary => data.join("sovereign.db"),
        CorePersona::Duress => data.join("sovereign-duress.db"),
    }
}
pub(crate) fn persona_key_db_filename(persona: CorePersona, base: &str) -> String {
    match persona {
        CorePersona::Primary => base.to_string(),
        CorePersona::Duress => format!("{}.duress.db", base.trim_end_matches(".db")),
    }
}
pub(crate) fn persona_index_filename(persona: CorePersona) -> &'static str {
    match persona {
        CorePersona::Primary => "index.key",
        CorePersona::Duress => "index.duress.key",
    }
}

/// Per-persona model-TOFU store path (MODELTRUST-003-PERSONA).
///
/// The store is encrypted under the persona's AccountKey, and primary/duress
/// have different AccountKeys — so a single shared `model_tofu.json` (what the
/// shell used) can't be read across personas and the two clobber each other on
/// write, thrashing the anchor on every persona switch. Give each persona its
/// own file, like the key DBs and index key. (The `*.duress.*` name is the same
/// existing at-rest duress-existence surface as those siblings — ATREST-011,
/// tracked separately — not a new leak.)
pub(crate) fn persona_model_tofu_path(persona: CorePersona) -> PathBuf {
    let crypto = crypto_dir();
    match persona {
        CorePersona::Primary => crypto.join("model_tofu.json"),
        CorePersona::Duress => crypto.join("model_tofu.duress.json"),
    }
}

pub(crate) fn load_or_create_salt() -> anyhow::Result<Vec<u8>> {
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    let p = dir.join("salt");
    if p.exists() {
        Ok(std::fs::read(&p)?)
    } else {
        let s = sovereign_crypto::random_hex_32().into_bytes();
        sovereign_crypto::fs_private::write_private(&p, &s)?;
        Ok(s)
    }
}
pub(crate) fn load_or_create_device_id() -> anyhow::Result<String> {
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    let p = dir.join("device_id");
    if p.exists() {
        Ok(std::fs::read_to_string(&p)?.trim().to_string())
    } else {
        let id = sovereign_crypto::random_hex_32();
        sovereign_crypto::fs_private::write_private(&p, id.as_bytes())?;
        Ok(id)
    }
}

/// Onboarding: create the two-persona AuthStore and persist it (+ salt/device_id).
pub(crate) fn create_auth_store(primary: &[u8], duress: &[u8]) -> anyhow::Result<AuthStore> {
    let salt = load_or_create_salt()?;
    let device_id = load_or_create_device_id()?;
    let store = AuthStore::create(primary, duress, &salt, &device_id)?;
    store.save(&auth_store_path())?;
    Ok(store)
}

/// Build the EncryptedGraphDB decorator around `raw_db` using the persona's
/// per-entity key DBs + blind-index key (created on first use). Fail-CLOSED:
/// any error here must abort the login rather than fall back to plaintext.
pub(crate) fn build_encrypted_db(
    raw_db: Arc<dyn GraphDB>,
    device_key: Arc<sovereign_crypto::device_key::DeviceKey>,
    kek: Arc<sovereign_crypto::kek::Kek>,
    persona: CorePersona,
) -> anyhow::Result<Arc<sovereign_db::encrypted::EncryptedGraphDB>> {
    use sovereign_crypto::index_key::IndexKey;
    use sovereign_crypto::key_db::KeyDatabase;
    use tokio::sync::RwLock;

    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    // Sealed under the KEK (content-chain root; spec §Encryption Scheme), with
    // transparent migration of any legacy DeviceKey-sealed file to the KEK.
    let load_or_new = |filename: &str| -> anyhow::Result<KeyDatabase> {
        let path = dir.join(filename);
        if !path.exists() {
            return Ok(KeyDatabase::new(path));
        }
        Ok(match KeyDatabase::load(&path, &kek) {
            Ok(db) => db,
            Err(_) => {
                let db = KeyDatabase::load_legacy(&path, &device_key)?;
                db.save(&kek)?;
                db
            }
        })
    };
    let documents_kdb = load_or_new(&persona_key_db_filename(persona, "keys.db"))?;
    let messages_kdb = load_or_new(&persona_key_db_filename(persona, "keys.messages.db"))?;
    let threads_kdb = load_or_new(&persona_key_db_filename(persona, "keys.threads.db"))?;
    let conversations_kdb = load_or_new(&persona_key_db_filename(persona, "keys.conversations.db"))?;
    let contacts_kdb = load_or_new(&persona_key_db_filename(persona, "keys.contacts.db"))?;
    let share_records_kdb = load_or_new(&persona_key_db_filename(persona, "keys.share_records.db"))?;
    let index_key =
        IndexKey::load_or_create(dir.join(persona_index_filename(persona)), &kek, &device_key)?;

    Ok(Arc::new(sovereign_db::encrypted::EncryptedGraphDB::new(
        raw_db,
        Arc::new(RwLock::new(documents_kdb)),
        Arc::new(RwLock::new(messages_kdb)),
        Arc::new(RwLock::new(threads_kdb)),
        Arc::new(RwLock::new(conversations_kdb)),
        Arc::new(RwLock::new(contacts_kdb)),
        Arc::new(RwLock::new(share_records_kdb)),
        kek,
        Arc::new(index_key),
        device_key,
    )))
}

/// F1 Surface 2 finalize: reconstruct the recovered secrets, re-create
/// `auth.store` under a NEW passphrase, and install the session.
///
/// The security-critical, salt-subtle core lives in
/// [`sovereign_crypto::recovery_store::install_recovered_auth_store`] — shared
/// with the Tauri owner so there is one verify-before-commit, not a copy per
/// face. Here we only: reconstruct the KEK + AccountKey from the collected
/// guardian shares, hand them to that shared core (which verifies the recovered
/// KEK opens the on-disk content BEFORE overwriting anything, then writes the
/// new store), and install the session over it — the exact `install_session`
/// path a normal login takes, so recovery and login converge. On success the
/// in-progress recovery is cleared.
///
/// Fail-CLOSED: too few shares, an unopenable bundle, or content the recovered
/// KEK cannot decrypt each return `Err` with the prior `auth.store` intact.
pub(crate) async fn recover_and_install(
    new_passphrase: &[u8],
) -> anyhow::Result<(
    CorePersona,
    Arc<dyn GraphDB>,
    Arc<sovereign_crypto::account_key::AccountKey>,
    Arc<sovereign_crypto::device_key::DeviceKey>,
    Arc<sovereign_crypto::kek::Kek>,
)> {
    use sovereign_p2p::access_recovery::AccessRecovery;

    let dir = crate::recovery::access_dir();
    let mut rec = AccessRecovery::load(&dir)
        .ok_or_else(|| anyhow::anyhow!("no access recovery in progress"))?;
    if !rec.have_enough() {
        anyhow::bail!(
            "not enough guardian shares yet ({}/{})",
            rec.shares_collected(),
            rec.threshold
        );
    }
    // The shares are sealed at rest (RECOVERY-001) — decrypt them into memory
    // under the held passphrase before reconstructing.
    let seal_key =
        sovereign_crypto::recovery_seal::derive_seal_key(new_passphrase, rec.seal_salt())
            .map_err(|e| anyhow::anyhow!("derive seal key: {e}"))?;
    rec.unseal(&seal_key)
        .map_err(|e| anyhow::anyhow!("unseal shares: {e}"))?;
    // Reconstruct the Recovery Key from the shares and open the bundle.
    let (kek, account_key) = rec
        .open()
        .map_err(|e| anyhow::anyhow!("could not open the recovery bundle: {e}"))?;

    // Shared verify-before-commit + re-create auth.store under the new pass.
    let store = sovereign_crypto::recovery_store::install_recovered_auth_store(
        &crypto_dir(),
        &kek,
        &account_key,
        new_passphrase,
    )
    .map_err(|e| anyhow::anyhow!(e))?;

    // Install the session over the new store — same path as login. Synced
    // content decrypts under the recovered KEK (content chain roots at the KEK).
    let session = install_session(&store, new_passphrase).await?;

    // Done — clear the in-progress recovery so a relaunch starts clean.
    AccessRecovery::cancel(&dir);
    Ok(session)
}

/// Authenticate + install the at-rest encryption for the matching persona.
/// Returns the persona, the live EncryptedGraphDB (over the persona's raw DB),
/// the AccountKey (vault-secret encryption), and the DeviceKey (the P2P identity
/// key — derives the libp2p keypair + the paired-store key).
/// Fail-CLOSED: a wrong password or an encryption-install error returns Err.
pub(crate) async fn install_session(
    store: &AuthStore,
    password: &[u8],
) -> anyhow::Result<(
    CorePersona,
    Arc<dyn GraphDB>,
    Arc<sovereign_crypto::account_key::AccountKey>,
    Arc<sovereign_crypto::device_key::DeviceKey>,
    Arc<sovereign_crypto::kek::Kek>,
)> {
    let auth = store
        .authenticate(password)
        .map_err(|_| anyhow::anyhow!("Invalid password"))?;
    let persona = map_persona(auth.persona);
    let device_key = Arc::new(auth.device_key);
    let kek = Arc::new(auth.kek);
    // The AccountKey encrypts vault secrets (e.g. the saved email password,
    // stored as a PiiRecord) — keep it for the session.
    let account_key = Arc::new(auth.account_key);
    let raw = open_db_at(&persona_raw_db_path(persona))
        .await
        .ok_or_else(|| anyhow::anyhow!("could not open the persona database"))?;
    // The DeviceKey + KEK are consumed by the EncryptedGraphDB; clone the Arcs
    // first so P2P can derive its identity from the device key and onboarding
    // can seal the canary store under the KEK.
    let encrypted = build_encrypted_db(raw, device_key.clone(), kek.clone(), persona)?;
    Ok((
        persona,
        encrypted as Arc<dyn GraphDB>,
        account_key,
        device_key,
        kek,
    ))
}
