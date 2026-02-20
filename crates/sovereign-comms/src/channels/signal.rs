use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Message, MessageDirection,
};
use sovereign_db::GraphDB;

use crate::channel::{ChannelStatus, CommunicationChannel, OutgoingMessage, SyncResult};
use crate::config::SignalAccountConfig;
use crate::error::CommsError;

/// Signal channel implementation using the linked-device protocol.
///
/// Connects as a secondary device (like Signal Desktop) and syncs messages
/// via the Signal protocol. Uses `presage` under the hood when the `signal`
/// feature is enabled.
pub struct SignalChannel {
    config: SignalAccountConfig,
    db: Arc<dyn GraphDB>,
    status: ChannelStatus,
    last_sync: Option<DateTime<Utc>>,
}

impl SignalChannel {
    pub fn new(config: SignalAccountConfig, db: Arc<dyn GraphDB>) -> Self {
        Self {
            config,
            db,
            status: ChannelStatus::Disconnected,
            last_sync: None,
        }
    }

    /// Get or create a conversation for a Signal chat, using a local cache.
    async fn get_or_create_conversation(
        &self,
        title: &str,
        participant_ids: Vec<String>,
        cache: &mut HashMap<String, Conversation>,
    ) -> Result<Conversation, CommsError> {
        if let Some(conv) = cache.get(title) {
            return Ok(conv.clone());
        }

        let conv = Conversation::new(
            title.to_string(),
            ChannelType::Signal,
            participant_ids,
        );
        let created = self.db.create_conversation(conv).await.map_err(CommsError::from)?;
        cache.insert(title.to_string(), created.clone());
        Ok(created)
    }

    /// Resolve a phone number to a contact ID, creating a stub if needed.
    async fn resolve_contact_id(
        &self,
        phone: &str,
        display_name: Option<&str>,
    ) -> Result<String, CommsError> {
        if let Some(contact) = self.db.find_contact_by_address(phone).await? {
            return Ok(contact.id_string().unwrap_or_default());
        }

        let name = display_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| phone.to_string());
        let mut contact = Contact::new(name, false);
        contact.addresses.push(ChannelAddress {
            channel: ChannelType::Signal,
            address: phone.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            is_primary: true,
        });
        let created = self.db.create_contact(contact).await?;
        Ok(created.id_string().unwrap_or_default())
    }
}

#[async_trait]
impl CommunicationChannel for SignalChannel {
    async fn connect(&mut self) -> Result<(), CommsError> {
        self.status = ChannelStatus::Connecting;

        if self.config.phone_number.is_empty() {
            self.status = ChannelStatus::Error("No phone number configured".into());
            return Err(CommsError::ConfigError(
                "Signal phone number is required".into(),
            ));
        }

        #[cfg(feature = "signal")]
        {
            use presage::manager::ReceivingMode;
            use presage_store_sqlite::SqliteStore;

            // Ensure store directory exists
            std::fs::create_dir_all(&self.config.store_path)
                .map_err(|e| CommsError::ConfigError(format!(
                    "Failed to create Signal store at {}: {e}",
                    self.config.store_path
                )))?;

            let db_path = format!("{}/signal.db", self.config.store_path);
            let store = SqliteStore::open(&db_path, None)
                .await
                .map_err(|e| CommsError::NotConnected(format!(
                    "Failed to open Signal store: {e}"
                )))?;

            // Check if we're already registered/linked
            match store.is_registered() {
                true => {
                    tracing::info!("Signal: already linked as secondary device");
                    self.status = ChannelStatus::Connected;
                }
                false => {
                    self.status = ChannelStatus::Error(
                        "Not linked — run Signal pairing first".into(),
                    );
                    return Err(CommsError::AuthFailed(
                        "Signal device not linked. Use the pairing flow to connect.".into(),
                    ));
                }
            }
        }

        #[cfg(not(feature = "signal"))]
        {
            // Without the Signal protocol library, mark as connected for structure testing
            tracing::info!(
                "Signal channel initialized (protocol library not compiled in)"
            );
            self.status = ChannelStatus::Connected;
        }

        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), CommsError> {
        self.status = ChannelStatus::Disconnected;
        Ok(())
    }

    fn status(&self) -> ChannelStatus {
        self.status.clone()
    }

    fn channel_type(&self) -> ChannelType {
        ChannelType::Signal
    }

    async fn fetch_messages(
        &self,
        _since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>, CommsError> {
        #[cfg(feature = "signal")]
        {
            use presage::manager::ReceivingMode;
            use presage_store_sqlite::SqliteStore;
            use futures::StreamExt;

            let db_path = format!("{}/signal.db", self.config.store_path);
            let store = SqliteStore::open(&db_path, None)
                .await
                .map_err(|e| CommsError::FetchFailed(format!("Store open: {e}")))?;

            let mut manager = presage::Manager::load_registered(store)
                .await
                .map_err(|e| CommsError::FetchFailed(format!("Manager load: {e}")))?;

            let mut messages = Vec::new();
            let mut receiving = manager.receive_messages(ReceivingMode::WaitForContacts)
                .await
                .map_err(|e| CommsError::FetchFailed(format!("Receive: {e}")))?;

            // Pre-load conversation cache and own contact ID
            let conversations = self.db.list_conversations(Some(&ChannelType::Signal)).await?;
            let mut conv_cache: HashMap<String, Conversation> = conversations
                .into_iter()
                .map(|c| (c.title.clone(), c))
                .collect();
            let my_id = self.resolve_contact_id(
                &self.config.phone_number,
                self.config.device_name.as_deref(),
            ).await?;

            // Collect available messages (non-blocking drain)
            while let Ok(Some(content)) = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                receiving.next(),
            ).await {
                if let Some(content) = content {
                    let sender = content.metadata.sender.uuid.to_string();
                    let from_id = self.resolve_contact_id(&sender, None).await?;

                    if let Some(body) = content.body.as_deref() {
                        let title = format!("Signal: {sender}");
                        let conv = self.get_or_create_conversation(
                            &title,
                            vec![from_id.clone(), my_id.clone()],
                            &mut conv_cache,
                        ).await?;
                        let conv_id = conv.id_string().unwrap_or_default();

                        let mut msg = Message::new(
                            conv_id,
                            ChannelType::Signal,
                            MessageDirection::Inbound,
                            from_id,
                            vec![my_id.clone()],
                            body.to_string(),
                        );
                        msg.received_at = Some(Utc::now());
                        msg.external_id = Some(format!(
                            "signal:{}",
                            content.metadata.timestamp
                        ));
                        messages.push(msg);
                    }
                }
            }

            Ok(messages)
        }

        #[cfg(not(feature = "signal"))]
        {
            Ok(vec![])
        }
    }

    async fn send_message(&self, msg: &OutgoingMessage) -> Result<String, CommsError> {
        #[cfg(feature = "signal")]
        {
            use presage_store_sqlite::SqliteStore;

            let db_path = format!("{}/signal.db", self.config.store_path);
            let store = SqliteStore::open(&db_path, None)
                .await
                .map_err(|e| CommsError::SendFailed(format!("Store open: {e}")))?;

            let mut manager = presage::Manager::load_registered(store)
                .await
                .map_err(|e| CommsError::SendFailed(format!("Manager load: {e}")))?;

            for recipient in &msg.to {
                let recipient_uuid = recipient.parse()
                    .map_err(|e| CommsError::SendFailed(format!(
                        "Invalid recipient UUID '{recipient}': {e}"
                    )))?;

                manager.send_message(recipient_uuid, msg.body.clone(), vec![])
                    .await
                    .map_err(|e| CommsError::SendFailed(format!("Send: {e}")))?;
            }

            let msg_id = format!("signal:sent:{}", Utc::now().timestamp_millis());
            Ok(msg_id)
        }

        #[cfg(not(feature = "signal"))]
        {
            let _ = msg;
            Err(CommsError::ConfigError(
                "Signal feature not enabled".into(),
            ))
        }
    }

    async fn sync(&mut self) -> Result<SyncResult, CommsError> {
        let messages = self.fetch_messages(self.last_sync).await?;

        let mut new_messages = 0u32;

        for msg in &messages {
            if let Some(ref ext_id) = msg.external_id {
                let existing = self.db.search_messages(ext_id).await?;
                if !existing.is_empty() {
                    continue;
                }
            }

            self.db.create_message(msg.clone()).await?;
            new_messages += 1;
        }

        self.last_sync = Some(Utc::now());

        Ok(SyncResult {
            new_messages,
            updated_conversations: 0,
            new_contacts: 0,
        })
    }

    async fn resolve_contact(&self, address: &str) -> Result<Contact, CommsError> {
        if let Some(contact) = self.db.find_contact_by_address(address).await? {
            return Ok(contact);
        }

        let mut contact = Contact::new(address.to_string(), false);
        contact.addresses.push(ChannelAddress {
            channel: ChannelType::Signal,
            address: address.to_string(),
            display_name: None,
            is_primary: true,
        });
        self.db.create_contact(contact).await.map_err(CommsError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_channel_type() {
        // Verify ChannelType::Signal displays correctly
        assert_eq!(ChannelType::Signal.to_string(), "signal");
    }

    #[test]
    fn signal_config_defaults() {
        let toml_str = r#"phone_number = "+15551234567""#;
        let cfg: SignalAccountConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.phone_number, "+15551234567");
        assert!(cfg.store_path.ends_with("signal"));
    }
}
