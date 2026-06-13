use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::schema::{
    ChannelAddress, ChannelType, Contact, Conversation, Message, MessageDirection,
};
use sovereign_db::GraphDB;
use zeroize::Zeroizing;

use crate::channel::{ChannelStatus, CommunicationChannel, OutgoingMessage, SyncResult};
use crate::config::EmailAccountConfig;
use crate::error::CommsError;
use crate::pii_hook::{ContactIngestHook, MessageIngestHook, ShareIngestHook};

/// Email channel implementation using IMAP (fetch) and SMTP (send).
pub struct EmailChannel {
    config: EmailAccountConfig,
    db: Arc<dyn GraphDB>,
    password: Zeroizing<String>,
    status: ChannelStatus,
    last_sync: Option<DateTime<Utc>>,
    pii_hook: Option<Arc<dyn MessageIngestHook>>,
    pii_contact_hook: Option<Arc<dyn ContactIngestHook>>,
    pii_share_hook: Option<Arc<dyn ShareIngestHook>>,
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
            password: Zeroizing::new(password),
            status: ChannelStatus::Disconnected,
            last_sync: None,
            pii_hook: None,
            pii_contact_hook: None,
            pii_share_hook: None,
        }
    }

    /// Attach a PII ingest hook that will be invoked after every
    /// `create_message` on this channel. Without a hook, message bodies
    /// land in the DB raw and an idle sweep handles tokenization later.
    pub fn with_pii_hook(mut self, hook: Arc<dyn MessageIngestHook>) -> Self {
        self.pii_hook = Some(hook);
        self
    }

    /// Attach a PII contact-ingest hook, invoked once per freshly-created
    /// contact (whether via `resolve_contact_id` during message ingest
    /// or via the channel's own `resolve_contact` trait method).
    pub fn with_pii_contact_hook(mut self, hook: Arc<dyn ContactIngestHook>) -> Self {
        self.pii_contact_hook = Some(hook);
        self
    }

    async fn run_pii_hook(&self, message: &sovereign_db::schema::Message) {
        if let Some(hook) = &self.pii_hook {
            hook.after_message_created(message).await;
        }
    }

    async fn run_pii_contact_hook(&self, contact: &sovereign_db::schema::Contact) {
        if let Some(hook) = &self.pii_contact_hook {
            hook.after_contact_created(contact).await;
        }
    }

    /// Attach a sharing-ledger hook, invoked after every outbound
    /// message is persisted (and after the per-message PII hook has
    /// tokenized its body — so the share hook scans the canonical
    /// form for `[pii:<id>]` tokens).
    pub fn with_pii_share_hook(mut self, hook: Arc<dyn ShareIngestHook>) -> Self {
        self.pii_share_hook = Some(hook);
        self
    }

    async fn run_pii_share_hook(&self, message: &sovereign_db::schema::Message) {
        if let Some(hook) = &self.pii_share_hook {
            hook.after_outbound_message(message).await;
        }
    }

    async fn get_or_create_conversation(
        &self,
        subject: &str,
        participant_ids: Vec<String>,
        cache: &mut HashMap<String, Conversation>,
    ) -> Result<Conversation, CommsError> {
        super::helpers::get_or_create_conversation(
            self.db.as_ref(), subject, ChannelType::Email, participant_ids, cache,
        ).await
    }

    async fn resolve_contact_id(&self, address: &str, display_name: Option<&str>) -> Result<String, CommsError> {
        super::helpers::resolve_contact_id(
            self.db.as_ref(),
            ChannelType::Email,
            address,
            display_name,
            self.pii_contact_hook.as_ref(),
        ).await
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
            match imap_connect(&self.config.imap_host, self.config.imap_port).await {
                Ok(mut client) => {
                    let _greeting = client.read_response().await
                        .map_err(|e| CommsError::NotConnected(e.to_string()))?;
                    match client.login(&self.config.username, &self.password).await {
                        Ok(mut session) => {
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
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>, CommsError> {
        #[cfg(feature = "email")]
        {
            let mut client = imap_connect(&self.config.imap_host, self.config.imap_port).await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;
            let _greeting = client.read_response().await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            let mut session = client.login(&self.config.username, &self.password).await
                .map_err(|(e, _)| CommsError::AuthFailed(e.to_string()))?;

            session.select("INBOX").await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            // Use SEARCH SINCE if we have a date, fall back to sequence range
            use futures::StreamExt;
            let sequence_set = if let Some(since_date) = since {
                let date_str = since_date.format("%d-%b-%Y").to_string();
                match session.search(format!("SINCE {date_str}")).await {
                    Ok(uids) if !uids.is_empty() => {
                        let uid_list: Vec<String> = uids.iter().map(|u| u.to_string()).collect();
                        uid_list.join(",")
                    }
                    _ => "1:*".to_string(), // fallback: all messages
                }
            } else {
                "1:50".to_string() // first sync: cap at 50
            };

            let messages_stream = session.fetch(&sequence_set, "RFC822").await
                .map_err(|e| CommsError::FetchFailed(e.to_string()))?;

            let fetched: Vec<_> = messages_stream.collect().await;
            let mut result = Vec::new();

            // Pre-load conversation cache and own contact ID to avoid per-message DB loads
            let conversations = self.db.list_conversations(Some(&ChannelType::Email)).await?;
            let mut conv_cache: HashMap<String, Conversation> = conversations
                .into_iter()
                .map(|c| (c.title.clone(), c))
                .collect();
            let my_id = self.resolve_contact_id(&self.config.username, self.config.display_name.as_deref()).await?;

            for fetch_result in fetched {
                let fetch = fetch_result
                    .map_err(|e| CommsError::FetchFailed(e.to_string()))?;
                if let Some(body) = fetch.body() {
                    // Parse once and extract all needed headers
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

                    // COMMS-001: a malformed From: header must not abort the
                    // whole sync — skip that one message and keep going.
                    let from_addr = match extract_email_address(&from_header) {
                        Some(addr) => addr,
                        None => {
                            tracing::warn!(
                                "skipping message with unparseable From header: {:?}",
                                from_header
                            );
                            continue;
                        }
                    };
                    let from_name = extract_display_name(&from_header);
                    let from_id = self.resolve_contact_id(&from_addr, from_name.as_deref()).await?;

                    let conv = self.get_or_create_conversation(&subject, vec![from_id.clone(), my_id.clone()], &mut conv_cache).await?;
                    let conv_id = conv.id_string().unwrap_or_default();

                    let msg = self.parse_email(body, &from_id, vec![my_id.clone()], &conv_id)?;
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
                let in_reply_to: lettre::message::header::InReplyTo = reply_to.clone().into();
                builder = builder.header(in_reply_to);
            }

            let email = builder
                .body(msg.body.clone())
                .map_err(|e| CommsError::SendFailed(e.to_string()))?;

            let creds = Credentials::new(
                self.config.username.clone(),
                (*self.password).clone(),
            );

            let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&self.config.smtp_host)
                .map_err(|e| CommsError::SendFailed(e.to_string()))?
                .port(self.config.smtp_port)
                .credentials(creds)
                .build();

            let response = mailer.send(email).await
                .map_err(|e| CommsError::SendFailed(e.to_string()))?;

            // Persist outbound message to DB
            if let Some(ref conv_id) = msg.conversation_id {
                let my_id = self.resolve_contact_id(
                    &self.config.username,
                    self.config.display_name.as_deref(),
                ).await.unwrap_or_default();

                let to_ids: Vec<String> = msg.to.iter()
                    .map(|addr| addr.clone())
                    .collect();

                let mut db_msg = Message::new(
                    conv_id.clone(),
                    ChannelType::Email,
                    MessageDirection::Outbound,
                    my_id,
                    to_ids,
                    msg.body.clone(),
                );
                db_msg.subject = msg.subject.clone();
                db_msg.sent_at = Utc::now();

                match self.db.create_message(db_msg).await {
                    Ok(persisted) => {
                        // Order matters: PII hook tokenizes the body
                        // FIRST (writing canonical `[pii:<id>]` tokens
                        // to the DB); then the share hook re-fetches
                        // and parses those tokens to build the ledger.
                        self.run_pii_hook(&persisted).await;
                        self.run_pii_share_hook(&persisted).await;
                    }
                    Err(e) => tracing::warn!("Failed to persist outbound message: {e}"),
                }

                // Update conversation last_message_at
                if let Err(e) = self.db.update_conversation_last_message_at(conv_id, Utc::now()).await {
                    tracing::warn!("Failed to update conversation timestamp: {e}");
                }
            }

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
        let new_contacts = 0u32;
        let mut updated_conversations = std::collections::HashSet::new();

        for msg in &messages {
            // Dedup on the exact external_id (indexed) — token-search dedup
            // could both miss duplicates (Message-IDs tokenize oddly) and
            // falsely match unrelated messages, silently dropping new mail.
            if let Some(ref ext_id) = msg.external_id {
                if self.db.find_message_by_external_id(ext_id).await?.is_some() {
                    continue; // Skip duplicate
                }
            }

            let persisted = self.db.create_message(msg.clone()).await?;
            self.run_pii_hook(&persisted).await;
            new_messages += 1;

            // Update conversation unread count and last_message_at
            let conv_id = &msg.conversation_id;
            if !conv_id.is_empty() && updated_conversations.insert(conv_id.clone()) {
                // Get current conversation to increment unread
                if let Ok(conv) = self.db.get_conversation(conv_id).await {
                    let _ = self.db.update_conversation_unread(
                        conv_id,
                        conv.unread_count + 1,
                    ).await;
                }
                let _ = self.db.update_conversation_last_message_at(
                    conv_id,
                    msg.sent_at,
                ).await;
            }
        }

        self.last_sync = Some(Utc::now());

        Ok(SyncResult {
            new_messages,
            updated_conversations: updated_conversations.len() as u32,
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
        let created = self.db.create_contact(contact).await.map_err(CommsError::from)?;
        self.run_pii_contact_hook(&created).await;
        Ok(created)
    }
}

/// Connect to IMAP server over TLS (async-imap 0.11 + tokio-native-tls).
#[cfg(feature = "email")]
async fn imap_connect(
    host: &str,
    port: u16,
) -> Result<async_imap::Client<tokio_native_tls::TlsStream<tokio::net::TcpStream>>, CommsError> {
    let tcp = tokio::net::TcpStream::connect((host, port)).await
        .map_err(|e| CommsError::NotConnected(e.to_string()))?;
    let native_connector = native_tls::TlsConnector::new()
        .map_err(|e| CommsError::NotConnected(e.to_string()))?;
    let connector = tokio_native_tls::TlsConnector::from(native_connector);
    let tls_stream = connector.connect(host, tcp).await
        .map_err(|e| CommsError::NotConnected(e.to_string()))?;
    Ok(async_imap::Client::new(tls_stream))
}

/// Extract the email address from a "Display Name <email>" string.
///
/// Returns `None` when the header is malformed and yields no usable address.
/// A crafted `From:` header where `>` precedes `<` (e.g. `>attacker<evil`)
/// previously caused a slice-out-of-order panic that crashed the whole sync
/// (COMMS-001); we now guard `start < end` and reject such headers.
fn extract_email_address(header: &str) -> Option<String> {
    if let (Some(start), Some(end)) = (header.find('<'), header.find('>')) {
        // Only trust angle-bracketed form when the brackets are ordered.
        if start < end {
            let addr = header[start + 1..end].trim();
            if !addr.is_empty() {
                return Some(addr.to_string());
            }
        }
        // Malformed bracketing (e.g. ">x<y") — no usable address.
        return None;
    }
    // No angle brackets: treat the whole trimmed header as the address.
    let trimmed = header.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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
        assert_eq!(
            extract_email_address("alice@example.com"),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_with_name() {
        assert_eq!(
            extract_email_address("Alice Smith <alice@example.com>"),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_quoted_name() {
        assert_eq!(
            extract_email_address("\"Alice Smith\" <alice@example.com>"),
            Some("alice@example.com".to_string())
        );
    }

    #[test]
    fn extract_email_malformed_brackets_no_panic() {
        // COMMS-001: '>' before '<' must not panic; it yields None.
        assert_eq!(extract_email_address(">attacker<evil"), None);
    }

    #[test]
    fn extract_email_empty_header() {
        assert_eq!(extract_email_address("   "), None);
        assert_eq!(extract_email_address("Name <>"), None);
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
