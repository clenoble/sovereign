use anyhow::Result;
use sovereign_core::config::AppConfig;
use sovereign_db::schema::{thing_to_raw, RelationType};
use sovereign_db::GraphDB;
use sovereign_docs::DocumentService;

use crate::setup::create_db;

// ── Document writes (C-DOC adoption step 3) ──────────────────────────────
//
// The CLI writes documents through the document service, like every other
// host: format v2, versions, personal-data ingest, trust and provenance. The
// service only exists over the persona's encrypted store, so these commands
// authenticate first (the passphrase prompt the import already had) — they
// used to write plaintext rows into the raw database.

/// Creates an empty document; prints its id. `is_owned`: the user's own
/// (provenance written) or external (provenance import, from the command
/// line).
pub async fn create_doc(
    config: &AppConfig,
    title: String,
    thread_id: String,
    is_owned: bool,
) -> Result<()> {
    let (_db, docs) = open_primary_documents(config, "create a document").await?;
    let id = create_doc_with(&docs, title, thread_id, is_owned).await?;
    println!("{id}");
    Ok(())
}

/// [`create_doc`] over an open service.
pub async fn create_doc_with(docs: &DocumentService, title: String, thread_id: String, is_owned: bool) -> Result<String> {
    let new = if is_owned {
        sovereign_ai::docs_host::new_user_document(title, thread_id, "")
    } else {
        sovereign_ai::docs_host::new_document(
            title,
            thread_id,
            "",
            sovereign_docs::Trust::External,
            sovereign_docs::Provenance::Import { source_path: "command line".into() },
            sovereign_docs::Origin::Editor { session: sovereign_ai::docs_host::new_edit_session() },
        )
    };
    let created = sovereign_ai::docs_host::create(docs, new)
        .await
        .map_err(|e| anyhow::anyhow!(sovereign_ai::docs_host::doc_error_message(&e)))?;
    Ok(created.id)
}

/// Changes a document's title and/or text (a text-only write: refused for a
/// document with several text blocks). An explicit save: it is a version.
pub async fn update_doc(
    config: &AppConfig,
    id: String,
    title: Option<String>,
    content: Option<String>,
) -> Result<()> {
    let (_db, docs) = open_primary_documents(config, "update a document").await?;
    update_doc_with(&docs, &id, title, content).await?;
    let view = docs.read(&id).await.map_err(|e| anyhow::anyhow!(sovereign_ai::docs_host::doc_error_message(&e)))?;
    println!("{id}\t{}", view.title);
    println!("{}", view.plain_text());
    Ok(())
}

/// [`update_doc`] over an open service.
pub async fn update_doc_with(
    docs: &DocumentService,
    id: &str,
    title: Option<String>,
    content: Option<String>,
) -> Result<sovereign_docs::SaveOutcome> {
    let msg = |e: sovereign_docs::DocError| anyhow::anyhow!(sovereign_ai::docs_host::doc_error_message(&e));
    let base = docs.read(id).await.map_err(msg)?.revision;
    let session = sovereign_ai::docs_host::new_edit_session();
    let edit = sovereign_docs::Edit {
        base,
        title,
        content: content.map(sovereign_docs::EditContent::PlainText),
        origin: sovereign_docs::Origin::Editor { session },
        explicit: true,
        pasted_tokens: vec![],
    };
    let out = docs.save(id, edit).await.map_err(msg)?;
    docs.end_session(id, session).await.map_err(msg)?;
    Ok(out)
}

/// The primary persona's encrypted store and document service, after the
/// passphrase (the duress passphrase is refused: these commands target the
/// primary workspace).
#[cfg(feature = "encryption")]
async fn open_primary_documents(
    config: &AppConfig,
    what: &str,
) -> Result<(std::sync::Arc<sovereign_db::encrypted::EncryptedGraphDB>, DocumentService)> {
    use sovereign_crypto::auth::{AuthStore, PersonaKind};

    let auth_path = crate::setup::crypto_dir().join("auth.store");
    if !auth_path.exists() {
        anyhow::bail!("no account found ({}). Onboard in the app first.", auth_path.display());
    }
    let store = AuthStore::load(&auth_path)?;
    let pass = rpassword::prompt_password(format!("Sovereign passphrase to {what} (or Ctrl-C to cancel): "))?;
    let auth = store
        .authenticate(pass.as_bytes())
        .map_err(|_| anyhow::anyhow!("authentication failed"))?;
    if matches!(auth.persona, PersonaKind::Duress) {
        anyhow::bail!("this command targets your primary workspace — use your primary password");
    }
    let device_key = std::sync::Arc::new(auth.device_key);
    let kek = std::sync::Arc::new(auth.kek);
    if let Err(e) = crate::setup::complete_auth(auth.persona, &device_key, &kek) {
        tracing::warn!("complete_auth side-effect failed: {e}");
    }
    let raw: std::sync::Arc<dyn GraphDB> = std::sync::Arc::new(create_db(config).await?);
    let db = crate::setup::build_encrypted_db(raw, device_key, kek, sovereign_core::auth::PersonaKind::Primary)?;
    let docs = sovereign_ai::docs_host::build_document_service(
        db.clone(),
        Some(std::sync::Arc::new(auth.account_key)),
        config.p2p.device_name.clone(),
    );
    Ok((db, docs))
}

#[cfg(not(feature = "encryption"))]
async fn open_primary_documents(
    _config: &AppConfig,
    what: &str,
) -> Result<(std::sync::Arc<sovereign_db::encrypted::EncryptedGraphDB>, DocumentService)> {
    anyhow::bail!("to {what}, the CLI needs the encryption feature (documents are written through the encrypted store)")
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

/// Moves a document to the trash through an open service (C-DOC adoption
/// step 5, S5-2: `delete-doc` trashes, nothing is hard-deleted). A document
/// already in the trash, purged or unknown is refused, and nothing changes.
pub async fn delete_doc_with(docs: &DocumentService, id: &str) -> Result<()> {
    let message = |e: sovereign_docs::DocError| anyhow::anyhow!(sovereign_ai::docs_host::doc_error_message(&e));
    let view = docs.read(id).await.map_err(message)?;
    if view.trashed_at.is_some() {
        anyhow::bail!("{id} is already in the trash; nothing was changed");
    }
    docs.trash(id).await.map_err(message)?;
    Ok(())
}

/// `delete-doc`: moves a document to the trash (recoverable until the 30-day
/// purge). Needs the account, like every document write of the CLI: the trash
/// is the document service's, and the service only exists over the encrypted
/// store (S5-2).
pub async fn delete_doc(config: &AppConfig, id: String) -> Result<()> {
    let (_db, docs) = open_primary_documents(config, "delete a document").await?;
    delete_doc_with(&docs, &id).await?;
    println!("Moved {id} to the trash (recoverable for 30 days)");
    Ok(())
}

pub async fn create_thread(
    config: &AppConfig,
    name: String,
    description: String,
) -> Result<()> {
    // Through the one thread-write path, stamped (THREAD-sync S1): like every
    // document write of the CLI, it needs the account (the service exists
    // only over the encrypted store).
    let (_db, docs) = open_primary_documents(config, "create a thread").await?;
    let id = docs
        .thread_create(&name, &description)
        .await
        .map_err(|e| anyhow::anyhow!(sovereign_ai::docs_host::doc_error_message(&e)))?;
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
    let outgoing = db.list_outgoing_relationships(&doc_id).await?;
    let incoming = db.list_incoming_relationships(&doc_id).await?;
    for r in &outgoing {
        let id = r.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
        println!("{id}\tout\t{}\tstrength={:.2}", r.relation_type, r.strength);
    }
    for r in &incoming {
        let id = r.id.as_ref().map(|t| thing_to_raw(t)).unwrap_or_default();
        println!("{id}\tin\t{}\tstrength={:.2}", r.relation_type, r.strength);
    }
    println!("({} relationships)", outgoing.len() + incoming.len());
    Ok(())
}

/// Refused (C-DOC adoption step 3): versions are made by the document
/// service — at creation, at every `update-doc` (an explicit save), and by
/// amount of change in the editors — never on request with a free message.
pub async fn commit_doc(_config: &AppConfig, doc_id: String, _message: String) -> Result<()> {
    anyhow::bail!(
        "versions of {doc_id} are made by the document service (each `update-doc` is one); \
         `commit` no longer writes one — `list-commits` shows them"
    )
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

    // Update each document with encrypted content and nonce. The nonce MUST
    // be persisted with the ciphertext: without it the row reads back as raw
    // ciphertext and a re-run would encrypt it a second time.
    for result in &results {
        db.set_document_content_encryption(
            &result.doc_id,
            &result.encrypted_content,
            &result.nonce_b64,
        )
        .await?;
        tracing::info!("Encrypted {}", result.doc_id);
    }

    // Persist key database
    let crypto_dir = crate::setup::crypto_dir();
    let device_id = crate::setup::load_or_create_device_id()?;
    let salt_path = crypto_dir.join("salt");
    let salt = std::fs::read(&salt_path)?;
    let pass = rpassword::prompt_password("Re-enter passphrase to save key DB: ")?;
    // CRYPTO-001: derive with the same version-aware KDF as init_crypto —
    // single-pass HKDF here would undo the Argon2id brute-force hardening.
    let master = MasterKey::derive(pass.as_bytes(), &salt, &crate::setup::cli_kdf())?;
    let device_key = DeviceKey::derive(&master, &device_id)?;
    // keys.db is sealed under the KEK (content-chain root); unwrap the KEK
    // (stored wrapped under the device key) to re-seal the store.
    let kek_path = crypto_dir.join("kek.wrapped");
    let wrapped: sovereign_crypto::kek::WrappedKek =
        serde_json::from_slice(&std::fs::read(&kek_path)?)?;
    let kek = sovereign_crypto::kek::Kek::unwrap(&wrapped, &device_key)?;
    key_db_guard.save(&kek)?;

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

// ── Bulk import (stub-first migration) ───────────────────────────────────

fn import_options(single_thread: Option<String>) -> sovereign_import::ImportOptions {
    let mut opts = sovereign_import::ImportOptions::default();
    if let Some(name) = single_thread {
        opts.thread_mode = sovereign_import::ThreadMode::SingleThread(name);
    }
    opts
}

/// Dry-run: scan the folder and print the plan. Writes nothing, needs no auth.
pub fn import_dry_run(dir: &std::path::Path, single_thread: Option<String>) -> Result<()> {
    let opts = import_options(single_thread);
    let manifest = sovereign_import::plan(dir, &opts)?;
    print!("{}", sovereign_import::render_manifest(&manifest));
    println!(
        "\nDry-run only — nothing was written. Re-run with --execute to import \
         (you'll be prompted for your password)."
    );
    Ok(())
}

/// Execute the import against the PRIMARY persona's encrypted workspace.
/// Authenticates (prompts for the passphrase; the duress passphrase is
/// refused rather than landing the corpus in the decoy), builds the same
/// `EncryptedGraphDB` the app uses, and creates every document through the
/// document service (C-DOC adoption step 3): encrypted at rest, a version
/// from creation, personal data tokenized, trust **undetermined**.
#[cfg(feature = "encryption")]
pub async fn import_execute(
    config: &AppConfig,
    dir: &std::path::Path,
    single_thread: Option<String>,
) -> Result<()> {
    let opts = import_options(single_thread);
    let manifest = sovereign_import::plan(dir, &opts)?;
    // Show the plan first, then act on it.
    print!("{}", sovereign_import::render_manifest(&manifest));

    // IMPORT-002: the passphrase entry is the affirmative consent. Restate the
    // trust boundary right before it so the choice is informed, not buried above.
    println!(
        "\n⚠  Proceeding imports the document(s) above as UNDETERMINED content (treated as external \
         until you adopt them) — see the trust notice."
    );
    let (db, docs) = open_primary_documents(config, "import").await?;

    let outcome = sovereign_import::execute(db.as_ref(), &docs, &manifest).await?;
    // The raw body of the last documents created (the hook preserves each one
    // at the next write; nothing writes after the last).
    sovereign_ai::docs_host::flush_raw_bodies_of(db.as_ref()).await;
    println!(
        "\nImported: {} document(s) ({} stub), {} thread(s) created, {} reused, {} skipped (already present).",
        outcome.docs_created,
        outcome.stubs_created,
        outcome.threads_created,
        outcome.threads_reused,
        outcome.docs_skipped_existing,
    );
    // Surface WHICH files were skipped, not just the count — a skip must be
    // visible, never a silent truncation (GRAM-0010).
    if !outcome.skipped_paths.is_empty() {
        println!("Skipped (already present in the workspace):");
        for p in &outcome.skipped_paths {
            println!("  {p}");
        }
    }
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

    /// C-DOC adoption step 3: the CLI creates and updates through the document
    /// service — owned/written or external/import, a version from creation,
    /// and every update an explicit save (a version).
    #[tokio::test]
    async fn create_and_update_doc_go_through_the_service() {
        use sovereign_ai::docs_host::{build_document_service, testing};
        let store = testing::encrypted_memory_store("cli-create").await;
        let docs = build_document_service(store, Some(testing::account_key()), "cli".into());

        let id = create_doc_with(&docs, "Test Title".into(), "thread:t".into(), true).await.unwrap();
        let view = docs.read(&id).await.unwrap();
        assert_eq!(view.trust, sovereign_docs::Trust::Owned);
        assert_eq!(view.provenance, sovereign_docs::Provenance::Written);
        assert_eq!(docs.versions(&id).await.unwrap().len(), 1);

        let ext = create_doc_with(&docs, "Other".into(), "thread:t".into(), false).await.unwrap();
        assert_eq!(docs.read(&ext).await.unwrap().trust, sovereign_docs::Trust::External);

        let out = update_doc_with(&docs, &id, Some("New title".into()), Some("Some text".into())).await.unwrap();
        assert!(out.changed && out.version.is_some(), "an update is a version");
        let view = docs.read(&id).await.unwrap();
        assert_eq!((view.title.as_str(), view.plain_text().as_str()), ("New title", "Some text"));
        assert_eq!(docs.versions(&id).await.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn commit_is_refused() {
        let config = test_config();
        let result = commit_doc(&config, "document:x".into(), "msg".into()).await;
        assert!(result.unwrap_err().to_string().contains("document service"));
    }

    #[tokio::test]
    async fn list_docs_empty_db() {
        let config = test_config();
        let result = list_docs(&config, None).await;
        assert!(result.is_ok());
    }

    /// Points `SOVEREIGN_DATA_DIR` at a temporary directory of this test
    /// process (once, before any lookup), and checks no account is there: the
    /// CLI looks for the account in `<data dir>/crypto`, and a test that
    /// reached a real one would wait for its passphrase.
    fn isolated_data_dir() {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            let dir = std::env::temp_dir().join(format!("sovereign-cli-unit-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            std::env::set_var("SOVEREIGN_DATA_DIR", &dir);
        });
        assert!(
            !crate::setup::crypto_dir().join("auth.store").exists(),
            "the test data directory holds an account: refusing to run (it would ask for a passphrase)"
        );
    }

    /// THREAD-sync S1: `create-thread` writes through the stamped thread path,
    /// so, like `create-doc`, it needs the account. Without one it refuses and
    /// writes nothing; with the service (over an encrypted store, as the
    /// document commands are tested) the thread is created, stamped, and
    /// listed. (Migrated by the THREAD-sync test author: it used to create
    /// through the plain store.)
    #[tokio::test]
    async fn create_and_list_threads() {
        isolated_data_dir();
        let config = test_config();
        let err = create_thread(&config, "MyThread".into(), "desc".into()).await.unwrap_err();
        assert!(err.to_string().contains("no account found"), "{err}");
        assert!(list_threads(&config).await.is_ok());

        use sovereign_ai::docs_host::{build_document_service, testing};
        let store = testing::encrypted_memory_store("cli-thread").await;
        let docs = build_document_service(store.clone(), Some(testing::account_key()), "cli".into());
        let id = docs.thread_create("MyThread", "desc").await.unwrap();
        let listed = store.list_threads().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!((listed[0].name.as_str(), listed[0].description.as_str()), ("MyThread", "desc"));
        assert_eq!(listed[0].id_string().as_deref(), Some(id.as_str()));
        assert!(docs.thread_stamp(&id).await.unwrap().is_some(), "stamped");
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
