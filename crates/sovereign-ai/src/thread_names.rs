//! The assistant's side of exact thread names (v0.0.12 item 6, A1;
//! `doc/plans/V0012-lot-assistant.md`): a thread named in an action resolves
//! through `sovereign_db::resolve_thread_name` — the one exact-name
//! resolution — and when it does not resolve, the user is told why, in words:
//! the close names to choose from (at most five) and a question, that no
//! thread has the name, or the threads that share it with their number of
//! documents and their creation date in local time (C-HOST AT11).

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
        // C-MERGE MG3: an ended thread is still named; what a request may do
        // with it is the caller's (`ended_answer` for one that would put a
        // document in it).
        ThreadNameMatch::Found(t) | ThreadNameMatch::Ended(t) => Ok(t),
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
                    // AT11: local time, in History's format, as documents are.
                    crate::local_time::history_format(t.created_at)
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

/// The day thread `t` ended, in local time (MG2; AT11's offset), `YYYY-MM-DD`.
pub(crate) fn ended_day(t: &Thread) -> String {
    t.ended_at
        .map(|e| e.with_timezone(&crate::local_time::offset_at(e)).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// C-MERGE MG3: what a request that would put a document in thread `t` is
/// answered when `t` has ended — « '{name}' was merged into '{target}' on
/// {YYYY-MM-DD} and takes no new document. Use '{live end}' instead? Nothing
/// was changed. », or the sentence for no live end. `None`: `t` takes
/// documents.
pub(crate) async fn ended_answer(db: &dyn GraphDB, t: &Thread) -> Option<String> {
    if !t.is_ended() {
        return None;
    }
    Some(match sovereign_db::ended_thread_answer(db, t, &ended_day(t)).await {
        Ok(answer) => answer,
        Err(e) => format!("'{}' has ended, and the threads could not be read ({e}). Nothing was changed.", t.name.trim()),
    })
}

/// C-MERGE MG3: a merge into ended thread `t` — « '{into}' has ended (merged
/// into '{target}'): merge into '{live end}' instead. Nothing was changed. »,
/// or the sentence for no live end. `None`: `t` is not ended.
pub(crate) async fn into_ended_answer(db: &dyn GraphDB, t: &Thread) -> Option<String> {
    if !t.is_ended() {
        return None;
    }
    let name = t.name.trim();
    let target = sovereign_db::merged_into_name(db, t).await;
    Some(match sovereign_db::live_end(db, t).await {
        Ok(Some(end)) => format!(
            "'{name}' has ended (merged into '{target}'): merge into '{}' instead. Nothing was changed.",
            end.name.trim()
        ),
        Ok(None) => sovereign_db::no_live_end_words(name, &target),
        Err(e) => format!("'{name}' has ended, and the threads could not be read ({e}). Nothing was changed."),
    })
}

/// C-MERGE MG5, the words: « Undid the merge of '{name}' into '{target}':
/// '{name}' is live again ».
pub(crate) fn undone_words(name: &str, target: &str) -> String {
    format!("Undid the merge of '{name}' into '{target}': '{name}' is live again")
}

/// C-MERGE MG5, the words: « thread '{name}' is not merged into another
/// thread; nothing was undone ».
pub(crate) fn undo_refused_words(name: &str) -> String {
    format!("thread '{name}' is not merged into another thread; nothing was undone")
}
