//! The assistant's side of exact document titles (v0.0.12 item 23, T1;
//! `doc/plans/V0012-item23-document-titles.md`): a document named in an
//! action resolves through `sovereign_db::resolve_document_title` — the one
//! exact-title resolution, live documents only — and when it does not
//! resolve, the user (or, for a chat tool, the model) is told why, in words:
//! the close titles to choose from (at most five) and a question, that no
//! document has the title, or the documents that share it, each with its
//! thread and its last modified date, and that the action can be done from
//! the document itself on the canvas.

use sovereign_db::schema::Document;
use sovereign_db::{DocumentTitleMatch, GraphDB, MAX_CLOSE_TITLES};

/// Why a title named no single document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Unresolved {
    /// No live document has the title (with or without close titles).
    NoMatch(String),
    /// Two or more live documents have it: none was picked.
    Ambiguous(String),
    /// The store could not be read.
    Unreadable(String),
}

impl Unresolved {
    /// The sentence that says why.
    pub(crate) fn message(&self) -> &str {
        match self {
            Unresolved::NoMatch(m) | Unresolved::Ambiguous(m) | Unresolved::Unreadable(m) => m,
        }
    }
}

impl std::fmt::Display for Unresolved {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

/// The one live document titled `title` (ignoring case and surrounding
/// spaces), or why there is none to act on.
pub(crate) async fn resolve_document_or_explain(db: &dyn GraphDB, title: &str) -> Result<Document, Unresolved> {
    let wanted = title.trim();
    let m = sovereign_db::resolve_document_title(db, wanted)
        .await
        .map_err(|e| Unresolved::Unreadable(format!("the documents could not be read ({e})")))?;
    match m {
        DocumentTitleMatch::Found(d) => Ok(d),
        DocumentTitleMatch::NoMatch { close } if close.is_empty() => {
            Err(Unresolved::NoMatch(format!("no document has the title '{wanted}'")))
        }
        DocumentTitleMatch::NoMatch { close } => {
            let titles: Vec<String> = close.iter().take(MAX_CLOSE_TITLES).map(|d| format!("'{}'", d.title.trim())).collect();
            Err(Unresolved::NoMatch(format!(
                "no document is titled exactly '{wanted}'. Close titles: {}. Which one do you mean?",
                titles.join(", ")
            )))
        }
        DocumentTitleMatch::Ambiguous(docs) => {
            let mut listed = vec![];
            for d in docs.iter().take(MAX_CLOSE_TITLES) {
                let thread = match db.get_thread(&d.thread_id).await {
                    Ok(t) if t.deleted_at.is_none() => format!("in '{}'", t.name),
                    _ => "in no thread".to_string(),
                };
                listed.push(format!(
                    "'{}' {thread}, modified {}",
                    d.title.trim(),
                    d.modified_at.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M")
                ));
            }
            let more = if docs.len() > MAX_CLOSE_TITLES {
                format!(" and {} more", docs.len() - MAX_CLOSE_TITLES)
            } else {
                String::new()
            };
            Err(Unresolved::Ambiguous(format!(
                "{} documents are titled '{wanted}': {}{more}. None was picked: the assistant cannot tell them \
                 apart, so do this from the document itself on the canvas.",
                docs.len(),
                listed.join("; ")
            )))
        }
    }
}
