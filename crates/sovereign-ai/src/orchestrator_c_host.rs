//! Contract C-HOST, the orchestrator's side (spec ch. 08, *Action targets —
//! identity, not position or name*, AT1–AT12; ch. 11, *A skill asked for in
//! the chat*). A child of `orchestrator`, so it reaches the orchestrator's
//! private state; everything here runs in the crate the hosts link, without a
//! window (D16).
//!
//! - **AT1** — [`Orchestrator::describe`] reads what a proposal shows from the
//!   store, by the ids resolved before it: its sentence and its
//!   [`ActionTarget`] (kind, id, displayed title, revision seen).
//! - **AT4** — [`Orchestrator::recheck`] reads it again after the approval;
//!   anything changed, the action is proposed again with a new request id; the
//!   object gone, it is refused. For Destruct, the target and everything it
//!   contains (each document's revision included); for the other levels, the
//!   target alone.
//! - **AT6** — request ids and deadlines are given in
//!   `Orchestrator::propose_bound` (orchestrator.rs).
//! - **AT7** — [`Undo`], recorded by request id when a rename, a move or a
//!   merge is carried out; [`Orchestrator::undo_request`] addresses the same
//!   ids.
//! - **AT9** — the skills asked for in the chat ([`Orchestrator::resolve_skill`],
//!   [`Orchestrator::run_skill`]): the orchestrator reaches the core skills
//!   itself, with no registry installed by the host.
//! - **AT10** — [`Orchestrator::resolve_create_document`] and
//!   [`Orchestrator::prepare_tool`]: no thread named, nothing is created.

use std::sync::atomic::AtomicU64;

use sha2::{Digest, Sha256};
use sovereign_core::interfaces::{ActionOutcome, OrchestratorEvent};
use sovereign_core::security::{ActionTarget, RequestId, TargetKind};
use sovereign_skills::{CoreSkill, SkillContext, SkillDocument, SkillOutput};

use super::{after_word, clean_name, find_ascii_ci, quoted_spans, unresolved_outcome, NameError, Orchestrator, Prepared, ReadName, ThreadRef};
use crate::document_titles::Unresolved;

/// How long a proposal waits for its decision, from when it is issued (ch. 08,
/// *Action Gravity*; AT6: the proposal carries the deadline).
pub(super) const PROPOSAL_SECONDS: i64 = 120;

/// AT4: how many times one request is proposed at most — the first proposal
/// and the proposals again after a change. Beyond, nothing is done.
pub(super) const MAX_PROPOSALS: usize = 3;

/// AT4: what a proposal made again after a change starts with.
pub(super) const PROPOSED_AGAIN: &str = "Changed since you were asked, so asked again: ";

/// The first request id of an orchestrator (AT6): from the clock, never 0
/// (the neutral `RequestId::default()`).
pub(super) fn first_request_id() -> AtomicU64 {
    let now = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(1);
    AtomicU64::new(u64::try_from(now).unwrap_or(1).max(1))
}

/// AT4: what the re-check after an approval found.
pub(super) enum Recheck {
    /// What the proposal showed is what is there: the action runs.
    Same,
    /// The object is gone, or the action has nothing left to do: this
    /// outcome, and nothing is done.
    Gone(ActionOutcome),
    /// It changed: proposed again, as it is now.
    Changed(Box<Prepared>),
}

/// AT4: a request that kept changing under its proposals.
pub(super) fn changed_again() -> ActionOutcome {
    ActionOutcome::Failed {
        reason: format!(
            "what was proposed changed again after {MAX_PROPOSALS} proposals; nothing was done. Ask again when it is settled."
        ),
    }
}

/// AT7: what undoes an action carried out, by the ids it acted on.
#[derive(Debug, Clone)]
pub(super) enum Undo {
    /// Thread `thread_id` was renamed from `old_name` to `new_name`.
    Rename { thread_id: String, old_name: String, new_name: String },
    /// Document `doc_id` was moved from `from_thread` to `to_thread`.
    /// `from_name` is the name the thread it came from had then, for a reply
    /// that names it once it is gone.
    Move { doc_id: String, title: String, from_thread: String, from_name: String, to_thread: String },
    /// Thread `source_id` was merged into another (C-MERGE MG1: it ended,
    /// nothing moved), under `merge_stamp`. Its undo is refused when the
    /// merge changed since (undone, or redone).
    Merge { source_id: String, source_name: String, merge_stamp: Option<String> },
}

/// AT9: a skill's parameters, read from the person's request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum SkillCall {
    FindReplace { find: String, replace: String },
}

/// AT1 for a chat write tool: the proposal's sentence and target, resolved
/// before it is proposed.
#[derive(Debug, Clone, Default)]
pub(super) struct ToolPrep {
    pub description: Option<String>,
    pub target: ActionTarget,
}

/// A thread as it is now, read by its id.
struct ThreadNow {
    id: String,
    name: String,
    description: String,
}

impl ThreadNow {
    fn thread_ref(&self) -> ThreadRef {
        ThreadRef { id: self.id.clone(), name: self.name.clone() }
    }
}

/// A short digest of `parts`, for what a proposal showed (AT4) and the
/// revision of a thread (AT1).
fn digest(parts: &[&str]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0x1f]);
    }
    h.finalize().iter().take(8).map(|b| format!("{b:02x}")).collect()
}

/// The revision of a document as stored: its write sequence and a digest of
/// its title, content and thread. Any edit, rename or move changes it.
fn doc_revision(d: &sovereign_db::schema::Document) -> String {
    format!("{}-{}", d.write_seq, digest(&[&d.title, &d.content, &d.thread_id]))
}

fn target(kind: TargetKind, id: &str, title: &str, revision: &str) -> ActionTarget {
    ActionTarget { kind, id: id.to_string(), title: title.to_string(), revision: revision.to_string() }
}

/// find_replace's two texts from the request: the first two quoted spans
/// after the word "replace" ("replace \"two\" with \"deux\"").
fn find_replace_params(query: &str) -> Option<(String, String)> {
    let from = after_word(query, "replace")?;
    let spans: Vec<String> = quoted_spans(query).into_iter().filter(|r| r.start >= from).map(|r| query[r].to_string()).collect();
    match spans.as_slice() {
        [find, replace, ..] if !find.is_empty() => Some((find.clone(), replace.clone())),
        _ => None,
    }
}

/// Every byte index of `needle` (ASCII, case-insensitive) in `hay`, last first.
fn all_ascii_ci_rev(hay: &str, needle: &str) -> Vec<usize> {
    let mut at = vec![];
    let mut from = 0;
    while let Some(rel) = find_ascii_ci(&hay[from..], needle) {
        at.push(from + rel);
        from += rel + 1;
        while from < hay.len() && !hay.is_char_boundary(from) {
            from += 1;
        }
    }
    at.reverse();
    at
}

/// The second line of a chat tool's refusal, for the model alone (DEL-0004:
/// the first line is the user's, and is the turn's reason).
const FOR_THE_MODEL: &str = "Ask the user this question; do not guess.";

/// What a chat tool whose object is not found says (AT1; DEL-0004): its first
/// line for the user — the reason, a question when the reason holds none,
/// "Nothing was changed." — and a second line for the model.
fn not_found(what: &str, why: &str, question: &str) -> String {
    let why = why.trim().trim_end_matches('.');
    if why.ends_with('?') {
        format!("{what}: {why} Nothing was changed.
{FOR_THE_MODEL}")
    } else {
        format!("{what}: {why}. {question} Nothing was changed.
{FOR_THE_MODEL}")
    }
}

/// A chat write tool neither proposed nor run (AT1, AT10): what the model is
/// told, and whether it is because a name did not resolve — no exact match, or
/// two or more (RT2: every later write of the turn is then shown).
#[derive(Debug, Clone)]
pub(super) struct ToolRefused {
    pub said: String,
    pub name_not_found: bool,
}

impl From<String> for ToolRefused {
    fn from(said: String) -> Self {
        ToolRefused { said, name_not_found: false }
    }
}

/// A chat tool's reply to a name that did not resolve (RT1: unchanged, the
/// close names included), marked as such when no object, or more than one,
/// has the name. A store that could not be read is not a name not found.
fn name_not_found(what: &str, u: &Unresolved, question: &str) -> ToolRefused {
    ToolRefused {
        said: not_found(what, u.message(), question),
        name_not_found: matches!(u, Unresolved::NoMatch(_) | Unresolved::Ambiguous(_)),
    }
}

/// RT4: what precedes a proposal shown because a name in the turn was not
/// found (the Fable maille, by delegation of Céline, 2026-11-18).
pub(super) const ASKED_AFTER_NO_MATCH: &str = "Asked because a name in this request was not found.";

/// RV1: how a version is named, in one of three forms and no other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum VersionName {
    /// « previous », « the previous version », « its previous version », or no
    /// version named: the version before the latest.
    Previous,
    /// A date and a time as History writes them, in local time
    /// (`YYYY-MM-DD HH:MM`).
    At(String),
    /// A date alone (`YYYY-MM-DD`), in local time.
    Day(String),
}

impl VersionName {
    /// The version as a reply names it.
    fn words(&self) -> String {
        match self {
            VersionName::Previous => "its previous version".into(),
            VersionName::At(m) => format!("its version saved {m}"),
            VersionName::Day(d) => format!("its version of {d}"),
        }
    }
}

/// RV1: what every reply that names no single version ends with.
const HOW_TO_NAME: &str =
    "To name a version, say 'previous', or its date and time as History shows them (YYYY-MM-DD HH:MM).";

/// RV1: the version `words` name, when they are one of the three forms.
/// Loose words (« yesterday's », « the one from Monday ») name none.
fn version_name(words: &str) -> Option<VersionName> {
    let w = unquoted(words.trim().trim_end_matches(['.', '!', '?']).trim());
    let lower = w.to_ascii_lowercase();
    if matches!(lower.as_str(), "previous" | "the previous version" | "its previous version") {
        return Some(VersionName::Previous);
    }
    // Strict forms: parsed, then written back identically, so « 2026-11-7 »
    // or « 2026-11-17 9:05 » are not read as a version.
    if let Ok(t) = chrono::NaiveDateTime::parse_from_str(&w, "%Y-%m-%d %H:%M") {
        if t.format("%Y-%m-%d %H:%M").to_string() == w {
            return Some(VersionName::At(w));
        }
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(&w, "%Y-%m-%d") {
        if d.format("%Y-%m-%d").to_string() == w {
            return Some(VersionName::Day(w));
        }
    }
    None
}

/// `s` without its quotes when it is one quoted span whole; else `s`.
fn unquoted(s: &str) -> String {
    match whole_quote(s) {
        Some(inner) => inner,
        None => s.to_string(),
    }
}

/// The contents of `s` when `s` is exactly one quoted span (its quotes
/// included).
fn whole_quote(s: &str) -> Option<String> {
    match quoted_spans(s).as_slice() {
        [r] if s[..r.start].chars().count() == 1 && s[r.end..].chars().count() == 1 => Some(s[r.clone()].to_string()),
        _ => None,
    }
}

/// RV2: the words of a restore — the person's own, after the word
/// « restore » — or, when her request holds none, the router's target. True
/// when they are the person's.
fn restore_words(target: Option<&str>, query: &str) -> Option<(String, bool)> {
    if let Some(at) = after_word(query, "restore") {
        let rest = query[at..].trim().trim_end_matches(['.', '!', '?']).trim();
        if !rest.is_empty() {
            return Some((rest.to_string(), true));
        }
    }
    target.map(str::trim).filter(|t| !t.is_empty()).map(|t| (t.to_string(), false))
}

/// A title in a restore's words. Quoted, it is taken as given. Unquoted and
/// read from the person's request, it is resolved as written, then stripped of
/// a leading « the » (T7). Taken from the router's target, it is taken as it
/// is.
fn title_part(s: &str, from_request: bool) -> ReadName {
    let s = s.trim();
    if let Some(inner) = whole_quote(s) {
        return ReadName { written: inner.clone(), stripped: inner, quoted: true };
    }
    let written = s.trim_end_matches(['.', '!', '?']).trim().to_string();
    if from_request {
        let stripped = clean_name(&written, &[]);
        ReadName { written, stripped, quoted: false }
    } else {
        ReadName::given(written)
    }
}

/// RV2: the byte index of the last « to » (as " to ") of `words` outside
/// quotes.
fn last_unquoted_to(words: &str) -> Option<usize> {
    let spans = quoted_spans(words);
    all_ascii_ci_rev(words, " to ").into_iter().find(|&at| !spans.iter().any(|r| at + 1 >= r.start && at < r.end))
}

/// RV1: the one version `name` names among `versions` (newest first), or the
/// reply that says why none is taken.
fn pick_version<'a>(
    versions: &'a [sovereign_docs::types::VersionInfo],
    title: &str,
    name: &VersionName,
) -> Result<&'a sovereign_docs::types::VersionInfo, ActionOutcome> {
    let shown = |v: &sovereign_docs::types::VersionInfo| crate::local_time::history_format(v.timestamp);
    let (matching, day): (Vec<&sovereign_docs::types::VersionInfo>, &str) = match name {
        // Newest first: [1] is the version before the latest.
        VersionName::Previous => {
            return versions.get(1).ok_or_else(|| ActionOutcome::Failed {
                reason: format!("'{title}' has no earlier version. Nothing was changed."),
            })
        }
        VersionName::At(m) => (versions.iter().filter(|v| shown(v) == *m).collect(), &m[..10]),
        VersionName::Day(d) => (versions.iter().filter(|v| shown(v).starts_with(d.as_str())).collect(), d.as_str()),
    };
    if let [one] = matching.as_slice() {
        return Ok(one);
    }
    let of_day: Vec<String> = versions.iter().map(shown).filter(|s| s.starts_with(day)).collect();
    let listed = of_day.join(", ");
    let reason = match (name, matching.len(), of_day.len()) {
        (_, _, 0) => format!("'{title}' has no version on {day}. Nothing was changed. {HOW_TO_NAME}"),
        (VersionName::At(m), 0, _) => format!(
            "'{title}' has no version saved at {m}. Its versions on {day}: {listed}. Which one do you mean? Nothing was changed. {HOW_TO_NAME}"
        ),
        // Several versions in the minute named read the same as History shows
        // them, so she cannot name either one here: she is sent to History
        // instead of being asked which (the Fable maille, by delegation of
        // Céline, 2026-11-19).
        (VersionName::At(m), n, _) => format!(
            "'{title}' has {n} versions saved at {m}. Its versions on {day}: {listed}. Versions saved in the same minute cannot be told apart here: open the document's History to choose between them. Nothing was changed. {HOW_TO_NAME}"
        ),
        // A date alone whose versions were all saved in one minute: they read
        // the same, so she is sent to History too (the Fable maille, by
        // delegation of Céline, 2026-11-19). A day with versions at different
        // minutes is still asked about, even when two of them share a minute.
        (_, _, n) if of_day.iter().all(|s| *s == of_day[0]) => format!(
            "'{title}' has {n} versions on {day}: {listed}. Versions saved in the same minute cannot be told apart here: open the document's History to choose between them. Nothing was changed. {HOW_TO_NAME}"
        ),
        (_, _, n) => format!(
            "'{title}' has {n} versions on {day}: {listed}. Which one do you mean? Nothing was changed. {HOW_TO_NAME}"
        ),
    };
    Err(ActionOutcome::Failed { reason })
}

/// A restore's sentence: « Restore 'Notes' to its previous version, saved … »
/// or « … to its version saved … », the date in local time, in History's
/// format.
pub(super) fn restore_sentence(verb: &str, title: &str, previous: bool, when: chrono::DateTime<chrono::Utc>) -> String {
    let when = crate::local_time::history_format(when);
    if previous {
        format!("{verb} '{title}' to its previous version, saved {when}")
    } else {
        format!("{verb} '{title}' to its version saved {when}")
    }
}

impl Orchestrator {
    /// The proposal for `intent` on what `p` resolved: the sentence, the
    /// thread and document ids, and the action target (AT1).
    pub(super) fn proposal_for(
        &self,
        intent: &sovereign_core::interfaces::UserIntent,
        p: &Prepared,
    ) -> sovereign_core::security::ProposedAction {
        let mut proposal = crate::action_gate::build_proposal(intent);
        if let Some(d) = &p.description {
            proposal.description = d.clone();
        }
        if let Some(t) = &p.thread {
            proposal.thread_id = Some(t.id.clone());
        }
        if let Some((doc_id, _)) = &p.doc {
            proposal.doc_id = Some(doc_id.clone());
        }
        proposal.target = p.target.clone();
        proposal
    }

    /// The thread `t` as it is now, or why it cannot be acted on.
    async fn thread_now(&self, t: &ThreadRef) -> Result<ThreadNow, ActionOutcome> {
        match self.db.get_thread(&t.id).await {
            Ok(row) if row.deleted_at.is_none() => {
                Ok(ThreadNow { id: t.id.clone(), name: row.name, description: row.description })
            }
            _ => Err(ActionOutcome::Failed { reason: format!("thread '{}' no longer exists; nothing was done", t.name) }),
        }
    }

    /// The document `id` as it is now, or why it cannot be acted on.
    /// C-TPII: the document as stored (its revision is the stored one), and
    /// its title as shown (TP3).
    async fn doc_now(&self, id: &str, title: &str) -> Result<(sovereign_db::schema::Document, String), ActionOutcome> {
        let n = self.document_still_live(id, title).await?;
        Ok((n.doc, n.shown))
    }

    /// The documents a thread holds, each with its revision, in a stable
    /// order (AT4, Destruct: everything it contains).
    async fn contents(&self, thread_id: &str) -> Vec<String> {
        let mut v: Vec<String> = self
            .db
            .list_documents(Some(thread_id))
            .await
            .unwrap_or_default()
            .iter()
            .map(|d| format!("{}@{}", d.id_string().unwrap_or_default(), doc_revision(d)))
            .collect();
        v.sort();
        v
    }

    /// AT1 and AT4: what the proposal of `action` shows, read from the store by
    /// the ids `p` holds — its sentence, its action target, and the digest the
    /// re-check compares. Called when the action is prepared and again after
    /// the approval. `Err`: the object is gone, or nothing is left to do.
    pub(super) async fn describe(&self, action: &str, mut p: Prepared) -> Result<Prepared, ActionOutcome> {
        match action {
            "rename_thread" | "split_thread" | "create_milestone" | "create_document" => {
                let Some(t) = p.thread.clone() else { return Ok(p) };
                let t = self.thread_now(&t).await?;
                let rev = digest(&["thread", &t.id, &t.name, &t.description]);
                p.description = Some(match action {
                    "rename_thread" => {
                        let new = p.new_name.clone().unwrap_or_default();
                        if new == t.name.trim() {
                            return Err(ActionOutcome::NoChange {
                                reason: format!("thread '{}' already has that name", t.name),
                            });
                        }
                        // AT12: the new name between quotation marks, as written.
                        format!("Rename thread '{}' to '{}'", t.name, new)
                    }
                    "split_thread" => format!(
                        "Create an empty thread '{}' to split '{}' (documents can then be moved into it)",
                        p.new_name.clone().unwrap_or_default(),
                        t.name
                    ),
                    "create_milestone" => {
                        format!("Create milestone '{}' on '{}'", p.title.clone().unwrap_or_default(), t.name)
                    }
                    _ => format!("Create document '{}' in '{}'", p.title.clone().unwrap_or_default(), t.name),
                });
                p.target = target(TargetKind::Thread, &t.id, &t.name, &rev);
                p.seen = rev;
                p.thread = Some(t.thread_ref());
            }
            "delete_thread" => {
                let Some(t) = p.thread.clone() else { return Ok(p) };
                let t = self.thread_now(&t).await?;
                let contents = self.contents(&t.id).await;
                let follows = match contents.len() {
                    0 => "it holds no documents".to_string(),
                    1 => "its 1 document goes to the trash".to_string(),
                    n => format!("its {n} documents go to the trash"),
                };
                let mut parts = vec!["thread", t.id.as_str(), t.name.as_str(), t.description.as_str()];
                parts.extend(contents.iter().map(String::as_str));
                let rev = digest(&parts);
                p.description = Some(format!("Delete thread '{}': {follows}", t.name));
                p.target = target(TargetKind::Thread, &t.id, &t.name, &rev);
                p.seen = rev;
                p.thread = Some(t.thread_ref());
            }
            sovereign_core::security::UNDO_MERGE_ACTION => {
                let Some(t) = p.thread.clone() else { return Ok(p) };
                let row = match self.db.get_thread(&t.id).await {
                    Ok(row) if row.deleted_at.is_none() => row,
                    _ => {
                        return Err(ActionOutcome::Failed {
                            reason: format!("thread '{}' no longer exists; nothing was done", t.name),
                        })
                    }
                };
                if !row.is_ended() {
                    return Err(ActionOutcome::Failed { reason: crate::thread_names::undo_refused_words(&row.name) });
                }
                let into = sovereign_db::merged_into_name(self.db.as_ref(), &row).await;
                let stamp = row.merge_stamp.clone().unwrap_or_default();
                let rev = digest(&["thread", &t.id, &row.name, &stamp]);
                p.description = Some(format!("Undo the merge of '{}' into '{into}': '{}' is live again", row.name, row.name));
                p.target = target(TargetKind::Thread, &t.id, &row.name, &rev);
                p.seen = rev;
            }
            "merge_threads" => {
                let (Some(into), Some(from)) = (p.thread.clone(), p.other.clone()) else { return Ok(p) };
                let into = self.thread_now(&into).await?;
                let from = self.thread_now(&from).await?;
                if into.id == from.id {
                    return Err(ActionOutcome::NoChange {
                        reason: format!("'{}' and '{}' are the same thread", into.name, from.name),
                    });
                }
                // C-MERGE MG1, the words (the Fable maille, by delegation of
                // Céline, 2026-11-17): the source ends and keeps its documents.
                let (f, i) = (from.name.as_str(), into.name.as_str());
                p.description = Some(match self.contents(&from.id).await.len() {
                    0 => format!("Merge thread '{f}' into '{i}': '{f}' ends now; it holds no documents"),
                    1 => format!("Merge thread '{f}' into '{i}': '{f}' ends now and keeps its 1 document; new ones go to '{i}'"),
                    n => format!("Merge thread '{f}' into '{i}': '{f}' ends now and keeps its {n} documents; new ones go to '{i}'"),
                });
                // MG6: level 3, so the re-check is the target alone (AT4): the
                // thread merged away, not the documents it keeps.
                let rev = digest(&["thread", &from.id, &from.name, &from.description]);
                // The target is the thread merged away (the one that ends, and
                // the one an undo makes live again, AT7); the proposal also
                // showed the thread kept, by name.
                p.target = target(TargetKind::Thread, &from.id, &from.name, &rev);
                p.seen = digest(&[&rev, &into.id, &into.name]);
                p.thread = Some(into.thread_ref());
                p.other = Some(from.thread_ref());
            }
            "move_document" => {
                let (Some(dest), Some((doc_id, title))) = (p.thread.clone(), p.doc.clone()) else { return Ok(p) };
                let (doc, shown) = self.doc_now(&doc_id, &title).await?;
                let dest = self.thread_now(&dest).await?;
                if doc.thread_id == dest.id {
                    return Err(ActionOutcome::NoChange { reason: format!("'{shown}' is already in '{}'", dest.name) });
                }
                let from = match self.db.get_thread(&doc.thread_id).await {
                    Ok(row) if row.deleted_at.is_none() => format!(" from '{}'", row.name),
                    _ => String::new(),
                };
                let rev = doc_revision(&doc);
                p.description = Some(format!("Move document '{shown}'{from} to '{}'", dest.name));
                p.target = target(TargetKind::Document, &doc_id, &shown, &rev);
                p.seen = digest(&[&rev, &dest.id, &dest.name]);
                p.thread = Some(dest.thread_ref());
                p.doc = Some((doc_id, shown));
            }
            "adopt" | "restore" | "word_count" | "find_replace" | "duplicate" => {
                let Some((doc_id, title)) = p.doc.clone() else { return Ok(p) };
                let (doc, t) = self.doc_now(&doc_id, &title).await?;
                let rev = doc_revision(&doc);
                p.description = Some(match action {
                    "adopt" => {
                        if doc.is_owned {
                            return Err(ActionOutcome::NoChange { reason: format!("'{t}' is already yours") });
                        }
                        format!("Adopt '{t}' as your own document")
                    }
                    "restore" => self.describe_restore(&t, &mut p).await,
                    "word_count" => format!("Count the words of '{t}' (the Word Count skill)"),
                    "find_replace" => match &p.skill {
                        Some(SkillCall::FindReplace { find, replace }) => format!(
                            "Find and replace in '{t}': replace \"{find}\" with \"{replace}\" (the Find & Replace skill)"
                        ),
                        _ => format!("Find and replace in '{t}' (the Find & Replace skill)"),
                    },
                    _ => {
                        let thread = match self.db.get_thread(&doc.thread_id).await {
                            Ok(row) if row.deleted_at.is_none() => format!(" in '{}'", row.name),
                            _ => String::new(),
                        };
                        format!("Duplicate '{t}' as 'Copy of {t}'{thread} (the Duplicate skill)")
                    }
                });
                p.target = target(TargetKind::Document, &doc_id, &t, &rev);
                p.seen = rev;
                p.doc = Some((doc_id, t));
            }
            "swap_model" => {
                let Some(path) = p.model.clone() else { return Ok(p) };
                let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                p.description = Some(format!("Switch to the model {name}"));
                // A file, not a stored object: nothing to compare after the
                // approval; the swap checks that it is still there.
                p.target = target(TargetKind::Model, &name, &name, "");
                p.seen = String::new();
            }
            _ => {}
        }
        Ok(p)
    }

    /// AT1 and RV1: a restore names the version it restores — its date, in
    /// local time and History's format. The version was chosen and pinned when
    /// the request was read ([`Self::resolve_restore`]); it is kept as pinned,
    /// so the proposal, the re-check after the approval and the restore all
    /// name the same one.
    async fn describe_restore(&self, title: &str, p: &mut Prepared) -> String {
        match &p.version {
            Some((_, when)) => restore_sentence("Restore", title, p.version_previous, *when),
            None => format!("Restore '{title}' to its previous version"),
        }
    }

    /// RV1 and RV2: what a restore acts on, read from the person's words — the
    /// document, and the version she named — before anything is proposed.
    /// `Err`: nothing is restored or proposed, and the reply says why.
    pub(super) async fn resolve_restore(&self, target: Option<&str>, query: &str) -> Result<Prepared, ActionOutcome> {
        let Some((words, from_request)) = restore_words(target, query) else {
            return Err(ActionOutcome::Failed { reason: "restore: no target given".into() });
        };
        let whole = title_part(&words, from_request);
        // The words before the last « to » outside quotes, and the words after it.
        let cut = last_unquoted_to(&words).map(|at| (title_part(&words[..at], from_request), words[at + 4..].trim().to_string()));
        // RV2: the second reading exists only when the words after the last
        // « to » are a version name in one of RV1's forms.
        let named = cut.as_ref().and_then(|(title, after)| version_name(after).map(|v| (title.clone(), v)));

        let first = self.document_read(&whole).await;
        let second = match &named {
            Some((title, _)) => Some(self.document_read(title).await),
            None => None,
        };
        let two_ways = |n: &ReadName| {
            ActionOutcome::Failed {
                reason: format!(
                    "\"{}\" can be read 2 ways: '{}', or '{}'. Nothing was changed; put the title in quotes (restore 'title').",
                    words, n.written, n.stripped
                ),
            }
        };
        let (doc, version, parsed) = match (first, second) {
            (Err(NameError::TwoReadings), _) => return Err(two_ways(&whole)),
            (_, Some(Err(NameError::TwoReadings))) => {
                let title = named.as_ref().map(|(t, _)| t.clone()).unwrap_or_else(|| whole.clone());
                return Err(two_ways(&title));
            }
            // Both readings find a document: nothing is changed, and both are named.
            (Ok(a), Some(Ok(b))) => {
                let v = named.as_ref().map(|(_, v)| v.clone()).unwrap_or(VersionName::Previous);
                let after = cut.as_ref().map(|(_, a)| a.as_str()).unwrap_or_default();
                return Err(ActionOutcome::Failed {
                    reason: format!(
                        "\"{words}\" can be read 2 ways: '{a}' to its previous version, or '{b}' to {v}. Nothing was changed; put the title in quotes (restore '{a}', or restore '{b}' to {after}).",
                        a = a.shown,
                        b = b.shown,
                        v = v.words(),
                    ),
                });
            }
            // One reading finds a document: it is the one taken. A whole or
            // quoted title keeps the trust path; a title cut at « to » out of
            // unquoted words is always proposed.
            (Ok(a), _) => (a, VersionName::Previous, false),
            (Err(_), Some(Ok(b))) => {
                let (title, v) = named.clone().unwrap_or((whole.clone(), VersionName::Previous));
                (b, v, !title.quoted)
            }
            (Err(NameError::Refused(no)), _) => {
                // Neither reading finds a document. When the words before the
                // last « to » are a document's title, the words after it named
                // no version: answered as RV1 answers for that document.
                if let Some((title, after)) = &cut {
                    if named.is_none() {
                        if let Ok(d) = self.document_read(title).await {
                            return Err(self.no_version_named(&d, after).await);
                        }
                    }
                }
                return Err(no);
            }
        };

        let Some(svc) = self.document_service() else {
            return Err(ActionOutcome::Failed { reason: "the workspace is locked".into() });
        };
        let doc_id = doc.doc.id_string().unwrap_or_default();
        let versions = svc.versions(&doc_id).await.map_err(|e| ActionOutcome::Failed { reason: crate::docs_host::doc_error_message(&e) })?;
        let picked = pick_version(&versions, &doc.shown, &version)?;
        // RV1: the version named is already the current one.
        if let Ok(c) = self.db.get_commit(&picked.commit_id).await {
            if c.snapshot.title == doc.doc.title && c.snapshot.content == doc.doc.content {
                return Err(ActionOutcome::NoChange {
                    reason: format!("This is already the current version of '{}'. Nothing was changed.", doc.shown),
                });
            }
        }
        Ok(Prepared {
            doc: Some((doc_id, doc.shown)),
            version: Some((picked.commit_id.clone(), picked.timestamp)),
            version_previous: version == VersionName::Previous,
            parsed,
            ..Prepared::default()
        })
    }

    /// RV1, words that are no version name: the document's latest versions (at
    /// most five), and how to name one.
    async fn no_version_named(&self, doc: &crate::document_titles::Named, after: &str) -> ActionOutcome {
        let title = &doc.shown;
        let versions = match self.document_service() {
            Some(svc) => match svc.versions(&doc.doc.id_string().unwrap_or_default()).await {
                Ok(v) => v,
                Err(e) => return ActionOutcome::Failed { reason: crate::docs_host::doc_error_message(&e) },
            },
            None => return ActionOutcome::Failed { reason: "the workspace is locked".into() },
        };
        let after = after.trim().trim_end_matches(['.', '!', '?']).trim();
        let reason = if versions.is_empty() {
            format!("'{after}' names no version of '{title}', which has no saved version yet. Nothing was changed. {HOW_TO_NAME}")
        } else {
            let latest: Vec<String> = versions.iter().take(5).map(|v| crate::local_time::history_format(v.timestamp)).collect();
            format!(
                "'{after}' names no version of '{title}'. Its latest versions: {}. Nothing was changed. {HOW_TO_NAME}",
                latest.join(", ")
            )
        };
        ActionOutcome::Failed { reason }
    }

    /// AT4: what the proposal showed, read again after the approval.
    pub(super) async fn recheck(&self, action: &str, p: &Prepared) -> Recheck {
        if p.seen.is_empty() {
            return Recheck::Same;
        }
        match self.describe(action, p.clone()).await {
            Ok(fresh) if fresh.seen == p.seen => Recheck::Same,
            Ok(fresh) => Recheck::Changed(Box::new(fresh)),
            Err(outcome) => Recheck::Gone(outcome),
        }
    }

    /// AT10, direct request: the thread a new document goes in, named in the
    /// router's target ("Minutes in Garden") or at the end of the request
    /// ("… in Garden"), by its exact name. None named: nothing is created, and
    /// the reply asks which thread.
    pub(super) async fn resolve_create_document(&self, target: Option<&str>, query: &str) -> Result<Prepared, ActionOutcome> {
        let raw = target.map(str::trim).filter(|t| !t.is_empty()).unwrap_or("Untitled Document");
        let mut readings: Vec<(String, String)> = vec![];
        for at in all_ascii_ci_rev(raw, " in ") {
            readings.push((raw[..at].trim().to_string(), raw[at + 4..].trim().to_string()));
        }
        if let Some(&at) = all_ascii_ci_rev(query, " in ").first() {
            let tail = query[at + 4..].trim().trim_end_matches(['.', '!', '?']).trim().to_string();
            readings.push((raw.to_string(), tail.clone()));
            readings.push((raw.to_string(), clean_name(&tail, &[])));
        }
        for (title, thread) in readings {
            if title.is_empty() || thread.is_empty() {
                continue;
            }
            match self.thread_lookup(&thread).await {
                Ok(t) => {
                    // C-MERGE MG3: an ended thread takes no new document.
                    self.refuse_ended(&t).await?;
                    return Ok(Prepared { thread: Some(t), title: Some(title), parsed: true, ..Prepared::default() });
                }
                Err(Unresolved::NoMatch(_)) => continue,
                Err(u) => return Err(unresolved_outcome(u, "name", false)),
            }
        }
        let any = self.db.list_threads().await.map(|ts| ts.iter().any(|t| t.deleted_at.is_none())).unwrap_or(false);
        Err(ActionOutcome::Failed {
            reason: if any {
                format!("which thread should '{raw}' go in? Name it: create a document '{raw}' in <thread>. Nothing was created")
            } else {
                format!("there is no thread yet to put '{raw}' in; which thread should it go in? Create the thread first. Nothing was created")
            },
        })
    }

    /// AT9: the document a skill runs on, by its exact title, and what the
    /// skill needs from the request.
    pub(super) async fn resolve_skill(&self, action: &str, target: Option<&str>, query: &str) -> Result<Prepared, ActionOutcome> {
        let Some(target) = target.filter(|t| !t.trim().is_empty()) else {
            return Err(ActionOutcome::Failed { reason: format!("{action}: no document given") });
        };
        let doc = self.document_named(target).await?;
        let doc_id = doc.doc.id_string().unwrap_or_default();
        let skill = match action {
            "find_replace" => {
                let Some((find, replace)) = find_replace_params(query) else {
                    return Err(ActionOutcome::Failed {
                        reason: "find and replace needs the text to find and its replacement, each in quotes: replace \"old\" with \"new\". Nothing was changed".into(),
                    });
                };
                Some(SkillCall::FindReplace { find, replace })
            }
            _ => None,
        };
        Ok(Prepared { doc: Some((doc_id, doc.shown)), skill, ..Prepared::default() })
    }

    /// AT9: runs the skill on the document resolved before; says which skill
    /// ran and what it did.
    pub(super) async fn run_skill(&self, action: &str, p: Prepared) -> ActionOutcome {
        let Some((doc_id, title)) = p.doc else { return self.failed(action, "no document given") };
        // C-TPII: `doc` as stored (the skill reads it, a copy's title is made
        // from it); `title` as shown, for the replies.
        let (doc, title) = match self.document_still_live(&doc_id, &title).await {
            Ok(n) => (n.doc, n.shown),
            Err(o) => return o,
        };
        let Some(svc) = self.document_service() else {
            return self.failed("", "the workspace is locked; nothing was done");
        };
        let view = match svc.read(&doc_id).await {
            Ok(v) => v,
            Err(e) => return self.failed("", crate::docs_host::doc_error_message(&e)),
        };
        let fields = crate::docs_host::view_fields(&view);
        let skill_doc = SkillDocument { id: doc_id.clone(), title: doc.title.clone(), content: fields.clone() };
        let ctx_for = |s: &dyn CoreSkill| SkillContext {
            granted: s.required_capabilities().into_iter().collect(),
            db: None,
            llm: None,
        };
        match action {
            "word_count" => {
                let skill = sovereign_skills::skills::word_count::WordCountSkill;
                match skill.execute("count", &skill_doc, "", &ctx_for(&skill)) {
                    Ok(SkillOutput::StructuredData { kind, json }) => {
                        let v: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
                        let words = v["words"].as_u64().unwrap_or(0);
                        let chars = v["characters"].as_u64().unwrap_or(0);
                        self.log_action("word_count", &title);
                        let _ = self.event_tx.send(OrchestratorEvent::SkillResult {
                            skill: skill.name().into(),
                            action: "count".into(),
                            kind,
                            data: json,
                        });
                        ActionOutcome::Reported {
                            summary: format!(
                                "Word Count skill on '{title}': {words} word{}, {chars} character{}",
                                if words == 1 { "" } else { "s" },
                                if chars == 1 { "" } else { "s" }
                            ),
                        }
                    }
                    Ok(_) => self.failed("Word Count skill", "it gave no count"),
                    Err(e) => self.failed("Word Count skill", e),
                }
            }
            "find_replace" => {
                let Some(SkillCall::FindReplace { find, replace }) = p.skill else {
                    return self.failed("Find & Replace skill", "no text to find was given");
                };
                let skill = sovereign_skills::skills::find_replace::FindReplaceSkill;
                let params = serde_json::json!({ "find": find, "replace": replace }).to_string();
                match skill.execute("find_replace", &skill_doc, &params, &ctx_for(&skill)) {
                    Ok(SkillOutput::ContentUpdate(cf)) => {
                        let n = fields.body.matches(find.as_str()).count();
                        let edit = match crate::docs_host::skill_output_edit(&fields, &cf) {
                            Ok(Some(edit)) => edit,
                            Ok(None) => {
                                return ActionOutcome::NoChange { reason: format!("\"{find}\" does not occur in '{title}'") }
                            }
                            Err(why) => return self.failed("Find & Replace skill", why),
                        };
                        match svc.apply_skill_output(&doc_id, view.revision.clone(), skill.name(), edit).await {
                            Ok(out) if out.changed => {
                                self.log_action("find_replace", &title);
                                let _ = self.event_tx.send(OrchestratorEvent::DocumentOpened { doc_id });
                                ActionOutcome::Changed {
                                    summary: format!(
                                        "Find & Replace skill: replaced {n} occurrence{} of \"{find}\" with \"{replace}\" in '{title}'",
                                        if n == 1 { "" } else { "s" }
                                    ),
                                }
                            }
                            Ok(_) => ActionOutcome::NoChange { reason: format!("'{title}' was left as it was") },
                            Err(e) => self.failed("Find & Replace skill", crate::docs_host::doc_error_message(&e)),
                        }
                    }
                    Ok(_) => ActionOutcome::NoChange { reason: format!("\"{find}\" does not occur in '{title}'") },
                    Err(e) => self.failed("Find & Replace skill", e),
                }
            }
            "duplicate" => {
                // C-TPII: the copy's title is made from the stored one, so its
                // tokens stay tokens, never labels.
                let stored_copy = format!("Copy of {}", doc.title);
                let copy = format!("Copy of {title}");
                let origin = sovereign_docs::Origin::Skill { name: "duplicate-document".into() };
                match crate::docs_host::duplicate(&svc, &doc_id, stored_copy, doc.thread_id.clone(), origin).await {
                    Ok(out) => {
                        self.log_action("duplicate", &title);
                        let _ = self.event_tx.send(OrchestratorEvent::DocumentCreated {
                            doc_id: out.id,
                            title: copy.clone(),
                            thread_id: doc.thread_id.clone(),
                        });
                        ActionOutcome::Changed { summary: format!("Duplicate skill: copied '{title}' as '{copy}'") }
                    }
                    Err(e) => self.failed("Duplicate skill", crate::docs_host::doc_error_message(&e)),
                }
            }
            _ => self.failed(action, "this skill is not available yet"),
        }
    }

    /// AT1 and AT10 for a chat write tool: what it acts on, resolved before it
    /// is proposed (Céline, 2026-11-14: « Oui, partout »). An object that is
    /// not found, or not named, is neither proposed nor run: `Err` is what the
    /// model is told — that it cannot be found, and a question to ask.
    pub(super) async fn prepare_tool(&self, call: &crate::tools::ToolCall) -> Result<ToolPrep, ToolRefused> {
        let arg = |k: &str| call.arguments.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
        let thread_target = |t: &sovereign_db::schema::Thread| {
            let id = t.id_string().unwrap_or_default();
            let rev = digest(&["thread", &id, &t.name, &t.description]);
            target(TargetKind::Thread, &id, &t.name, &rev)
        };
        match call.name.as_str() {
            "create_document" => {
                let title = arg("title").unwrap_or("Untitled Document");
                let Some(name) = arg("thread_name") else {
                    return Err(format!(
                        "Document not created: no thread was named. Which thread should '{title}' go in? Nothing was created.
{FOR_THE_MODEL}"
                    ).into());
                };
                match crate::thread_names::resolve_thread(self.db.as_ref(), name).await {
                    Ok(t) => {
                        // C-MERGE MG3: an ended thread takes no new document.
                        if let Some(answer) = crate::thread_names::ended_answer(self.db.as_ref(), &t).await {
                            return Err(format!("Document not created: {answer}\n{FOR_THE_MODEL}").into());
                        }
                        Ok(ToolPrep {
                            description: Some(format!("Create document '{title}' in '{}'", t.name)),
                            target: thread_target(&t),
                        })
                    }
                    Err(u) => Err(name_not_found("Document not created", &u, "Which thread do you mean?")),
                }
            }
            "rename_thread" => {
                let Some(old) = arg("old_name") else {
                    return Err(format!("Thread not renamed: no thread was named. Which thread should be renamed? Nothing was changed.
{FOR_THE_MODEL}").into());
                };
                match crate::thread_names::resolve_thread(self.db.as_ref(), old).await {
                    Ok(t) => Ok(ToolPrep { description: None, target: thread_target(&t) }),
                    Err(u) => Err(name_not_found("Thread not renamed", &u, "Which thread do you mean?")),
                }
            }
            "move_document" => {
                let Some(title) = arg("document_title") else {
                    return Err(format!("Document not moved: no document was named. Which document should be moved? Nothing was changed.
{FOR_THE_MODEL}").into());
                };
                let key = self.pii_key();
                let n = match crate::document_titles::resolve_document_or_explain(self.db.as_ref(), key.as_deref(), title).await {
                    Ok(n) => n,
                    Err(u) => return Err(name_not_found("Document not moved", &u, "Which document do you mean?")),
                };
                let Some(dest) = arg("thread_name") else {
                    return Err(format!(
                        "Document not moved: no thread was named. Which thread should '{}' go to? Nothing was changed.
{FOR_THE_MODEL}",
                        n.shown
                    ).into());
                };
                match crate::thread_names::resolve_thread(self.db.as_ref(), dest).await {
                    Err(u) => return Err(name_not_found("Document not moved", &u, "Which thread do you mean?")),
                    // C-MERGE MG3: an ended thread takes no moved document.
                    Ok(t) => {
                        if let Some(answer) = crate::thread_names::ended_answer(self.db.as_ref(), &t).await {
                            return Err(format!("Document not moved: {answer}\n{FOR_THE_MODEL}").into());
                        }
                    }
                }
                let id = n.doc.id_string().unwrap_or_default();
                Ok(ToolPrep { description: None, target: target(TargetKind::Document, &id, &n.shown, &doc_revision(&n.doc)) })
            }
            _ => Ok(ToolPrep::default()),
        }
    }

    /// AT7: undoes what the proposal `request_id` carried out, by the ids it
    /// acted on. Once undone, it is forgotten.
    pub(super) async fn undo_request(&self, request_id: &RequestId) -> ActionOutcome {
        let record = self.undo_log.lock().ok().and_then(|l| l.get(request_id).cloned());
        let Some(record) = record else {
            return ActionOutcome::Failed {
                reason: "nothing to undo for this request: it carried out no action that has an undo, or it was undone already".into(),
            };
        };
        let Some(svc) = self.document_service() else {
            return ActionOutcome::Failed { reason: "the workspace is locked; nothing was undone".into() };
        };
        let outcome = match record {
            Undo::Rename { thread_id, old_name, new_name } => match self.db.get_thread(&thread_id).await {
                Ok(row) if row.deleted_at.is_none() && row.name == new_name => {
                    match svc.thread_update(&thread_id, Some(&old_name), None).await {
                        Ok(()) => {
                            self.log_action("undo_rename_thread", &format!("{new_name} → {old_name}"));
                            let _ = self.event_tx.send(OrchestratorEvent::ThreadRenamed {
                                thread_id,
                                name: old_name.clone(),
                            });
                            ActionOutcome::Changed { summary: format!("Renamed '{new_name}' back to '{old_name}'") }
                        }
                        Err(e) => self.failed("Undo failed", crate::docs_host::doc_error_message(&e)),
                    }
                }
                Ok(row) if row.deleted_at.is_none() => ActionOutcome::Failed {
                    reason: format!("thread '{new_name}' was renamed to '{}' since; nothing was undone", row.name),
                },
                _ => ActionOutcome::Failed { reason: format!("thread '{new_name}' no longer exists; nothing was undone") },
            },
            Undo::Move { doc_id, title, from_thread, from_name, to_thread } => match self.db.get_document(&doc_id).await {
                Ok(d) if d.deleted_at.is_none() && d.thread_id == to_thread => {
                    // AT7 (v2.0.89): the thread it came from gone since
                    // (deleted, merged away), the undo is refused, and the
                    // reply names the thread (Céline: « Nommer le fil »).
                    let from_live = matches!(self.db.get_thread(&from_thread).await, Ok(t) if t.deleted_at.is_none());
                    if !from_live {
                        return ActionOutcome::Failed {
                            reason: format!(
                                "'{title}' can't go back: the thread '{from_name}' is gone; nothing was undone"
                            ),
                        };
                    }
                    match svc.move_to_thread(&doc_id, &from_thread).await {
                        Ok(()) => {
                            let back_to = self.db.get_thread(&from_thread).await.map(|t| t.name).unwrap_or_default();
                            self.log_action("undo_move_document", &title);
                            let _ = self.event_tx.send(OrchestratorEvent::DocumentMoved {
                                doc_id,
                                new_thread_id: from_thread,
                            });
                            ActionOutcome::Changed { summary: format!("Moved '{title}' back to '{back_to}'") }
                        }
                        Err(e) => self.failed("Undo failed", crate::docs_host::doc_error_message(&e)),
                    }
                }
                Ok(d) if d.deleted_at.is_none() => {
                    ActionOutcome::Failed { reason: format!("'{title}' was moved again since; nothing was undone") }
                }
                _ => ActionOutcome::Failed { reason: format!("'{title}' is in the trash or gone; nothing was undone") },
            },
            Undo::Merge { source_id, source_name, merge_stamp } => {
                self.undo_merge_of(&source_id, &source_name, Some(merge_stamp)).await
            }
        };
        if matches!(outcome, ActionOutcome::Changed { .. }) {
            if let Ok(mut log) = self.undo_log.lock() {
                log.remove(request_id);
            }
        }
        outcome
    }

    /// C-MERGE MG5 and AT7: undoes the merge of thread `id` (named `name` when
    /// it was resolved), clearing its two fields under a new merge stamp, so
    /// the undo travels as the merge did. `made`: for the undo of a request,
    /// the merge stamp its merge wrote; refused when the merge changed since
    /// (undone, or redone). `None`: the undo by naming the thread, at any time.
    pub(super) async fn undo_merge_of(&self, id: &str, name: &str, made: Option<Option<String>>) -> ActionOutcome {
        let Some(svc) = self.document_service() else {
            return ActionOutcome::Failed { reason: "the workspace is locked; nothing was undone".into() };
        };
        let row = match self.db.get_thread(id).await {
            Ok(row) if row.deleted_at.is_none() => row,
            _ => return ActionOutcome::Failed { reason: format!("thread '{name}' no longer exists; nothing was undone") },
        };
        if !row.is_ended() {
            return ActionOutcome::Failed { reason: crate::thread_names::undo_refused_words(&row.name) };
        }
        if let Some(made) = made {
            if row.merge_stamp != made {
                return ActionOutcome::Failed {
                    reason: format!("the merge of '{}' changed since (undone, or merged again); nothing was undone", row.name),
                };
            }
        }
        let into = sovereign_db::merged_into_name(self.db.as_ref(), &row).await;
        match svc.thread_unmerge(id).await {
            Ok(()) => {
                self.log_action("undo_merge_threads", &format!("{} ← {into}", row.name));
                // Both hosts redraw the canvas on this event; the name is
                // unchanged.
                let _ = self.event_tx.send(OrchestratorEvent::ThreadRenamed { thread_id: id.to_string(), name: row.name.clone() });
                ActionOutcome::Changed { summary: crate::thread_names::undone_words(&row.name, &into) }
            }
            Err(e) => self.failed("Undo failed", crate::docs_host::doc_error_message(&e)),
        }
    }
}
