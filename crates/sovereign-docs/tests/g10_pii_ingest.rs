//! G10 — personal-data ingest on every write of text.
//!
//! Spec ch. 03, *Document service — contract (C-DOC)*, G10: "**Personal-data
//! ingest runs on every write of text** — create, save, skill output, import,
//! sync, restore — every time a text field changes, not only the first time.
//! The text fields are the title, every `text` block, and every caption and alt
//! text (decided 2026-10-11); the hook receives all of them on each such write,
//! and nothing else. The stored text is exactly what it returns; an answer
//! naming a field it was not given is refused. If the hook fails, the content
//! is stored as given and the document is marked not scanned (decided
//! 2026-10-11). Writes that change no text field (identical save, trash,
//! restore from trash, migration) do not run it."
//!
//! Ch. 09, *The "no stray PII" ingest invariant*: "Every content-creation path
//! runs the pipeline before content settles … After ingest, a body holds
//! tokens, not raw PII values."

mod common;

use common::*;
use sovereign_docs::{
    Edit, EditContent, EditSession, Origin, PiiOutcome, Provenance, RemoteDocument, TextField,
    Trust, WritePath,
};

fn with_secret(s: &str) -> String {
    format!("{s}: write to {SECRET} today")
}

/// Runs each write path once and returns the ids written.
async fn every_path(s: &Stack) -> Vec<(WritePath, String)> {
    let mut out = vec![];
    // create
    let id = s.create_text("Created", &with_secret("create")).await;
    out.push((WritePath::Create, id.clone()));
    // save (editor)
    s.editor_save(&id, 1, v2(vec![text_block(1, &with_secret("save"))])).await.unwrap();
    out.push((WritePath::Save, id.clone()));
    // skill
    let base = s.revision(&id).await;
    s.svc
        .apply_skill_output(&id, base, "markdown_editor", EditContent::PlainText(with_secret("skill")))
        .await
        .unwrap();
    out.push((WritePath::Skill, id.clone()));
    // import
    let mut new = new_text_doc("Imported", &with_secret("import"));
    new.origin = Origin::Import;
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "C:/x.md".into() };
    let imp = s.svc.create(new).await.unwrap().id;
    out.push((WritePath::Import, imp));
    // sync
    let synced = "document:fromphone".to_string();
    s.svc
        .apply_remote(RemoteDocument {
            id: synced.clone(),
            title: "From the phone".into(),
            content: v2(vec![text_block(1, &with_secret("sync"))]),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: Some(Provenance::Written),
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    out.push((WritePath::Sync, synced));
    // restore: a legacy commit holding raw PII, restored
    let legacy = s.legacy("Old", &with_secret("restore")).await;
    let c = s.edb.commit_document(&legacy, "legacy").await.unwrap().id_string().unwrap();
    s.assistant_save(&legacy, "clean").await.unwrap();
    s.svc.restore_version(&legacy, &c).await.unwrap();
    out.push((WritePath::RestoreVersion, legacy));
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_every_write_path_calls_the_hook_for_its_document() {
    let s = Stack::new("g10-paths").await;
    let paths = every_path(&s).await;
    let reqs = s.pii.requests();
    for (path, id) in &paths {
        assert!(
            reqs.iter().any(|r| r.path == *path && &r.document_id == id),
            "{path:?} on {id} did not run ingest"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_no_raw_value_settles_on_any_path() {
    let s = Stack::new("g10-tokenized").await;
    s.pii.set(PiiMode::Tokenize);
    for (path, id) in every_path(&s).await {
        let (_, stored) = s.stored(&id).await;
        assert!(!stored.contains(SECRET), "{path:?}: raw value stored");
        assert!(stored.contains("[pii:"), "{path:?}: the hook's tokens are stored");
        assert!(s.svc.read(&id).await.unwrap().pii_scanned, "{path:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_an_already_scanned_document_is_scanned_again_when_its_text_changes() {
    let s = Stack::new("g10-rescan").await;
    s.pii.set(PiiMode::Tokenize);
    let id = s.create_text("T", "nothing personal").await;
    s.editor_save(&id, 1, v2(vec![text_block(1, &with_secret("later"))])).await.unwrap();
    assert!(!s.stored(&id).await.1.contains(SECRET));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_the_hook_sees_every_text_field_and_cannot_touch_anything_else() {
    let s = Stack::new("g10-fields").await;
    s.pii.set(PiiMode::Tokenize);
    let id = s.create_text("T", "x").await;
    s.pii.clear();
    let img = image_block(2, "/a.png", Some("ab"));
    let unk = unknown_block(3);
    let vid = serde_json::json!({"type": "video", "id": hex(4), "ref": {"path": "/v.mp4", "sha256": "cd"}, "caption": "clip"});
    s.editor_save(&id, 1, v2(vec![text_block(1, &with_secret("a")), img.clone(), unk.clone(), vid.clone()]))
        .await
        .unwrap();
    let mut fields = s.pii.fields_of_last(&id);
    let mut want = vec![
        TextField::Title,
        TextField::Text(bid(1)),
        TextField::Caption(bid(2)),
        TextField::Alt(bid(2)),
        TextField::Caption(bid(4)),
    ];
    let key = |f: &TextField| format!("{f:?}");
    fields.sort_by_key(key);
    want.sort_by_key(key);
    assert_eq!(fields, want, "title, every text, caption and alt; nothing of the unknown block");
    let blocks = assert_v2(&s.stored(&id).await.1);
    assert_eq!(blocks[1], img, "fields without personal data come back unchanged");
    assert_eq!(blocks[2], unk);
    assert_eq!(blocks[3], vid);
}

/// Decided 2026-10-11: titles, captions and alt texts go through ingest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_no_raw_value_settles_in_a_title_caption_or_alt_text() {
    let s = Stack::new("g10-title-caption").await;
    s.pii.set(PiiMode::Tokenize);
    let mut img = image_block(2, "/a.png", Some("ab"));
    img["caption"] = serde_json::json!(with_secret("caption"));
    img["alt"] = serde_json::json!(with_secret("alt"));
    let content = v2(vec![text_block(1, "body"), img]);
    let mut ids = vec![];
    // create (title and v2 content)
    let mut new = new_text_doc(&with_secret("title"), "");
    new.content = sovereign_docs::NewContent::V2(content.clone());
    ids.push(s.svc.create(new).await.unwrap().id);
    // import
    let mut new = new_text_doc(&with_secret("imported title"), "body");
    new.origin = Origin::Import;
    new.trust = Trust::Undetermined;
    new.provenance = Provenance::Import { source_path: "C:/y.md".into() };
    ids.push(s.svc.create(new).await.unwrap().id);
    // save (title + content)
    let id = s.create_text("plain", "x").await;
    let base = s.revision(&id).await;
    s.svc
        .save(&id, Edit {
            base,
            title: Some(with_secret("saved title")),
            content: Some(EditContent::V2(content.clone())),
            origin: Origin::Editor { session: EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    ids.push(id);
    // sync
    s.svc
        .apply_remote(RemoteDocument {
            id: "document:titled".into(),
            title: with_secret("synced title"),
            content: content.clone(),
            thread_id: "thread:t1".into(),
            trust: Some(Trust::Owned),
            provenance: Some(Provenance::Written),
            modified_at: s.clock.get(),
            deleted_at: None,
            device: "phone".into(),
            trust_decision: None,
        })
        .await
        .unwrap();
    ids.push("document:titled".into());
    for id in ids {
        let (title, stored) = s.stored(&id).await;
        assert!(!title.contains(SECRET), "{id}: raw value in the title");
        assert!(!stored.contains(SECRET), "{id}: raw value in a caption or alt");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_a_title_only_save_runs_the_hook_on_the_title() {
    let s = Stack::new("g10-title-only").await;
    s.pii.set(PiiMode::Tokenize);
    let id = s.create_text("T", "x").await;
    s.pii.clear();
    let base = s.revision(&id).await;
    let out = s
        .svc
        .save(&id, Edit {
            base,
            title: Some(with_secret("renamed")),
            content: None,
            origin: Origin::Editor { session: EditSession(1) },
            explicit: false,
            pasted_tokens: vec![],
        })
        .await
        .unwrap();
    assert_eq!(out.pii, PiiOutcome::Scanned);
    assert!(s.pii.fields_of_last(&id).contains(&TextField::Title));
    assert!(!s.stored(&id).await.0.contains(SECRET));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_a_failing_hook_stores_the_content_and_marks_it_not_scanned() {
    let s = Stack::new("g10-fail").await;
    let id = s.create_text("T", "x").await;
    s.pii.set(PiiMode::Fail);
    let typed = with_secret("unscanned");
    let out = s.editor_save(&id, 1, v2(vec![text_block(1, &typed)])).await.expect("nothing typed is lost");
    assert!(matches!(out.pii, PiiOutcome::Failed { .. }));
    assert_eq!(texts(&s.stored(&id).await.1), vec![typed]);
    assert!(!s.svc.read(&id).await.unwrap().pii_scanned);
    assert!(s.stored_doc(&id).await.pii_scanned_at.is_none(), "the sweep will find it");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_a_hook_answer_naming_an_unknown_block_is_refused() {
    let s = Stack::new("g10-rogue").await;
    let id = s.create_text("T", "x").await;
    s.pii.set(PiiMode::Rogue);
    let typed = "kept as typed";
    let out = s.editor_save(&id, 1, v2(vec![text_block(1, typed)])).await.unwrap();
    assert!(matches!(out.pii, PiiOutcome::Failed { .. }));
    let stored = s.stored(&id).await.1;
    assert_eq!(texts(&stored), vec![typed]);
    assert!(!stored.contains("rogue"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn g10_writes_that_do_not_change_text_do_not_run_the_hook() {
    let s = Stack::new("g10-no-text").await;
    let id = s.create_text("T", "x").await;
    let legacy = s.legacy("L", "legacy raw").await;
    s.pii.clear();
    // identical save
    let same = s.stored(&id).await.1;
    let out = s.editor_save(&id, 1, same).await.unwrap();
    assert_eq!(out.pii, PiiOutcome::NotRun);
    // trash and back
    s.svc.trash(&id).await.unwrap();
    s.svc.restore_from_trash(&id).await.unwrap();
    // migration
    s.svc.migrate_all().await.unwrap();
    let _ = legacy;
    assert!(s.pii.requests().is_empty(), "{} calls", s.pii.requests().len());
}
