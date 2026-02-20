use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Message, MessageDirection,
};
use sovereign_db::GraphDB;

use crate::channel::{ChannelStatus, CommunicationChannel, OutgoingMessage, SyncResult};
use crate::config::WhatsAppAccountConfig;
use crate::error::CommsError;

/// WhatsApp channel implementation using Meta's Cloud API.
///
/// Requires a WhatsApp Business account and access token.
/// Uses `reqwest` for HTTP calls to the Graph API.
pub struct WhatsAppChannel {
    config: WhatsAppAccountConfig,
    db: Arc<dyn GraphDB>,
    access_token: String,
    status: ChannelStatus,
    last_sync: Option<DateTime<Utc>>,
    #[cfg(feature = "whatsapp")]
    client: reqwest::Client,
}

impl WhatsAppChannel {
    pub fn new(
        config: WhatsAppAccountConfig,
        db: Arc<dyn GraphDB>,
        access_token: String,
    ) -> Self {
        Self {
            config,
            db,
            access_token,
            status: ChannelStatus::Disconnected,
            last_sync: None,
            #[cfg(feature = "whatsapp")]
            client: reqwest::Client::new(),
        }
    }

    /// Build the base API URL for a given endpoint.
    #[cfg(feature = "whatsapp")]
    fn api_url(&self, endpoint: &str) -> String {
        format!(
            "{}/{}/{}",
            self.config.api_url, self.config.api_version, endpoint
        )
    }

    /// Get or create a conversation for a WhatsApp chat, using a local cache.
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
            ChannelType::WhatsApp,
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
            channel: ChannelType::WhatsApp,
            address: phone.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            is_primary: true,
        });
        let created = self.db.create_contact(contact).await?;
        Ok(created.id_string().unwrap_or_default())
    }
}

/// Webhook payload from the WhatsApp Cloud API.
#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WebhookPayload {
    entry: Vec<WebhookEntry>,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WebhookEntry {
    changes: Vec<WebhookChange>,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WebhookChange {
    value: WebhookValue,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WebhookValue {
    #[serde(default)]
    messages: Vec<IncomingWaMessage>,
    #[serde(default)]
    contacts: Vec<WaContact>,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct IncomingWaMessage {
    from: String,
    id: String,
    timestamp: String,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    text: Option<WaTextBody>,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WaTextBody {
    body: String,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WaContact {
    #[serde(default)]
    profile: Option<WaProfile>,
    wa_id: String,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct WaProfile {
    name: String,
}

/// Send message request body for the Cloud API.
#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Serialize)]
struct SendMessageRequest {
    messaging_product: String,
    to: String,
    #[serde(rename = "type")]
    msg_type: String,
    text: SendTextBody,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Serialize)]
struct SendTextBody {
    body: String,
}

/// Response from the send message API.
#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct SendMessageResponse {
    messages: Vec<SendMessageId>,
}

#[cfg(feature = "whatsapp")]
#[derive(Debug, serde::Deserialize)]
struct SendMessageId {
    id: String,
}

#[async_trait]
impl CommunicationChannel for WhatsAppChannel {
    async fn connect(&mut self) -> Result<(), CommsError> {
        self.status = ChannelStatus::Connecting;

        if self.config.phone_number_id.is_empty() || self.access_token.is_empty() {
            self.status = ChannelStatus::Error("Missing WhatsApp configuration".into());
            return Err(CommsError::ConfigError(
                "WhatsApp phone_number_id and access token are required".into(),
            ));
        }

        #[cfg(feature = "whatsapp")]
        {
            // Verify the token by fetching the business profile
            let url = self.api_url(&format!(
                "{}/whatsapp_business_profile",
                self.config.phone_number_id
            ));
            let resp = self
                .client
                .get(&url)
                .bearer_auth(&self.access_token)
                .query(&[("fields", "about,address,description,vertical")])
                .send()
                .await
                .map_err(|e| CommsError::NotConnected(format!("API request failed: {e}")))?;

            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();
                self.status = ChannelStatus::Error(format!("API {status}"));
                return Err(CommsError::AuthFailed(format!(
                    "WhatsApp API returned {status}: {body}"
                )));
            }

            self.status = ChannelStatus::Connected;
            tracing::info!("WhatsApp Cloud API connected");
        }

        #[cfg(not(feature = "whatsapp"))]
        {
            tracing::info!(
                "WhatsApp channel initialized (reqwest not compiled in)"
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
        ChannelType::WhatsApp
    }

    async fn fetch_messages(
        &self,
        _since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>, CommsError> {
        // The WhatsApp Cloud API uses webhooks (push-based), not polling.
        // Messages arrive via HTTP POST to a configured webhook URL.
        // This method would be called by the webhook handler to process
        // incoming payloads, or could poll a local queue.
        //
        // For now, return empty — the real flow is:
        // 1. Webhook receives POST from Meta
        // 2. parse_webhook_payload() converts to Messages
        // 3. Messages are stored in the DB
        Ok(vec![])
    }

    async fn send_message(&self, msg: &OutgoingMessage) -> Result<String, CommsError> {
        #[cfg(feature = "whatsapp")]
        {
            let mut last_id = String::new();

            for recipient in &msg.to {
                let url = self.api_url(&format!(
                    "{}/messages",
                    self.config.phone_number_id
                ));

                let request = SendMessageRequest {
                    messaging_product: "whatsapp".into(),
                    to: recipient.clone(),
                    msg_type: "text".into(),
                    text: SendTextBody {
                        body: msg.body.clone(),
                    },
                };

                let resp = self
                    .client
                    .post(&url)
                    .bearer_auth(&self.access_token)
                    .json(&request)
                    .send()
                    .await
                    .map_err(|e| CommsError::SendFailed(format!("Request failed: {e}")))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    return Err(CommsError::SendFailed(format!(
                        "WhatsApp API returned {status}: {body}"
                    )));
                }

                let response: SendMessageResponse = resp
                    .json()
                    .await
                    .map_err(|e| CommsError::SendFailed(format!("Parse response: {e}")))?;

                if let Some(msg_id) = response.messages.first() {
                    last_id = msg_id.id.clone();
                }
            }

            Ok(last_id)
        }

        #[cfg(not(feature = "whatsapp"))]
        {
            let _ = msg;
            Err(CommsError::ConfigError(
                "WhatsApp feature not enabled".into(),
            ))
        }
    }

    async fn sync(&mut self) -> Result<SyncResult, CommsError> {
        // WhatsApp uses webhooks, so sync is a no-op for polling.
        // The webhook handler stores messages directly.
        self.last_sync = Some(Utc::now());
        Ok(SyncResult {
            new_messages: 0,
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
            channel: ChannelType::WhatsApp,
            address: address.to_string(),
            display_name: None,
            is_primary: true,
        });
        self.db.create_contact(contact).await.map_err(CommsError::from)
    }
}

/// Parse an incoming WhatsApp webhook payload into Messages.
/// Called by the webhook HTTP handler when Meta posts to the callback URL.
#[cfg(feature = "whatsapp")]
pub async fn parse_webhook_payload(
    payload: &str,
    db: &Arc<dyn GraphDB>,
    own_phone_id: &str,
    channel: &WhatsAppChannel,
) -> Result<Vec<Message>, CommsError> {
    let webhook: WebhookPayload = serde_json::from_str(payload)
        .map_err(|e| CommsError::ParseError(format!("Webhook JSON: {e}")))?;

    let mut messages = Vec::new();

    // Pre-load conversation cache and own contact ID
    let conversations = db.list_conversations(Some(&ChannelType::WhatsApp)).await?;
    let mut conv_cache: HashMap<String, Conversation> = conversations
        .into_iter()
        .map(|c| (c.title.clone(), c))
        .collect();
    let my_id = channel.resolve_contact_id(own_phone_id, None).await?;

    for entry in &webhook.entry {
        for change in &entry.changes {
            let value = &change.value;

            // Build a name map from contacts
            let mut name_map = HashMap::new();
            for contact in &value.contacts {
                if let Some(ref profile) = contact.profile {
                    name_map.insert(contact.wa_id.clone(), profile.name.clone());
                }
            }

            for wa_msg in &value.messages {
                if wa_msg.msg_type != "text" {
                    continue;
                }

                let body = wa_msg
                    .text
                    .as_ref()
                    .map(|t| t.body.clone())
                    .unwrap_or_default();

                let display_name = name_map.get(&wa_msg.from);
                let from_id = channel
                    .resolve_contact_id(&wa_msg.from, display_name.map(|s| s.as_str()))
                    .await?;

                let title = display_name
                    .map(|n| format!("WhatsApp: {n}"))
                    .unwrap_or_else(|| format!("WhatsApp: {}", wa_msg.from));

                let conv = channel
                    .get_or_create_conversation(&title, vec![from_id.clone(), my_id.clone()], &mut conv_cache)
                    .await?;
                let conv_id = conv.id_string().unwrap_or_default();

                let sent_at = wa_msg
                    .timestamp
                    .parse::<i64>()
                    .ok()
                    .and_then(|ts| DateTime::from_timestamp(ts, 0))
                    .unwrap_or_else(Utc::now);

                let mut msg = Message::new(
                    conv_id,
                    ChannelType::WhatsApp,
                    MessageDirection::Inbound,
                    from_id,
                    vec![my_id.clone()],
                    body,
                );
                msg.sent_at = sent_at;
                msg.received_at = Some(Utc::now());
                msg.external_id = Some(format!("wa:{}", wa_msg.id));

                messages.push(msg);
            }
        }
    }

    Ok(messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whatsapp_channel_type() {
        assert_eq!(ChannelType::WhatsApp.to_string(), "whatsapp");
    }

    #[test]
    fn whatsapp_config_defaults() {
        let toml_str = r#"
            phone_number_id = "123"
            business_account_id = "456"
        "#;
        let cfg: WhatsAppAccountConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.phone_number_id, "123");
        assert!(cfg.api_url.contains("graph.facebook.com"));
        assert_eq!(cfg.api_version, "v21.0");
    }

    #[cfg(feature = "whatsapp")]
    #[test]
    fn serialize_send_request() {
        let req = SendMessageRequest {
            messaging_product: "whatsapp".into(),
            to: "+15551234567".into(),
            msg_type: "text".into(),
            text: SendTextBody {
                body: "Hello!".into(),
            },
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("whatsapp"));
        assert!(json.contains("Hello!"));
    }

    #[cfg(feature = "whatsapp")]
    #[test]
    fn parse_webhook_contact() {
        let json = r#"{
            "profile": { "name": "Alice" },
            "wa_id": "15551234567"
        }"#;
        let contact: WaContact = serde_json::from_str(json).unwrap();
        assert_eq!(contact.wa_id, "15551234567");
        assert_eq!(contact.profile.unwrap().name, "Alice");
    }
}
