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
    let load_or_new = |filename: &str| -> anyhow::Result<KeyDatabase> {
        let path = dir.join(filename);
        Ok(if path.exists() {
            KeyDatabase::load(&path, &device_key)?
        } else {
            KeyDatabase::new(path)
        })
    };
    let documents_kdb = load_or_new(&persona_key_db_filename(persona, "keys.db"))?;
    let messages_kdb = load_or_new(&persona_key_db_filename(persona, "keys.messages.db"))?;
    let threads_kdb = load_or_new(&persona_key_db_filename(persona, "keys.threads.db"))?;
    let conversations_kdb = load_or_new(&persona_key_db_filename(persona, "keys.conversations.db"))?;
    let contacts_kdb = load_or_new(&persona_key_db_filename(persona, "keys.contacts.db"))?;
    let share_records_kdb = load_or_new(&persona_key_db_filename(persona, "keys.share_records.db"))?;
    let index_key =
        IndexKey::load_or_create(dir.join(persona_index_filename(persona)), &device_key, kek.as_ref())?;

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
