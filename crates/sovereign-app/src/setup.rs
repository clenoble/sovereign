use anyhow::Result;
use sovereign_core::config::AppConfig;
use sovereign_db::GraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};

#[cfg(feature = "encryption")]
use std::sync::Arc;

pub async fn create_db(config: &AppConfig) -> Result<SurrealGraphDB> {
    let mode = match config.database.mode.as_str() {
        "memory" => StorageMode::Memory,
        _ => {
            // Anchor relative paths to sovereign_dir(), which respects
            // SOVEREIGN_DATA_DIR when set (mobile entry point sets this to
            // the app sandbox) and falls back to ~/.sovereign on desktop.
            // Using sovereign_dir() instead of home_dir().join(".sovereign")
            // is what makes Android persistence work: home_dir() returns "."
            // when $HOME is unset (Android), which then resolves against a
            // read-only filesystem root.
            let raw = std::path::Path::new(&config.database.path);
            let resolved = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                sovereign_core::sovereign_dir().join(raw)
            };
            if let Some(parent) = resolved.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let resolved_str = resolved.to_string_lossy().into_owned();
            tracing::info!("Database path: {resolved_str}");
            StorageMode::Persistent(resolved_str)
        }
    };
    let db = SurrealGraphDB::new(mode).await?;
    db.connect().await?;
    db.init_schema().await?;
    Ok(db)
}

#[cfg(feature = "encryption")]
pub fn crypto_dir() -> std::path::PathBuf {
    sovereign_core::sovereign_dir().join("crypto")
}

/// Load or create a stable device ID for this machine.
#[cfg(feature = "encryption")]
pub fn load_or_create_device_id() -> Result<String> {
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("device_id");
    if path.exists() {
        Ok(std::fs::read_to_string(&path)?.trim().to_string())
    } else {
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::write(&path, &id)?;
        tracing::info!("Generated new device ID: {id}");
        Ok(id)
    }
}

/// Initialize the crypto subsystem: MasterKey → DeviceKey → KEK → KeyDatabase.
/// Returns (DeviceKey, Kek, KeyDatabase) for use by EncryptedGraphDB and P2P.
#[cfg(feature = "encryption")]
pub fn init_crypto() -> Result<(
    sovereign_crypto::device_key::DeviceKey,
    std::sync::Arc<tokio::sync::Mutex<sovereign_crypto::key_db::KeyDatabase>>,
    std::sync::Arc<sovereign_crypto::kek::Kek>,
)> {
    use sovereign_crypto::{
        device_key::DeviceKey,
        kek::Kek,
        key_db::KeyDatabase,
        master_key::MasterKey,
    };

    let device_id = load_or_create_device_id()?;
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;

    // Derive master key from passphrase (WSL2 — no TPM)
    let salt_path = dir.join("salt");
    let salt = if salt_path.exists() {
        std::fs::read(&salt_path)?
    } else {
        let mut s = vec![0u8; 32];
        use rand::Rng;
        rand::rng().fill_bytes(&mut s);
        std::fs::write(&salt_path, &s)?;
        s
    };

    let pass = rpassword::prompt_password("Sovereign passphrase: ")?;
    if pass.is_empty() {
        anyhow::bail!("Passphrase cannot be empty");
    }
    let master = MasterKey::from_passphrase(pass.as_bytes(), &salt)?;
    let device_key = DeviceKey::derive(&master, &device_id)?;

    // Load or create KEK
    let kek_path = dir.join("kek.wrapped");
    let kek = if kek_path.exists() {
        let wrapped_bytes = std::fs::read(&kek_path)?;
        let wrapped: sovereign_crypto::kek::WrappedKek = serde_json::from_slice(&wrapped_bytes)?;
        Kek::unwrap(&wrapped, &device_key)?
    } else {
        let kek = Kek::generate();
        let wrapped = kek.wrap(&device_key)?;
        std::fs::write(&kek_path, serde_json::to_vec(&wrapped)?)?;
        kek
    };

    // Load or create KeyDatabase
    let key_db_path = dir.join("keys.db");
    let key_db = if key_db_path.exists() {
        KeyDatabase::load(&key_db_path, &device_key)?
    } else {
        KeyDatabase::new(key_db_path)
    };

    tracing::info!("Crypto subsystem initialized (device: {device_id})");
    Ok((
        device_key,
        std::sync::Arc::new(tokio::sync::Mutex::new(key_db)),
        std::sync::Arc::new(kek),
    ))
}

// ── Two-phase auth ──────────────────────────────────────────────────

/// Result of preparing authentication (before GUI).
#[cfg(feature = "encryption")]
pub enum AuthPrepareResult {
    /// Auth store exists — show login screen.
    LoginRequired(sovereign_crypto::auth::AuthStore),
    /// No auth store — first launch, show onboarding + registration.
    RegistrationRequired { salt: Vec<u8>, device_id: String },
}

/// Prepare the crypto directory and load/detect the AuthStore.
/// Does NOT prompt for password — that happens in the GUI.
#[cfg(feature = "encryption")]
pub fn prepare_auth() -> Result<AuthPrepareResult> {
    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;
    let auth_path = dir.join("auth.store");
    let device_id = load_or_create_device_id()?;
    let salt = load_or_create_salt(&dir)?;

    if auth_path.exists() {
        let store = sovereign_crypto::auth::AuthStore::load(&auth_path)?;
        Ok(AuthPrepareResult::LoginRequired(store))
    } else {
        Ok(AuthPrepareResult::RegistrationRequired { salt, device_id })
    }
}

/// Load or create the persistent salt.
#[cfg(feature = "encryption")]
fn load_or_create_salt(dir: &std::path::Path) -> Result<Vec<u8>> {
    let salt_path = dir.join("salt");
    if salt_path.exists() {
        Ok(std::fs::read(&salt_path)?)
    } else {
        let mut s = vec![0u8; 32];
        use rand::Rng;
        rand::rng().fill_bytes(&mut s);
        std::fs::write(&salt_path, &s)?;
        Ok(s)
    }
}

/// Complete authentication after GUI login — loads persona-specific KeyDatabase.
#[cfg(feature = "encryption")]
pub fn complete_auth(
    persona: sovereign_crypto::auth::PersonaKind,
    device_key: &sovereign_crypto::device_key::DeviceKey,
    kek: &sovereign_crypto::kek::Kek,
) -> Result<(
    std::sync::Arc<tokio::sync::Mutex<sovereign_crypto::key_db::KeyDatabase>>,
    std::sync::Arc<sovereign_crypto::kek::Kek>,
)> {
    use sovereign_crypto::{kek::Kek, key_db::KeyDatabase};

    let dir = crypto_dir();
    let suffix = match persona {
        sovereign_crypto::auth::PersonaKind::Primary => "",
        sovereign_crypto::auth::PersonaKind::Duress => ".duress",
    };
    let key_db_path = dir.join(format!("keys{suffix}.db"));
    let key_db = if key_db_path.exists() {
        KeyDatabase::load(&key_db_path, device_key)?
    } else {
        KeyDatabase::new(key_db_path)
    };

    // Reconstruct KEK from bytes (we can't clone Arc, but we have the bytes)
    let kek_copy = Kek::from_bytes(*kek.as_bytes());

    Ok((
        std::sync::Arc::new(tokio::sync::Mutex::new(key_db)),
        std::sync::Arc::new(kek_copy),
    ))
}

/// Construct an `EncryptedGraphDB` wrapping `raw_db`, loading or creating the
/// six per-entity `KeyDatabase` files and the per-DB `IndexKey` from
/// `crypto_dir()`. Generic across desktop (RocksDB) and mobile (SurrealKV)
/// inner backends — the only platform-dependent thing is which feature flag
/// compiled which `SurrealGraphDB` storage path.
///
/// File layout (all under `crypto_dir()`):
///   - `keys.db`              — documents (existing, predates 2b)
///   - `keys.messages.db`     — messages (Phase 2a)
///   - `keys.threads.db`      — threads (Phase 2b)
///   - `keys.conversations.db`— conversations (Phase 2b)
///   - `keys.contacts.db`     — contacts (Phase 2b)
///   - `keys.share_records.db`— share records (Phase 2b)
///   - `index.key`            — single 32-byte HMAC-SHA256 blind-index key
#[cfg(feature = "encryption")]
pub fn build_encrypted_db(
    raw_db: Arc<dyn sovereign_db::GraphDB>,
    device_key: &sovereign_crypto::device_key::DeviceKey,
    kek: Arc<sovereign_crypto::kek::Kek>,
) -> Result<Arc<sovereign_db::encrypted::EncryptedGraphDB>> {
    use sovereign_crypto::index_key::IndexKey;
    use sovereign_crypto::key_db::KeyDatabase;
    use tokio::sync::RwLock;

    let dir = crypto_dir();
    std::fs::create_dir_all(&dir)?;

    // Load-or-create each per-entity-type KeyDatabase. `KeyDatabase::load`
    // returns an existing file decrypted under DeviceKey; absent files start
    // empty and are persisted on first key creation by EncryptedGraphDB.
    let load_or_new = |filename: &str| -> Result<KeyDatabase> {
        let path = dir.join(filename);
        Ok(if path.exists() {
            KeyDatabase::load(&path, device_key)?
        } else {
            KeyDatabase::new(path)
        })
    };

    let documents_kdb = load_or_new("keys.db")?;
    let messages_kdb = load_or_new("keys.messages.db")?;
    let threads_kdb = load_or_new("keys.threads.db")?;
    let conversations_kdb = load_or_new("keys.conversations.db")?;
    let contacts_kdb = load_or_new("keys.contacts.db")?;
    let share_records_kdb = load_or_new("keys.share_records.db")?;

    // Single per-DB blind-index key, used uniformly across the six entity
    // types. Same plaintext token hashes to the same value across all entity
    // tables under this DB, but cross-table search isn't a feature so this
    // leakage is bounded.
    let index_key_path = dir.join("index.key");
    let index_key = IndexKey::load_or_create(index_key_path, device_key, kek.as_ref())?;

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
    )))
}

/// Get the persona-specific DB path.
#[allow(dead_code)]
pub fn persona_db_path(
    config: &AppConfig,
    persona: sovereign_core::auth::PersonaKind,
) -> String {
    match persona {
        sovereign_core::auth::PersonaKind::Primary => config.database.path.clone(),
        sovereign_core::auth::PersonaKind::Duress => {
            let base = &config.database.path;
            if base.ends_with(".db") {
                format!("{}-duress.db", base.trim_end_matches(".db"))
            } else {
                format!("{}-duress", base)
            }
        }
    }
}

/// Derive a 32-byte session log encryption key from the account key via HKDF-SHA256.
///
/// The session log derivation moved from DeviceKey to AccountKey in v0.0.5
/// so paired devices can read each other's session log entries (currently
/// the session log is per-device, but this is forward-compat for v0.0.6
/// which may sync it). The HKDF info string is unchanged.
#[cfg(all(feature = "encryption", feature = "encrypted-log"))]
pub fn derive_session_log_key(
    account_key: &sovereign_crypto::account_key::AccountKey,
) -> [u8; 32] {
    use sovereign_crypto::aead::KEY_SIZE;

    let hk = hkdf::Hkdf::<sha2::Sha256>::new(None, account_key.as_bytes());
    let mut key = [0u8; KEY_SIZE];
    hk.expand(b"sovereign-session-log", &mut key)
        .expect("HKDF expand for session log key");
    key
}

/// Wrap an orchestrator method call into a spawn-and-log callback.
pub fn orch_callback(
    orch: &std::sync::Arc<sovereign_ai::orchestrator::Orchestrator>,
    label: &'static str,
    method: for<'a> fn(&'a sovereign_ai::orchestrator::Orchestrator, &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send + 'a>>,
) -> Box<dyn Fn(String) + Send + 'static> {
    let orch = orch.clone();
    Box::new(move |text: String| {
        let orch = orch.clone();
        tokio::spawn(async move {
            if let Err(e) = method(&orch, &text).await {
                tracing::error!("{label}: {e}");
            }
        });
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_db_memory_mode() {
        let mut config = AppConfig::default();
        config.database.mode = "memory".into();
        let db = create_db(&config).await.unwrap();
        // Verify schema is initialized by listing (should return empty vec, not error)
        let threads = db.list_threads().await.unwrap();
        assert!(threads.is_empty());
    }

    #[tokio::test]
    async fn create_db_persistent_mode_uses_path() {
        let mut config = AppConfig::default();
        config.database.mode = "persistent".into();
        config.database.path = "test_sovereign_setup.db".into();
        let db = create_db(&config).await.unwrap();
        let docs = db.list_documents(None).await.unwrap();
        assert!(docs.is_empty());
        // Clean up
        let _ = std::fs::remove_dir_all("test_sovereign_setup.db");
    }
}
