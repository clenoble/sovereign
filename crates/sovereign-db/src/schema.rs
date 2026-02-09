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
}

impl std::fmt::Display for RelationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::References => write!(f, "references"),
            Self::DerivedFrom => write!(f, "derivedfrom"),
            Self::Continues => write!(f, "continues"),
            Self::Contradicts => write!(f, "contradicts"),
            Self::Supports => write!(f, "supports"),
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
