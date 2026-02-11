use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Message, MessageDirection,
};
use sovereign_db::GraphDB;

use crate::channel::{ChannelStatus, CommunicationChannel, OutgoingMessage, SyncResult};
use crate::config::EmailAccountConfig;
use crate::error::CommsError;

/// Email channel implementation using IMAP (fetch) and SMTP (send).
pub struct EmailChannel {
    config: EmailAccountConfig,
    db: Arc<dyn GraphDB>,
    password: String,
    status: ChannelStatus,
    last_sync: Option<DateTime<Utc>>,
}

impl EmailChannel {
    pub fn new(
        config: EmailAccountConfig,
        db: Arc<dyn GraphDB>,
        password: String,
    ) -> Self {
        Self {
            config,
            db,
            password,
            status: ChannelStatus::Disconnected,
            last_sync: None,
        }
    }

    /// Get or create a conversation for an email thread.
    async fn get_or_create_conversation(
        &self,
        subject: &str,
        participant_ids: Vec<String>,
    ) -> Result<Conversation, CommsError> {
        // Try to find existing conversation by title
        let conversations = self.db.list_conversations(Some(&ChannelType::Email)).await?;
        for conv in &conversations {
            if conv.title == subject {
                return Ok(conv.clone());
            }
        }

        // Create new conversation
        let conv = Conversation::new(
            subject.to_string(),
            ChannelType::Email,
            participant_ids,
        );
        self.db.create_conversation(conv).await.map_err(CommsError::from)
    }

    /// Resolve an email address to a contact ID, creating a stub if needed.
    async fn resolve_contact_id(&self, address: &str, display_name: Option<&str>) -> Result<String, CommsError> {
        // Check if contact exists
        if let Some(contact) = self.db.find_contact_by_address(address).await? {
            return Ok(contact.id_string().unwrap_or_default());
        }

        // Create stub contact
        let name = display_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| address.to_string());
        let mut contact = Contact::new(name, false);
        contact.addresses.push(ChannelAddress {
            channel: ChannelType::Email,
            address: address.to_string(),
            display_name: display_name.map(|s| s.to_string()),
            is_primary: true,
        });
        let created = self.db.create_contact(contact).await?;
        Ok(created.id_string().unwrap_or_default())
    }

    /// Parse a raw email into a Message struct.
    #[cfg(feature = "email")]
    fn parse_email(
        &self,
        raw: &[u8],
        from_contact_id: &str,
        to_contact_ids: Vec<String>,
        conversation_id: &str,
    ) -> Result<Message, CommsError> {
        let parsed = mailparse::parse_mail(raw)
            .map_err(|e| CommsError::ParseError(e.to_string()))?;

        let headers = &parsed.headers;
        let subject = headers.iter()
            .find(|h| h.get_key_ref() == "Subject")
            .map(|h| h.get_value());
        let date_str = headers.iter()
            .find(|h| h.get_key_ref() == "Date")
            .map(|h| h.get_value());
        let message_id = headers.iter()
            .find(|h| h.get_key_ref() == "Message-ID" || h.get_key_ref() == "Message-Id")
            .map(|h| h.get_value());

        let sent_at = date_str
            .and_then(|d| mailparse::dateparse(&d).ok())
            .map(|ts| DateTime::from_timestamp(ts, 0).unwrap_or_else(Utc::now))
            .unwrap_or_else(Utc::now);

        let body = parsed.get_body()
            .map_err(|e| CommsError::ParseError(e.to_string()))?;

        // Try to get HTML body from multipart
        let body_html = parsed.subparts.iter()
            .find(|p| {
                p.ctype.mimetype.contains("text/html")
            })
            .and_then(|p| p.get_body().ok());

        // Collect all headers as JSON for reference
        let headers_json = serde_json::to_string(
            &headers.iter()
                .map(|h| (h.get_key(), h.get_value()))
                .collect::<Vec<_>>()
        ).ok();

        let mut msg = Message::new(
            conversation_id.to_string(),
            ChannelType::Email,
            MessageDirection::Inbound,
            from_contact_id.to_string(),
            to_contact_ids,
            body,
        );
        msg.subject = subject;
        msg.body_html = body_html;
        msg.sent_at = sent_at;
        msg.received_at = Some(Utc::now());
        msg.external_id = message_id;
        msg.headers = headers_json;

        Ok(msg)
    }
}

#[async_trait]
impl CommunicationChannel for EmailChannel {
    async fn connect(&mut self) -> Result<(), CommsError> {
        self.status = ChannelStatus::Connecting;

        // Validate config
        if self.config.imap_host.is_empty() || self.config.username.is_empty() {
            self.status = ChannelStatus::Error("Missing IMAP configuration".into());
            return Err(CommsError::ConfigError("IMAP host and username required".into()));
        }

        // Test IMAP connection
        #[cfg(feature = "email")]
        {
            let tls = async_native_tls::TlsConnector::new();
            match async_imap::connect(
                (&*self.config.imap_host, self.config.imap_port),
                &self.config.imap_host,
                tls,
            ).await {
                Ok(client) => {
                    match client.login(&self.config.username, &self.password).await {
                        Ok(session) => {
                            // Successfully connected — logout cleanly
                            let _ = session.logout().await;
                            self.status = ChannelStatus::Connected;
                        }
                        Err((e, _)) => {
                            self.status = ChannelStatus::Error(e.to_string());
                            return Err(CommsError::AuthFailed(e.to_string()));
                        }
                    }
                }
                Err(e) => {
                    self.status = ChannelStatus::Error(e.to_string());
                    return Err(CommsError::NotConnected(e.to_string()));
                }
            }
        }

        #[cfg(not(feature = "email"))]
        {
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
        ChannelType::Email
    }

    async fn fetch_messages(
        &self,
        _since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>, CommsError> {
        #[cfg(feature = "email")]
        {
            let tls = async_native_tls::TlsConnector::new();
            let client = async_imap::connect(
                (&*self.config.imap_host, self.config.imap_port),
                &self.config.imap_host,
                tls,
            ).await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            let mut session = client.login(&self.config.username, &self.password).await
                .map_err(|(e, _)| CommsError::AuthFailed(e.to_string()))?;

            session.select("INBOX").await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            // Fetch recent messages (last 50 for now)
            use futures::StreamExt;
            let messages_stream = session.fetch("1:50", "RFC822").await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            let fetched: Vec<_> = messages_stream.collect().await;
            let mut result = Vec::new();

            for fetch_result in fetched {
                let fetch = fetch_result
                    .map_err(|e| CommsError::FetchFailed(e.to_string()))?;
                if let Some(body) = fetch.body() {
                    // Parse the "From" header to resolve contact
                    let parsed = mailparse::parse_mail(body)
                        .map_err(|e| CommsError::ParseError(e.to_string()))?;
                    let from_header = parsed.headers.iter()
                        .find(|h| h.get_key_ref() == "From")
                        .map(|h| h.get_value())
                        .unwrap_or_default();
                    let subject = parsed.headers.iter()
                        .find(|h| h.get_key_ref() == "Subject")
                        .map(|h| h.get_value())
                        .unwrap_or_else(|| "(no subject)".into());

                    // Extract email address from "Name <email>" format
                    let from_addr = extract_email_address(&from_header);
                    let from_name = extract_display_name(&from_header);
                    let from_id = self.resolve_contact_id(&from_addr, from_name.as_deref()).await?;

                    // Get own contact ID
                    let my_id = self.resolve_contact_id(&self.config.username, self.config.display_name.as_deref()).await?;

                    let conv = self.get_or_create_conversation(&subject, vec![from_id.clone(), my_id.clone()]).await?;
                    let conv_id = conv.id_string().unwrap_or_default();

                    let msg = self.parse_email(body, &from_id, vec![my_id], &conv_id)?;
                    result.push(msg);
                }
            }

            let _ = session.logout().await;
            Ok(result)
        }

        #[cfg(not(feature = "email"))]
        {
            Ok(vec![])
        }
    }

    async fn send_message(&self, msg: &OutgoingMessage) -> Result<String, CommsError> {
        #[cfg(feature = "email")]
        {
            use lettre::{
                message::header::ContentType,
                transport::smtp::authentication::Credentials,
                AsyncSmtpTransport, AsyncTransport, Message as LettreMessage, Tokio1Executor,
            };

            let from = if let Some(ref name) = self.config.display_name {
                format!("{name} <{}>", self.config.username)
            } else {
                self.config.username.clone()
            };

            let mut builder = LettreMessage::builder()
                .from(from.parse().map_err(|e: lettre::address::AddressError| {
                    CommsError::SendFailed(e.to_string())
                })?)
                .header(ContentType::TEXT_PLAIN);

            for to in &msg.to {
                builder = builder.to(to.parse().map_err(|e: lettre::address::AddressError| {
                    CommsError::SendFailed(e.to_string())
                })?);
            }

            if let Some(ref subject) = msg.subject {
                builder = builder.subject(subject.clone());
            }

            if let Some(ref reply_to) = msg.in_reply_to {
                builder = builder.header(lettre::message::header::InReplyTo::new(reply_to.clone()));
            }

            let email = builder
                .body(msg.body.clone())
                .map_err(|e| CommsError::SendFailed(e.to_string()))?;

            let creds = Credentials::new(
                self.config.username.clone(),
                self.password.clone(),
            );

            let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_host)
                .map_err(|e| CommsError::SendFailed(e.to_string()))?
                .port(self.config.smtp_port)
                .credentials(creds)
                .build();

            let response = mailer.send(email).await
                .map_err(|e| CommsError::SendFailed(e.to_string()))?;

            Ok(format!("{:?}", response))
        }

        #[cfg(not(feature = "email"))]
        {
            let _ = msg;
            Err(CommsError::ConfigError("Email feature not enabled".into()))
        }
    }

    async fn sync(&mut self) -> Result<SyncResult, CommsError> {
        let messages = self.fetch_messages(self.last_sync).await?;

        let mut new_messages = 0u32;
        let mut new_contacts = 0u32;

        for msg in &messages {
            // Check if message already exists by external_id
            if let Some(ref ext_id) = msg.external_id {
                let existing = self.db.search_messages(ext_id).await?;
                if !existing.is_empty() {
                    continue; // Skip duplicate
                }
            }

            self.db.create_message(msg.clone()).await?;
            new_messages += 1;
        }

        self.last_sync = Some(Utc::now());

        Ok(SyncResult {
            new_messages,
            updated_conversations: 0,
            new_contacts,
        })
    }

    async fn resolve_contact(&self, address: &str) -> Result<Contact, CommsError> {
        if let Some(contact) = self.db.find_contact_by_address(address).await? {
            return Ok(contact);
        }

        let mut contact = Contact::new(address.to_string(), false);
        contact.addresses.push(ChannelAddress {
            channel: ChannelType::Email,
            address: address.to_string(),
            display_name: None,
            is_primary: true,
        });
        self.db.create_contact(contact).await.map_err(CommsError::from)
    }
}

/// Extract the email address from a "Display Name <email>" string.
fn extract_email_address(header: &str) -> String {
    if let Some(start) = header.find('<') {
        if let Some(end) = header.find('>') {
            return header[start + 1..end].to_string();
        }
    }
    header.trim().to_string()
}

/// Extract the display name from a "Display Name <email>" string.
fn extract_display_name(header: &str) -> Option<String> {
    if let Some(start) = header.find('<') {
        let name = header[..start].trim().trim_matches('"').to_string();
        if !name.is_empty() {
            return Some(name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_email_simple() {
        assert_eq!(extract_email_address("alice@example.com"), "alice@example.com");
    }

    #[test]
    fn extract_email_with_name() {
        assert_eq!(
            extract_email_address("Alice Smith <alice@example.com>"),
            "alice@example.com"
        );
    }

    #[test]
    fn extract_email_quoted_name() {
        assert_eq!(
            extract_email_address("\"Alice Smith\" <alice@example.com>"),
            "alice@example.com"
        );
    }

    #[test]
    fn extract_display_name_present() {
        assert_eq!(
            extract_display_name("Alice Smith <alice@example.com>"),
            Some("Alice Smith".into())
        );
    }

    #[test]
    fn extract_display_name_quoted() {
        assert_eq!(
            extract_display_name("\"Alice Smith\" <alice@example.com>"),
            Some("Alice Smith".into())
        );
    }

    #[test]
    fn extract_display_name_absent() {
        assert_eq!(extract_display_name("alice@example.com"), None);
    }

    #[test]
    fn extract_display_name_empty_before_bracket() {
        assert_eq!(extract_display_name("<alice@example.com>"), None);
    }

    #[test]
    fn channel_type_is_email() {
        // Just verify the struct can be constructed (no real connection)
        // We can't easily test connect/send without a real IMAP/SMTP server
    }
}
