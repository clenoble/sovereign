//! Schema definitions for Sovereign OS document graph

use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

/// Document node in the graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: Option<Thing>,
    pub title: String,
    pub doc_type: DocumentType,
    pub content: String,
    pub thread_id: String,
    pub is_owned: bool,
    pub created_at: i64,
    pub modified_at: i64,
    pub spatial_x: f32,
    pub spatial_y: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DocumentType {
    Markdown,
    Image,
    PDF,
    Web,
    Data,
    Spreadsheet,
}

/// Thread (project/topic grouping)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: Option<Thing>,
    pub name: String,
    pub description: String,
    pub created_at: i64,
}

/// Relationship edge between documents
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelatedTo {
    pub id: Option<Thing>,
    #[serde(rename = "in")]
    pub in_: Thing,
    pub out: Thing,
    pub relation_type: RelationType,
    pub strength: f32, // 0.0 - 1.0
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RelationType {
    References,
    DerivedFrom,
    Continues,
    Contradicts,
    Supports,
}

impl Document {
    pub fn new(
        title: String,
        doc_type: DocumentType,
        thread_id: String,
        is_owned: bool,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

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
}

impl Thread {
    pub fn new(name: String, description: String) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Self {
            id: None,
            name,
            description,
            created_at: now,
        }
    }
}
