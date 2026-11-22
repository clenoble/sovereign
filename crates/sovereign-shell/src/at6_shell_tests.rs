//! C-HOST AT6 in the shell — **decisions are bound to their proposal** (spec
//! ch. 08, *Action targets*, AT6: « Each proposal has a request id; a decision
//! applies only to the proposal with that id. The deadline runs from when the
//! proposal was issued, not from when a host displays it: the proposal carries
//! its deadline, and a host counts down to it. »).
//!
//! Found by the Fable maille (claude-coord, from-fable-2nde-position/0050): the
//! orchestrator binds each decision and carries the deadline, but the shell
//! drops both, answers with a bare `ActionDecision` on the bare channel, and its
//! modal counts 110 s from receipt.
//!
//! - **The modal**, at `approval.rs`'s level, through the seam
//!   `DecisionSlot::open_bound` / `open_request` (« types and signatures »
//!   commit): it closes at the carried deadline less its margin, and an answer is
//!   bound to the request of the prompt on screen.
//! - **The call sites**, read in `app.rs`'s production part, as
//!   `at8_identity_tests::at8_app_call_sites_carry_ids_not_positions` does: the
//!   prompt keeps the proposal's request id and deadline, the answer is a
//!   `RequestDecision` on the bound channel (`set_request_decision_rx` wired), and
//!   the bare channel has no sender left.
//!
//! Written by the LH test author (PR-0011); the implementer does not edit this
//! file.

use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use sovereign_core::security::RequestId;

use crate::approval::{AnswerGate, DecisionSlot, MODAL_SAFETY_MARGIN};

const SEEN: Duration = Duration::from_millis(600);

fn wall() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-11-14T20:00:00Z").unwrap().with_timezone(&Utc)
}

// ------------------------------------------------------------ the modal --

/// AT6: a proposal issued 90 s before the shell received it carries a deadline
/// 30 s away: the modal closes at that deadline less its margin (20 s after
/// receipt), not 110 s after receipt.
#[test]
fn at6_the_modal_closes_at_the_carried_deadline_less_its_margin() {
    let (t0, w0) = (Instant::now(), wall());
    let deadline = w0 + chrono::Duration::seconds(30);
    let mut s = DecisionSlot::new();
    s.open_bound(t0, w0, RequestId(7), deadline);
    s.mark_shown(t0);
    let last = t0 + Duration::from_secs(30) - MODAL_SAFETY_MARGIN;
    assert!(!s.expire(last - Duration::from_millis(1)), "still open just before the carried deadline less the margin");
    assert!(s.expire(last), "closed at the carried deadline less the margin, not 110 s after receipt");
}

/// AT6: a proposal received when its carried deadline, less the margin, has
/// already passed is never answered: the answer is expired at once.
#[test]
fn at6_a_proposal_received_past_its_deadline_is_never_answered() {
    let (t0, w0) = (Instant::now(), wall());
    let deadline = w0 + margin() - chrono::Duration::seconds(1);
    let mut s = DecisionSlot::new();
    s.open_bound(t0, w0, RequestId(8), deadline);
    s.mark_shown(t0);
    assert_eq!(s.answer(t0 + SEEN), AnswerGate::Expired, "nothing is sent for a proposal past its deadline");
}

/// [`MODAL_SAFETY_MARGIN`] on the wall clock.
fn margin() -> chrono::Duration {
    chrono::Duration::from_std(MODAL_SAFETY_MARGIN).unwrap()
}

/// AT6 (how far « a decision for an earlier request id is discarded by the
/// shell's path » goes without the orchestrator's half): an answer is bound to
/// the request of the prompt on screen. A prompt for request 1 replaced by one
/// for request 2: what is open, and what an answer would carry, is request 2,
/// never 1. (The orchestrator's half, a decision bound to another id discarded,
/// is `orchestrator_lh_api_tests::at6_decision_applies_only_to_its_own_proposal`.)
#[test]
fn at6_an_answer_is_bound_to_the_request_of_the_prompt_on_screen() {
    let (t0, w0) = (Instant::now(), wall());
    let deadline = w0 + chrono::Duration::seconds(120);
    let mut s = DecisionSlot::new();
    assert_eq!(s.open_request(), None, "no prompt open, no request");
    s.open_bound(t0, w0, RequestId(1), deadline);
    assert_eq!(s.open_request(), Some(RequestId(1)));
    s.open_bound(t0 + SEEN, w0 + chrono::Duration::milliseconds(600), RequestId(2), deadline + chrono::Duration::seconds(1));
    assert_eq!(s.open_request(), Some(RequestId(2)), "the newer prompt replaces the older one, and its request with it");
    s.mark_shown(t0 + SEEN);
    assert_eq!(s.open_request(), Some(RequestId(2)), "what an answer is bound to, just before it is given");
    assert_eq!(s.answer(t0 + SEEN + SEEN), AnswerGate::Send);
    assert_eq!(s.open_request(), None, "answered: nothing open, no request left to answer");
}

// ------------------------------------------------- the call sites in App --

/// `app.rs`'s production part before the development switches (`run_shot`
/// and the probes after it), as code lines: comments dropped, whitespace
/// removed.
fn app_production() -> Vec<(usize, String)> {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/app.rs")).unwrap();
    let end = src
        .find("#[cfg(any(debug_assertions, feature = \"dev-switches\"))]\npub(crate) fn run_shot")
        .expect("the development switches start at run_shot");
    src[..end]
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l.split("//").next().unwrap_or("").chars().filter(|c| !c.is_whitespace()).collect()))
        .collect()
}

/// The squashed body of `fn name(` in `code`: from its line to the next line
/// that starts another `fn` at any depth (enough for the short functions read
/// here).
fn body(code: &[(usize, String)], name: &str) -> String {
    let start = code.iter().position(|(_, l)| l.contains(&format!("fn{name}("))).unwrap_or_else(|| panic!("fn {name} in app.rs"));
    let mut out = code[start].1.clone();
    for (_, l) in &code[start + 1..] {
        if l.starts_with("fn") || l.starts_with("pub(crate)fn") || l.starts_with("pubfn") || l.starts_with("asyncfn") {
            break;
        }
        out.push_str(l);
    }
    out
}

/// AT6 at the call sites (Fable 0050). In `app.rs`'s production part:
/// - the prompt keeps the proposal's request id and deadline
///   (`set_action_prompt` reads `proposal.request_id` and `proposal.deadline`,
///   and opens the slot bound to them);
/// - the answer is sent as a `RequestDecision` (`decide_action` builds one) whose
///   request id is read from the slot (`open_request`, Fable 0051), and the bound
///   channel is wired (`set_request_decision_rx` is called);
/// - the bare channel has no sender left: no `Sender<ActionDecision>`, and
///   `set_decision_rx` is not called.
#[test]
fn at6_the_shell_sends_decisions_bound_to_their_proposal() {
    let prod = app_production();
    let joined: String = prod.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n");
    let mut wrong = vec![];

    let prompt = body(&prod, "set_action_prompt");
    for needed in ["proposal.request_id", "proposal.deadline", "open_bound("] {
        if !prompt.contains(needed) {
            wrong.push(format!("set_action_prompt: `{needed}` is not used (the prompt must keep the proposal's request id and deadline)"));
        }
    }
    let decide = body(&prod, "decide_action");
    if !decide.contains("RequestDecision") {
        wrong.push("decide_action: the answer is not sent as a RequestDecision".into());
    }
    // Fable 0051: the request id comes from the prompt on screen, read from the
    // slot (not RequestId::default(), not a field of its own). The slot holds no
    // request once answered (at6_an_answer_is_bound_to_the_request_of_the_prompt_on_screen),
    // so it must be read before the answer closes it.
    if !decide.contains("open_request(") {
        wrong.push("decide_action: the answer's request id is not read from the slot (open_request)".into());
    }
    if !joined.contains("set_request_decision_rx(") {
        wrong.push("the bound channel is not wired: set_request_decision_rx is never called".into());
    }
    for (n, l) in &prod {
        if l.contains("Sender<ActionDecision>") {
            wrong.push(format!("app.rs:{n}: a sender of bare ActionDecision: {l}"));
        }
        if l.contains(".set_decision_rx(") {
            wrong.push(format!("app.rs:{n}: the bare channel is wired: {l}"));
        }
    }
    assert!(wrong.is_empty(), "AT6 in the shell:\n{}", wrong.join("\n"));
}
