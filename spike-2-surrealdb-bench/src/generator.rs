//! Test data generator for benchmark

use crate::schema::{Document, DocumentType, RelationType, Thread};
use rand::Rng;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;

const THREAD_NAMES: &[&str] = &[
    "Research",
    "Development",
    "Design",
    "Admin",
    "Documentation",
    "Testing",
    "Marketing",
    "Customer Support",
];

const DOC_TITLES: &[&str] = &[
    "Meeting Notes",
    "Project Plan",
    "Research Summary",
    "Bug Report",
    "Feature Specification",
    "Design Mockup",
    "API Documentation",
    "Test Results",
    "User Feedback",
    "Technical Debt",
];

pub struct DataGenerator {
    rng: rand::rngs::ThreadRng,
}

impl DataGenerator {
    pub fn new() -> Self {
        Self {
            rng: rand::thread_rng(),
        }
    }

    /// Generate threads
    pub fn generate_threads(&mut self, count: usize) -> Vec<Thread> {
        (0..count)
            .map(|i| {
                let name = if i < THREAD_NAMES.len() {
                    THREAD_NAMES[i].to_string()
                } else {
                    format!("Thread {}", i)
                };
                Thread::new(name, format!("Description for thread {}", i))
            })
            .collect()
    }

    /// Generate documents with realistic distribution
    pub fn generate_documents(&mut self, count: usize, thread_ids: &[String]) -> Vec<Document> {
        (0..count)
            .map(|i| {
                let thread_id = thread_ids[i % thread_ids.len()].clone();
                let title = format!(
                    "{} #{}",
                    DOC_TITLES[i % DOC_TITLES.len()],
                    i / DOC_TITLES.len()
                );

                let doc_type = match i % 6 {
                    0 => DocumentType::Markdown,
                    1 => DocumentType::Image,
                    2 => DocumentType::PDF,
                    3 => DocumentType::Web,
                    4 => DocumentType::Data,
                    _ => DocumentType::Spreadsheet,
                };

                let is_owned = self.rng.gen_bool(0.7); // 70% owned, 30% external

                let mut doc = Document::new(title, doc_type, thread_id, is_owned);

                // Generate spatial coordinates
                doc.spatial_x = (i % 20) as f32 * 220.0;
                doc.spatial_y = (i / 20) as f32 * 150.0;

                // Generate realistic content
                doc.content = self.generate_content(i);

                doc
            })
            .collect()
    }

    fn generate_content(&mut self, seed: usize) -> String {
        let paragraphs = 1 + (seed % 5);
        (0..paragraphs)
            .map(|p| {
                format!(
                    "Lorem ipsum dolor sit amet, paragraph {}. Document seed {}.",
                    p, seed
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Insert threads into database
    pub async fn insert_threads(
        db: &Surreal<Db>,
        threads: Vec<Thread>,
    ) -> anyhow::Result<Vec<String>> {
        let mut thread_ids = Vec::new();

        for thread in threads {
            let created: Option<Thread> = db.create("thread").content(thread).await?;
            if let Some(t) = created {
                if let Some(id) = &t.id {
                    thread_ids.push(id.to_string());
                }
            }
        }

        Ok(thread_ids)
    }

    /// Insert documents into database in batches
    pub async fn insert_documents(
        db: &Surreal<Db>,
        documents: Vec<Document>,
        batch_size: usize,
    ) -> anyhow::Result<Vec<String>> {
        let mut doc_ids = Vec::new();

        for doc in documents {
            let created: Option<Document> = db.create("document").content(doc).await?;
            if let Some(d) = created {
                if let Some(id) = &d.id {
                    doc_ids.push(id.to_string());
                }
            }
        }

        Ok(doc_ids)
    }

    /// Create relationships between documents
    pub async fn create_relationships(
        &mut self,
        db: &Surreal<Db>,
        doc_ids: &[String],
        relationships_per_doc: usize,
    ) -> anyhow::Result<usize> {
        let mut count = 0;

        for (i, from_id) in doc_ids.iter().enumerate() {
            let num_relations = self.rng.gen_range(1..=relationships_per_doc);

            for _ in 0..num_relations {
                // Pick a random target document
                let to_idx = self.rng.gen_range(0..doc_ids.len());
                if to_idx == i {
                    continue; // Skip self-references
                }
                let to_id = &doc_ids[to_idx];

                let relation_type = match self.rng.gen_range(0..5) {
                    0 => RelationType::References,
                    1 => RelationType::DerivedFrom,
                    2 => RelationType::Continues,
                    3 => RelationType::Contradicts,
                    _ => RelationType::Supports,
                };

                let strength = self.rng.gen_range(0.1..1.0);

                // Use SurrealQL RELATE statement
                let query = format!(
                    "RELATE {}->related_to->{} SET relation_type = $type, strength = $strength, created_at = $created",
                    from_id, to_id
                );

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64;

                db.query(&query)
                    .bind(("type", serde_json::to_string(&relation_type)?))
                    .bind(("strength", strength))
                    .bind(("created", now))
                    .await?;

                count += 1;
            }
        }

        Ok(count)
    }
}
