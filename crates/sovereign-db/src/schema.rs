//! Schema definitions for Sovereign OS document graph

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

/// Format a Thing ID as "table:key" without backtick escaping.
pub fn thing_to_raw(t: &Thing) -> String {
    format!("{}:{}", t.tb, t.id)
}

/// Document node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<Thing>,
    pub title: String,
    pub content: String,
    pub thread_id: String,
    pub is_owned: bool,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub spatial_x: f32,
    pub spatial_y: f32,
    #[serde(default)]
    pub head_commit: Option<String>,
    /// Soft-delete timestamp (ISO 8601). None means the document is active.
    #[serde(default)]
    pub deleted_at: Option<String>,
    /// Base64-encoded encryption nonce. None means content is plaintext.
    #[serde(default)]
    pub encryption_nonce: Option<String>,
}

/// Thread (project/topic grouping)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: Option<Thing>,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
    /// Soft-delete timestamp (ISO 8601). None means the thread is active.
    #[serde(default)]
    pub deleted_at: Option<String>,
}

/// Relationship edge between documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedTo {
    pub id: Option<Thing>,
    #[serde(rename = "in")]
    pub in_: Option<Thing>,
    pub out: Option<Thing>,
    pub relation_type: RelationType,
    pub strength: f32,
    pub created_at: DateTime<Utc>,
}

/// Relationship type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    References,
    DerivedFrom,
    Continues,
    Contradicts,
    Supports,
    BranchesFrom,
    ContactOf,
    AttachedTo,
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::References => write!(f, "references"),
            Self::DerivedFrom => write!(f, "derivedfrom"),
            Self::Continues => write!(f, "continues"),
            Self::Contradicts => write!(f, "contradicts"),
            Self::Supports => write!(f, "supports"),
            Self::BranchesFrom => write!(f, "branchesfrom"),
            Self::ContactOf => write!(f, "contactof"),
            Self::AttachedTo => write!(f, "attachedto"),
        }
    }
}

impl std::str::FromStr for RelationType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "references" => Ok(Self::References),
            "derivedfrom" | "derived_from" => Ok(Self::DerivedFrom),
            "continues" => Ok(Self::Continues),
            "contradicts" => Ok(Self::Contradicts),
            "supports" => Ok(Self::Supports),
            "branchesfrom" | "branches_from" => Ok(Self::BranchesFrom),
            "contactof" | "contact_of" => Ok(Self::ContactOf),
            "attachedto" | "attached_to" => Ok(Self::AttachedTo),
            _ => Err(format!("Unknown relation type: {s}")),
        }
    }
}

/// A snapshot of a single document at commit time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentSnapshot {
    pub document_id: String,
    pub title: String,
    pub content: String,
}

/// A per-document version control commit with parent chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: Option<Thing>,
    pub document_id: String,
    pub parent_commit: Option<String>,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub snapshot: DocumentSnapshot,
}

impl Document {
    pub fn new(title: String, thread_id: String, is_owned: bool) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            content: r#"{"body":"","images":[]}"#.to_string(),
            thread_id,
            is_owned,
            created_at: now,
            modified_at: now,
            spatial_x: 0.0,
            spatial_y: 0.0,
            head_commit: None,
            deleted_at: None,
            encryption_nonce: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

impl Commit {
    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

/// A timeline milestone marking a significant point in a thread's history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Milestone {
    pub id: Option<Thing>,
    pub title: String,
    pub timestamp: DateTime<Utc>,
    pub thread_id: String,
    #[serde(default)]
    pub description: String,
}

impl Milestone {
    pub fn new(title: String, thread_id: String, description: String) -> Self {
        Self {
            id: None,
            title,
            timestamp: Utc::now(),
            thread_id,
            description,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

impl Thread {
    pub fn new(name: String, description: String) -> Self {
        Self {
            id: None,
            name,
            description,
            created_at: Utc::now(),
            deleted_at: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

// --- Unified Communications types ---

/// Communication channel type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Email,
    Sms,
    Signal,
    WhatsApp,
    Matrix,
    Phone,
    Custom(String),
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Email => write!(f, "email"),
            Self::Sms => write!(f, "sms"),
            Self::Signal => write!(f, "signal"),
            Self::WhatsApp => write!(f, "whatsapp"),
            Self::Matrix => write!(f, "matrix"),
            Self::Phone => write!(f, "phone"),
            Self::Custom(s) => write!(f, "custom:{s}"),
        }
    }
}

/// Message read status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReadStatus {
    Unread,
    Read,
    Archived,
}

impl Default for ReadStatus {
    fn default() -> Self {
        Self::Unread
    }
}

/// Message direction
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MessageDirection {
    Inbound,
    Outbound,
}

/// A contact's address on a specific channel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelAddress {
    pub channel: ChannelType,
    pub address: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub is_primary: bool,
}

/// Contact node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    pub id: Option<Thing>,
    pub name: String,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub notes: String,
    pub addresses: Vec<ChannelAddress>,
    pub is_owned: bool,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub encryption_nonce: Option<String>,
}

impl Contact {
    pub fn new(name: String, is_owned: bool) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            name,
            avatar: None,
            notes: String::new(),
            addresses: Vec::new(),
            is_owned,
            created_at: now,
            modified_at: now,
            deleted_at: None,
            encryption_nonce: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

/// Message node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: Option<Thing>,
    pub conversation_id: String,
    pub channel: ChannelType,
    pub direction: MessageDirection,
    pub from_contact_id: String,
    pub to_contact_ids: Vec<String>,
    #[serde(default)]
    pub subject: Option<String>,
    pub body: String,
    #[serde(default)]
    pub body_html: Option<String>,
    pub sent_at: DateTime<Utc>,
    #[serde(default)]
    pub received_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub read_status: ReadStatus,
    #[serde(default)]
    pub attachment_doc_ids: Vec<String>,
    #[serde(default)]
    pub external_id: Option<String>,
    #[serde(default)]
    pub headers: Option<String>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub encryption_nonce: Option<String>,
}

impl Message {
    pub fn new(
        conversation_id: String,
        channel: ChannelType,
        direction: MessageDirection,
        from_contact_id: String,
        to_contact_ids: Vec<String>,
        body: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            conversation_id,
            channel,
            direction,
            from_contact_id,
            to_contact_ids,
            subject: None,
            body,
            body_html: None,
            sent_at: now,
            received_at: None,
            read_status: ReadStatus::Unread,
            attachment_doc_ids: Vec::new(),
            external_id: None,
            headers: None,
            created_at: now,
            deleted_at: None,
            encryption_nonce: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

/// Conversation node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conversation {
    pub id: Option<Thing>,
    pub title: String,
    pub channel: ChannelType,
    pub participant_contact_ids: Vec<String>,
    #[serde(default)]
    pub last_message_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub unread_count: u32,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub linked_thread_id: Option<String>,
}

impl Conversation {
    pub fn new(title: String, channel: ChannelType, participant_contact_ids: Vec<String>) -> Self {
        Self {
            id: None,
            title,
            channel,
            participant_contact_ids,
            last_message_at: None,
            unread_count: 0,
            created_at: Utc::now(),
            deleted_at: None,
            linked_thread_id: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn branches_from_display_and_parse() {
        let rt = RelationType::BranchesFrom;
        assert_eq!(rt.to_string(), "branchesfrom");

        let parsed: RelationType = "branchesfrom".parse().unwrap();
        assert_eq!(parsed, RelationType::BranchesFrom);

        let parsed2: RelationType = "branches_from".parse().unwrap();
        assert_eq!(parsed2, RelationType::BranchesFrom);
    }

    #[test]
    fn branches_from_serde_roundtrip() {
        let rt = RelationType::BranchesFrom;
        let json = serde_json::to_string(&rt).unwrap();
        assert_eq!(json, "\"branchesfrom\"");
        let back: RelationType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, RelationType::BranchesFrom);
    }
}
