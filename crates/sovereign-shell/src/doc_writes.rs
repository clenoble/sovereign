//! Lot 0 — the shell's document writes, kept out of `app.rs` so their
//! invariants can be tested against a `GraphDB` without a window.
//!
//! Three rules live here:
//! - **A skill never writes an empty body over a non-empty document**, and never
//!   writes back a document whose stored content it could not read
//!   ([`skill_write_verdict`]). Lot 0.1, review `06-skills-comms.md` N1.
//! - **A skill overwrite always has a restore point**: a document with no commit
//!   yet (bulk-imported, freshly synced) is committed as-is before the skill's
//!   version lands ([`apply_skill_update`]).
//! - **A write error is never reported as success**: every helper returns the
//!   database error to the caller, which surfaces it (lot 0.3).

use sovereign_core::content::ContentFields;
use sovereign_db::traits::GraphDB;
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

/// What a skill receives for a stored document, and whether that is a faithful
/// reading of it.
pub(crate) struct SkillInput {
    pub(crate) fields: ContentFields,
    /// False when the stored content is non-empty but is not a readable content
    /// envelope — today `ContentFields::parse` then hands the skill an empty
    /// body (raw shell-editor text, bulk-imported markdown).
    pub(crate) faithful: bool,
}

/// Parse stored content for a skill.
///
/// TRACK A SEAM: `ContentFields::parse` is becoming tolerant (a bare string is
/// read as the body) with a way to tell whether it fell back to raw text. When
/// that lands, `faithful` should come from that API instead of the local
/// envelope check below; the rest of the guard does not change.
pub(crate) fn skill_input(stored: &str) -> SkillInput {
    // Track A's tolerant read never returns an empty body for raw text, but a
    // skill's ContentUpdate is written back as the envelope, and the shell
    // still displays `content` raw — so a raw document stays refused until the
    // v2 document format and its migration land (spec ch. 03, decision K1).
    let (fields, kind) = ContentFields::parse_with_kind(stored);
    let faithful = stored.trim().is_empty() || kind != sovereign_core::content::ContentKind::Raw;
    SkillInput { fields, faithful }
}

/// Decide whether a skill's `ContentUpdate` may be written over the stored
/// document. `Err` carries the user-facing reason; nothing is written then.
pub(crate) fn skill_write_verdict(
    stored: &str,
    input: &SkillInput,
    output: &ContentFields,
) -> Result<(), &'static str> {
    if !input.faithful {
        return Err("This document isn't in the format skills can read yet, so the skill saw an \
                    empty document. Nothing was changed.");
    }
    if input.fields.body.is_empty() && !stored.trim().is_empty() {
        return Err("The skill received an empty body for a document that isn't empty. \
                    Nothing was changed.");
    }
    if !input.fields.body.is_empty() && output.body.trim().is_empty() {
        return Err("The skill would have replaced the document's text with nothing. \
                    Nothing was changed.");
    }
    Ok(())
}

/// The result of a skill write that reached the database.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SkillApplied {
    /// Written and snapshotted.
    Committed,
    /// Written, but the post-write snapshot failed — the prior state is still
    /// in history (restore point), the new state is not.
    WrittenNotSnapshotted(String),
}

/// Write a skill's new content, making sure a restore point exists first.
///
/// A document with no commit (`head_commit` None) is committed as it stands
/// before anything is overwritten; if that snapshot fails, nothing is written.
pub(crate) async fn apply_skill_update(
    db: &dyn GraphDB,
    doc_id: &str,
    new_content: &str,
    skill_name: &str,
) -> Result<SkillApplied, String> {
    let doc = db
        .get_document(doc_id)
        .await
        .map_err(|e| format!("Couldn't read the document: {e}. Nothing was changed."))?;
    if doc.head_commit.is_none() {
        db.commit_document(doc_id, &format!("Before skill: {skill_name}"))
            .await
            .map_err(|e| {
                format!("Couldn't save a restore point first ({e}). Nothing was changed.")
            })?;
    }
    db.update_document(doc_id, None, Some(new_content))
        .await
        .map_err(|e| format!("Couldn't write the document: {e}. Nothing was changed."))?;
    match db.commit_document(doc_id, &format!("Skill: {skill_name}")).await {
        Ok(_) => Ok(SkillApplied::Committed),
        Err(e) => Ok(SkillApplied::WrittenNotSnapshotted(e.to_string())),
    }
}

/// Delete a document from the canvas — reversibly (lot 0.4).
///
/// Soft delete stamps `deleted_at`: the document leaves every listing, sync
/// carries the deletion as a tombstone instead of re-pushing the row from a
/// paired device, and `restore_soft_deleted_document` brings it back. This is
/// the reversible default pending Céline's decision K3 (gated soft delete +
/// purge, or an explicit permanent delete beside it). The shell never
/// hard-deletes a document.
pub(crate) async fn delete_document_reversibly(
    db: &dyn GraphDB,
    doc_id: &str,
) -> sovereign_db::DbResult<()> {
    db.soft_delete_document(doc_id).await
}

/// The result of saving a doc window's edit buffer (lot 0.3).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    /// Written and snapshotted into History.
    Saved,
    /// Written, but the History snapshot failed.
    SavedNotSnapshotted(String),
    /// Nothing was written — the edits must stay open in the window.
    NotSaved(String),
}

/// Persist an edited body and snapshot it into version history.
pub(crate) async fn save_body(db: &dyn GraphDB, doc_id: &str, body: &str) -> SaveOutcome {
    if let Err(e) = db.update_document(doc_id, None, Some(body)).await {
        return SaveOutcome::NotSaved(e.to_string());
    }
    match db.commit_document(doc_id, "Saved edit").await {
        Ok(_) => SaveOutcome::Saved,
        Err(e) => SaveOutcome::SavedNotSnapshotted(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_db::mock::MockGraphDB;
    use sovereign_db::schema::Document;
    use sovereign_skills::{traits::SkillOutput, SkillContext, SkillDocument};

    fn rt() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap()
    }

    fn registry() -> SkillRegistry {
        crate::app::build_skill_registry()
    }

    fn envelope(body: &str) -> String {
        ContentFields { body: body.into(), ..Default::default() }.serialize()
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
    /// it (empty params): whatever the skill returns, the guard never lets an
    /// empty body replace a non-empty one, and never writes back a document the
    /// skill could not read. Holds whether or not the menu hides an action.
    #[test]
    fn a_skill_never_writes_an_empty_body_over_a_non_empty_document() {
        let reg = registry();
        let docs = [
            "plain text body, written by the shell editor".to_string(),
            "# Imported\n\nSome **markdown** from a bulk import.".to_string(),
            envelope("an envelope body with an email a@b.example and 4111 1111 1111 1111"),
            "{not quite json".to_string(),
        ];
        for stored in &docs {
            let input = skill_input(stored);
            for s in reg.all_skills() {
                let granted = s.required_capabilities().into_iter().collect();
                let ctx = SkillContext { granted, db: None, llm: None };
                for (action, _) in s.actions() {
                    let doc = SkillDocument {
                        id: "document:1".into(),
                        title: "T".into(),
                        content: input.fields.clone(),
                    };
                    let Ok(SkillOutput::ContentUpdate(out)) =
                        reg.execute_skill(s.name(), &action, &doc, "", &ctx)
                    else {
                        continue;
                    };
                    if skill_write_verdict(stored, &input, &out).is_ok() {
                        assert!(input.faithful, "{} {action} wrote over unreadable content", s.name());
                        assert!(
                            !(out.body.trim().is_empty() && !stored.trim().is_empty()),
                            "{} {action} emptied {stored:?}",
                            s.name()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn verdict_refuses_raw_content_and_emptied_bodies_and_allows_real_edits() {
        let raw = "raw shell text";
        let input = skill_input(raw);
        assert!(!input.faithful);
        let out = ContentFields { body: "anything".into(), ..Default::default() };
        assert!(skill_write_verdict(raw, &input, &out).is_err());

        let env = envelope("hello");
        let input = skill_input(&env);
        assert!(input.faithful);
        let emptied = ContentFields::default();
        assert!(skill_write_verdict(&env, &input, &emptied).is_err());
        let edited = ContentFields { body: "HELLO".into(), ..Default::default() };
        assert!(skill_write_verdict(&env, &input, &edited).is_ok());

        // An empty document may receive content.
        let input = skill_input("");
        assert!(skill_write_verdict("", &input, &edited).is_ok());
    }

    #[test]
    fn a_document_without_history_gets_a_restore_point_before_a_skill_overwrite() {
        rt().block_on(async {
            let db = MockGraphDB::new();
            let mut doc = Document::new("Imported".into(), "thread:1".into(), true);
            doc.content = envelope("original");
            let doc = db.create_document(doc).await.unwrap();
            let id = doc.id_string().unwrap();
            assert!(db.get_document(&id).await.unwrap().head_commit.is_none());

            let out = apply_skill_update(&db, &id, &envelope("changed"), "redactor").await;
            assert_eq!(out, Ok(SkillApplied::Committed));

            let commits = db.list_document_commits(&id).await.unwrap();
            assert_eq!(commits.len(), 2, "restore point + skill commit");
            let restore = commits.iter().find(|c| c.message.starts_with("Before skill")).unwrap();
            assert_eq!(restore.snapshot.content, envelope("original"));
            assert_eq!(db.get_document(&id).await.unwrap().content, envelope("changed"));
        });
    }

    #[test]
    fn a_document_with_history_is_not_snapshotted_twice() {
        rt().block_on(async {
            let db = MockGraphDB::new();
            let mut doc = Document::new("Doc".into(), "thread:1".into(), true);
            doc.content = envelope("v1");
            let id = db.create_document(doc).await.unwrap().id_string().unwrap();
            db.commit_document(&id, "Saved edit").await.unwrap();
            apply_skill_update(&db, &id, &envelope("v2"), "redactor").await.unwrap();
            assert_eq!(db.list_document_commits(&id).await.unwrap().len(), 2);
        });
    }

    #[test]
    fn a_failed_skill_write_is_an_error_not_a_success() {
        rt().block_on(async {
            let db = MockGraphDB::new();
            let r = apply_skill_update(&db, "document:missing", &envelope("x"), "redactor").await;
            assert!(r.is_err());
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

    #[test]
    fn a_save_is_written_and_snapshotted() {
        rt().block_on(async {
            let db = MockGraphDB::new();
            let id = db
                .create_document(Document::new("D".into(), "thread:1".into(), true))
                .await
                .unwrap()
                .id_string()
                .unwrap();
            assert_eq!(save_body(&db, &id, "typed text").await, SaveOutcome::Saved);
            assert_eq!(db.get_document(&id).await.unwrap().content, "typed text");
            assert_eq!(db.list_document_commits(&id).await.unwrap().len(), 1);
        });
    }

    /// Lot 0.3: a write error is never reported as success.
    #[test]
    fn a_write_error_is_never_reported_as_success() {
        rt().block_on(async {
            let db = MockGraphDB::new();
            let out = save_body(&db, "document:gone", "typed text").await;
            assert!(matches!(out, SaveOutcome::NotSaved(_)), "{out:?}");
        });
    }
}
