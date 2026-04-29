//! Schema definitions for Sovereign GE document graph

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

/// Format a Thing ID as "table:key" without backtick escaping.
///
/// Uses `Id::to_raw()` to extract the unescaped key — the default `Display`
/// impl on `Id` quotes special characters (e.g. colons) with backticks,
/// which would break round-trips with `raw_to_thing` and produce surprising
/// IDs in frontend DTOs.
pub fn thing_to_raw(t: &Thing) -> String {
    format!("{}:{}", t.tb, t.id.to_raw())
}

/// Parse a `"table:key"` string back into a Thing. Splits on the first
/// colon; everything after it is the key. Returns None if the input has
/// no colon, an empty table, or an empty key.
pub fn raw_to_thing(s: &str) -> Option<Thing> {
    let (table, key) = s.split_once(':')?;
    if table.is_empty() || key.is_empty() {
        return None;
    }
    Some(Thing::from((table.to_string(), key.to_string())))
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
    /// Source URL if this document was fetched from the web.
    #[serde(default)]
    pub source_url: Option<String>,
    /// Content classification: "Factual", "Opinion", or "Fiction".
    #[serde(default)]
    pub reliability_classification: Option<String>,
    /// Reliability score (0.0–5.0, CRABE scale).
    #[serde(default)]
    pub reliability_score: Option<f32>,
    /// JSON array of rubric scores: [{indicator, analysis, score}].
    #[serde(default)]
    pub reliability_assessment: Option<String>,
    /// When the reliability assessment was last run.
    #[serde(default)]
    pub assessed_at: Option<DateTime<Utc>>,
    // --- PII pipeline fields (see doc/plans/pii-management-dashboard.md) ---
    /// Base64-encoded ciphertext of the original (pre-tokenization) body.
    /// `content.body` holds the canonical (tokenized) form; this preserves
    /// the original for L3-gated reveal, surviving LLM-NER false positives.
    #[serde(default)]
    pub body_raw_encrypted: Option<String>,
    /// Base64-encoded XChaCha20 nonce paired with `body_raw_encrypted`.
    #[serde(default)]
    pub body_raw_nonce: Option<String>,
    /// When this document was last processed by the PII pipeline. None means
    /// the document has not yet been scanned.
    #[serde(default)]
    pub pii_scanned_at: Option<DateTime<Utc>>,
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

// --- AI-suggested links ---

/// Source of a suggested link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionSource {
    /// Background memory consolidation
    Consolidation,
    /// Suggested during a chat interaction
    Chat,
}

/// Lifecycle status of a suggested link.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SuggestionStatus {
    Pending,
    Accepted,
    Dismissed,
}

/// AI-suggested relationship between two documents.
///
/// Stored in a separate `suggested_link` edge table, structurally distinct
/// from user-created `related_to` edges. Carries provenance (source, rationale)
/// and lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestedLink {
    pub id: Option<Thing>,
    #[serde(rename = "in")]
    pub in_: Option<Thing>,
    pub out: Option<Thing>,
    pub relation_type: RelationType,
    pub strength: f32,
    /// LLM's explanation of why these documents are related.
    pub rationale: String,
    pub source: SuggestionSource,
    pub status: SuggestionStatus,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}

impl SuggestedLink {
    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
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
            source_url: None,
            reliability_classification: None,
            reliability_score: None,
            reliability_assessment: None,
            assessed_at: None,
            body_raw_encrypted: None,
            body_raw_nonce: None,
            pii_scanned_at: None,
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
    /// Entity this contact belongs to (e.g. an employee at an org).
    /// None means the contact is unassigned.
    #[serde(default)]
    pub entity_id: Option<String>,
    /// When this contact was last processed by the PII pipeline.
    #[serde(default)]
    pub pii_scanned_at: Option<DateTime<Utc>>,
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
            entity_id: None,
            pii_scanned_at: None,
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
    // --- PII pipeline fields ---
    /// Base64-encoded ciphertext of the original (pre-tokenization) body.
    /// `body` holds the canonical (tokenized) form.
    #[serde(default)]
    pub body_raw_encrypted: Option<String>,
    /// Base64-encoded XChaCha20 nonce paired with `body_raw_encrypted`.
    #[serde(default)]
    pub body_raw_nonce: Option<String>,
    /// When this message was last processed by the PII pipeline.
    #[serde(default)]
    pub pii_scanned_at: Option<DateTime<Utc>>,
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
            body_raw_encrypted: None,
            body_raw_nonce: None,
            pii_scanned_at: None,
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

// === PII Management & Dashboard schemas ===
//
// See doc/plans/pii-management-dashboard.md for the design.
// Invariant: at-rest, document/message bodies hold reference tokens of the
// form `[pii:<record_id>]`; raw values live encrypted in `PiiRecord` and in
// the per-source `body_raw_encrypted` / `body_raw_nonce` blobs.

/// Kind of business / personal entity that aggregates PII, contacts,
/// stored secrets, and sharing records.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum EntityKind {
    /// The user themselves — sentinel entity for self-PII (own bank account,
    /// own passport, etc.).
    #[serde(rename = "self")]
    SelfEntity,
    /// An organization or business (Acme Corp, my bank, my insurance).
    Org,
    /// A person who isn't an organization (a doctor, a friend).
    Person,
    /// A service or platform (GitHub, an API provider, a SaaS).
    Service,
}

impl Default for EntityKind {
    fn default() -> Self {
        Self::Org
    }
}

/// Aggregating node: a business entity, organization, person, or service that
/// PII belongs to or is shared with. The organizing axis of the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id: Option<Thing>,
    pub name: String,
    pub kind: EntityKind,
    /// Domains associated with this entity (e.g. `acme.com`, `acme.ch`).
    /// Used for auto-attribution when scanning email addresses or browser URLs.
    #[serde(default)]
    pub domains: Vec<String>,
    /// Linked Contact node IDs (raw `contact:abc` strings).
    #[serde(default)]
    pub contact_ids: Vec<String>,
    #[serde(default)]
    pub notes: String,
    pub is_owned: bool,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

impl Entity {
    pub fn new(name: String, kind: EntityKind) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            name,
            kind,
            domains: Vec::new(),
            contact_ids: Vec::new(),
            notes: String::new(),
            is_owned: true,
            created_at: now,
            modified_at: now,
            deleted_at: None,
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

/// Category of a PII finding or stored secret.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PiiKind {
    Email,
    Phone,
    /// US Social Security Number.
    Ssn,
    CreditCard,
    Ipv4,
    /// Swiss AVS / AHV social-insurance number.
    Avs,
    /// ISO 13616 international bank account number.
    Iban,
    Passport,
    /// Date of birth.
    Dob,
    Address,
    PersonName,
    OrgName,
    Password,
    ApiToken,
    BankAccount,
    /// Free-form government-issued ID (national ID card, driver's licence, etc.).
    DocumentId,
    /// Free-form vault note.
    Note,
    Other,
}

/// What kind of node a `SourceRef` points at.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Document,
    Message,
    Contact,
    /// File-based session log (orchestrator chat history). source_id
    /// is a synthetic `"session:<rfc3339>"` since the log is JSONL,
    /// not a SurrealDB row.
    SessionLog,
    /// Vault entries created directly by the user have no body source.
    /// Reserved for future use.
    UserInput,
}

/// Reference back to a span in source content where a PII finding was
/// discovered. Offsets are into the source's tokenized canonical body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceRef {
    pub source_kind: SourceKind,
    pub source_id: String,
    /// UTF-8 byte offset of the reference token's start.
    pub span_start: usize,
    /// UTF-8 byte offset of the reference token's end (exclusive).
    pub span_end: usize,
}

/// Lifecycle of a PII finding from detection through user review.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReviewState {
    /// Detected but not confirmed by the user (typically below NER confidence
    /// threshold, or new-domain entity awaiting confirmation).
    Unreviewed,
    /// User confirmed this is real PII attributable to this entity.
    Confirmed,
    /// User dismissed as a false positive.
    Dismissed,
}

impl Default for ReviewState {
    fn default() -> Self {
        Self::Unreviewed
    }
}

/// A piece of personal information — either discovered during scanning
/// (`stored_secret = false`) or deliberately stored by the user
/// (`stored_secret = true`, the KeePass role). Value is encrypted at rest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiiRecord {
    pub id: Option<Thing>,
    pub kind: PiiKind,
    /// Base64-encoded ciphertext of the PII value (XChaCha20-Poly1305 AEAD).
    pub value_encrypted: String,
    /// Base64-encoded 24-byte XChaCha20 nonce.
    pub value_nonce: String,
    /// Optional human label (e.g. "main bank password"). Plaintext.
    #[serde(default)]
    pub label: Option<String>,
    /// Entity this PII belongs to or is associated with.
    #[serde(default)]
    pub entity_id: Option<String>,
    /// True = user-entered vault entry. False = discovered via scan.
    #[serde(default)]
    pub stored_secret: bool,
    /// Detection confidence (1.0 for regex, 0.0–1.0 for LLM-NER).
    /// Vault entries default to 1.0.
    pub confidence: f32,
    /// All source locations referencing this record. Vault entries default to
    /// empty.
    #[serde(default)]
    pub sources: Vec<SourceRef>,
    pub discovered_at: DateTime<Utc>,
    /// Last time the value was decrypted and shown to the user.
    #[serde(default)]
    pub last_revealed_at: Option<DateTime<Utc>>,
    /// Number of times the value has been used (e.g. autofilled into a form).
    /// Useful staleness signal.
    #[serde(default)]
    pub use_count: u32,
    pub review_state: ReviewState,
    #[serde(default)]
    pub deleted_at: Option<String>,
}

impl PiiRecord {
    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

/// Channel through which a PII disclosure occurred.
///
/// Broader than `ChannelType` because signup-via-web is a disclosure that
/// doesn't correspond to any `Message`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ShareChannel {
    Email,
    Sms,
    Signal,
    WhatsApp,
    Matrix,
    Phone,
    /// Web form submission (signup, contact form, etc.).
    Web,
    /// Manual log entry by the user (e.g. recording a phone disclosure).
    Other,
}

/// Edge: PiiRecord → Entity. Records that a PII value was disclosed to an
/// entity at a moment in time. Always outbound; receiving PII isn't tracked
/// here (those become `PiiRecord`s with `entity_id = sender`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareRecord {
    pub id: Option<Thing>,
    pub pii_record_id: String,
    pub to_entity_id: String,
    /// Optional Message that carried the disclosure. None for web-form or
    /// manual disclosures.
    #[serde(default)]
    pub via_message_id: Option<String>,
    /// Optional URL where the disclosure happened (e.g. signup form URL).
    #[serde(default)]
    pub via_url: Option<String>,
    pub shared_at: DateTime<Utc>,
    pub channel: ShareChannel,
}

impl ShareRecord {
    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_to_thing_round_trip() {
        let t = raw_to_thing("entity:abc123").unwrap();
        assert_eq!(thing_to_raw(&t), "entity:abc123");
    }

    #[test]
    fn raw_to_thing_rejects_malformed() {
        assert!(raw_to_thing("noseparator").is_none());
        assert!(raw_to_thing(":noTable").is_none());
        assert!(raw_to_thing("noKey:").is_none());
    }

    #[test]
    fn raw_to_thing_handles_keys_with_colons() {
        // Keys can themselves contain colons; split_once is correct here.
        let t = raw_to_thing("entity:abc:def").unwrap();
        assert_eq!(thing_to_raw(&t), "entity:abc:def");
    }

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

    // === PII schema tests ===

    #[test]
    fn entity_kind_self_serializes_lowercase() {
        // The variant is named `SelfEntity` to avoid the reserved keyword,
        // but it must serialize as "self" to match user-facing semantics.
        let json = serde_json::to_string(&EntityKind::SelfEntity).unwrap();
        assert_eq!(json, "\"self\"");
        let back: EntityKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, EntityKind::SelfEntity);
    }

    #[test]
    fn entity_kind_default_is_org() {
        assert_eq!(EntityKind::default(), EntityKind::Org);
    }

    #[test]
    fn entity_new_sets_sane_defaults() {
        let e = Entity::new("Acme Corp".to_string(), EntityKind::Org);
        assert_eq!(e.name, "Acme Corp");
        assert_eq!(e.kind, EntityKind::Org);
        assert!(e.domains.is_empty());
        assert!(e.contact_ids.is_empty());
        assert!(e.is_owned);
        assert!(e.deleted_at.is_none());
    }

    #[test]
    fn pii_kind_avs_iban_round_trip() {
        let json = serde_json::to_string(&PiiKind::Avs).unwrap();
        assert_eq!(json, "\"avs\"");
        let json2 = serde_json::to_string(&PiiKind::Iban).unwrap();
        assert_eq!(json2, "\"iban\"");
        let back: PiiKind = serde_json::from_str("\"credit_card\"").unwrap();
        assert_eq!(back, PiiKind::CreditCard);
    }

    #[test]
    fn review_state_default_is_unreviewed() {
        assert_eq!(ReviewState::default(), ReviewState::Unreviewed);
    }

    #[test]
    fn pii_record_round_trip() {
        let rec = PiiRecord {
            id: None,
            kind: PiiKind::Email,
            value_encrypted: "ZXhhbXBsZQ==".into(),
            value_nonce: "bm9uY2U=".into(),
            label: Some("work email".into()),
            entity_id: Some("entity:acme".into()),
            stored_secret: false,
            confidence: 1.0,
            sources: vec![SourceRef {
                source_kind: SourceKind::Document,
                source_id: "document:abc".into(),
                span_start: 10,
                span_end: 30,
            }],
            discovered_at: Utc::now(),
            last_revealed_at: None,
            use_count: 0,
            review_state: ReviewState::Confirmed,
            deleted_at: None,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: PiiRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, PiiKind::Email);
        assert_eq!(back.label.as_deref(), Some("work email"));
        assert_eq!(back.sources.len(), 1);
        assert_eq!(back.sources[0].span_start, 10);
        assert_eq!(back.review_state, ReviewState::Confirmed);
    }

    #[test]
    fn share_channel_web_round_trip() {
        let json = serde_json::to_string(&ShareChannel::Web).unwrap();
        assert_eq!(json, "\"web\"");
        let back: ShareChannel = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ShareChannel::Web);
    }

    #[test]
    fn share_record_round_trip_without_message() {
        // Web-form disclosures have no Message but should still serialize.
        let rec = ShareRecord {
            id: None,
            pii_record_id: "pii_record:xyz".into(),
            to_entity_id: "entity:acme".into(),
            via_message_id: None,
            via_url: Some("https://acme.com/signup".into()),
            shared_at: Utc::now(),
            channel: ShareChannel::Web,
        };
        let json = serde_json::to_string(&rec).unwrap();
        let back: ShareRecord = serde_json::from_str(&json).unwrap();
        assert_eq!(back.channel, ShareChannel::Web);
        assert!(back.via_message_id.is_none());
        assert_eq!(back.via_url.as_deref(), Some("https://acme.com/signup"));
    }

    #[test]
    fn document_new_defaults_pii_fields_to_none() {
        let doc = Document::new("test".into(), "thread:t1".into(), true);
        assert!(doc.body_raw_encrypted.is_none());
        assert!(doc.body_raw_nonce.is_none());
        assert!(doc.pii_scanned_at.is_none());
    }

    #[test]
    fn message_new_defaults_pii_fields_to_none() {
        let msg = Message::new(
            "conv:c1".into(),
            ChannelType::Email,
            MessageDirection::Outbound,
            "contact:from".into(),
            vec!["contact:to".into()],
            "hello".into(),
        );
        assert!(msg.body_raw_encrypted.is_none());
        assert!(msg.body_raw_nonce.is_none());
        assert!(msg.pii_scanned_at.is_none());
    }

    #[test]
    fn contact_new_defaults_pii_fields_to_none() {
        let c = Contact::new("Alice".into(), false);
        assert!(c.entity_id.is_none());
        assert!(c.pii_scanned_at.is_none());
    }

    #[test]
    fn document_deserializes_old_payload_without_pii_fields() {
        // Backward-compat: existing on-disk documents without PII fields must
        // deserialize cleanly with the new fields defaulted.
        let old_json = r#"{
            "id": null,
            "title": "Legacy",
            "content": "{}",
            "thread_id": "thread:1",
            "is_owned": true,
            "created_at": "2026-01-01T00:00:00Z",
            "modified_at": "2026-01-01T00:00:00Z",
            "spatial_x": 0.0,
            "spatial_y": 0.0
        }"#;
        let doc: Document = serde_json::from_str(old_json).unwrap();
        assert_eq!(doc.title, "Legacy");
        assert!(doc.body_raw_encrypted.is_none());
        assert!(doc.pii_scanned_at.is_none());
    }
}
