use std::sync::Arc;

use sovereign_core::content::ContentFields;
use sovereign_db::schema::Document;
use sovereign_db::surreal::SurrealGraphDB;
use sovereign_db::GraphDB;

use crate::traits::{CoreSkill, SkillDocument, SkillOutput};

pub struct FileImportSkill {
    db: Arc<SurrealGraphDB>,
}

impl FileImportSkill {
    pub fn new(db: Arc<SurrealGraphDB>) -> Self {
        Self { db }
    }
}

impl CoreSkill for FileImportSkill {
    fn name(&self) -> &str {
        "file-import"
    }

    fn activate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn deactivate(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    fn execute(
        &self,
        action: &str,
        _doc: &SkillDocument,
        params: &str,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "import" => {
                let path = std::path::Path::new(params);
                if !path.exists() {
                    anyhow::bail!("File not found: {params}");
                }

                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("")
                    .to_lowercase();

                let text = match ext.as_str() {
                    "txt" | "md" | "csv" | "json" | "toml" | "yaml" | "yml" | "rs"
                    | "py" | "js" | "ts" | "html" | "css" | "xml" => {
                        std::fs::read_to_string(path)
                            .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?
                    }
                    "pdf" => {
                        pdf_extract::extract_text(path)
                            .map_err(|e| anyhow::anyhow!("Failed to extract PDF text: {e}"))?
                    }
                    _ => {
                        // Try reading as UTF-8 text, fall back to lossy conversion
                        match std::fs::read_to_string(path) {
                            Ok(s) => s,
                            Err(_) => {
                                let bytes = std::fs::read(path)
                                    .map_err(|e| anyhow::anyhow!("Failed to read file: {e}"))?;
                                String::from_utf8_lossy(&bytes).into_owned()
                            }
                        }
                    }
                };

                let title = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Imported Document".into());

                let cf = ContentFields {
                    body: text,
                    images: vec![],
                };

                let mut doc = Document::new(title.clone(), String::new(), true);
                doc.content = cf.serialize();

                let db = self.db.clone();
                let created = tokio::task::block_in_place(|| {
                    tokio::runtime::Handle::current().block_on(db.create_document(doc))
                })?;

                let doc_id = created.id_string().unwrap_or_default();
                let json = serde_json::json!({
                    "doc_id": doc_id,
                    "title": title,
                });

                Ok(SkillOutput::StructuredData {
                    kind: "import_result".into(),
                    json: json.to_string(),
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("import".into(), "Import File".into())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::surreal::StorageMode;
    use std::io::Write;

    async fn setup_db() -> Arc<SurrealGraphDB> {
        let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
        db.connect().await.unwrap();
        db.init_schema().await.unwrap();
        Arc::new(db)
    }

    fn make_doc() -> SkillDocument {
        SkillDocument {
            id: "document:dummy".into(),
            title: "Dummy".into(),
            content: ContentFields::default(),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn import_creates_document() {
        let db = setup_db().await;

        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        writeln!(tmp, "Hello from imported file").unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let skill = FileImportSkill::new(db.clone());
        let result = skill.execute("import", &make_doc(), &path).unwrap();
        match result {
            SkillOutput::StructuredData { kind, json } => {
                assert_eq!(kind, "import_result");
                let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                assert!(!v["doc_id"].as_str().unwrap().is_empty());
            }
            _ => panic!("Expected StructuredData"),
        }

        // Verify document was created in DB
        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn import_nonexistent_file_errors() {
        let db = setup_db().await;
        let skill = FileImportSkill::new(db);
        let result = skill.execute("import", &make_doc(), "/nonexistent/file.txt");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn import_binary_file_uses_lossy_conversion() {
        let db = setup_db().await;
        let mut tmp = tempfile::Builder::new()
            .suffix(".bin")
            .tempfile()
            .unwrap();
        // Write some bytes including non-UTF-8
        use std::io::Write;
        tmp.write_all(&[0x48, 0x65, 0x6c, 0x6c, 0x6f, 0xff, 0xfe])
            .unwrap();
        let path = tmp.path().to_string_lossy().to_string();

        let skill = FileImportSkill::new(db.clone());
        let result = skill.execute("import", &make_doc(), &path);
        assert!(result.is_ok());

        let docs = db.list_documents(None).await.unwrap();
        assert_eq!(docs.len(), 1);
    }
}
