//! Two threads of the name sought, in the app (spec ch. 03, *Two threads of the
//! name sought: asked once, remembered*, FC1, FC2 and FC6; v0.0.13, item 9):
//! the browser's « Web » thread and the skills' find-or-create (the daily
//! journal's « Journal »).
//!
//! Seams by the test author of item 9 (types and signatures); the code is the
//! implementer's. The commands `thread_for_name` and `remember_thread_choice`
//! (`tauri_commands::threads`) answer these; `save_web_page` takes its thread
//! from [`web_thread`] and `execute_skill` runs through [`execute_skill_on`].

use std::sync::Arc;

use serde::Serialize;
use sovereign_docs::DocumentService;

use crate::tauri_commands::SkillResultDto;

/// FC1: a thread offered when she is asked which one: its id, its name, its
/// number of documents, and its creation time (RFC 3339, UTC; the screen shows
/// it in local time, in History's format).
#[derive(Serialize, Clone, Debug)]
pub struct ThreadOfferedDto {
    pub id: String,
    pub name: String,
    pub documents: usize,
    pub created_at: String,
}

/// FC1, FC2: what to do with a name a find-or-create looks for. Serialized
/// with a `kind` tag (`"use"`, `"create"`, `"ask"`).
#[derive(Serialize, Clone, Debug)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThreadForNameDto {
    /// One live thread of the name, or the one remembered while its choice
    /// holds (FC2).
    Use { id: String },
    /// No live thread has the name: the call creates it.
    Create,
    /// Two or more live threads have the name and no choice holds: ask which
    /// (FC1), among these.
    Ask { name: String, threads: Vec<ThreadOfferedDto> },
}

/// FC1, FC2: what to do with `name` ([`ThreadForNameDto`]).
pub(crate) async fn thread_for_name(docs: &DocumentService, name: &str) -> Result<ThreadForNameDto, String> {
    Ok(match sovereign_db::find_or_create_choice(docs.store(), name).await? {
        sovereign_db::FindOrCreate::Use(id) => ThreadForNameDto::Use { id },
        sovereign_db::FindOrCreate::Create => ThreadForNameDto::Create,
        sovereign_db::FindOrCreate::Ask(threads) => ask_dto(docs, name, &threads).await,
    })
}

/// FC1: the question, each thread with its number of documents and its
/// creation time.
async fn ask_dto(docs: &DocumentService, name: &str, threads: &[sovereign_db::schema::Thread]) -> ThreadForNameDto {
    let mut offered = Vec::with_capacity(threads.len());
    for t in threads {
        let Some(id) = t.id_string() else { continue };
        let documents = docs.list(Some(&id)).await.map(|d| d.len()).unwrap_or(0);
        offered.push(ThreadOfferedDto {
            id,
            name: t.name.trim().to_string(),
            documents,
            created_at: t.created_at.to_rfc3339(),
        });
    }
    ThreadForNameDto::Ask { name: name.trim().to_string(), threads: offered }
}

/// FC2: her answer, remembered: `thread_id` is the thread to use for `name`.
pub(crate) async fn remember_choice(docs: &DocumentService, name: &str, thread_id: &str) -> Result<(), String> {
    sovereign_db::remember_thread_choice(docs.store(), name, thread_id).await
}

/// The thread a saved web page goes to: `thread_id` when the screen gave one;
/// otherwise the « Web » thread: the one live thread of that name, the one
/// remembered (FC2), or a new one when there is none. Two or more with no
/// choice held: refused with the sentence (« N threads are named 'Web'; none
/// was picked … »), creating nothing; the screen asks first (FC1,
/// `thread_for_name`), so this refusal is what a call nobody asked about meets
/// (FC6).
pub(crate) async fn web_thread(docs: &DocumentService, thread_id: Option<String>) -> Result<String, String> {
    if let Some(t) = thread_id.filter(|t| !t.is_empty()) {
        return Ok(t);
    }
    // The thread named exactly « Web » (v0.0.12 item 6, A1: never « Web
    // clippings »), created through the one thread-write path, stamped
    // (THREAD-sync S1).
    match sovereign_db::find_or_create_choice(docs.store(), WEB).await? {
        sovereign_db::FindOrCreate::Use(id) => Ok(id),
        sovereign_db::FindOrCreate::Create => docs
            .thread_create(WEB, "Browsed web content")
            .await
            .map_err(|e| sovereign_ai::docs_host::doc_error_message(&e)),
        sovereign_db::FindOrCreate::Ask(threads) => {
            Err(sovereign_ai::docs_host::AskWhichThread { name: WEB.to_string(), threads }.to_string())
        }
    }
}

/// The body of `execute_skill`, on the document service, the skill registry,
/// the store access skills are given and their model access. A skill whose
/// find-or-create needs her answer (`docs_host::AskWhichThread`) answers
/// `kind: "ask_thread"`, with the question as `structured_json` (a
/// [`ThreadForNameDto::Ask`]); nothing is written, and the screen runs the
/// skill again once she answered.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_skill_on(
    docs: Arc<DocumentService>,
    registry: &sovereign_skills::SkillRegistry,
    inner: Arc<dyn sovereign_skills::SkillDbAccess>,
    llm: Option<Arc<dyn sovereign_skills::SkillLlmAccess>>,
    skill_name: &str,
    action: &str,
    doc_id: &str,
    params: &str,
) -> Result<SkillResultDto, String> {
    use crate::err::ToStringErr;
    use sovereign_skills::traits::{SkillContext, SkillDocument, SkillOutput};
    // C-DOC adoption step 2: the skill reads the document through the
    // service, and a content update is written back here, through the service
    // (`apply_skill_output`: the prior state versioned first, the result after)
    // — the frontend reloads the document instead of saving the output itself.
    let view = docs.read(doc_id).await.map_err(|e| sovereign_ai::docs_host::doc_error_message(&e))?;
    let fields = sovereign_ai::docs_host::view_fields(&view);
    let skill_doc = SkillDocument { id: doc_id.to_string(), title: view.title.clone(), content: fields.clone() };
    // Auto-grant exactly the capabilities the skill declares.
    let granted: std::collections::HashSet<_> =
        registry.find_skill(skill_name).map(|s| s.required_capabilities().into_iter().collect()).unwrap_or_default();
    // C-DOC adoption step 3: what the skill creates (daily journal, file
    // import, duplicate, a WASM plugin's documents) goes through the document
    // service; its reads go to the store access skills always had.
    let skill_db: Arc<dyn sovereign_skills::SkillDbAccess> =
        Arc::new(sovereign_ai::docs_host::ServiceSkillDb::new(inner, docs.clone(), skill_name));
    let ctx = SkillContext { granted, db: Some(skill_db), llm };
    let output = match registry.execute_skill(skill_name, action, &skill_doc, params, &ctx) {
        Ok(o) => o,
        // FC1: the skill's find-or-create needs her answer; nothing was written.
        Err(e) => match e.downcast_ref::<sovereign_ai::docs_host::AskWhichThread>() {
            Some(ask) => {
                let question = ask_dto(&docs, &ask.name, &ask.threads).await;
                let json = serde_json::to_string(&question).str_err()?;
                return Ok(result("ask_thread", |r| r.structured_json = Some(json)));
            }
            None => return Err(e.to_string()),
        },
    };

    match output {
        SkillOutput::ContentUpdate(cf) => {
            crate::doc_saves::apply_skill(&docs, doc_id, view.revision.clone(), skill_name, &fields, &cf).await?;
            // What is stored now (personal data tokenized by the ingest).
            let stored = crate::doc_saves::read_document(&docs, doc_id).await?;
            Ok(result("content_update", |r| {
                r.body = Some(stored.body);
                r.images = Some(stored.images);
                r.videos = Some(stored.videos);
            }))
        }
        SkillOutput::File { name, mime_type, data } => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
            Ok(result("file", |r| {
                r.file_name = Some(name);
                r.file_mime = Some(mime_type);
                r.file_data_base64 = Some(b64);
            }))
        }
        SkillOutput::StructuredData { kind, json } => Ok(result("structured_data", |r| {
            r.structured_kind = Some(kind);
            r.structured_json = Some(json);
        })),
        SkillOutput::None => Ok(result("none", |_| {})),
    }
}

/// The name of the browser's thread.
const WEB: &str = "Web";

/// A skill's result of `kind`, with the fields `fill` sets.
fn result(kind: &str, fill: impl FnOnce(&mut SkillResultDto)) -> SkillResultDto {
    let mut r = SkillResultDto {
        kind: kind.into(),
        body: None,
        images: None,
        videos: None,
        file_name: None,
        file_mime: None,
        file_data_base64: None,
        structured_kind: None,
        structured_json: None,
    };
    fill(&mut r);
    r
}
