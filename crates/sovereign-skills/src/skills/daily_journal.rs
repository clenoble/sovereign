use chrono::Utc;

use crate::manifest::Capability;
use crate::traits::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

pub struct DailyJournalSkill;

const JOURNAL_THREAD_NAME: &str = "Journal";
const JOURNAL_THREAD_DESCRIPTION: &str =
    "Daily journal entries created by the Daily Journal skill.";

impl CoreSkill for DailyJournalSkill {
    fn name(&self) -> &str {
        "daily-journal"
    }

    fn required_capabilities(&self) -> Vec<Capability> {
        vec![Capability::WriteAllDocuments]
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
        _params: &str,
        ctx: &SkillContext,
    ) -> anyhow::Result<SkillOutput> {
        match action {
            "today" => {
                let db = ctx
                    .db
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("Daily Journal requires database access"))?;

                let date_title = Utc::now().format("%Y-%m-%d").to_string();
                let thread_id = db.find_or_create_thread(
                    JOURNAL_THREAD_NAME,
                    JOURNAL_THREAD_DESCRIPTION,
                )?;

                let (existed, doc_id) = match find_existing(db, &thread_id, &date_title)? {
                    Some(id) => (true, id),
                    None => {
                        let initial_body = format!("# {date_title}\n\n");
                        (
                            false,
                            db.create_document(&date_title, &thread_id, &initial_body)?,
                        )
                    }
                };

                let json = serde_json::to_string(&serde_json::json!({
                    "doc_id": doc_id,
                    "thread_id": thread_id,
                    "date": date_title,
                    "created": !existed,
                }))?;
                Ok(SkillOutput::StructuredData {
                    kind: "journal_entry".into(),
                    json,
                })
            }
            _ => anyhow::bail!("Unknown action: {action}"),
        }
    }

    fn actions(&self) -> Vec<(String, String)> {
        vec![("today".into(), "Open Today's Journal".into())]
    }

    fn file_types(&self) -> Vec<String> {
        // Universal — the action ignores the current document; it operates
        // on the workspace as a whole.
        vec![]
    }
}

fn find_existing(
    db: &std::sync::Arc<dyn crate::traits::SkillDbAccess>,
    thread_id: &str,
    date_title: &str,
) -> anyhow::Result<Option<String>> {
    let docs = db.list_documents(Some(thread_id))?;
    Ok(docs
        .into_iter()
        .find(|(_, title)| title == date_title)
        .map(|(id, _)| id))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::sync::Arc;

    use super::*;
    use sovereign_core::content::ContentFields;

    use crate::traits::SkillDbAccess;

    /// Mock DB that records create_document calls and lets tests preload
    /// existing documents.
    struct MockDb {
        existing: RefCell<Vec<(String, String, String)>>, // (id, thread_id, title)
        created: RefCell<Vec<(String, String, String)>>,  // (title, thread_id, content)
        thread_calls: RefCell<u32>,
    }

    impl MockDb {
        fn new() -> Self {
            Self {
                existing: RefCell::new(Vec::new()),
                created: RefCell::new(Vec::new()),
                thread_calls: RefCell::new(0),
            }
        }
        fn with_existing(self, id: &str, thread_id: &str, title: &str) -> Self {
            self.existing.borrow_mut().push((id.into(), thread_id.into(), title.into()));
            self
        }
    }

    // Safety: tests are single-threaded and only mutate via &self via RefCell.
    // SkillDbAccess requires Send + Sync; RefCell is !Sync, so we wrap in
    // unsafe impl just for the test stub.
    unsafe impl Send for MockDb {}
    unsafe impl Sync for MockDb {}

    impl SkillDbAccess for MockDb {
        fn search_documents(&self, _: &str) -> anyhow::Result<Vec<(String, String, String)>> {
            Ok(vec![])
        }
        fn get_document(&self, _: &str) -> anyhow::Result<(String, String, String)> {
            Ok(("".into(), "".into(), "".into()))
        }
        fn list_documents(&self, thread_id: Option<&str>) -> anyhow::Result<Vec<(String, String)>> {
            let want = thread_id.unwrap_or("");
            Ok(self
                .existing
                .borrow()
                .iter()
                .filter(|(_, tid, _)| tid == want)
                .map(|(id, _, title)| (id.clone(), title.clone()))
                .collect())
        }
        fn create_document(&self, title: &str, thread_id: &str, content: &str) -> anyhow::Result<String> {
            let id = format!("document:created-{}", self.created.borrow().len() + 1);
            self.created.borrow_mut().push((title.into(), thread_id.into(), content.into()));
            self.existing.borrow_mut().push((id.clone(), thread_id.into(), title.into()));
            Ok(id)
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
        fn find_or_create_thread(&self, _name: &str, _desc: &str) -> anyhow::Result<String> {
            *self.thread_calls.borrow_mut() += 1;
            Ok("thread:journal".into())
        }
    }

    fn dummy_doc() -> SkillDocument {
        SkillDocument {
            id: "document:any".into(),
            title: "Any".into(),
            content: ContentFields::default(),
        }
    }

    fn ctx_with(db: Arc<MockDb>) -> SkillContext {
        SkillContext {
            granted: [Capability::WriteAllDocuments].into_iter().collect(),
            db: Some(db as Arc<dyn SkillDbAccess>),
            llm: None,
        }
    }

    #[test]
    fn creates_entry_when_none_exists_today() {
        let db = Arc::new(MockDb::new());
        let skill = DailyJournalSkill;
        let result = skill.execute("today", &dummy_doc(), "", &ctx_with(db.clone())).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["created"], true);
            assert_eq!(v["thread_id"], "thread:journal");
            assert!(v["doc_id"].as_str().unwrap().starts_with("document:created-"));
        } else {
            panic!("expected StructuredData");
        }
        assert_eq!(db.created.borrow().len(), 1);
        assert_eq!(*db.thread_calls.borrow(), 1);
    }

    #[test]
    fn returns_existing_entry_without_duplicating() {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        let db = Arc::new(MockDb::new().with_existing("document:existing", "thread:journal", &today));
        let skill = DailyJournalSkill;
        let result = skill.execute("today", &dummy_doc(), "", &ctx_with(db.clone())).unwrap();
        if let SkillOutput::StructuredData { json, .. } = result {
            let v: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert_eq!(v["created"], false);
            assert_eq!(v["doc_id"], "document:existing");
        }
        // No new document was created
        assert_eq!(db.created.borrow().len(), 0);
    }

    #[test]
    fn errors_when_no_db_in_context() {
        let skill = DailyJournalSkill;
        let ctx = SkillContext {
            granted: [Capability::WriteAllDocuments].into_iter().collect(),
            db: None,
            llm: None,
        };
        let result = skill.execute("today", &dummy_doc(), "", &ctx);
        assert!(result.is_err());
    }

    #[test]
    fn unknown_action_errors() {
        let db = Arc::new(MockDb::new());
        let skill = DailyJournalSkill;
        let result = skill.execute("yesterday", &dummy_doc(), "", &ctx_with(db));
        assert!(result.is_err());
    }
}
