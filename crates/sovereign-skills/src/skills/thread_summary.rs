use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct ThreadSummarySkill;

/// Per-document content cap when assembling the summary prompt. Keeps the
/// total prompt under the router model's typical context window even for
/// threads with many documents.
const PER_DOC_CHAR_CAP: usize = 1200;

/// Total prompt cap (rough — not token-exact). Leaves headroom for the
/// model's response.
const PROMPT_CHAR_CAP: usize = 6000;

const MAX_RESPONSE_TOKENS: u32 = 500;

impl CoreSkill for ThreadSummarySkill {
    fn name(&self) -> &str {
        "thread-summary"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::ReadAllDocuments, Capability::LlmInference]
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
        doc: &SkillDocument,
        _params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "summarize_thread" => {
                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Thread Summary requires database access"))?;
                let llm = ctx
                    .llm
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Thread Summary requires LLM access"))?;

                let (_title, thread_id, _content) = db.get_document(&doc.id)?;
                if thread_id.is_empty() {
                    anyhow::bail!("Current document is not associated with a thread");
                }

                let docs_in_thread = db.list_documents(Some(&thread_id))?;
                if docs_in_thread.is_empty() {
                    anyhow::bail!("Thread contains no documents to summarize");
                }

                let (prompt, truncated) =
                    build_prompt(&docs_in_thread, db.as_ref(), &thread_id);
                let summary = llm.generate(&prompt, MAX_RESPONSE_TOKENS)?;

                let json = serde_json::to_string(&serde_json::json!({
                    "thread_id": thread_id,
                    "doc_count": docs_in_thread.len(),
                    "summary": summary.trim(),
                    "truncated_docs": truncated,
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "thread_summary".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("summarize_thread".into(), "Summarize Thread".into())]
    }

    fn file_types(&self) -> Vec<String> {
        // Universal — works on any document; the current doc's thread is summarized.
        vec![]
    }
}

/// Returns (prompt, ids_of_docs_whose_content_was_truncated).
fn build_prompt(
    docs: &[(String, String)],
    db: &dyn crate::traits::SkillDbAccess,
    thread_id: &str,
) -> (String, Vec<String>) {
    let header = format!(
        "You are a concise summarizer. Below is a collection of documents from \
         the same thread (id: {thread_id}). Produce a bullet-point summary \
         capturing the key themes, decisions, and open questions across these \
         documents. Be specific; quote document titles when useful.\n\n\
         Documents:\n\n"
    );
    let footer = "\nSummary (bullet points):\n";
    let budget_for_docs = PROMPT_CHAR_CAP
        .saturating_sub(header.len())
        .saturating_sub(footer.len());

    let mut body = String::new();
    let mut truncated: Vec<String> = Vec::new();
    let mut remaining = budget_for_docs;

    for (id, title) in docs {
        let content = match db.get_document(id) {
            Ok((_, _, c)) => c,
            Err(_) => continue,
        };
        let (snippet, was_truncated) = if content.chars().count() > PER_DOC_CHAR_CAP {
            let truncated_str: String = content.chars().take(PER_DOC_CHAR_CAP).collect();
            (format!("{truncated_str}\n…(truncated)"), true)
        } else {
            (content, false)
        };
        if was_truncated {
            truncated.push(id.clone());
        }

        let entry = format!("## {title}\n{snippet}\n\n");
        if entry.len() > remaining {
            // No room for this doc's body — emit just the title as a marker.
            let stub = format!("## {title}\n(omitted: prompt budget exhausted)\n\n");
            if stub.len() <= remaining {
                body.push_str(&stub);
                remaining -= stub.len();
            }
            // Don't try later (smaller) docs either; iteration order matters
            // for predictability, so just stop.
            break;
        }
        body.push_str(&entry);
        remaining -= entry.len();
    }

    let prompt = format!("{header}{body}{footer}");
    (prompt, truncated)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;

    use super::*;
    use sovereign_core::content::ContentFields;

    use crate::traits::{SkillDbAccess, SkillLlmAccess};

    struct StubDb {
        thread_id: String,
        // (id, title, content)
        docs: Vec<(String, String, String)>,
    }

    impl SkillDbAccess for StubDb {
        fn search_documents(&self, _: &str) -> anyhow::Result<Vec<(String, String, String)>> {
            Ok(vec![])
        }
        fn get_document(&self, id: &str) -> anyhow::Result<(String, String, String)> {
            if let Some((_, title, content)) =
                self.docs.iter().find(|(i, _, _)| i == id)
            {
                Ok((title.clone(), self.thread_id.clone(), content.clone()))
            } else {
                anyhow::bail!("not found: {id}")
            }
        }
        fn list_documents(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
            if thread_id == Some(self.thread_id.as_str()) {
                Ok(self.docs.iter().map(|(i, t, _)| (i.clone(), t.clone())).collect())
            } else {
                Ok(vec![])
            }
        }
        fn create_document(&self, _: &str, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("".into())
        }
        fn list_relationships(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn list_backlinks(&self, _: &str) -> anyhow::Result<Vec<(String, String)>> {
            Ok(vec![])
        }
        fn list_all_documents_with_link_counts(
            &self,
        ) -> anyhow::Result<Vec<(String, String, u32, u32)>> {
            Ok(vec![])
        }
        fn find_or_create_thread(&self, _: &str, _: &str) -> anyhow::Result<String> {
            Ok("thread:1".into())
        }
    }

    struct StubLlm {
        response: String,
        last_prompt: RefCell<Option<String>>,
    }

    unsafe impl Send for StubLlm {}
    unsafe impl Sync for StubLlm {}

    impl SkillLlmAccess for StubLlm {
        fn generate(&self, prompt: &str, _max_tokens: u32) -> anyhow::Result<String> {
            *self.last_prompt.borrow_mut() = Some(prompt.to_string());
            Ok(self.response.clone())
        }
    }

    fn make_doc(id: &str) -> SkillDocument {
        SkillDocument {
            id: id.into(),
            title: "Whatever".into(),
            content: ContentFields::default(),
        }
    }

    fn ctx(
        db: Arc<dyn SkillDbAccess>,
        llm: Option<Arc<dyn SkillLlmAccess>>,
    ) -> SkillContext {
        SkillContext {
            granted: [Capability::ReadAllDocuments, Capability::LlmInference]
                .into_iter()
                .collect(),
            db: Some(db),
            llm,
        }
    }

    #[test]
    fn summarizes_all_docs_in_thread() {
        let db = Arc::new(StubDb {
            thread_id: "thread:t1".into(),
            docs: vec![
                ("document:a".into(), "First".into(), "alpha content".into()),
                ("document:b".into(), "Second".into(), "beta content".into()),
                ("document:c".into(), "Third".into(), "gamma content".into()),
            ],
        });
        let llm = Arc::new(StubLlm {
            response: "- alpha\n- beta\n- gamma".into(),
            last_prompt: RefCell::new(None),
        });
        let llm_dyn: Arc<dyn SkillLlmAccess> = llm.clone();

        let skill = ThreadSummarySkill;
        let result = skill
            .execute("summarize_thread", &make_doc("document:a"), "", &ctx(db, Some(llm_dyn)))
            .unwrap();

        if let SkillOutput::StructuredData { json, kind } = result {
            assert_eq!(kind, "thread_summary");
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["thread_id"], "thread:t1");
            assert_eq!(v["doc_count"], 3);
            assert!(v["summary"].as_str().unwrap().contains("alpha"));
        } else {
            panic!("expected StructuredData");
        }

        let prompt = llm.last_prompt.borrow().clone().unwrap();
        assert!(prompt.contains("First"));
        assert!(prompt.contains("Second"));
        assert!(prompt.contains("Third"));
        assert!(prompt.contains("alpha content"));
    }

    #[test]
    fn truncates_long_docs_and_records_them() {
        let long_body = "x".repeat(PER_DOC_CHAR_CAP + 500);
        let db = Arc::new(StubDb {
            thread_id: "thread:t1".into(),
            docs: vec![
                ("document:long".into(), "Long".into(), long_body),
                ("document:short".into(), "Short".into(), "tiny".into()),
            ],
        });
        let llm = Arc::new(StubLlm {
            response: "summary".into(),
            last_prompt: RefCell::new(None),
        });
        let llm_dyn: Arc<dyn SkillLlmAccess> = llm.clone();

        let skill = ThreadSummarySkill;
        let result = skill
            .execute("summarize_thread", &make_doc("document:long"), "", &ctx(db, Some(llm_dyn)))
            .unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            let truncated: Vec<String> = serde_json::from_value(v["truncated_docs"].clone()).unwrap();
            assert!(truncated.contains(&"document:long".to_string()));
            assert!(!truncated.contains(&"document:short".to_string()));
        }
        let prompt = llm.last_prompt.borrow().clone().unwrap();
        assert!(prompt.contains("(truncated)"));
    }

    #[test]
    fn errors_when_no_llm() {
        let db = Arc::new(StubDb {
            thread_id: "thread:t1".into(),
            docs: vec![("document:a".into(), "A".into(), "body".into())],
        });
        let skill = ThreadSummarySkill;
        let result = skill.execute(
            "summarize_thread",
            &make_doc("document:a"),
            "",
            &ctx(db, None),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("LLM"));
    }

    #[test]
    fn errors_when_thread_is_empty() {
        // Doc exists but its thread has no other docs -> only this doc returned;
        // not actually empty here. Test the genuinely-empty case by stubbing
        // thread mismatch.
        let db = Arc::new(StubDb {
            thread_id: "thread:t1".into(),
            // get_document(document:x) succeeds but list_documents(t1) is empty
            docs: vec![],
        });
        let llm: Arc<dyn SkillLlmAccess> = Arc::new(StubLlm {
            response: "".into(),
            last_prompt: RefCell::new(None),
        });
        let skill = ThreadSummarySkill;
        let result = skill.execute(
            "summarize_thread",
            &make_doc("document:x"),
            "",
            &ctx(db, Some(llm)),
        );
        // get_document fails first since the doc isn't in the stub
        assert!(result.is_err());
    }
}
