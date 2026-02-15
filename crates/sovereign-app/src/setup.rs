use anyhow::Result;
use sovereign_core::config::AppConfig;
use sovereign_db::GraphDB;
use sovereign_db::surreal::{StorageMode, SurrealGraphDB};

pub async fn create_db(config: &AppConfig) -> Result<SurrealGraphDB> {
    let mode = match config.database.mode.as_str() {
        "memory" => StorageMode::Memory,
        _ => StorageMode::Persistent(config.database.path.clone()),
    };
    let db = SurrealGraphDB::new(mode).await?;
    db.connect().await?;
    db.init_schema().await?;
    Ok(db)
}

#[cfg(feature = "encryption")]
pub fn crypto_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    std::path::PathBuf::from(home).join(".sovereign").join("crypto")
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
