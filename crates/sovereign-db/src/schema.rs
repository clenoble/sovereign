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
    pub doc_type: DocumentType,
    pub content: String,
    pub thread_id: String,
    pub is_owned: bool,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub spatial_x: f32,
    pub spatial_y: f32,
}

/// Document type classification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DocumentType {
    Markdown,
    Image,
    Pdf,
    Web,
    Data,
    Spreadsheet,
}

impl std::fmt::Display for DocumentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Markdown => write!(f, "markdown"),
            Self::Image => write!(f, "image"),
            Self::Pdf => write!(f, "pdf"),
            Self::Web => write!(f, "web"),
            Self::Data => write!(f, "data"),
            Self::Spreadsheet => write!(f, "spreadsheet"),
        }
    }
}

impl std::str::FromStr for DocumentType {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "markdown" => Ok(Self::Markdown),
            "image" => Ok(Self::Image),
            "pdf" => Ok(Self::Pdf),
            "web" => Ok(Self::Web),
            "data" => Ok(Self::Data),
            "spreadsheet" => Ok(Self::Spreadsheet),
            _ => Err(format!("Unknown document type: {s}")),
        }
    }
}

/// Thread (project/topic grouping)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: Option<Thing>,
    pub name: String,
    pub description: String,
    pub created_at: DateTime<Utc>,
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
    pub doc_type: DocumentType,
}

/// A version control commit — snapshot of all documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Commit {
    pub id: Option<Thing>,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub snapshots: Vec<DocumentSnapshot>,
}

impl Document {
    pub fn new(title: String, doc_type: DocumentType, thread_id: String, is_owned: bool) -> Self {
        let now = Utc::now();
        Self {
            id: None,
            title,
            doc_type,
            content: String::new(),
            thread_id,
            is_owned,
            created_at: now,
            modified_at: now,
            spatial_x: 0.0,
            spatial_y: 0.0,
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
        }
    }

    pub fn id_string(&self) -> Option<String> {
        self.id.as_ref().map(|t| thing_to_raw(t))
    }
}
