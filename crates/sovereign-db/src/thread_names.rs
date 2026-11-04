//! Threads named by a caller resolve by **exact name** (v0.0.12 item 6, A1;
//! `doc/plans/V0012-lot-assistant.md`).
//!
//! The single-match lookups (`find_thread_by_name`, `…_token_hashes`) return
//! the first blind-index match, with no preference for the exact name: "Work"
//! could resolve to "Work Archive". [`resolve_thread_name`] is the one
//! resolution every caller uses instead: it takes **every** candidate the
//! store's name lookup offers (the blind index on the encrypted layer), and
//! compares their decrypted names to the given one, ignoring case and
//! surrounding spaces. Only live threads count.

use crate::error::DbResult;
use crate::schema::Thread;
use crate::traits::GraphDB;

/// At most this many close names are offered when no thread has the exact
/// name (A1, the contract's [choice]).
pub const MAX_CLOSE_NAMES: usize = 5;

/// What a thread name resolves to.
#[derive(Debug, Clone)]
pub enum ThreadNameMatch {
    /// Exactly one live thread has that name.
    Found(Thread),
    /// No live thread has that name. `close` holds the names of the
    /// candidates the lookup offered (at most [`MAX_CLOSE_NAMES`], distinct,
    /// in the store's order); empty when no thread comes close.
    NoMatch { close: Vec<String> },
    /// Two or more live threads have that exact name: none is picked.
    Ambiguous(Vec<Thread>),
}

impl ThreadNameMatch {
    /// The thread, when exactly one has the name.
    pub fn found(self) -> Option<Thread> {
        match self {
            ThreadNameMatch::Found(t) => Some(t),
            _ => None,
        }
    }
}

/// Two thread names are the same name: equal ignoring case and surrounding
/// spaces.
pub fn same_thread_name(a: &str, b: &str) -> bool {
    a.trim().to_lowercase() == b.trim().to_lowercase()
}

/// Resolves `name` to the one live thread whose name equals it, ignoring case
/// and surrounding spaces — and to nothing else.
pub async fn resolve_thread_name<D: GraphDB + ?Sized>(db: &D, name: &str) -> DbResult<ThreadNameMatch> {
    let wanted = name.trim();
    if wanted.is_empty() {
        return Ok(ThreadNameMatch::NoMatch { close: vec![] });
    }
    let candidates: Vec<Thread> = db
        .thread_name_candidates(wanted)
        .await?
        .into_iter()
        .filter(|t| t.deleted_at.is_none())
        .collect();
    let (mut exact, others): (Vec<Thread>, Vec<Thread>) =
        candidates.into_iter().partition(|t| same_thread_name(&t.name, wanted));
    match exact.len() {
        1 => Ok(ThreadNameMatch::Found(exact.remove(0))),
        0 => {
            let mut close: Vec<String> = vec![];
            for t in others {
                let n = t.name.trim().to_string();
                if !n.is_empty() && !close.iter().any(|c| c == &n) {
                    close.push(n);
                }
                if close.len() == MAX_CLOSE_NAMES {
                    break;
                }
            }
            Ok(ThreadNameMatch::NoMatch { close })
        }
        _ => Ok(ThreadNameMatch::Ambiguous(exact)),
    }
}

/// For callers that find **or create** a thread by name (the import's folder
/// threads, a skill's thread, the browser's "Web" thread), where nobody can be
/// asked "which one?": the id of the one live thread with that exact name,
/// `Ok(None)` when there is none (the caller creates it), and an error when
/// two or more have it (refused, none picked).
pub async fn existing_thread_for_find_or_create<D: GraphDB + ?Sized>(
    db: &D,
    name: &str,
) -> Result<Option<String>, String> {
    match resolve_thread_name(db, name).await.map_err(|e| e.to_string())? {
        ThreadNameMatch::Found(t) => Ok(t.id_string()),
        ThreadNameMatch::NoMatch { .. } => Ok(None),
        ThreadNameMatch::Ambiguous(ts) => Err(format!(
            "{} threads are named '{}'; none was picked — rename one of them first",
            ts.len(),
            name.trim()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockGraphDB;

    async fn db_with(names: &[&str]) -> MockGraphDB {
        let db = MockGraphDB::new();
        for n in names {
            db.create_thread(Thread::new((*n).into(), String::new())).await.unwrap();
        }
        db
    }

    #[tokio::test]
    async fn exact_name_ignoring_case_and_spaces() {
        let db = db_with(&["Work Archive", "Work"]).await;
        for typed in ["Work", " work ", "WORK"] {
            let t = resolve_thread_name(&db, typed).await.unwrap().found().expect(typed);
            assert_eq!(t.name, "Work");
        }
    }

    #[tokio::test]
    async fn no_exact_match_offers_close_names() {
        let db = db_with(&["Work Archive", "Garden"]).await;
        match resolve_thread_name(&db, "Work").await.unwrap() {
            ThreadNameMatch::NoMatch { close } => assert_eq!(close, vec!["Work Archive".to_string()]),
            other => panic!("{other:?}"),
        }
        match resolve_thread_name(&db, "Submarine").await.unwrap() {
            ThreadNameMatch::NoMatch { close } => assert!(close.is_empty()),
            other => panic!("{other:?}"),
        }
    }

    #[tokio::test]
    async fn two_exact_names_are_ambiguous() {
        let db = db_with(&["Work", "work"]).await;
        assert!(matches!(resolve_thread_name(&db, "WORK").await.unwrap(), ThreadNameMatch::Ambiguous(v) if v.len() == 2));
        assert!(existing_thread_for_find_or_create(&db, "Work").await.is_err());
    }
}
