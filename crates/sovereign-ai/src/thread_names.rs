//! The assistant's side of exact thread names (v0.0.12 item 6, A1;
//! `doc/plans/V0012-lot-assistant.md`): a thread named in an action resolves
//! through `sovereign_db::resolve_thread_name` — the one exact-name
//! resolution — and when it does not resolve, the user is told why, in words:
//! the close names to choose from (at most five) and a question, that no
//! thread has the name, or the threads that share it with their number of
//! documents.

use sovereign_db::schema::Thread;
use sovereign_db::{GraphDB, ThreadNameMatch};

use crate::document_titles::Unresolved;

/// The one live thread named `name` (ignoring case and surrounding spaces),
/// or the sentence that tells the user why there is none to act on.
pub(crate) async fn resolve_thread_or_explain(db: &dyn GraphDB, name: &str) -> Result<Thread, String> {
    resolve_thread(db, name).await.map_err(|u| u.message().to_string())
}

/// As [`resolve_thread_or_explain`], with the kind of failure kept — no
/// match, ambiguous, unreadable — so that the assistant's actions can add a
/// suggestion to a no-match reply and weigh a name read two ways by whether
/// each reading resolved (v0.0.12 item 23, T7).
pub(crate) async fn resolve_thread(db: &dyn GraphDB, name: &str) -> Result<Thread, Unresolved> {
    let wanted = name.trim();
    let m = sovereign_db::resolve_thread_name(db, wanted)
        .await
        .map_err(|e| Unresolved::Unreadable(format!("the threads could not be read ({e})")))?;
    match m {
        ThreadNameMatch::Found(t) => Ok(t),
        ThreadNameMatch::NoMatch { close } if close.is_empty() => {
            Err(Unresolved::NoMatch(format!("no thread is named '{wanted}'")))
        }
        ThreadNameMatch::NoMatch { close } => {
            let names: Vec<String> = close.iter().map(|n| format!("'{n}'")).collect();
            Err(Unresolved::NoMatch(format!(
                "no thread is named exactly '{wanted}'. Close names: {}. Which one do you mean?",
                names.join(", ")
            )))
        }
        ThreadNameMatch::Ambiguous(threads) => {
            let mut listed = vec![];
            for t in &threads {
                let count = match t.id_string() {
                    Some(id) => db.list_documents(Some(&id)).await.map(|d| d.len()).unwrap_or(0),
                    None => 0,
                };
                listed.push(format!(
                    "'{}' ({}, created {})",
                    t.name,
                    documents(count),
                    t.created_at.format("%Y-%m-%d %H:%M")
                ));
            }
            Err(Unresolved::Ambiguous(format!(
                "{} threads are named '{wanted}': {}. Which one do you mean? Rename one of them to tell them apart.",
                threads.len(),
                listed.join("; ")
            )))
        }
    }
}

/// "1 document", "5 documents".
pub(crate) fn documents(n: usize) -> String {
    if n == 1 {
        "1 document".to_string()
    } else {
        format!("{n} documents")
    }
}
