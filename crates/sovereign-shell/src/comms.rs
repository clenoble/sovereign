//! Email (IMAP/SMTP) wiring for the native shell — Batch 6.
//!
//! `sovereign-comms` implements the engine (IMAP fetch + SMTP send) but it was
//! never wired into a running app; this is the first place it actually runs.
//! Config (host/port/username) persists to `comms.toml`; the password is held in
//! memory for the session only (plaintext in `comms.toml` would violate the
//! field-encrypted at-rest model — vault-backed storage is a follow-up).

use std::sync::Arc;

use sovereign_comms::channel::{CommunicationChannel, OutgoingMessage, SyncResult};
use sovereign_comms::channels::email::EmailChannel;
use sovereign_comms::config::{CommsConfig, EmailAccountConfig};
use sovereign_comms::error::CommsError;
use sovereign_crypto::account_key::AccountKey;
use sovereign_crypto::vault::EncryptedBlob;
use sovereign_db::schema::{PiiKind, PiiRecord, ReviewState};
use sovereign_db::traits::GraphDB;

/// Label that identifies the email-account password in the PII vault.
pub(crate) const EMAIL_PW_LABEL: &str = "Email account password";

/// Path to the comms config file (host/port/username; never the password).
fn comms_config_path() -> std::path::PathBuf {
    sovereign_core::sovereign_dir().join("comms.toml")
}

/// Load the saved email account config (host/port/username), if any.
pub(crate) fn load_email_config() -> Option<EmailAccountConfig> {
    let text = std::fs::read_to_string(comms_config_path()).ok()?;
    let cfg: CommsConfig = toml::from_str(&text).ok()?;
    cfg.email
}

/// Persist the email account config to `comms.toml` (merging into any existing
/// CommsConfig so Signal/WhatsApp settings aren't clobbered). Never writes the
/// password.
pub(crate) fn save_email_config(email: &EmailAccountConfig) -> Result<(), String> {
    let dir = sovereign_core::sovereign_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut cfg: CommsConfig = std::fs::read_to_string(comms_config_path())
        .ok()
        .and_then(|t| toml::from_str(&t).ok())
        .unwrap_or_default();
    cfg.enabled = true;
    cfg.email = Some(email.clone());
    let serialized = toml::to_string(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(comms_config_path(), serialized).map_err(|e| e.to_string())
}

/// One-shot email sync: connect IMAP + fetch new mail into the DB. Returns the
/// sync result (new message / contact counts). Run on the tokio runtime.
pub(crate) async fn sync_email(
    db: Arc<dyn GraphDB>,
    cfg: EmailAccountConfig,
    password: String,
) -> Result<SyncResult, CommsError> {
    let mut channel = EmailChannel::new(cfg, db, password);
    channel.connect().await?;
    channel.sync().await
}

/// Persist the email password as an encrypted vault entry (a stored-secret
/// PiiRecord, kind=Password) so it survives sessions and is managed in the PII
/// dashboard. Encrypted with the AccountKey (XChaCha20-Poly1305), same as every
/// other vault secret. Replaces an existing entry rather than duplicating.
pub(crate) async fn store_email_password(
    db: &Arc<dyn GraphDB>,
    key: &AccountKey,
    password: &str,
) -> Result<(), String> {
    let blob = EncryptedBlob::encrypt_str(password, key).map_err(|e| e.to_string())?;
    let existing = db
        .list_pii_records(None, None, Some(true))
        .await
        .unwrap_or_default()
        .into_iter()
        .find(|r| r.label.as_deref() == Some(EMAIL_PW_LABEL) && r.deleted_at.is_none());
    if let Some(rec) = existing {
        if let Some(id) = rec.id_string() {
            return db
                .update_pii_record_value(&id, &blob.ciphertext_b64, &blob.nonce_b64)
                .await
                .map_err(|e| e.to_string());
        }
    }
    let rec = PiiRecord {
        id: None,
        kind: PiiKind::Password,
        value_encrypted: blob.ciphertext_b64,
        value_nonce: blob.nonce_b64,
        label: Some(EMAIL_PW_LABEL.to_string()),
        entity_id: None,
        stored_secret: true,
        confidence: 1.0,
        sources: Vec::new(),
        discovered_at: chrono::Utc::now(),
        last_revealed_at: None,
        use_count: 0,
        review_state: ReviewState::Confirmed,
        deleted_at: None,
    };
    db.create_pii_record(rec).await.map(|_| ()).map_err(|e| e.to_string())
}

/// Load + decrypt the saved email password from the vault, if present.
pub(crate) async fn load_email_password(db: &Arc<dyn GraphDB>, key: &AccountKey) -> Option<String> {
    let rec = db
        .list_pii_records(None, None, Some(true))
        .await
        .ok()?
        .into_iter()
        .find(|r| r.label.as_deref() == Some(EMAIL_PW_LABEL) && r.deleted_at.is_none())?;
    EncryptedBlob::from_pair(rec.value_encrypted, rec.value_nonce).decrypt_to_string(key).ok()
}

/// Send one email via SMTP (the channel opens its own transport). Returns the
/// provider message id.
pub(crate) async fn send_email(
    db: Arc<dyn GraphDB>,
    cfg: EmailAccountConfig,
    password: String,
    msg: OutgoingMessage,
) -> Result<String, CommsError> {
    let channel = EmailChannel::new(cfg, db, password);
    channel.send_message(&msg).await
}
