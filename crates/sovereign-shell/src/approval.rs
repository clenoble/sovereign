//! Lot 0.7 (shell side) — approval hygiene for the action-confirmation and
//! injection-decision modals (review `05-orchestrator.md` item 2).
//!
//! The orchestrator's decision channels are not tied to a proposal: whatever
//! decision is in the channel answers whichever proposal is waiting. So the
//! shell must only ever send an answer for the prompt the user actually saw,
//! while the orchestrator is still waiting for it:
//!
//! - **Local deadline.** The orchestrator gives up after 120 s and fails closed
//!   (Reject / Redact) without telling the shell. The modal closes itself a
//!   little before that, so a late click can never sit in the channel and
//!   answer the *next* proposal.
//! - **Shown, and seen.** A click (or Esc) counts only for the prompt drawn on
//!   screen, and only once it has been on screen long enough to be read — a
//!   prompt that replaced another under the cursor cannot inherit the click
//!   aimed at its predecessor.
//!
//! Track A drains stale decisions on the orchestrator side; the two halves are
//! independent and each holds on its own.

use std::time::{Duration, Instant};

/// How long the orchestrator waits for a decision (`orchestrator.rs`
/// `wait_for_decision` / `wait_for_injection_decision`).
pub(crate) const ORCHESTRATOR_DECISION_TIMEOUT: Duration = Duration::from_secs(120);

/// The modal closes this much earlier than the orchestrator gives up, covering
/// the delay between the orchestrator sending the proposal and the shell
/// draining it.
pub(crate) const MODAL_SAFETY_MARGIN: Duration = Duration::from_secs(10);

/// A prompt must have been on screen this long before an answer counts.
pub(crate) const MIN_VISIBLE: Duration = Duration::from_millis(400);

/// What to do with a user's answer to the open prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AnswerGate {
    /// Send it: it answers the prompt on screen, in time. The slot is closed.
    Send,
    /// No prompt is open.
    NoneOpen,
    /// The open prompt has not been drawn yet (it replaced the one the user was
    /// looking at). Nothing is sent; the prompt stays open.
    NotShown,
    /// The prompt appeared too recently to have been read. Nothing is sent;
    /// the prompt stays open.
    TooSoon,
    /// The prompt outlived the orchestrator's wait. Nothing is sent; the slot is
    /// closed.
    Expired,
}

#[derive(Debug)]
struct OpenPrompt {
    #[cfg_attr(not(test), allow(dead_code))]
    id: u64,
    deadline: Instant,
    shown_at: Option<Instant>,
}

/// One modal's lifecycle: at most one open prompt, which a newer one replaces.
#[derive(Debug, Default)]
pub(crate) struct DecisionSlot {
    next_id: u64,
    open: Option<OpenPrompt>,
}

impl DecisionSlot {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A prompt arrived at `now`. It replaces any unanswered one (that one is
    /// left to the orchestrator's own timeout, which fails closed). Returns its id.
    pub(crate) fn open(&mut self, now: Instant) -> u64 {
        self.next_id += 1;
        let deadline = now + ORCHESTRATOR_DECISION_TIMEOUT - MODAL_SAFETY_MARGIN;
        self.open = Some(OpenPrompt { id: self.next_id, deadline, shown_at: None });
        self.next_id
    }

    /// The frame drew the open prompt at `now` (first draw counts).
    pub(crate) fn mark_shown(&mut self, now: Instant) {
        if let Some(p) = &mut self.open {
            p.shown_at.get_or_insert(now);
        }
    }

    /// Close the open prompt if its deadline has passed. True if it closed one.
    pub(crate) fn expire(&mut self, now: Instant) -> bool {
        match &self.open {
            Some(p) if now >= p.deadline => {
                self.open = None;
                true
            }
            _ => false,
        }
    }

    /// The user answered what is on screen, at `now`. See [`AnswerGate`].
    pub(crate) fn answer(&mut self, now: Instant) -> AnswerGate {
        let Some(p) = &self.open else { return AnswerGate::NoneOpen };
        if now >= p.deadline {
            self.open = None;
            return AnswerGate::Expired;
        }
        let Some(shown_at) = p.shown_at else { return AnswerGate::NotShown };
        if now.duration_since(shown_at) < MIN_VISIBLE {
            return AnswerGate::TooSoon;
        }
        self.open = None;
        AnswerGate::Send
    }

    /// The id of the open prompt, if any.
    #[cfg(test)]
    pub(crate) fn open_id(&self) -> Option<u64> {
        self.open.as_ref().map(|p| p.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEEN: Duration = Duration::from_millis(600);

    #[test]
    fn the_modal_closes_before_the_orchestrator_stops_waiting() {
        assert!(MODAL_SAFETY_MARGIN > Duration::ZERO);
        assert!(MODAL_SAFETY_MARGIN < ORCHESTRATOR_DECISION_TIMEOUT);
        let t0 = Instant::now();
        let mut s = DecisionSlot::new();
        s.open(t0);
        s.mark_shown(t0);
        let last_safe = t0 + ORCHESTRATOR_DECISION_TIMEOUT - MODAL_SAFETY_MARGIN;
        assert!(!s.expire(last_safe - Duration::from_millis(1)));
        assert!(s.expire(last_safe));
        assert_eq!(s.open_id(), None);
    }

    #[test]
    fn an_answer_after_the_deadline_is_never_sent() {
        let t0 = Instant::now();
        let mut s = DecisionSlot::new();
        s.open(t0);
        s.mark_shown(t0);
        let late = t0 + ORCHESTRATOR_DECISION_TIMEOUT;
        assert_eq!(s.answer(late), AnswerGate::Expired);
        assert_eq!(s.answer(late), AnswerGate::NoneOpen, "and the slot is closed");
    }

    #[test]
    fn an_answer_is_sent_once_for_the_prompt_on_screen() {
        let t0 = Instant::now();
        let mut s = DecisionSlot::new();
        s.open(t0);
        s.mark_shown(t0);
        assert_eq!(s.answer(t0 + SEEN), AnswerGate::Send);
        assert_eq!(s.answer(t0 + SEEN), AnswerGate::NoneOpen, "a double click sends nothing more");
    }

    #[test]
    fn an_answer_is_never_delivered_to_a_proposal_other_than_the_one_shown() {
        let t0 = Instant::now();
        let mut s = DecisionSlot::new();
        let a = s.open(t0);
        s.mark_shown(t0);
        // B replaces A before the next frame draws it.
        let b = s.open(t0 + SEEN);
        assert_ne!(a, b);
        assert_eq!(s.answer(t0 + SEEN), AnswerGate::NotShown, "the click aimed at A must not answer B");
        assert_eq!(s.open_id(), Some(b), "B stays open");
        // B is drawn; a click right away is still too soon to be B's.
        let tb = t0 + SEEN + Duration::from_millis(16);
        s.mark_shown(tb);
        assert_eq!(s.answer(tb + Duration::from_millis(50)), AnswerGate::TooSoon);
        assert_eq!(s.answer(tb + SEEN), AnswerGate::Send);
    }

    #[test]
    fn a_new_prompt_gets_a_fresh_deadline() {
        let t0 = Instant::now();
        let mut s = DecisionSlot::new();
        s.open(t0);
        let t1 = t0 + Duration::from_secs(100);
        s.open(t1);
        s.mark_shown(t1);
        assert!(!s.expire(t0 + ORCHESTRATOR_DECISION_TIMEOUT - MODAL_SAFETY_MARGIN));
        assert_eq!(s.answer(t1 + SEEN), AnswerGate::Send);
    }
}
