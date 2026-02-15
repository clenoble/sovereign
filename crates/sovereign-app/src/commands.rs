use anyhow::Result;
use sovereign_core::config::AppConfig;
use sovereign_db::schema::{thing_to_raw, Document, RelationType, Thread};
use sovereign_db::GraphDB;

use crate::setup::create_db;

pub async fn create_doc(
    config: &AppConfig,
    title: String,
    thread_id: String,
    is_owned: bool,
) -> Result<()> {
    let db = create_db(config).await?;
    let doc = Document::new(title, thread_id, is_owned);
    let created = db.create_document(doc).await?;
    let id = created.id_string().unwrap_or_default();
    println!("{id}");
    Ok(())
}

pub async fn get_doc(config: &AppConfig, id: String) -> Result<()> {
    let db = create_db(config).await?;
    let doc = db.get_document(&id).await?;
    println!("{}", serde_json::to_string_pretty(&doc)?);
    Ok(())
}

pub async fn list_docs(config: &AppConfig, thread_id: Option<String>) -> Result<()> {
    let db = create_db(config).await?;
    let docs = db.list_documents(thread_id.as_deref()).await?;
    for doc in &docs {
        let id = doc.id_string().unwrap_or_default();
        println!("{id}\t{}", doc.title);
    }
    println!("({} documents)", docs.len());
    Ok(())
}

pub async fn update_doc(
    config: &AppConfig,
    id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<()> {
    let db = create_db(config).await?;
    let updated = db
        .update_document(&id, title.as_deref(), content.as_deref())
        .await?;
    println!("{}", serde_json::to_string_pretty(&updated)?);
    Ok(())
}

pub async fn delete_doc(config: &AppConfig, id: String) -> Result<()> {
    let db = create_db(config).await?;
    db.delete_document(&id).await?;
    println!("Deleted {id}");
    Ok(())
}

pub async fn create_thread(
    config: &AppConfig,
    name: String,
    description: String,
) -> Result<()> {
    let db = create_db(config).await?;
    let thread = Thread::new(name, description);
    let created = db.create_thread(thread).await?;
    let id = created.id_string().unwrap_or_default();
    println!("{id}");
    Ok(())
}

pub async fn list_threads(config: &AppConfig) -> Result<()> {
    let db = create_db(config).await?;
    let threads = db.list_threads().await?;
    for t in &threads {
        let id = t.id_string().unwrap_or_default();
        println!("{id}\t{}", t.name);
    }
    println!("({} threads)", threads.len());
    Ok(())
}

pub async fn add_relationship(
    config: &AppConfig,
    from: String,
    to: String,
    relation_type: String,
    strength: f32,
) -> Result<()> {
    if !(0.0..=1.0).contains(&strength) {
        anyhow::bail!("Relationship strength must be between 0.0 and 1.0, got {strength}");
    }
    let db = create_db(config).await?;
    let rel_type: RelationType = relation_type
        .parse()
        .map_err(|e: String| anyhow::anyhow!(e))?;
    let rel = db.create_relationship(&from, &to, rel_type, strength).await?;
    let id = rel.id.map(|t| thing_to_raw(&t)).unwrap_or_default();
    println!("{id}");
    Ok(())
}

pub async fn list_relationships(config: &AppConfig, doc_id: String) -> Result<()> {
    let db = create_db(config).await?;
    let rels = db.list_relationships(&doc_id).await?;
    for r in &rels {
        let id = r.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
        println!("{id}\t{}\tstrength={:.2}", r.relation_type, r.strength);
    }
    println!("({} relationships)", rels.len());
    Ok(())
}

pub async fn commit_doc(config: &AppConfig, doc_id: String, message: String) -> Result<()> {
    let db = create_db(config).await?;
    let commit = db.commit_document(&doc_id, &message).await?;
    let id = commit.id.map(|t| thing_to_raw(&t)).unwrap_or_default();
    println!("{id} ({})", commit.snapshot.title);
    Ok(())
}

pub async fn list_commits(config: &AppConfig, doc_id: String) -> Result<()> {
    let db = create_db(config).await?;
    let commits = db.list_document_commits(&doc_id).await?;
    for c in &commits {
        let id = c.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
        println!("{id}\t{}\t{}", c.timestamp.format("%Y-%m-%d %H:%M:%S"), c.message);
    }
    println!("({} commits)", commits.len());
    Ok(())
}

#[cfg(feature = "encryption")]
pub async fn encrypt_data(
    config: &AppConfig,
    key_db: std::sync::Arc<tokio::sync::Mutex<sovereign_crypto::key_db::KeyDatabase>>,
    kek: std::sync::Arc<sovereign_crypto::kek::Kek>,
) -> Result<()> {
    use sovereign_crypto::{device_key::DeviceKey, master_key::MasterKey};

    let db = create_db(config).await?;

    // Gather unencrypted documents
    let docs = db.list_documents(None).await?;
    let plans: Vec<sovereign_crypto::migration::DocumentEncryptionPlan> = docs
        .iter()
        .filter(|d| d.encryption_nonce.is_none())
        .map(|d| sovereign_crypto::migration::DocumentEncryptionPlan {
            doc_id: d.id_string().unwrap_or_default(),
            plaintext_content: d.content.clone(),
        })
        .collect();

    if plans.is_empty() {
        println!("All documents are already encrypted.");
        return Ok(());
    }

    println!("Encrypting {} documents...", plans.len());
    let total = plans.len();
    let progress: sovereign_crypto::migration::ProgressCallback =
        Box::new(move |done, total| {
            println!("  [{done}/{total}]");
        });
    let mut key_db_guard = key_db.lock().await;
    let results =
        sovereign_crypto::migration::encrypt_documents(&plans, &mut key_db_guard, &kek, Some(&progress))?;

    // Update each document with encrypted content and nonce
    for result in &results {
        db.update_document(
            &result.doc_id,
            None,
            Some(&result.encrypted_content),
        )
        .await?;
        tracing::info!(
            "Encrypted {}: nonce={}",
            result.doc_id,
            result.nonce_b64
        );
    }

    // Persist key database
    let crypto_dir = crate::setup::crypto_dir();
    let device_id = crate::setup::load_or_create_device_id()?;
    let salt_path = crypto_dir.join("salt");
    let salt = std::fs::read(&salt_path)?;
    let pass = rpassword::prompt_password("Re-enter passphrase to save key DB: ")?;
    let master = MasterKey::from_passphrase(pass.as_bytes(), &salt)?;
    let device_key = DeviceKey::derive(&master, &device_id)?;
    key_db_guard.save(&device_key)?;

    println!("Encrypted {total} documents. Key database saved.");
    Ok(())
}

pub async fn list_contacts(config: &AppConfig) -> Result<()> {
    let db = create_db(config).await?;
    let contacts = db.list_contacts().await?;
    for c in &contacts {
        let id = c.id_string().unwrap_or_default();
        let addrs: Vec<&str> = c.addresses.iter().map(|a| a.address.as_str()).collect();
        println!("{id}\t{}\t[{}]", c.name, addrs.join(", "));
    }
    println!("({} contacts)", contacts.len());
    Ok(())
}

pub async fn list_conversations(config: &AppConfig, channel: Option<String>) -> Result<()> {
    let db = create_db(config).await?;
    let channel_filter = channel.as_ref().and_then(|ch| {
        match ch.to_lowercase().as_str() {
            "email" => Some(sovereign_db::schema::ChannelType::Email),
            "sms" => Some(sovereign_db::schema::ChannelType::Sms),
            "signal" => Some(sovereign_db::schema::ChannelType::Signal),
            "whatsapp" => Some(sovereign_db::schema::ChannelType::WhatsApp),
            "matrix" => Some(sovereign_db::schema::ChannelType::Matrix),
            "phone" => Some(sovereign_db::schema::ChannelType::Phone),
            _ => None,
        }
    });
    let convs = db.list_conversations(channel_filter.as_ref()).await?;
    for c in &convs {
        let id = c.id_string().unwrap_or_default();
        let last = c.last_message_at
            .map(|t| t.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| "-".into());
        println!("{id}\t{}\t{}\tunread={}", c.title, last, c.unread_count);
    }
    println!("({} conversations)", convs.len());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::config::AppConfig;

    fn test_config() -> AppConfig {
        let mut config = AppConfig::default();
        config.database.mode = "memory".into();
        config
    }

    #[tokio::test]
    async fn create_doc_succeeds() {
        let config = test_config();
        let result = create_doc(
            &config,
            "Test Title".into(),
            "thread:t".into(),
            true,
        ).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_docs_empty_db() {
        let config = test_config();
        let result = list_docs(&config, None).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn create_and_list_threads() {
        let config = test_config();
        assert!(create_thread(&config, "MyThread".into(), "desc".into()).await.is_ok());
        assert!(list_threads(&config).await.is_ok());
    }

    #[tokio::test]
    async fn add_relationship_validates_strength_range() {
        let config = test_config();
        // Out of range — should fail immediately
        let result = add_relationship(
            &config,
            "document:a".into(),
            "document:b".into(),
            "References".into(),
            1.5,
        ).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("between 0.0 and 1.0"));

        let result = add_relationship(
            &config,
            "document:a".into(),
            "document:b".into(),
            "References".into(),
            -0.1,
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn add_relationship_rejects_invalid_type() {
        let config = test_config();
        let result = add_relationship(
            &config,
            "document:a".into(),
            "document:b".into(),
            "NotAType".into(),
            0.5,
        ).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn commit_nonexistent_doc_fails() {
        let config = test_config();
        // Each in-memory DB is isolated, so committing a non-existent doc should fail
        let result = commit_doc(&config, "document:nonexistent".into(), "msg".into()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_commits_empty_db() {
        let config = test_config();
        let result = list_commits(&config, "document:nonexistent".into()).await;
        // list_commits on non-existent doc returns empty, not error
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn list_contacts_empty_db() {
        let config = test_config();
        assert!(list_contacts(&config).await.is_ok());
    }

    #[tokio::test]
    async fn list_conversations_empty_db() {
        let config = test_config();
        assert!(list_conversations(&config, None).await.is_ok());
    }

    #[tokio::test]
    async fn list_conversations_with_channel_filter() {
        let config = test_config();
        assert!(list_conversations(&config, Some("email".into())).await.is_ok());
        assert!(list_conversations(&config, Some("unknown".into())).await.is_ok());
    }
}
