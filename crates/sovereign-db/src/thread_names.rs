//! Threads named by a caller resolve by **exact name** (v0.0.12 item 6, A1;
//! `doc/plans/V0012-lot-assistant.md`).
//!
//! The single-match lookups (`find_thread_by_name`, `…_token_hashes`) return
//! the first blind-index match, with no preference for the exact name: "Work"
//! could resolve to "Work Archive". [`resolve_thread_name`] is the one
//! resolution every caller uses instead: it takes **every** candidate the
//! store's name lookup offers (the blind index on the encrypted layer), and
//! compares their decrypted names to the given one, ignoring case and
//! surrounding spaces. Only live threads count; a thread merged into another
//! (C-MERGE, spec ch. 03, MG3) is not a live thread of its name, and a
//! find-or-create by its name goes to its live end.

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
    /// No live thread has that name, and exactly one thread merged into
    /// another (ended, C-MERGE MG3) has it.
    Ended(Thread),
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
    let (exact, others): (Vec<Thread>, Vec<Thread>) =
        candidates.into_iter().partition(|t| same_thread_name(&t.name, wanted));
    let (mut exact, mut ended): (Vec<Thread>, Vec<Thread>) = exact.into_iter().partition(|t| !t.is_ended());
    if exact.is_empty() && !ended.is_empty() {
        return Ok(if ended.len() == 1 {
            ThreadNameMatch::Ended(ended.remove(0))
        } else {
            ThreadNameMatch::Ambiguous(ended)
        });
    }
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

/// What a find-or-create caller (the browser's « Web » thread, the daily
/// journal, the import) does with a name (spec ch. 03, *Two threads of the
/// name sought: asked once, remembered*, FC1, FC2, FC4).
#[derive(Debug, Clone)]
pub enum FindOrCreate {
    /// Use this thread: the one live thread of the name, the live end of an
    /// ended one, or the one remembered while its choice holds (FC2).
    Use(String),
    /// No live thread has the name: the caller creates it.
    Create,
    /// Two or more live threads have the name and no choice holds: ask which
    /// (FC1), among these.
    Ask(Vec<Thread>),
}

/// FC1, FC2, FC4: what to do with `name`, a remembered choice included while
/// the live threads of that name are exactly the ones it was chosen among.
pub async fn find_or_create_choice<D: GraphDB + ?Sized>(db: &D, name: &str) -> Result<FindOrCreate, String> {
    let found = resolve_thread_name(db, name).await.map_err(|e| e.to_string())?;
    let live: Vec<String> = match &found {
        ThreadNameMatch::Found(t) => t.id_string().into_iter().collect(),
        ThreadNameMatch::Ambiguous(ts) if ts.iter().all(|t| !t.is_ended()) => ts.iter().filter_map(Thread::id_string).collect(),
        _ => Vec::new(),
    };
    // FC4: a choice holds while the live threads of the name are exactly the
    // ones it was chosen among; otherwise it is forgotten.
    let kept = db.thread_choice(name).await.map_err(|e| e.to_string())?;
    let holding = match kept {
        Some(c) if same_set(&c.among, &live) && live.contains(&c.chosen) => Some(c.chosen),
        Some(_) => {
            db.forget_thread_choice(name).await.map_err(|e| e.to_string())?;
            None
        }
        None => None,
    };
    match found {
        ThreadNameMatch::Found(t) => t.id_string().map(FindOrCreate::Use).ok_or_else(|| "a thread with no id".to_string()),
        ThreadNameMatch::Ended(t) => match live_end(db, &t).await.map_err(|e| e.to_string())? {
            Some(end) => end.id_string().map(FindOrCreate::Use).ok_or_else(|| "a thread with no id".to_string()),
            None => Err(no_live_end_sentence(db, &t).await),
        },
        ThreadNameMatch::NoMatch { .. } => Ok(FindOrCreate::Create),
        ThreadNameMatch::Ambiguous(ts) if ts.iter().all(|t| !t.is_ended()) => Ok(match holding {
            Some(chosen) => FindOrCreate::Use(chosen),
            None => FindOrCreate::Ask(ts),
        }),
        // Only ended threads of the name, several of them: nothing to ask among.
        ThreadNameMatch::Ambiguous(ts) => Err(format!(
            "{} threads are named '{}'; none was picked — rename one of them first",
            ts.len(),
            name.trim()
        )),
    }
}

/// The same ids, in any order.
fn same_set(a: &[String], b: &[String]) -> bool {
    let mut a = a.to_vec();
    let mut b = b.to_vec();
    a.sort();
    a.dedup();
    b.sort();
    b.dedup();
    a == b
}

/// FC2, FC3: remembers that `chosen` is the thread to use for `name`, among
/// the live threads of that name now, in this session's own store, sealed at
/// rest as a thread's name is; it does not travel to other devices.
pub async fn remember_thread_choice<D: GraphDB + ?Sized>(db: &D, name: &str, chosen: &str) -> Result<(), String> {
    let among: Vec<String> = match resolve_thread_name(db, name).await.map_err(|e| e.to_string())? {
        ThreadNameMatch::Ambiguous(ts) if ts.iter().all(|t| !t.is_ended()) => {
            ts.iter().filter_map(Thread::id_string).collect()
        }
        _ => Vec::new(),
    };
    if !among.iter().any(|id| id == chosen) {
        return Err(format!(
            "'{}' is not one of the threads named '{}' now; nothing was remembered",
            chosen,
            name.trim()
        ));
    }
    let choice = crate::schema::ThreadChoice { name: name.trim().to_lowercase(), chosen: chosen.to_string(), among };
    db.remember_thread_choice(&choice).await.map_err(|e| e.to_string())
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
        ThreadNameMatch::Ended(t) => match live_end(db, &t).await.map_err(|e| e.to_string())? {
            Some(end) => Ok(end.id_string()),
            None => Err(no_live_end_sentence(db, &t).await),
        },
        ThreadNameMatch::NoMatch { .. } => Ok(None),
        ThreadNameMatch::Ambiguous(ts) => Err(format!(
            "{} threads are named '{}'; none was picked — rename one of them first",
            ts.len(),
            name.trim()
        )),
    }
}

/// C-MERGE MG3: the live end of `thread`: the thread at the end of its chain
/// of merges, followed within a bound (the number of threads). `thread`
/// itself when it is not ended. `None` when the chain has no live end: a loop
/// (two devices' merges), a deleted or unknown thread on the way.
pub async fn live_end<D: GraphDB + ?Sized>(db: &D, thread: &Thread) -> DbResult<Option<Thread>> {
    if thread.deleted_at.is_some() {
        return Ok(None);
    }
    if !thread.is_ended() {
        return Ok(Some(thread.clone()));
    }
    let bound = db.list_threads_including_deleted().await?.len();
    let mut current = thread.clone();
    for _ in 0..bound {
        let Some(next_id) = current.merged_into.clone() else { break };
        current = match db.get_thread(&next_id).await {
            Ok(t) => t,
            Err(crate::error::DbError::NotFound(_)) | Err(crate::error::DbError::InvalidId(_)) => return Ok(None),
            Err(e) => return Err(e),
        };
        if current.deleted_at.is_some() {
            return Ok(None);
        }
        if !current.is_ended() {
            return Ok(Some(current));
        }
    }
    Ok(None)
}

/// The name of the thread `thread` was merged into, as the sentences show it
/// (its id when it cannot be read, or when it has shrunk to its id).
pub async fn merged_into_name<D: GraphDB + ?Sized>(db: &D, thread: &Thread) -> String {
    let Some(id) = thread.merged_into.as_deref() else { return String::new() };
    match db.get_thread(id).await {
        Ok(t) if !t.name.trim().is_empty() => t.name,
        _ => id.to_string(),
    }
}

/// C-MERGE MG3, the words (the Fable maille, by delegation of Céline,
/// 2026-11-17): an ended thread with no live end.
pub fn no_live_end_words(name: &str, target: &str) -> String {
    format!("'{name}' has ended (merged into '{target}'), and no live thread takes its place. Nothing was changed.")
}

async fn no_live_end_sentence<D: GraphDB + ?Sized>(db: &D, thread: &Thread) -> String {
    no_live_end_words(thread.name.trim(), &merged_into_name(db, thread).await)
}

/// The day an ended thread ended, as shown (MG2: « the day shown is in the
/// device's local time »), `YYYY-MM-DD`; empty when it has no end date.
pub fn ended_day(thread: &Thread) -> String {
    thread
        .ended_at
        .map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

/// C-MERGE MG3, the words: what a request that names ended thread `thread`
/// is answered. « '{name}' was merged into '{target}' on {YYYY-MM-DD} and
/// takes no new document. Use '{live end}' instead? Nothing was changed. »,
/// or, with no live end, « '{name}' has ended (merged into '{target}'), and
/// no live thread takes its place. Nothing was changed. »
///
/// `day` is the end date as the caller shows it (its local time).
pub async fn ended_thread_answer<D: GraphDB + ?Sized>(db: &D, thread: &Thread, day: &str) -> DbResult<String> {
    let name = thread.name.trim();
    let target = merged_into_name(db, thread).await;
    Ok(match live_end(db, thread).await? {
        Some(end) => format!(
            "'{name}' was merged into '{target}' on {day} and takes no new document. Use '{}' instead? Nothing was changed.",
            end.name.trim()
        ),
        None => no_live_end_words(name, &target),
    })
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
