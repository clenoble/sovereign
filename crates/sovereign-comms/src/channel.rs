use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sovereign_db::schema::{ChannelType, Contact, Message};

use crate::error::CommsError;

/// Connection status of a communication channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChannelStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

/// An outgoing message to be sent via a channel.
#[derive(Debug, Clone)]
pub struct OutgoingMessage {
    pub to: Vec<String>,
    pub subject: Option<String>,
    pub body: String,
    pub body_html: Option<String>,
    pub in_reply_to: Option<String>,
    /// Conversation to attribute this message to (for persistence).
    pub conversation_id: Option<String>,
}

/// Result of a sync operation.
#[derive(Debug, Clone)]
pub struct SyncResult {
    pub new_messages: u32,
    pub updated_conversations: u32,
    pub new_contacts: u32,
}

/// Abstraction over a communication channel (email, SMS, etc.).
#[async_trait]
pub trait CommunicationChannel: Send + Sync {
    /// Connect to the remote service.
    async fn connect(&mut self) -> Result<(), CommsError>;

    /// Disconnect from the remote service.
    async fn disconnect(&mut self) -> Result<(), CommsError>;

    /// Current connection status.
    fn status(&self) -> ChannelStatus;

    /// The channel type this implementation handles.
    fn channel_type(&self) -> ChannelType;

    /// Fetch new messages since the given timestamp.
    async fn fetch_messages(
        &self,
        since: Option<DateTime<Utc>>,
    ) -> Result<Vec<Message>, CommsError>;

    /// Send a message, returning the external message ID.
    async fn send_message(&self, msg: &OutgoingMessage) -> Result<String, CommsError>;

    /// Perform a full sync cycle: fetch new messages, update conversations.
    async fn sync(&mut self) -> Result<SyncResult, CommsError>;

    /// Resolve an address string to a Contact (create stub if needed).
    async fn resolve_contact(&self, address: &str) -> Result<Contact, CommsError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_status_eq() {
        assert_eq!(ChannelStatus::Connected, ChannelStatus::Connected);
        assert_ne!(ChannelStatus::Connected, ChannelStatus::Disconnected);
    }

    #[test]
    fn outgoing_message_clone() {
        let msg = OutgoingMessage {
            to: vec!["alice@example.com".into()],
            subject: Some("Test".into()),
            body: "Hello".into(),
            body_html: None,
            in_reply_to: None,
            conversation_id: None,
        };
        let cloned = msg.clone();
        assert_eq!(cloned.to, msg.to);
        assert_eq!(cloned.body, msg.body);
    }

    #[test]
    fn sync_result_clone() {
        let result = SyncResult {
            new_messages: 5,
            updated_conversations: 2,
            new_contacts: 1,
        };
        let cloned = result.clone();
        assert_eq!(cloned.new_messages, 5);
    }
}
