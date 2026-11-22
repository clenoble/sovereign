//! v0.0.13 item C1 — **the version named is the one restored**, and **a title containing « to » can
//! be restored** (spec ch. 08, *Action targets*, RV1 and RV2; Céline, 2026-11-18, choosing « Date
//! as History shows it »). Written before the code by its test author (PR-0011); the implementer
//! does not edit this file. A test that looks wrong is reported, not worked around.
//!
//! Every test runs the assistant's `restore` through `handle_query`, the router scripted to
//! classify the request as `restore` on the words after « restore » (the request's own words, as
//! RV2 reads them). Versions are written with the times the tests choose
//! (`GraphDB::insert_commit`, which seals them), and named as History shows them: local time at a
//! fixed offset (+05:30, `crate::local_time::fix_offset`), so local differs from UTC everywhere.
//!
//! « Nothing is restored » is read from the store; « the reply » is what she is told
//! (`assistant_harness::reply`).

use super::assistant_harness::*;
use super::lh_tests::drive;
use super::*;
use chrono::{DateTime, TimeZone, Utc};
use sovereign_db::schema::{Commit, DocumentSnapshot};

const PREVIOUS_HOW: &str = "previous";

fn test_offset() -> chrono::FixedOffset {
    chrono::FixedOffset::east_opt(5 * 3600 + 30 * 60).unwrap()
}

/// A date and time as History shows it at [`test_offset`].
fn local(t: DateTime<Utc>) -> String {
    t.with_timezone(&test_offset()).format("%Y-%m-%d %H:%M").to_string()
}

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(y, mo, d, h, mi, s).unwrap()
}

/// A router that classifies the request as `restore` on `target`.
fn restore_route(target: &str) -> Script {
    let t = target.to_string();
    Arc::new(move |_, _, i| if i == 0 { intent_json("restore", Some(&t)) } else { String::new() })
}

/// A live document titled `title` whose versions are `versions` (time, content), oldest first; its
/// current title and content are the newest version's. Returns its id.
async fn versioned(w: &World, title: &str, thread: &str, versions: &[(DateTime<Utc>, &str)]) -> String {
    let mut d = sovereign_db::schema::Document::new(title.into(), thread.to_string(), true);
    d.content = versions.last().map(|(_, c)| c.to_string()).unwrap_or_default();
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    let mut parent: Option<String> = None;
    for (when, content) in versions {
        let c = w
            .store
            .insert_commit(Commit {
                id: None,
                document_id: id.clone(),
                parent_commit: parent.clone(),
                message: "Saved".into(),
                timestamp: *when,
                snapshot: DocumentSnapshot {
                    document_id: id.clone(),
                    title: title.into(),
                    content: content.to_string(),
                    ..Default::default()
                },
                signature: None,
            })
            .await
            .expect("a version written");
        parent = c.id_string();
    }
    id
}

/// The document's text as stored now: plain, or the blocks' text of the v2 format a restore writes.
async fn content(w: &World, id: &str) -> String {
    let raw = w.store.get_document(id).await.unwrap().content;
    match serde_json::from_str::<serde_json::Value>(&raw) {
        Ok(v) if v.get("blocks").is_some() => v["blocks"]
            .as_array()
            .map(|bs| bs.iter().filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"))
            .unwrap_or_default(),
        _ => raw,
    }
}

/// Notes' four versions: one on the 16th, two on the 17th, the newest (current) on the 18th.
fn notes() -> Vec<(DateTime<Utc>, &'static str)> {
    vec![
        (at(2026, 11, 16, 8, 0, 0), "v1"),   // local 2026-11-16 13:30
        (at(2026, 11, 17, 8, 32, 0), "v2"),  // local 2026-11-17 14:02
        (at(2026, 11, 17, 10, 0, 0), "v3"),  // local 2026-11-17 15:30
        (at(2026, 11, 18, 9, 0, 0), "v4"),   // local 2026-11-18 14:30, current
    ]
}

/// One `restore` request, answered with `decision`; the events.
async fn restore(target: &str, tag: &str, docs: &[(&str, Vec<(DateTime<Utc>, &'static str)>)], decision: ActionDecision) -> (World, Vec<String>, Vec<OrchestratorEvent>) {
    let _ = tag;
    let mut w = world(tag, 4096, restore_route(target)).await;
    let t = w.thread("Garden").await;
    let mut ids = vec![];
    for (title, versions) in docs {
        ids.push(versioned(&w, title, &t, versions).await);
    }
    let evs = drive(&mut w, &format!("restore {target}"), vec![decision], |_| async {}).await;
    (w, ids, evs)
}

fn approve() -> ActionDecision {
    ActionDecision::Approve
}

fn reject() -> ActionDecision {
    ActionDecision::Reject("test: no".into())
}

// ── RV1: the version named is the one restored ────────────────────────────

/// No version named means « previous »: the version before the latest is proposed, named by its
/// date and time, and it is the one restored.
#[tokio::test]
async fn rv1_no_version_named_restores_the_previous_one() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes", "c1-previous", &[("Notes", notes())], approve()).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {}", reply(&evs));
    assert!(ps[0].description.contains(&local(notes()[2].0)), "names the previous version: {}", ps[0].description);
    assert_eq!(content(&w, &ids[0]).await, "v3", "the previous version is the one restored");
}

/// « previous » named in words restores the same version.
#[tokio::test]
async fn rv1_previous_in_words_restores_the_previous_one() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes to the previous version", "c1-previous-words", &[("Notes", notes())], approve()).await;
    assert_eq!(proposals(&evs).len(), 1, "one proposal: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v3");
}

/// A date and time as History shows it names that version, not the previous one: proposed by it,
/// and it is the one restored.
#[tokio::test]
async fn rv1_a_date_and_time_restores_that_version() {
    let _o = crate::local_time::fix_offset(test_offset());
    let named = local(notes()[0].0);
    let (w, ids, evs) = restore(&format!("Notes to {named}"), "c1-datetime", &[("Notes", notes())], approve()).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {}", reply(&evs));
    assert!(ps[0].description.contains(&named), "names the version asked for: {}", ps[0].description);
    assert_eq!(content(&w, &ids[0]).await, "v1", "the version named is the one restored");
}

/// A date alone with one version that day names that version.
#[tokio::test]
async fn rv1_a_date_alone_with_one_version_restores_it() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes to 2026-11-16", "c1-date-one", &[("Notes", notes())], approve()).await;
    assert_eq!(proposals(&evs).len(), 1, "one proposal: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v1");
}

/// A date alone with several versions that day: nothing proposed, nothing restored; the reply lists
/// the day's versions as History shows them and says how to name one.
#[tokio::test]
async fn rv1_a_date_with_several_versions_lists_them_and_restores_nothing() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes to 2026-11-17", "c1-date-many", &[("Notes", notes())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    let r = reply(&evs);
    for v in [notes()[1].0, notes()[2].0] {
        assert!(r.contains(&local(v)), "the day's version {} is listed:\n{r}", local(v));
    }
    assert!(r.contains(PREVIOUS_HOW), "the reply says how to name one:\n{r}");
}

/// Two versions in the same minute, named by that minute: nothing proposed, both listed.
#[tokio::test]
async fn rv1_two_versions_in_the_same_minute_are_asked_not_guessed() {
    let _o = crate::local_time::fix_offset(test_offset());
    let mut v = notes();
    v.insert(0, (at(2026, 11, 15, 8, 0, 10), "a"));
    v.insert(1, (at(2026, 11, 15, 8, 0, 40), "b"));
    let minute = local(at(2026, 11, 15, 8, 0, 10));
    let (w, ids, evs) = restore(&format!("Notes to {minute}"), "c1-minute", &[("Notes", v)], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    assert!(reply(&evs).contains(&minute), "the versions of that minute are listed:\n{}", reply(&evs));
}

/// A day with no version: nothing proposed or restored; the reply says so and how to name one.
#[tokio::test]
async fn rv1_a_day_with_no_version_says_so() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes to 2026-11-10", "c1-date-none", &[("Notes", notes())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains("2026-11-10"), "the reply names the day:\n{r}");
    assert!(r.contains(PREVIOUS_HOW), "and how to name a version:\n{r}");
}

/// Loose words name no version: nothing proposed or restored; the reply lists the latest versions,
/// at most five, and says how to name one.
#[tokio::test]
async fn rv1_loose_words_name_no_version_and_list_the_latest_five() {
    let _o = crate::local_time::fix_offset(test_offset());
    let seven: Vec<(DateTime<Utc>, &'static str)> =
        (0..7).map(|i| (at(2026, 11, 10 + i as u32, 6, 0, 0), ["a", "b", "c", "d", "e", "f", "g"][i])).collect();
    let (w, ids, evs) = restore("Notes to yesterday's", "c1-loose", &[("Notes", seven.clone())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "g", "nothing is restored");
    let r = reply(&evs);
    for (t, _) in &seven[2..] {
        assert!(r.contains(&local(*t)), "a latest version {} is listed:\n{r}", local(*t));
    }
    for (t, _) in &seven[..2] {
        assert!(!r.contains(&local(*t)), "at most five: {} is not listed:\n{r}", local(*t));
    }
    assert!(r.contains(PREVIOUS_HOW), "the reply says how to name one:\n{r}");
}

/// « previous » on a document with no earlier version.
#[tokio::test]
async fn rv1_previous_with_no_earlier_version_says_so() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Solo", "c1-solo", &[("Solo", vec![(at(2026, 11, 18, 9, 0, 0), "only")])], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "only");
    let r = reply(&evs);
    assert!(r.contains("'Solo' has no earlier version. Nothing was changed."), "the decided words:\n{r}");
}

/// The version named is already the current one: nothing changed, and the reply names the document.
#[tokio::test]
async fn rv1_the_current_version_named_says_so() {
    let _o = crate::local_time::fix_offset(test_offset());
    let current = local(notes()[3].0);
    let (w, ids, evs) = restore(&format!("Notes to {current}"), "c1-current", &[("Notes", notes())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4");
    let r = reply(&evs);
    assert!(
        r.contains("This is already the current version of 'Notes'. Nothing was changed."),
        "the decided words:\n{r}"
    );
}

/// Rejected, nothing is restored.
#[tokio::test]
async fn rv1_rejected_nothing_is_restored() {
    let _o = crate::local_time::fix_offset(test_offset());
    let named = local(notes()[0].0);
    let (w, ids, evs) = restore(&format!("Notes to {named}"), "c1-rejected", &[("Notes", notes())], reject()).await;
    assert_eq!(proposals(&evs).len(), 1, "one proposal: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4");
}

// ── RV2: a title containing « to » ────────────────────────────────────────

fn trip() -> Vec<(DateTime<Utc>, &'static str)> {
    vec![(at(2026, 11, 16, 8, 0, 0), "trip one"), (at(2026, 11, 18, 9, 0, 0), "trip two")]
}

fn rome() -> Vec<(DateTime<Utc>, &'static str)> {
    vec![
        (at(2026, 11, 15, 8, 0, 0), "rome one"),  // local 2026-11-15 13:30
        (at(2026, 11, 17, 8, 32, 0), "rome two"), // local 2026-11-17 14:02
        (at(2026, 11, 18, 9, 0, 0), "rome three"),
    ]
}

/// « restore Trip to Rome » restores the document titled « Trip to Rome », to its previous
/// version, even beside a document titled « Trip ».
#[tokio::test]
async fn rv2_a_title_containing_to_is_restored_whole() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Trip to Rome", "c1-rv2-whole", &[("Trip", trip()), ("Trip to Rome", rome())], approve()).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {}", reply(&evs));
    assert_eq!(ps[0].doc_id.as_deref(), Some(ids[1].as_str()), "the proposal carries « Trip to Rome »: {:?}", ps[0]);
    assert_eq!(content(&w, &ids[1]).await, "rome two", "« Trip to Rome », back to its previous version");
    assert_eq!(content(&w, &ids[0]).await, "trip two", "« Trip » untouched");
}

/// « restore Trip to Rome to {version} » names that version of « Trip to Rome ».
#[tokio::test]
async fn rv2_a_title_containing_to_with_a_version_restores_that_version() {
    let _o = crate::local_time::fix_offset(test_offset());
    let named = local(rome()[0].0);
    let (w, ids, evs) =
        restore(&format!("Trip to Rome to {named}"), "c1-rv2-version", &[("Trip", trip()), ("Trip to Rome", rome())], approve()).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {}", reply(&evs));
    assert_eq!(ps[0].doc_id.as_deref(), Some(ids[1].as_str()), "« Trip to Rome »: {:?}", ps[0]);
    assert!(ps[0].description.contains(&named), "the version named: {}", ps[0].description);
    assert_eq!(content(&w, &ids[1]).await, "rome one");
    assert_eq!(content(&w, &ids[0]).await, "trip two", "« Trip » untouched");
}

/// Both readings find a document: nothing changed; the reply names both and asks for the title in
/// quotes.
#[tokio::test]
async fn rv2_both_readings_found_asks_for_quotes() {
    let _o = crate::local_time::fix_offset(test_offset());
    let named = local(rome()[1].0);
    let long = format!("Plan to {named}");
    let docs: Vec<(&str, Vec<(DateTime<Utc>, &'static str)>)> = vec![("Plan", rome()), (long.as_str(), trip())];
    let (w, ids, evs) = restore(&long, "c1-rv2-both", &docs, approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "rome three", "nothing is restored");
    assert_eq!(content(&w, &ids[1]).await, "trip two", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains("in quotes"), "it asks for the title in quotes:\n{r}");
}

/// Neither reading finds a document, and the words before the last « to » are a document's title:
/// the words after it named no version, answered as RV1 answers for that document (its latest
/// versions, how to name one); nothing proposed or restored.
#[tokio::test]
async fn rv2_neither_reading_but_a_title_before_to_is_answered_as_rv1() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Trip to Rome", "c1-rv2-neither", &[("Trip", trip())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "trip two", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains(&local(trip()[0].0)), "Trip's versions are listed:\n{r}");
    assert!(r.contains(PREVIOUS_HOW), "and how to name one:\n{r}");
}

/// Five approvals of `restore` on the query path: the trust threshold.
fn trust_five(w: &World) {
    let mut t = w.orch.trust.lock().unwrap();
    for _ in 0..5 {
        t.record_approval(crate::trust::WORKFLOW_QUERY, "restore");
    }
}

/// A title cut at « to » out of unquoted words is a name read from unquoted words: its proposal is
/// shown even with trust reached.
#[tokio::test]
async fn rv2_a_title_cut_at_to_is_proposed_even_trusted() {
    let _o = crate::local_time::fix_offset(test_offset());
    let named = local(rome()[0].0);
    let target = format!("Trip to Rome to {named}");
    let mut w = world("c1-rv2-trust-cut", 4096, restore_route(&target)).await;
    let t = w.thread("Travel").await;
    versioned(&w, "Trip", &t, &trip()).await;
    let rome_id = versioned(&w, "Trip to Rome", &t, &rome()).await;
    trust_five(&w);
    let evs = drive(&mut w, &format!("restore {target}"), vec![reject()], |_| async {}).await;
    assert_eq!(proposals(&evs).len(), 1, "the cut title's proposal is shown, trusted or not: {}", reply(&evs));
    assert_eq!(content(&w, &rome_id).await, "rome three", "rejected: nothing restored");
}

/// A title taken whole keeps the trust path: with trust reached, « restore Trip to Rome » runs
/// unseen, on « Trip to Rome ».
#[tokio::test]
async fn rv2_a_whole_title_keeps_the_trust_path() {
    let _o = crate::local_time::fix_offset(test_offset());
    let mut w = world("c1-rv2-trust-whole", 4096, restore_route("Trip to Rome")).await;
    let t = w.thread("Travel").await;
    let trip_id = versioned(&w, "Trip", &t, &trip()).await;
    let rome_id = versioned(&w, "Trip to Rome", &t, &rome()).await;
    trust_five(&w);
    let evs = drive(&mut w, "restore Trip to Rome", vec![], |_| async {}).await;
    assert!(proposals(&evs).is_empty(), "trusted, a whole title runs unseen: {}", reply(&evs));
    assert_eq!(content(&w, &rome_id).await, "rome two", "« Trip to Rome », back to its previous version");
    assert_eq!(content(&w, &trip_id).await, "trip two", "« Trip » untouched");
}

// ── RV1: versions saved in the same minute (the Fable maille, by delegation of Céline, 2026-11-19) ─

/// The sentence when the versions that match all read the same as History shows them.
const SAME_MINUTE: &str =
    "Versions saved in the same minute cannot be told apart here: open the document's History to choose between them.";
const WHICH_ONE: &str = "Which one do you mean?";

/// Notes, with two versions saved in one minute on the 15th, before the four of `notes()`.
fn notes_with_a_shared_minute() -> Vec<(DateTime<Utc>, &'static str)> {
    let mut v = notes();
    v.insert(0, (at(2026, 11, 15, 8, 0, 10), "a"));
    v.insert(1, (at(2026, 11, 15, 8, 0, 40), "b"));
    v
}

/// A date and time that several versions share: nothing restored; the reply lists them, says they
/// cannot be told apart here and where to choose between them, and how to name a version; it does
/// not ask which one.
#[tokio::test]
async fn rv1_a_date_and_time_shared_by_versions_says_where_to_choose() {
    let _o = crate::local_time::fix_offset(test_offset());
    let minute = local(at(2026, 11, 15, 8, 0, 10));
    let (w, ids, evs) = restore(&format!("Notes to {minute}"), "c1-minute-way", &[("Notes", notes_with_a_shared_minute())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains(SAME_MINUTE), "the decided sentence:\n{r}");
    assert!(r.contains("Nothing was changed."), "it says nothing was changed:\n{r}");
    assert!(r.contains(&minute), "the versions are listed:\n{r}");
    assert!(r.contains(PREVIOUS_HOW), "and how to name one:\n{r}");
    assert!(!r.contains(WHICH_ONE), "it does not ask for what she cannot say:\n{r}");
}

/// A date alone whose versions all share one minute: the same sentence, with the day's versions.
#[tokio::test]
async fn rv1_a_date_whose_versions_share_one_minute_says_where_to_choose() {
    let _o = crate::local_time::fix_offset(test_offset());
    let (w, ids, evs) = restore("Notes to 2026-11-15", "c1-day-one-minute", &[("Notes", notes_with_a_shared_minute())], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains(SAME_MINUTE), "the decided sentence:\n{r}");
    assert!(r.contains(&local(at(2026, 11, 15, 8, 0, 10))), "the day's versions are listed:\n{r}");
    assert!(!r.contains(WHICH_ONE), "it does not ask for what she cannot say:\n{r}");
}

/// A day whose versions are at different minutes, two of them sharing one: she can name the
/// others, so the reply asks which one, without the sentence.
#[tokio::test]
async fn rv1_a_date_with_versions_at_different_minutes_asks_which_one() {
    let _o = crate::local_time::fix_offset(test_offset());
    let mut v = notes_with_a_shared_minute();
    v.insert(2, (at(2026, 11, 15, 9, 0, 0), "c"));
    let (w, ids, evs) = restore("Notes to 2026-11-15", "c1-day-minutes", &[("Notes", v)], approve()).await;
    assert!(proposals(&evs).is_empty(), "nothing is proposed: {}", reply(&evs));
    assert_eq!(content(&w, &ids[0]).await, "v4", "nothing is restored");
    let r = reply(&evs);
    assert!(r.contains(WHICH_ONE), "it asks which one:\n{r}");
    assert!(!r.contains(SAME_MINUTE), "the same-minute sentence is not said:\n{r}");
}
