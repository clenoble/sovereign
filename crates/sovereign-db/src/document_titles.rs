//! Documents named by a caller resolve by **exact title** (v0.0.12 item 23,
//! T1; `doc/plans/V0012-item23-document-titles.md`). Item 6's rule for
//! threads (`thread_names`), applied to documents.
//!
//! `search_documents_by_title` is a search: a blind-index match on the
//! title's tokens, newest first, at most 20. Taking its first result as "the
//! document named X" made "restore Report" restore "Quarterly Report Draft".
//! [`resolve_document_title`] is the one resolution every caller uses
//! instead: it takes **every** candidate the store's title lookup offers
//! (`GraphDB::document_title_candidates`, the blind index on the encrypted
//! layer, no limit), and compares their decrypted titles to the given one,
//! ignoring case and surrounding spaces. Only live documents count (none in
//! the trash), for every caller.

use crate::error::DbResult;
use crate::schema::Document;
use crate::traits::GraphDB;

/// At most this many close titles are offered when no document has the exact
/// title, and at most this many same-titled documents are listed when the
/// title is ambiguous (T1).
pub const MAX_CLOSE_TITLES: usize = 5;

/// What a document title resolves to.
#[derive(Debug, Clone)]
pub enum DocumentTitleMatch {
    /// Exactly one live document has that title.
    Found(Document),
    /// No live document has that title. `close` holds candidates the lookup
    /// offered (at most [`MAX_CLOSE_TITLES`], one per distinct title, in the
    /// store's order); empty when no document comes close.
    NoMatch { close: Vec<Document> },
    /// Two or more live documents have that exact title (all of them): none
    /// is picked.
    Ambiguous(Vec<Document>),
}

impl DocumentTitleMatch {
    /// The document, when exactly one has the title.
    pub fn found(self) -> Option<Document> {
        match self {
            DocumentTitleMatch::Found(d) => Some(d),
            _ => None,
        }
    }
}

/// Two document titles are the same title: equal ignoring case and
/// surrounding spaces.
pub fn same_document_title(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Resolves `title` to the one live document whose title equals it, ignoring
/// case and surrounding spaces — and to nothing else.
pub async fn resolve_document_title<D: GraphDB + ?Sized>(db: &D, title: &str) -> DbResult<DocumentTitleMatch> {
    let wanted = title.trim();
    if wanted.is_empty() {
        return Ok(DocumentTitleMatch::NoMatch { close: vec![] });
    }
    let candidates: Vec<Document> = db
        .document_title_candidates(wanted)
        .await?
        .into_iter()
        .filter(|d| d.deleted_at.is_none())
        .collect();
    let (mut exact, others): (Vec<Document>, Vec<Document>) =
        candidates.into_iter().partition(|d| same_document_title(&d.title, wanted));
    match exact.len() {
        1 => Ok(DocumentTitleMatch::Found(exact.remove(0))),
        0 => {
            let mut close: Vec<Document> = vec![];
            for d in others {
                let t = d.title.trim();
                if !t.is_empty() && !close.iter().any(|c| same_document_title(&c.title, t)) {
                    close.push(d);
                }
                if close.len() == MAX_CLOSE_TITLES {
                    break;
                }
            }
            Ok(DocumentTitleMatch::NoMatch { close })
        }
        _ => Ok(DocumentTitleMatch::Ambiguous(exact)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockGraphDB;

    async fn db_with(titles: &[&str]) -> MockGraphDB {
        let db = MockGraphDB::new();
        for t in titles {
            db.create_document(Document::new((*t).into(), "thread:t".into(), true)).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn exact_title_ignoring_case_and_spaces() {
        let db = db_with(&["Quarterly Report Draft", "Report"]).await;
        for typed in ["Report", " report ", "REPORT"] {
            let d = resolve_document_title(&db, typed).await.unwrap().found().expect(typed);
            assert_eq!(d.title, "Report");
        }
    }

    #[tokio::test]
    async fn no_exact_match_offers_close_titles() {
        let db = db_with(&["Quarterly Report Draft", "Seeds"]).await;
        match resolve_document_title(&db, "Report").await.unwrap() {
            DocumentTitleMatch::NoMatch { close } => {
                let titles: Vec<&str> = close.iter().map(|d| d.title.as_str()).collect();
                assert_eq!(titles, vec!["Quarterly Report Draft"]);
            }
            other => panic!("{other:?}"),
        }
        match resolve_document_title(&db, "Submarine").await.unwrap() {
            DocumentTitleMatch::NoMatch { close } => assert!(close.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn two_exact_titles_are_ambiguous() {
        let db = db_with(&["Daily journal", "daily Journal"]).await;
        assert!(matches!(
            resolve_document_title(&db, "DAILY JOURNAL").await.unwrap(),
            DocumentTitleMatch::Ambiguous(v) if v.len() == 2
        ));
    }
}
