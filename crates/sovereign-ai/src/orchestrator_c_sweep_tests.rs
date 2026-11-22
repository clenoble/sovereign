//! **C-SWEEP SW3** (spec ch. 09, v2.0.111): masked where it is read, for every
//! path to the model. The mask is computed on the text as it is read, with the
//! content's id; detected values reach the model as their type's label, a
//! token as its label, and a value she dismissed (SW5) as written, so that her
//! dismissal reaches the assistant too. A document whole: body and title. The
//! summary no longer refuses an unscanned document.
//!
//! Over the assistant harness's fake model, which records every prompt. The
//! orchestrator is given the persona's account key, as the hosts give it at
//! login (`set_pii_account_key`).
//!
//! The gate check (a call to the model outside the named ones) is
//! `sovereign-core/tests/c_sweep_sw3_model_calls.rs`; the incoming-change
//! analysis is held with sovereign-p2p's tests.
//!
//! Written before the code by the test author of lot LS(a); whoever implements
//! does not edit this file.

use super::assistant_harness::*;
use super::*;
use crate::docs_host::testing::account_key;
use crate::pii::review::PiiReview;

const IBAN: &str = "CH93 0076 2011 6238 5295 7";
const IBAN2: &str = "CH56 0483 5012 3456 7800 9";

/// The world, with the persona's account key given to the orchestrator.
async fn masked_world(tag: &str, script: Script) -> World {
    let w = world(tag, 4096, script).await;
    w.orch.set_pii_account_key(account_key());
    w
}

/// A document created through the editor (its values replaced, confirmed),
/// then every IBAN of it dismissed: its title and body hold the values as
/// written again.
async fn dismissed_doc(w: &World, title: &str, text: &str) -> String {
    let thread = w.thread("Dismissed").await;
    let id = w.doc(title, &thread, text).await;
    let records: Vec<String> = w
        .store
        .list_pii_records(None, None, Some(false))
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.kind == sovereign_db::schema::PiiKind::Iban)
        .filter(|r| r.sources.iter().any(|s| s.source_id == id))
        .filter_map(|r| r.id_string())
        .collect();
    assert!(!records.is_empty(), "test setup: the editor replaced the IBAN");
    PiiReview::new(w.docs.clone(), account_key()).dismiss(&records).await.unwrap();
    let view = w.docs.read(&id).await.unwrap();
    assert!(view.title.contains(IBAN) && view.plain_text().contains(IBAN), "test setup: restored as written");
    id
}

/// A document stored without ingest (not analysed), its values raw.
async fn unanalysed_doc(w: &World, title: &str, text: &str) -> String {
    let unkeyed = crate::docs_host::build_document_service(w.store.clone(), None, "t".into());
    let thread = w.thread("Raw").await;
    let id = crate::docs_host::create(&unkeyed, crate::docs_host::new_user_document(title, thread, text))
        .await
        .unwrap()
        .id;
    assert!(!w.docs.read(&id).await.unwrap().pii_scanned, "test setup: not analysed");
    id
}

fn tool_call(name: &str, title: &str) -> String {
    serde_json::json!({ "name": name, "arguments": { "title": title } }).to_string()
}

/// No call carries `value` raw.
fn never_raw(calls: &[Call], value: &str) {
    for (i, c) in calls.iter().enumerate() {
        assert!(!c.prompt.contains(value), "call {i} carries the undismissed value raw");
    }
}

// ------------------------------------------------------------------ titles --

/// SW3: a title is masked where it is read for a prompt: an unanalysed
/// document's title reaches the chat's workspace context as its type's label.
#[tokio::test]
async fn sw3_a_title_reaches_the_chat_context_as_its_label() {
    let w = masked_world("sw3-title-context", answer("Hello.")).await;
    unanalysed_doc(&w, &format!("Transfer {IBAN2}"), "nothing else").await;
    w.act("chat", None).await;
    let calls = w.rec.calls();
    assert!(!calls.is_empty());
    never_raw(&calls, IBAN2);
    assert!(calls[0].prompt.contains("Transfer [IBAN]"), "the title, with its label, in the context");
}

/// SW3: the chat tools read a document whole: the title the read tool gives
/// back is masked as its body is.
#[tokio::test]
async fn sw3_a_title_read_by_a_tool_is_masked() {
    let w = masked_world(
        "sw3-title-tool",
        Arc::new(|_, _, i| if i == 0 { tool_call("list_documents", "") } else { "Listed.".into() }),
    )
    .await;
    unanalysed_doc(&w, &format!("Transfer {IBAN2}"), "nothing else").await;
    w.act("chat", None).await;
    let calls = w.rec.calls();
    assert!(calls.len() >= 2, "the tool ran and the model read its output");
    never_raw(&calls, IBAN2);
}

// -------------------------------------------- the IBAN pair: the chat tools --

/// SW3, the contract's test, on the chat's context and tools: a dismissed
/// IBAN reaches the model as written, in a body and in a title; an
/// undismissed one as its label, in a body and in a title.
#[tokio::test]
async fn sw3_the_iban_pair_on_the_chat_context_and_tools() {
    let dismissed_title = format!("Refund {IBAN}");
    let undismissed_title = format!("Transfer {IBAN2}");
    let (d, u) = (dismissed_title.clone(), undismissed_title.clone());
    let w = masked_world(
        "sw3-pair-chat",
        Arc::new(move |_, _, i| match i {
            0 => tool_call("get_document", &d),
            1 => tool_call("get_document", &u),
            _ => "Read both.".into(),
        }),
    )
    .await;
    dismissed_doc(&w, &dismissed_title, &format!("refund to {IBAN}")).await;
    unanalysed_doc(&w, &undismissed_title, &format!("send to {IBAN2}")).await;

    w.act("chat", None).await;
    let calls = w.rec.calls();
    assert!(calls.len() >= 3, "both documents were read: {} calls", calls.len());
    never_raw(&calls, IBAN2);
    assert!(calls[0].prompt.contains(&dismissed_title), "the dismissed title as written, in the context");
    assert!(calls[0].prompt.contains("Transfer [IBAN]"), "the undismissed title as its label, in the context");
    let after_first = &calls[1].prompt;
    assert!(after_first.contains(&format!("refund to {IBAN}")), "the dismissed body as written, through the tool");
    let after_second = &calls[2].prompt;
    assert!(after_second.contains("send to [IBAN]"), "the undismissed body as its label, through the tool");
}

// ---------------------------------------------- the IBAN pair: the summary --

/// SW3: the summary no longer refuses an unscanned document; masking at read
/// covers it: its value reaches the model as its label. (Replaces the
/// summarize lot's S5 refusal test, retired for this guarantee.)
#[tokio::test]
async fn sw3_the_summary_no_longer_refuses_an_unscanned_document() {
    let w = masked_world("sw3-summary-unscanned", answer("A transfer.")).await;
    unanalysed_doc(&w, "Transfer note", &format!("send the money to {IBAN2} on Monday")).await;
    let evs = w.act("summarize", Some("Transfer note")).await;
    let calls = w.rec.calls();
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
    assert!(!calls.is_empty(), "the document was sent, masked");
    never_raw(&calls, IBAN2);
    assert!(calls.iter().any(|c| c.prompt.contains("[IBAN]")), "the value as its label");
}

/// SW3, the contract's test, on the summary: a dismissed IBAN reaches the
/// model as written.
#[tokio::test]
async fn sw3_the_summary_reads_a_dismissed_value_as_written() {
    let w = masked_world("sw3-summary-dismissed", answer("A refund.")).await;
    dismissed_doc(&w, &format!("Refund {IBAN}"), &format!("refund to {IBAN} on Monday")).await;
    let evs = w.act("summarize", Some(&format!("Refund {IBAN}"))).await;
    let calls = w.rec.calls();
    assert_eq!(executed(&evs), vec![true], "{}", reply(&evs));
    assert!(calls.iter().any(|c| c.prompt.contains(&format!("refund to {IBAN}"))), "her dismissal reaches the model");
}
