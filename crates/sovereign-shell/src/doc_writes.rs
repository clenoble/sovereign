//! The shell's document writes, kept out of `app.rs` so their invariants can
//! be tested against a real store without a window. Since C-DOC adoption
//! step 2 they go through the document service (`sovereign-docs`): every save
//! writes the v2 format, runs personal-data ingest, and never replaces a state
//! someone else wrote without a version of it.
//!
//! The rules that live here:
//! - **A skill never writes an empty text over a non-empty document**, never
//!   writes back a change to images or videos (until C-SKILL), and never
//!   collapses a document of several text blocks
//!   (`sovereign_ai::docs_host::skill_output_edit`; the service refuses the
//!   last). Lot 0.1, review `06-skills-comms.md` N1.
//! - **A skill's write has a restore point**: the service versions the prior
//!   state first and the skill's result after (`apply_skill_output`, G7).
//! - **A write error is never reported as success** (lot 0.3): every helper
//!   returns the refusal to the caller, which surfaces it.

use sovereign_ai::docs_host::{skill_output_edit, view_fields};
use sovereign_core::content::ContentFields;
#[cfg(test)] // the tests' store calls; production writes go through the service
use sovereign_db::traits::GraphDB;
use sovereign_docs::{DocumentService, Revision};
use sovereign_skills::SkillRegistry;

/// Skill actions the Skills menu must never offer. The shell runs a menu entry
/// with empty parameters, so text-editor "Save" would replace any body with "".
/// The skill stays registered (the editor and the AI can still call it with a
/// real body); it is only hidden from the parameterless menu.
const MENU_HIDDEN_ACTIONS: &[(&str, &str)] = &[("text-editor", "save")];

/// True if `(skill, action)` must not be run from the Skills menu.
pub(crate) fn is_menu_hidden(skill: &str, action: &str) -> bool {
    MENU_HIDDEN_ACTIONS.iter().any(|(s, a)| *s == skill && *a == action)
}

/// The Skills menu entries: `(index into registry.all_skills(), label)` for each
/// skill's first action, minus the hidden ones. The index is the one
/// `CtxAction::RunSkill` carries, so filtering never shifts which skill runs.
pub(crate) fn menu_skill_entries(registry: &SkillRegistry) -> Vec<(usize, String)> {
    registry
        .all_skills()
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            let first = s.actions().into_iter().next();
            match first {
                Some((id, _)) if is_menu_hidden(s.name(), &id) => None,
                Some((_, label)) => Some((i, label)),
                None => Some((i, s.name().to_string())),
            }
        })
        .collect()
}

/// What a skill receives for a stored document, read through the service:
/// the text blocks as the body (tokens as `[pii:<id>]`), image and video
/// blocks as media — whatever the stored form — and the revision the skill's
/// write-back is based on.
pub(crate) struct SkillInput {
    pub(crate) title: String,
    pub(crate) fields: ContentFields,
    pub(crate) revision: Revision,
}

pub(crate) async fn skill_input(docs: &DocumentService, doc_id: &str) -> Result<SkillInput, String> {
    let view = docs.read(doc_id).await.map_err(|e| format!("Couldn't read the document: {e}"))?;
    Ok(SkillInput { title: view.title.clone(), fields: view_fields(&view), revision: view.revision })
}

/// What a skill's write-back did.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SkillApplied {
    /// Written: the prior state and the result are both in History.
    Written,
    /// The skill returned the document unchanged: nothing written.
    Unchanged,
}

/// Write a skill's `ContentUpdate` back through the service. `Err` carries the
/// user-facing reason; nothing is written then.
pub(crate) async fn apply_skill_update(
    docs: &DocumentService,
    doc_id: &str,
    input: &SkillInput,
    output: &ContentFields,
    skill_name: &str,
) -> Result<SkillApplied, String> {
    let Some(edit) = skill_output_edit(&input.fields, output)? else { return Ok(SkillApplied::Unchanged) };
    let out = docs
        .apply_skill_output(doc_id, input.revision.clone(), skill_name, edit)
        .await
        .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    Ok(if out.changed { SkillApplied::Written } else { SkillApplied::Unchanged })
}

/// The canvas delete as it was before C-DOC adoption step 5 (lot 0.4): a soft
/// delete straight on the store.
///
/// **No longer called by the shell.** Since step 5 the canvas delete moves the
/// document to the trash through the document service
/// (`card_menu::trash_target`, called by `App::trash_document`; S5-1: no
/// production code outside `sovereign-db` and `sovereign-docs` deletes around
/// the service). This function is kept, compiled for tests only, because the
/// two lot-0.4 tests below call it and an implementer may not edit existing
/// tests; whether to remove them is the reviewer's call.
#[cfg(test)]
pub(crate) async fn delete_document_reversibly(
    db: &dyn GraphDB,
    doc_id: &str,
) -> sovereign_db::DbResult<()> {
    db.soft_delete_document(doc_id).await
}

// The document window's saves are the editor's (`app/editor_v2.rs`,
// `save_request`): the plain text window and its text save were removed with
// it (contract E2, `doc/plans/EDITOR-default.md`).

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_ai::docs_host::testing::{account_key, encrypted_memory_store};
    use sovereign_ai::docs_host::build_document_service;
    use sovereign_docs::EditContent;
    use sovereign_db::encrypted::EncryptedGraphDB;
    use sovereign_db::schema::Document;
    use sovereign_skills::{traits::SkillOutput, SkillContext, SkillDocument};
    use std::sync::Arc;

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    fn registry() -> SkillRegistry {
        crate::app::build_skill_registry()
    }

    fn envelope(body: &str) -> String {
        ContentFields { body: body.into(), ..Default::default() }.serialize()
    }

    const V2_DOC: &str = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000000c01","text":"one two [pii:pii_record:e1]"},{"type":"image","id":"00000000000000000000000000000c02","ref":{"path":"/i.png"},"alt":"a","caption":"cap"},{"type":"text","id":"00000000000000000000000000000c03","text":"three"}]}"#;
    const V2_ONE_TEXT: &str = r#"{"format":"sovereign.doc","v":2,"blocks":[{"type":"text","id":"00000000000000000000000000000d01","text":"only text"},{"type":"image","id":"00000000000000000000000000000d02","ref":{"path":"/i.png"},"alt":"a","caption":"cap"}]}"#;

    async fn fixture(tag: &str) -> (Arc<EncryptedGraphDB>, DocumentService) {
        let store = encrypted_memory_store(tag).await;
        let docs = build_document_service(store.clone(), Some(account_key()), "test".into());
        (store, docs)
    }

    /// A row as writers before the service left it: content written directly,
    /// no version.
    async fn legacy(store: &EncryptedGraphDB, content: &str) -> String {
        let mut d = Document::new("Doc".into(), "thread:1".into(), true);
        d.content = content.into();
        store.create_document(d).await.unwrap().id_string().unwrap()
    }

    fn stored_is_v2(content: &str) -> bool {
        let v: serde_json::Value = serde_json::from_str(content).unwrap();
        v["format"] == "sovereign.doc" && v["v"] == 2
    }

    #[test]
    fn text_editor_save_is_never_in_the_menu() {
        let reg = registry();
        let entries = menu_skill_entries(&reg);
        for (si, _) in &entries {
            let s = &reg.all_skills()[*si];
            let first = s.actions().into_iter().next().map(|(id, _)| id).unwrap_or_default();
            assert!(!is_menu_hidden(s.name(), &first), "{} {first} offered", s.name());
        }
        assert!(!entries.iter().any(|(si, _)| reg.all_skills()[*si].name() == "text-editor"));
        // Every other skill is still offered, at its own registry index.
        assert_eq!(entries.len(), reg.all_skills().len() - 1);
    }

    /// The invariant, over every built-in skill's menu action, as the shell runs
    /// it (empty params): whatever the skill returns, what reaches the service
    /// never empties a non-empty text, never drops or adds media, and is always
    /// text (never a collapsed structure).
    #[test]
    fn a_skill_never_writes_an_empty_body_over_a_non_empty_document() {
        rt().block_on(async {
            let (store, docs) = fixture("menu").await;
            let reg = registry();
            let forms = [
                "plain text body, written by the shell editor".to_string(),
                "# Imported\n\nSome **markdown** from a bulk import.".to_string(),
                envelope("an envelope body with an email a@b.example and 4111 1111 1111 1111"),
                V2_DOC.to_string(),
            ];
            for stored in &forms {
                let id = legacy(&store, stored).await;
                let input = skill_input(&docs, &id).await.unwrap();
                for s in reg.all_skills() {
                    let granted = s.required_capabilities().into_iter().collect();
                    let ctx = SkillContext { granted, db: None, llm: None };
                    for (action, _) in s.actions() {
                        let doc = SkillDocument { id: id.clone(), title: "T".into(), content: input.fields.clone() };
                        let Ok(SkillOutput::ContentUpdate(out)) = reg.execute_skill(s.name(), &action, &doc, "", &ctx)
                        else {
                            continue;
                        };
                        if let Ok(Some(edit)) = skill_output_edit(&input.fields, &out) {
                            let EditContent::PlainText(t) = edit else { panic!("{} {action}: not text", s.name()) };
                            assert!(
                                !(t.trim().is_empty() && !input.fields.body.trim().is_empty()),
                                "{} {action} emptied {stored:?}",
                                s.name()
                            );
                            assert_eq!(out.images, input.fields.images);
                            assert_eq!(out.videos, input.fields.videos);
                        }
                    }
                }
            }
        });
    }

    /// A skill reads a v2 document, a v1 envelope and raw text as their text
    /// (never the stored JSON), tokens kept as tokens.
    #[test]
    fn a_skill_reads_v2_v1_and_raw_documents_as_text() {
        rt().block_on(async {
            let (store, docs) = fixture("read").await;
            let reg = registry();
            let ctx = |s: &str| SkillContext {
                granted: reg.find_skill(s).unwrap().required_capabilities().into_iter().collect(),
                db: None,
                llm: None,
            };
            let cases = [
                (V2_DOC.to_string(), "one two [pii:pii_record:e1]\n\nthree"),
                (envelope("one two [pii:pii_record:e1]"), "one two [pii:pii_record:e1]"),
                ("one two [pii:pii_record:e1]".to_string(), "one two [pii:pii_record:e1]"),
            ];
            for (stored, text) in &cases {
                let id = legacy(&store, stored).await;
                let input = skill_input(&docs, &id).await.unwrap();
                assert_eq!(&input.fields.body, text, "{stored:?}");
                let doc = SkillDocument { id, title: "T".into(), content: input.fields.clone() };
                match reg.execute_skill("plaintext-export", "export", &doc, "", &ctx("plaintext-export")) {
                    Ok(SkillOutput::File { data, .. }) => {
                        let out = String::from_utf8(data).unwrap();
                        assert!(out.contains("[pii:pii_record:e1]"), "{out:?}");
                        assert!(!out.contains("sovereign.doc") && !out.contains("\"body\""), "{out:?}");
                    }
                    other => panic!("plaintext-export: {:?}", other.map(|_| ())),
                }
                match reg.execute_skill("word-count", "count", &doc, "", &ctx("word-count")) {
                    Ok(SkillOutput::StructuredData { json, .. }) => {
                        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
                        assert_eq!(v["words"], text.split_whitespace().count(), "{stored:?}");
                    }
                    other => panic!("word-count: {:?}", other.map(|_| ())),
                }
            }
        });
    }

    /// Adoption step 2: a skill's output is written through the service — raw
    /// text and v1 envelopes (refused or envelope-only before) end as v2, the
    /// prior state is versioned first and the result after, media kept; a v2
    /// document with one text block takes the output in that block; a v2
    /// document with several text blocks is refused, nothing written.
    #[test]
    fn a_skill_output_is_written_through_the_service() {
        rt().block_on(async {
            let (store, docs) = fixture("write").await;
            for stored in ["raw shell text".to_string(), envelope("enveloped"), V2_ONE_TEXT.to_string()] {
                let id = legacy(&store, &stored).await;
                let input = skill_input(&docs, &id).await.unwrap();
                let mut out = input.fields.clone();
                out.body = input.fields.body.to_uppercase();
                assert_eq!(apply_skill_update(&docs, &id, &input, &out, "upper").await, Ok(SkillApplied::Written));
                let row = store.get_document(&id).await.unwrap();
                assert!(stored_is_v2(&row.content), "{}", row.content);
                let after = docs.read(&id).await.unwrap();
                assert_eq!(after.plain_text(), input.fields.body.to_uppercase());
                assert_eq!(view_fields(&after).images, input.fields.images, "media kept");
                let commits = store.list_document_commits(&id).await.unwrap();
                assert!(commits.iter().any(|c| c.snapshot.content == stored), "prior state versioned: {stored}");
                assert!(commits.iter().any(|c| c.message.contains("upper")), "the result is a version");
            }

            let id = legacy(&store, V2_DOC).await;
            let input = skill_input(&docs, &id).await.unwrap();
            let out = ContentFields { body: "rewritten".into(), ..input.fields.clone() };
            let err = apply_skill_update(&docs, &id, &input, &out, "upper").await.unwrap_err();
            assert!(err.contains("2 blocks of text"), "{err}");
            assert_eq!(store.get_document(&id).await.unwrap().content, V2_DOC, "nothing written");
        });
    }

    #[test]
    fn a_skill_never_empties_a_document_or_changes_its_media() {
        rt().block_on(async {
            let (store, docs) = fixture("guard").await;
            let id = legacy(&store, V2_ONE_TEXT).await;
            let input = skill_input(&docs, &id).await.unwrap();
            let emptied = ContentFields { body: String::new(), ..input.fields.clone() };
            assert!(apply_skill_update(&docs, &id, &input, &emptied, "s").await.is_err());
            let no_media = ContentFields { body: "x".into(), ..Default::default() };
            assert!(apply_skill_update(&docs, &id, &input, &no_media, "s").await.is_err());
            assert_eq!(store.get_document(&id).await.unwrap().content, V2_ONE_TEXT);
            // An empty document may receive text.
            let empty = legacy(&store, "").await;
            let input = skill_input(&docs, &empty).await.unwrap();
            let out = ContentFields { body: "new".into(), ..Default::default() };
            assert_eq!(apply_skill_update(&docs, &empty, &input, &out, "s").await, Ok(SkillApplied::Written));
        });
    }

    /// A skill's write based on a state another writer replaced is refused.
    #[test]
    fn a_stale_skill_write_is_refused() {
        rt().block_on(async {
            let (store, docs) = fixture("stale").await;
            let id = legacy(&store, "start").await;
            let input = skill_input(&docs, &id).await.unwrap();
            store.update_document(&id, None, Some("changed by sync")).await.unwrap();
            let out = ContentFields { body: "START".into(), ..Default::default() };
            let err = apply_skill_update(&docs, &id, &input, &out, "upper").await.unwrap_err();
            assert!(err.contains("changed elsewhere"), "{err}");
            assert_eq!(docs.read(&id).await.unwrap().plain_text(), "changed by sync");
        });
    }

    /// Lot 0.4, against the real SurrealDB backend (in memory), not the mock:
    /// the canvas delete hides the document and keeps it restorable, content
    /// and history included.
    #[test]
    fn delete_is_reversible() {
        use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
        rt().block_on(async {
            let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
            db.connect().await.unwrap();
            db.init_schema().await.unwrap();
            let mut doc = Document::new("Keep me".into(), "thread:t".into(), true);
            doc.content = envelope("precious");
            let id = db.create_document(doc).await.unwrap().id_string().unwrap();
            db.commit_document(&id, "Saved edit").await.unwrap();

            delete_document_reversibly(&db, &id).await.unwrap();
            let listed = db.list_documents(None).await.unwrap();
            assert!(listed.iter().all(|d| d.id_string().as_deref() != Some(id.as_str())), "gone from the canvas");

            let back = db.restore_soft_deleted_document(&id).await.unwrap();
            assert!(back.deleted_at.is_none());
            assert_eq!(back.content, envelope("precious"));
            assert_eq!(db.list_document_commits(&id).await.unwrap().len(), 1, "history kept");
            let listed = db.list_documents(None).await.unwrap();
            assert!(listed.iter().any(|d| d.id_string().as_deref() == Some(id.as_str())), "back on the canvas");
        });
    }

    /// A delete that fails is an error the shell can surface, not silence.
    #[test]
    fn a_failed_delete_is_an_error() {
        use sovereign_db::surreal::{StorageMode, SurrealGraphDB};
        rt().block_on(async {
            let db = SurrealGraphDB::new(StorageMode::Memory).await.unwrap();
            db.connect().await.unwrap();
            db.init_schema().await.unwrap();
            assert!(delete_document_reversibly(&db, "document:nope").await.is_err());
        });
    }
}
