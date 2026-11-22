//! v0.0.13, contract **C-TPII** (spec ch. 08, *A title that holds personal
//! data*, TP1-TP5; Céline, 2026-11-17): a document whose title holds personal
//! data is found by the title as she typed it (with the value), by the title
//! as shown (with the label) and by its stored form (with the token); two
//! documents of one title are counted by document, by whichever form; and a
//! title in a reply is shown with its labels.
//!
//! - **TP1** — three forms of one title: as stored (a dismissed value stands as
//!   written, so the exact value finds it this way); with the values replaced
//!   by their tokens (each value detected in the words given, looked up among
//!   the records, one per document that holds it); as shown (each token as its
//!   type's label, in ch. 09 SW5's one form, `Transfer [IBAN]`).
//! - **TP2** — the exact-name rule's outcomes, counted by document: one acts;
//!   two or more, by the same form or by different ones, are two documents of
//!   one title (not done, none picked); a document matching by two forms
//!   counts once.
//! - **TP3** — the close titles, the list of documents that share a title, the
//!   proposal and the outcome line show each title as shown: never `[pii:…]`,
//!   never a value replaced by a token.
//! - **TP4** — one resolver for every caller: the orchestrator's direct-request
//!   path and the chat tools have the three forms (here, end to end); the scan
//!   of the callers is `sovereign-docs/tests/tp4_one_title_resolver.rs`.
//! - **TP5** — the detection is the save's: the documents here are created
//!   through the document service, whose save detects and tokenises the
//!   title, and the words given hold the same values.
//!
//! **The API these tests use** (the « types and signatures » commit, by the
//! test author): `crate::document_titles::resolve_document_title_forms(db,
//! account_key, title) -> DbResult<DocumentTitleMatch>`, and the account key
//! added to `resolve_document_or_explain(db, account_key, title)`. The
//! orchestrator holds the key it is given at login
//! (`Orchestrator::set_pii_account_key`); the tests install it the same way.
//!
//! "The reply" is `assistant_harness::reply` (chat messages, refusals, the
//! executed event read through `Debug`). A title as stored is read from the
//! store, never assumed: each test checks first that the save put a token in
//! it.
//!
//! Written before the code by another author; the implementer does not edit
//! this file.

use super::assistant_harness::*;
use super::*;
use sovereign_core::interfaces::{ActionOutcome, WriteOutcome};
use sovereign_db::schema::{Document, PiiKind, PiiRecord, ReviewState, SourceKind, SourceRef};
use sovereign_db::DocumentTitleMatch;

// ------------------------------------------------------------- the data --

/// An e-mail address the save's detection finds.
const MAIL: &str = "alice@example.com";
/// A title holding it, as she types it.
const TYPED: &str = "Transfer alice@example.com";
/// The same title as shown: the token as its type's label (ch. 09 SW5).
const SHOWN: &str = "Transfer [Email]";

/// Two IBANs the save's detection finds (the usual published examples).
const IBAN_CH: &str = "CH93 0076 2011 6238 5295 7";
const IBAN_DE: &str = "DE89 3704 0044 0532 0130 00";

// ------------------------------------------------------------- helpers --

/// The key the world's document service tokenises with, and that opens the
/// records.
fn key() -> Arc<sovereign_crypto::account_key::AccountKey> {
    crate::docs_host::testing::account_key()
}

/// A world whose orchestrator holds the account key, as after a login.
async fn keyed_world(tag: &str, script: Script) -> World {
    let w = world(tag, 4096, script).await;
    w.orch.set_pii_account_key(key());
    w
}

/// The title as stored.
async fn stored_title(w: &World, id: &str) -> String {
    w.store.get_document(id).await.unwrap().title
}

/// The save put a token in the title, in place of `value`: the stored form is
/// what this contract is about, so a test whose document was not tokenised
/// tests nothing.
async fn assert_tokenised(w: &World, id: &str, value: &str) -> String {
    let t = stored_title(w, id).await;
    assert!(t.contains("[pii:"), "precondition: the save tokenised the title: {t:?}");
    assert!(!t.contains(value), "precondition: the value is not stored as written: {t:?}");
    t
}

/// What `title` resolves to through the one resolver, with the key.
async fn resolve(w: &World, title: &str) -> DocumentTitleMatch {
    crate::document_titles::resolve_document_title_forms(w.store.as_ref(), Some(key().as_ref()), title)
        .await
        .unwrap()
}

/// The id of the one document found, or a failure naming what was found.
fn found_id(m: DocumentTitleMatch, what: &str) -> String {
    match m {
        DocumentTitleMatch::Found(d) => d.id_string().unwrap(),
        other => panic!("{what}: one document, got {other:?}"),
    }
}

/// The ids of the documents of an ambiguous title, sorted.
fn ambiguous_ids(m: DocumentTitleMatch, what: &str) -> Vec<String> {
    match m {
        DocumentTitleMatch::Ambiguous(v) => {
            let mut ids: Vec<String> = v.iter().map(|d| d.id_string().unwrap()).collect();
            ids.sort();
            ids
        }
        other => panic!("{what}: two documents of one title, got {other:?}"),
    }
}

/// A document whose title holds `value` as written, with that value's record
/// in the state « PII dismissed »: what a dismissal leaves (ch. 09 SW5, the
/// value restored as written). Written straight to the store, as the
/// dismissal's own version would be, so no save tokenises it again.
///
/// The record is made from one the save wrote for the same value (in the text
/// of a document titled « Scratch », which no test names), so it is written as
/// the store's records are, whatever fields they gain.
async fn dismissed_doc(w: &World, title: &str, value: &str, kind: PiiKind, thread: &str, text: &str) -> String {
    let scratch = w.doc("Scratch", thread, &format!("written to {value}")).await;
    let base = w
        .store
        .list_pii_records(None, None, Some(false))
        .await
        .unwrap()
        .into_iter()
        .find(|r| r.kind == kind && r.sources.iter().any(|s| s.source_kind == SourceKind::Document && s.source_id == scratch))
        .expect("precondition: the save recorded the value");

    let mut d = Document::new(title.into(), thread.into(), true);
    d.content = text.into();
    let id = w.store.create_document(d).await.unwrap().id_string().unwrap();
    let start = title.find(value).expect("the value is in the title");
    let source = SourceRef { source_id: id.clone(), span_start: start, span_end: start + value.len(), ..base.sources[0].clone() };
    w.store
        .create_pii_record(PiiRecord { id: None, sources: vec![source], review_state: ReviewState::Dismissed, ..base })
        .await
        .unwrap();
    id
}

/// The one executed event of `events`: (success, outcome).
fn the_outcome(events: &[OrchestratorEvent], what: &str) -> (bool, ActionOutcome) {
    let all: Vec<(bool, ActionOutcome)> = events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ActionExecuted { success, outcome, .. } => Some((*success, outcome.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(all.len(), 1, "{what}: exactly one executed event, got {events:?}");
    all.into_iter().next().unwrap()
}

/// The documents an event says were opened.
fn opened(events: &[OrchestratorEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::DocumentOpened { doc_id } => Some(doc_id.clone()),
            _ => None,
        })
        .collect()
}

/// TP3: `text` shows no stored form and none of `values`.
fn assert_no_stored_form_nor_value(text: &str, values: &[&str], what: &str) {
    assert!(!text.contains("[pii:"), "{what}: a title is never shown in its stored form: {text}");
    for v in values {
        assert!(!text.contains(v), "{what}: a value replaced by a token is never shown ({v}): {text}");
    }
}

/// The model's answer calling `name` with `args`.
fn tool_call(name: &str, args: serde_json::Value) -> String {
    format!("<tool_call>\n{}\n</tool_call>", serde_json::json!({ "name": name, "arguments": args }))
}

/// A model that answers the first call of the turn with `first`, and every
/// later call (after the tool's result) with a plain answer.
fn first_then_answer(first: String) -> Script {
    Arc::new(move |_: &str, _: u32, i: usize| if i == 0 { first.clone() } else { "All set.".to_string() })
}

/// Runs one chat turn on `message` through `execute_action("chat")`,
/// answering every proposal with `decision`. Returns every event and the
/// turn's one outcome.
async fn chat_turn(w: &mut World, message: &str, decision: ActionDecision) -> (Vec<OrchestratorEvent>, ChatOutcome) {
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let mut seen: Vec<OrchestratorEvent> = vec![];
    let orch = &w.orch;
    let turn = async {
        let r = orch.execute_action("chat", None, message).await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        for _ in 0..20_000 {
            let new = rec.take_events();
            let n = proposals(&new).len();
            seen.extend(new);
            for _ in 0..n {
                let _ = dtx.send(decision.clone()).await;
            }
            if done.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (res, ()) = tokio::join!(turn, driver);
    res.unwrap();
    seen.extend(w.rec.take_events());
    let outcomes: Vec<ChatOutcome> = seen
        .iter()
        .filter_map(|e| match e {
            OrchestratorEvent::ChatTurnEnded { outcome } => Some(outcome.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(outcomes.len(), 1, "one ChatTurnEnded per turn: {seen:?}");
    (seen, outcomes.into_iter().next().unwrap())
}

// ------------------------------------------------ TP1, the three forms --

/// TP1, the contract's test: a document titled with an e-mail address, its
/// title tokenised by the save, resolves by the title as she typed it, by the
/// title as shown and by its stored form — each ignoring case and surrounding
/// spaces.
#[tokio::test]
async fn tp1_typed_shown_and_stored_forms_each_find_the_document() {
    let w = keyed_world("tp1-three-forms", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    let stored = assert_tokenised(&w, &id, MAIL).await;
    let shouted_stored = format!("  {}  ", stored.replacen("Transfer", "TRANSFER", 1));
    let typed_forms = [TYPED, "  TRANSFER alice@example.com "];
    let shown_forms = [SHOWN, " transfer [EMAIL]"];
    let stored_forms = [stored.as_str(), shouted_stored.as_str()];
    for (form, words) in [("as stored", &stored_forms), ("as typed", &typed_forms), ("as shown", &shown_forms)] {
        for given in words.iter() {
            let what = format!("{form}: {given:?}");
            assert_eq!(found_id(resolve(&w, given).await, &what), id, "{what}");
        }
    }
}

/// TP1: a second document sharing a word of the title (« Transfer »), and a
/// third holding the same e-mail address in a longer title, are not the
/// title: the comparison decides, not the lookup. The typed and the shown
/// form each find exactly the one document.
#[tokio::test]
async fn tp1_a_shared_word_or_the_same_value_in_another_title_is_not_the_title() {
    let w = keyed_world("tp1-shared-word", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    w.doc("Transfer notes", &office, "other notes").await;
    let longer = w.doc("Transfer alice@example.com draft", &office, "a draft").await;
    assert_tokenised(&w, &id, MAIL).await;
    assert_tokenised(&w, &longer, MAIL).await;
    for given in [TYPED, SHOWN] {
        assert_eq!(found_id(resolve(&w, given).await, given), id, "{given}: the one document of that title");
    }
}

/// TP1: a value in the words given is compared as the title is, ignoring case
/// and surrounding spaces, nothing else (the Fable maille, by delegation of
/// Céline, 2026-11-17). The address typed in capitals finds the document whose
/// title holds it in lower case.
#[tokio::test]
async fn tp1_a_value_typed_in_capitals_finds_the_title_that_holds_it() {
    let w = keyed_world("tp1-value-case", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    for given in ["Transfer ALICE@EXAMPLE.COM", "  transfer Alice@Example.com  "] {
        assert_eq!(found_id(resolve(&w, given).await, given), id, "{given}: the value, ignoring its case");
    }
}

/// TP1 with an IBAN: the title as typed finds the document holding that
/// IBAN, not the one holding another.
#[tokio::test]
async fn tp1_the_typed_iban_finds_the_document_that_holds_it() {
    let w = keyed_world("tp1-typed-iban", answer("")).await;
    let office = w.thread("Office").await;
    let ch = w.doc(&format!("Transfer {IBAN_CH}"), &office, "to Zurich").await;
    let de = w.doc(&format!("Transfer {IBAN_DE}"), &office, "to Berlin").await;
    assert_tokenised(&w, &ch, IBAN_CH).await;
    assert_tokenised(&w, &de, IBAN_DE).await;
    let given = format!("Transfer {IBAN_CH}");
    assert_eq!(found_id(resolve(&w, &given).await, &given), ch);
}

/// TP1, first form: a value she dismissed stands as written in the title, so
/// the exact value finds the document, as stored.
///
/// Green against the stub (it is today's comparison). The mutation that turns
/// it red: a resolver that compares only the forms with tokens or labels (the
/// stub resolving the words with their detected values replaced by labels)
/// finds nothing.
#[tokio::test]
async fn tp1_a_dismissed_value_is_found_by_the_exact_value() {
    let w = keyed_world("tp1-dismissed", answer("")).await;
    let office = w.thread("Office").await;
    let id = dismissed_doc(&w, TYPED, MAIL, PiiKind::Email, &office, "kept as written").await;
    assert_eq!(stored_title(&w, &id).await, TYPED, "precondition: the dismissed value stands as written");
    assert_eq!(found_id(resolve(&w, TYPED).await, TYPED), id);
}

// ---------------------------------------------- TP2, counted by document --

/// TP2, the contract's test: two documents whose titles differ only by their
/// IBAN, named as shown, are two documents of one title — none picked.
#[tokio::test]
async fn tp2_titles_differing_only_by_their_iban_named_as_shown_are_two_documents() {
    let w = keyed_world("tp2-iban-shown", answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    let ch = w.doc(&format!("Transfer {IBAN_CH}"), &home, "to Zurich").await;
    let de = w.doc(&format!("Transfer {IBAN_DE}"), &office, "to Berlin").await;
    assert_tokenised(&w, &ch, IBAN_CH).await;
    assert_tokenised(&w, &de, IBAN_DE).await;
    let mut both = vec![ch, de];
    both.sort();
    assert_eq!(ambiguous_ids(resolve(&w, "Transfer [IBAN]").await, "Transfer [IBAN]"), both);
}

/// TP2 (and TP1, « the records »): two documents with the same title and the
/// same e-mail address in it — each with its own record of that value — named
/// as she typed it, are two documents of one title.
#[tokio::test]
async fn tp2_the_same_title_with_the_same_value_named_as_typed_is_two_documents() {
    let w = keyed_world("tp2-same-mail", answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    let a = w.doc(TYPED, &home, "first").await;
    let b = w.doc(TYPED, &office, "second").await;
    assert_tokenised(&w, &a, MAIL).await;
    assert_tokenised(&w, &b, MAIL).await;
    let mut both = vec![a, b];
    both.sort();
    assert_eq!(ambiguous_ids(resolve(&w, TYPED).await, TYPED), both);
}

/// TP2: two documents found by different forms — one by the value as written
/// (dismissed there), one by its token — are two documents of one title.
#[tokio::test]
async fn tp2_two_documents_by_different_forms_are_two_documents() {
    let w = keyed_world("tp2-two-forms", answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    let kept = dismissed_doc(&w, TYPED, MAIL, PiiKind::Email, &home, "kept as written").await;
    let tokenised = w.doc(TYPED, &office, "tokenised").await;
    assert_tokenised(&w, &tokenised, MAIL).await;
    let mut both = vec![kept, tokenised];
    both.sort();
    assert_eq!(ambiguous_ids(resolve(&w, TYPED).await, TYPED), both);
}

/// TP2: a document matching by two forms counts once. A title with no
/// personal data is its own stored form and its own shown form: it is found,
/// not two documents.
///
/// Green against the stub (it compares one form). The mutation that turns it
/// red: a resolver that adds the matches of each form without counting by
/// document (the stub returning its match twice, as `Ambiguous`).
#[tokio::test]
async fn tp2_a_document_matching_by_two_forms_counts_once() {
    let w = keyed_world("tp2-once", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc("Quarterly report", &office, "numbers").await;
    assert_eq!(stored_title(&w, &id).await, "Quarterly report", "precondition: nothing to tokenise");
    assert_eq!(found_id(resolve(&w, "Quarterly report").await, "Quarterly report"), id);
}

// -------------------------------------------- TP3, a title in a reply --

/// TP3 and TP4 on the direct-request path: « open » by each form opens the
/// document, and the outcome line names it as shown.
#[tokio::test]
async fn tp3_tp4_open_by_each_form_and_the_outcome_line_shows_the_label() {
    let w = keyed_world("tp3-open", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    let stored = assert_tokenised(&w, &id, MAIL).await;
    for given in [stored.as_str(), TYPED, SHOWN] {
        let evs = w.act("open", Some(given)).await;
        let (success, o) = the_outcome(&evs, given);
        assert!(success, "{given}: opened: {o:?}");
        assert_eq!(opened(&evs), vec![id.clone()], "{given}: this document, opened");
        let line = o.summary_line();
        assert!(line.contains(SHOWN), "{given}: the outcome line shows the title with its label: {line}");
        assert_no_stored_form_nor_value(&line, &[MAIL], given);
    }
}

/// TP3: no document has the title given — the close titles are shown with
/// their labels, never their stored form or a value a token replaced.
#[tokio::test]
async fn tp3_close_titles_show_labels() {
    let w = keyed_world("tp3-close", answer("")).await;
    let office = w.thread("Office").await;
    let mail = w.doc(TYPED, &office, "the transfer note").await;
    let iban = w.doc(&format!("Transfer {IBAN_CH}"), &office, "to Zurich").await;
    assert_tokenised(&w, &mail, MAIL).await;
    assert_tokenised(&w, &iban, IBAN_CH).await;

    // The resolver's sentence.
    let why = crate::document_titles::resolve_document_or_explain(w.store.as_ref(), Some(key().as_ref()), "Transfer")
        .await
        .expect_err("no document is titled exactly « Transfer »");
    let m = why.message();
    assert!(m.contains(SHOWN) && m.contains("Transfer [IBAN]"), "the close titles, as shown: {m}");
    assert_no_stored_form_nor_value(m, &[MAIL, IBAN_CH], "close titles");

    // The same, as the direct-request path says it.
    let evs = w.act("open", Some("Transfer")).await;
    let (success, _) = the_outcome(&evs, "open Transfer");
    assert!(!success, "not done");
    assert!(opened(&evs).is_empty(), "nothing opened");
    let r = reply(&evs);
    assert!(r.contains(SHOWN), "the close titles, as shown: {r}");
    assert_no_stored_form_nor_value(&r, &[MAIL, IBAN_CH], "open Transfer");
}

/// TP3 and TP2 on the direct-request path, the contract's test: two documents
/// whose titles differ only by their IBAN, « open » named as shown — not done,
/// none opened, both listed with their labels, no IBAN anywhere.
#[tokio::test]
async fn tp3_two_documents_by_their_iban_are_listed_with_labels() {
    let w = keyed_world("tp3-iban-list", answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    let ch = w.doc(&format!("Transfer {IBAN_CH}"), &home, "to Zurich").await;
    let de = w.doc(&format!("Transfer {IBAN_DE}"), &office, "to Berlin").await;
    assert_tokenised(&w, &ch, IBAN_CH).await;
    assert_tokenised(&w, &de, IBAN_DE).await;
    let before = snapshot(&w.store).await;
    let evs = w.act("open", Some("Transfer [IBAN]")).await;
    let (success, o) = the_outcome(&evs, "open Transfer [IBAN]");
    assert!(!success && matches!(o, ActionOutcome::Failed { .. }), "not done: {o:?}");
    assert!(opened(&evs).is_empty(), "none picked: {evs:?}");
    assert_eq!(snapshot(&w.store).await, before, "nothing changed");
    let r = reply(&evs);
    assert!(r.contains("Home") && r.contains("Office"), "both listed, each with its thread: {r}");
    assert!(r.to_lowercase().contains("canvas"), "the sentence for two documents of one title: {r}");
    assert!(r.matches("Transfer [IBAN]").count() >= 2, "each listed as shown: {r}");
    assert_no_stored_form_nor_value(&r, &[IBAN_CH, IBAN_DE, "CH93", "DE89"], "open Transfer [IBAN]");
}

/// TP3 and TP2, the contract's test: two documents with the same title and the
/// same e-mail address, « open » named as she typed it — not done, none
/// opened, both listed with the label.
#[tokio::test]
async fn tp3_two_documents_with_the_same_value_named_as_typed_are_listed_with_labels() {
    let w = keyed_world("tp3-mail-list", answer("")).await;
    let home = w.thread("Home").await;
    let office = w.thread("Office").await;
    let a = w.doc(TYPED, &home, "first").await;
    let b = w.doc(TYPED, &office, "second").await;
    assert_tokenised(&w, &a, MAIL).await;
    assert_tokenised(&w, &b, MAIL).await;
    let evs = w.act("open", Some(TYPED)).await;
    let (success, o) = the_outcome(&evs, "open, as typed");
    assert!(!success && matches!(o, ActionOutcome::Failed { .. }), "not done: {o:?}");
    assert!(opened(&evs).is_empty(), "none picked: {evs:?}");
    let r = reply(&evs);
    assert!(r.contains("Home") && r.contains("Office"), "both listed, each with its thread: {r}");
    assert!(r.to_lowercase().contains("canvas"), "the sentence for two documents of one title: {r}");
    assert!(r.matches(SHOWN).count() >= 2, "each listed as shown: {r}");
    assert!(!r.contains("[pii:"), "never the stored form: {r}");
}

// ------------------------------- TP3, the other replies that show a title --

/// TP3 for the « search » action (decided by the Fable maille, by delegation
/// of Céline, 2026-11-17): a search keeps its partial match, and the titles
/// its reply lists are shown with their labels.
#[tokio::test]
async fn tp3_the_search_reply_shows_titles_with_labels() {
    let w = keyed_world("tp3-search", answer("")).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    let evs = w.act("search", Some("Transfer")).await;
    let (_, o) = the_outcome(&evs, "search Transfer");
    let line = o.summary_line();
    assert!(line.contains(SHOWN), "the title found, as shown: {line}");
    assert_no_stored_form_nor_value(&line, &[MAIL], "the search reply");
}

/// TP3: the two functions that say « the title as shown » agree (decided by
/// the Fable maille, by delegation of Céline, 2026-11-17): the document
/// service's `DocumentView::labelled_title` and the assistant's
/// `document_titles::shown_title`, for one document in four cases — a token
/// of a record linked to it; a token whose record is deleted (the
/// placeholder); a token naming another document's record (left as written);
/// no token.
///
/// The values are e-mail addresses, whose label is the same in the table and
/// as the type's code name, so this test does not depend on the label's one
/// form (ch. 09 SW5, held in the LS(a) tests).
#[tokio::test]
async fn tp3_the_title_as_shown_is_one_function_of_the_title() {
    let w = keyed_world("tp3-shown-agree", answer("")).await;
    let office = w.thread("Office").await;

    // A token of a record linked to the document.
    let linked = w.doc(TYPED, &office, "the transfer note").await;
    let stored = assert_tokenised(&w, &linked, MAIL).await;
    let first_token = |t: &str| t[t.find("[pii:").unwrap() + "[pii:".len()..].split(']').next().unwrap().to_string();
    let record_of_linked = first_token(&stored);

    // A token whose record is deleted.
    let deleted = w.doc("Refund bob@example.org", &office, "a refund").await;
    let deleted_stored = assert_tokenised(&w, &deleted, "bob@example.org").await;
    let gone = first_token(&deleted_stored);
    w.store.soft_delete_pii_record(&gone).await.unwrap();

    // A token naming another document's record: not a token here.
    let mut d = Document::new(format!("Note [pii:{record_of_linked}]"), office.clone(), true);
    d.content = "a note".into();
    let foreign = w.store.create_document(d).await.unwrap().id_string().unwrap();

    // No token.
    let plain = w.doc("Plain title", &office, "plain").await;

    for (case, id) in [("linked", &linked), ("deleted record", &deleted), ("another's record", &foreign), ("no token", &plain)] {
        let view = w.docs.read(id).await.unwrap().labelled_title();
        let doc = w.store.get_document(id).await.unwrap();
        let shown = crate::document_titles::shown_title(w.store.as_ref(), &doc).await;
        assert_eq!(shown, view, "{case}: the assistant's title as shown is the service's");
    }
    assert_eq!(w.docs.read(&linked).await.unwrap().labelled_title(), SHOWN, "precondition: the service labels it");
}

/// TP3 for the links the assistant suggests (decided by the Fable maille, by
/// delegation of Céline, 2026-11-17): `LinkSuggested` carries the two titles
/// as shown, never their stored form nor a value.
#[tokio::test]
async fn tp3_a_suggested_link_names_both_titles_as_shown() {
    let scoring = r#"[{"pair":1,"related":true,"type":"supports","strength":0.9,"reason":"both about a transfer"}]"#;
    let w = keyed_world("tp3-link", answer(scoring)).await;
    let office = w.thread("Office").await;
    let home = w.thread("Home").await;
    let a = w.doc(TYPED, &office, "the transfer note").await;
    let b = w.doc("Refund bob@example.org", &home, "a refund").await;
    assert_tokenised(&w, &a, MAIL).await;
    assert_tokenised(&w, &b, "bob@example.org").await;
    w.rec.take_events();
    w.orch.consolidate_memory().await.unwrap();
    let links: Vec<(String, String)> = w
        .rec
        .take_events()
        .into_iter()
        .filter_map(|e| match e {
            OrchestratorEvent::LinkSuggested { from_title, to_title, .. } => Some((from_title, to_title)),
            _ => None,
        })
        .collect();
    assert_eq!(links.len(), 1, "precondition: one link suggested: {links:?}");
    let (from, to) = &links[0];
    let mut both = vec![from.clone(), to.clone()];
    both.sort();
    assert_eq!(both, vec!["Refund [Email]".to_string(), SHOWN.to_string()], "both titles as shown");
    assert_no_stored_form_nor_value(&format!("{from} {to}"), &[MAIL, "bob@example.org"], "the suggested link");
}

// ------------------------- a title written from a title (the Duplicate skill) --

/// TP3 shows a title with its labels, and only shows it: a title written from
/// another title is written from the stored one. The Duplicate skill names the
/// copy « Copy of {title} »; written from the title as shown, the copy would
/// store the label as text in place of the token, and the value would be lost.
/// So the copy's stored title holds a token, not a label, and that token's
/// record holds the value; the copy's title as shown has the label.
///
/// Found by the implementer of C-TPII. Green against today's code (the copy
/// is named from the stored title). The mutation that turns it red: the
/// Duplicate skill naming the copy from the title as shown
/// (`view.labelled_title()`).
#[tokio::test]
async fn tp3_a_copy_keeps_the_token_of_its_title_not_the_label() {
    let w_store_title = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let target = w_store_title.clone();
    // The router names the document by its stored form, which today's
    // resolver finds: this test is about the copy, not the lookup.
    let script: Script = Arc::new(move |_, _, i| {
        if i == 0 {
            intent_json("duplicate", Some(target.lock().unwrap().as_str()))
        } else {
            String::new()
        }
    });
    let mut w = keyed_world("tp3-duplicate", script).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    let stored = assert_tokenised(&w, &id, MAIL).await;
    *w_store_title.lock().unwrap() = stored.clone();

    // One request, every proposal approved.
    let (dtx, drx) = tokio::sync::mpsc::channel(8);
    w.orch.set_decision_rx(drx);
    w.rec.take_events();
    let rec = w.rec.clone();
    let done = std::sync::atomic::AtomicBool::new(false);
    let orch = &w.orch;
    let turn = async {
        let r = orch.handle_query("duplicate my transfer note").await;
        done.store(true, std::sync::atomic::Ordering::SeqCst);
        r
    };
    let driver = async {
        for _ in 0..20_000 {
            for _ in proposals(&rec.take_events()) {
                let _ = dtx.send(ActionDecision::Approve).await;
            }
            if done.load(std::sync::atomic::Ordering::SeqCst) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    let (res, ()) = tokio::join!(turn, driver);
    res.unwrap();

    let copies: Vec<Document> = w
        .store
        .list_documents(None)
        .await
        .unwrap()
        .into_iter()
        .filter(|d| d.deleted_at.is_none() && d.id_string().as_deref() != Some(id.as_str()))
        .collect();
    assert_eq!(copies.len(), 1, "one copy made: {:?}", copies.iter().map(|d| &d.title).collect::<Vec<_>>());
    let copy = &copies[0];
    let copy_id = copy.id_string().unwrap();

    assert!(copy.title.contains("[pii:"), "the copy's stored title holds a token: {:?}", copy.title);
    assert!(!copy.title.contains("[Email]"), "never a label stored as text: {:?}", copy.title);
    assert!(!copy.title.contains(MAIL), "nor the value as written: {:?}", copy.title);

    let view = w.docs.read(&copy_id).await.unwrap();
    let title_tokens: Vec<_> = view
        .tokens
        .iter()
        .filter(|t| t.field == sovereign_docs::TextField::Title)
        .filter(|t| matches!(t.status, sovereign_docs::TokenStatus::Token { .. }))
        .collect();
    assert_eq!(title_tokens.len(), 1, "the copy's title token resolves, in the copy: {:?}", view.tokens);
    let rec = w.store.get_pii_record(&title_tokens[0].record_id).await.unwrap();
    let value = sovereign_crypto::vault::EncryptedBlob::from_pair(rec.value_encrypted.clone(), rec.value_nonce.clone())
        .decrypt_to_string(&key())
        .unwrap();
    assert_eq!(value, MAIL, "the value is kept: its record opens to it");
    assert_eq!(view.labelled_title(), format!("Copy of {SHOWN}"), "the copy's title as shown has the label");
}

// ------------------------------------------ TP3, TP4: the chat tools --

/// TP4: the chat tool `get_document`, named with the title as she typed it,
/// reads that document: its text reaches the model.
#[tokio::test]
async fn tp4_the_chat_tool_get_document_finds_the_typed_title() {
    let script = first_then_answer(tool_call("get_document", serde_json::json!({ "title": TYPED })));
    let mut w = keyed_world("tp4-get", script).await;
    let office = w.thread("Office").await;
    let id = w.doc(TYPED, &office, "the quarterly transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    let (_evs, _o) = chat_turn(&mut w, "read my transfer note", ActionDecision::Approve).await;
    let calls = w.rec.calls();
    assert!(calls.len() >= 2, "the model was called again with the tool's result: {}", calls.len());
    assert!(
        !calls[0].prompt.contains("the quarterly transfer note"),
        "precondition: the text is not already in the turn's first prompt, so only the tool can bring it"
    );
    let after = &calls[1].prompt;
    assert!(after.contains("the quarterly transfer note"), "the document's text reached the model: {after}");
}

/// TP3 and TP4: the chat tool `move_document`, named with the title as shown,
/// is proposed on that document, and the proposal names it as shown.
#[tokio::test]
async fn tp3_tp4_the_chat_proposal_names_the_title_as_shown() {
    let script = first_then_answer(tool_call(
        "move_document",
        serde_json::json!({ "document_title": SHOWN, "thread_name": "Kitchen" }),
    ));
    let mut w = keyed_world("tp4-move-proposal", script).await;
    let office = w.thread("Office").await;
    w.thread("Kitchen").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    let (evs, _o) = chat_turn(&mut w, "move my transfer note to the kitchen", ActionDecision::Reject("not now".into())).await;
    let ps = proposals(&evs);
    assert_eq!(ps.len(), 1, "one proposal: {evs:?}");
    let p = &ps[0];
    assert_eq!(p.target.id, id, "proposed on that document: {p:?}");
    assert_eq!(p.target.title, SHOWN, "the proposal names the title as shown: {p:?}");
    assert_no_stored_form_nor_value(&format!("{p:?}"), &[MAIL], "the proposal");
}

/// TP4: approved, the chat tool `move_document` named with the title as she
/// typed it moves that document.
#[tokio::test]
async fn tp4_the_chat_tool_move_document_moves_the_typed_title() {
    let script = first_then_answer(tool_call(
        "move_document",
        serde_json::json!({ "document_title": TYPED, "thread_name": "Kitchen" }),
    ));
    let mut w = keyed_world("tp4-move", script).await;
    let office = w.thread("Office").await;
    let kitchen = w.thread("Kitchen").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    let (_evs, o) = chat_turn(&mut w, "move my transfer note to the kitchen", ActionDecision::Approve).await;
    assert_eq!(o.writes.len(), 1, "one write: {o:?}");
    assert_eq!(o.writes[0].outcome, WriteOutcome::Succeeded, "{}", o.summary_line());
    assert_eq!(w.thread_of(&id).await, kitchen, "that document moved");
}

/// TP3 through a chat tool: no document has the title given, and the turn's
/// outcome line, which gives the write's reason, shows the close titles as
/// shown.
#[tokio::test]
async fn tp3_the_chat_outcome_line_shows_close_titles_with_labels() {
    let script = first_then_answer(tool_call(
        "move_document",
        serde_json::json!({ "document_title": "Transfer", "thread_name": "Kitchen" }),
    ));
    let mut w = keyed_world("tp3-chat-close", script).await;
    let office = w.thread("Office").await;
    w.thread("Kitchen").await;
    let id = w.doc(TYPED, &office, "the transfer note").await;
    assert_tokenised(&w, &id, MAIL).await;
    let (evs, o) = chat_turn(&mut w, "move the transfer to the kitchen", ActionDecision::Approve).await;
    assert!(proposals(&evs).is_empty(), "nothing proposed: {evs:?}");
    let line = o.summary_line();
    assert!(line.contains(SHOWN), "the close title, as shown: {line}");
    assert_no_stored_form_nor_value(&line, &[MAIL], "the chat outcome line");
}
