//! Security type system for Sovereign GE.
//!
//! Defines action levels, data/control plane separation,
//! and the authorization primitives that every other phase depends on.

use serde::{Deserialize, Serialize};

/// Gravity level of an action, from least to most destructive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ActionLevel {
    /// Level 1 — read-only observation (search, list, view).
    Observe = 1,
    /// Level 2 — add metadata without changing content (tag, annotate, bookmark).
    Annotate = 2,
    /// Level 3 — change content or structure (edit, rename, move).
    Modify = 3,
    /// Level 4 — send data outside the system (export, share, email).
    Transmit = 4,
    /// Level 5 — irreversible destruction (delete, purge).
    Destruct = 5,
}

/// Whether an action originates from the user (Control plane)
/// or from document content (Data plane).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Plane {
    /// User-initiated via search bar, voice, or keyboard.
    Control,
    /// Derived from document text (e.g. embedded instructions).
    Data,
}

/// What kind of object an action target is (spec ch. 08, *Action targets*,
/// AT1). `None` is the neutral value of a proposal whose target is not
/// resolved into this type yet (C-HOST, lot LH: types and signatures first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TargetKind {
    #[default]
    None,
    Thread,
    Document,
    Version,
    Model,
    Skill,
}

/// The object a proposal acts on, resolved before proposing (AT1): its kind,
/// id, displayed title, and the revision seen when the proposal was built.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ActionTarget {
    pub kind: TargetKind,
    pub id: String,
    pub title: String,
    pub revision: String,
}

/// The id of one proposal (AT6): a decision applies only to the proposal with
/// that id. `RequestId::default()` is the neutral value of a proposal not yet
/// given one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RequestId(pub u64);

fn no_deadline() -> chrono::DateTime<chrono::Utc> {
    ProposedAction::NO_DEADLINE
}

/// A proposed action awaiting authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub action: String,
    pub level: ActionLevel,
    pub plane: Plane,
    pub doc_id: Option<String>,
    pub thread_id: Option<String>,
    pub description: String,
    /// The object acted on (AT1).
    #[serde(default)]
    pub target: ActionTarget,
    /// This proposal's id (AT6).
    #[serde(default)]
    pub request_id: RequestId,
    /// When the proposal expires, counted from when it was issued (AT6); a
    /// host counts down to it. [`ProposedAction::NO_DEADLINE`] until set.
    #[serde(default = "no_deadline")]
    pub deadline: chrono::DateTime<chrono::Utc>,
}

impl ProposedAction {
    /// The neutral deadline of a proposal not yet given one (the Unix epoch).
    pub const NO_DEADLINE: chrono::DateTime<chrono::Utc> = chrono::DateTime::<chrono::Utc>::UNIX_EPOCH;
}

/// The user's decision on a proposed action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionDecision {
    Approve,
    Reject(String),
}

/// A decision bound to the proposal it answers (AT6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDecision {
    pub request_id: RequestId,
    pub decision: ActionDecision,
}

/// The user's decision on how to handle a high-severity injection detected in
/// agent-loop tool output. Deliberately *separate* from [`ActionDecision`] so
/// injection handling never gets conflated with write-approval.
///
/// Decision 2026-07-18 (INJECTION-002): the agent loop must not silently
/// auto-redact — it surfaces the injection and lets the user choose. This gates
/// only the *extra* whole-content nuke; the structural `fence_external`
/// untrusted-data wrapping around tool output stays always-on regardless.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InjectionDecision {
    /// Replace the flagged content before it reaches the model. Also the
    /// fail-closed default (timeout / no channel wired).
    Redact,
    /// Let the content through (still `fence_external`-wrapped downstream) —
    /// the user accepts it.
    PassThrough,
    /// Abort the current turn; the flagged content never reaches the model.
    Abort,
}

/// Map an intent action string to its gravity level.
pub fn action_level(action: &str) -> ActionLevel {
    match action {
        "search" | "open" | "navigate" | "history" | "summarize" | "word_count"
        | "list_models" | "list_milestones" | "chat"
        | "sync_device" | "list_guardians" | "sync_status" | "list_devices"
        // UI panel toggles — read-only state changes the user can dismiss.
        | "open_pii_dashboard" | "open_models" | "open_inbox" | "browse"
        | "open_settings" => ActionLevel::Observe,
        "annotate" | "tag" | "bookmark" => ActionLevel::Annotate,
        "create_document" | "create_thread" | "rename_thread" | "move_document"
        | "restore" | "edit" | "find_replace" | "duplicate" | "import_file"
        | "swap_model" | "split_thread" | "adopt"
        | "create_milestone" | "delete_milestone"
        // C-MERGE MG6 (Céline, 2026-11-13: « Niveau 3 normal »): a merge keeps
        // its source and can be undone, so it is level 3, as is its undo.
        | "merge_threads" | UNDO_MERGE_ACTION => ActionLevel::Modify,
        "export" | "share" | "transmit"
        | "pair_device" | "enroll_guardian" | "rotate_shards" => ActionLevel::Transmit,
        "delete_thread" | "delete_document" | "purge"
        | "initiate_recovery" | "revoke_guardian" => ActionLevel::Destruct,
        _ => ActionLevel::Observe,
    }
}

/// C-MERGE MG5 (spec ch. 03; Céline, 2026-11-17: « Assistant seul »): the
/// action the router names when the person asks the assistant to undo the
/// merge of a thread, named by the thread (its target). Its handler clears the
/// thread's two merge fields under a new merge stamp. Name by the B1 test
/// author; the router's prompt, the handler and the level are the
/// implementer's.
pub const UNDO_MERGE_ACTION: &str = "undo_merge_threads";

/// Decide whether an action can proceed automatically.
/// Levels 1-2 are auto-approved. Levels 3-5 require confirmation.
pub fn authorize(level: ActionLevel) -> ActionDecision {
    match level {
        ActionLevel::Observe | ActionLevel::Annotate => ActionDecision::Approve,
        _ => ActionDecision::Reject("Requires user confirmation".into()),
    }
}

/// Visual state of the AI bubble, driven by orchestrator activity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BubbleVisualState {
    Idle,
    ProcessingOwned,
    ProcessingExternal,
    Proposing,
    Executing,
    Suggesting,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_level_observe() {
        assert_eq!(action_level("search"), ActionLevel::Observe);
        assert_eq!(action_level("open"), ActionLevel::Observe);
        assert_eq!(action_level("navigate"), ActionLevel::Observe);
        assert_eq!(action_level("history"), ActionLevel::Observe);
    }

    #[test]
    fn action_level_annotate() {
        assert_eq!(action_level("annotate"), ActionLevel::Annotate);
        assert_eq!(action_level("tag"), ActionLevel::Annotate);
        assert_eq!(action_level("bookmark"), ActionLevel::Annotate);
    }

    #[test]
    fn action_level_modify() {
        assert_eq!(action_level("create_thread"), ActionLevel::Modify);
        assert_eq!(action_level("rename_thread"), ActionLevel::Modify);
        assert_eq!(action_level("move_document"), ActionLevel::Modify);
        assert_eq!(action_level("restore"), ActionLevel::Modify);
    }

    #[test]
    fn action_level_transmit() {
        assert_eq!(action_level("export"), ActionLevel::Transmit);
        assert_eq!(action_level("share"), ActionLevel::Transmit);
    }

    #[test]
    fn action_level_destruct() {
        assert_eq!(action_level("delete_thread"), ActionLevel::Destruct);
        assert_eq!(action_level("delete_document"), ActionLevel::Destruct);
        assert_eq!(action_level("purge"), ActionLevel::Destruct);
    }

    #[test]
    fn action_level_skill_observe() {
        assert_eq!(action_level("summarize"), ActionLevel::Observe);
        assert_eq!(action_level("word_count"), ActionLevel::Observe);
    }

    #[test]
    fn action_level_skill_modify() {
        assert_eq!(action_level("find_replace"), ActionLevel::Modify);
        assert_eq!(action_level("duplicate"), ActionLevel::Modify);
        assert_eq!(action_level("import_file"), ActionLevel::Modify);
    }

    #[test]
    fn action_level_merge_destruct_split_modify() {
        // C-MERGE MG6 (spec v2.0.109; Céline, 2026-11-13: « Niveau 3 normal »):
        // a merge is reversible, so it is level 3. Until then, v0.0.12 item 23,
        // T4 (Céline, 2026-10-25): merging always asked, at Destruct. The name
        // is kept; the B1 test author changed the assertion.
        assert_eq!(action_level("merge_threads"), ActionLevel::Modify);
        assert_eq!(action_level("split_thread"), ActionLevel::Modify);
    }

    #[test]
    fn action_level_milestone_ops() {
        assert_eq!(action_level("create_milestone"), ActionLevel::Modify);
        assert_eq!(action_level("delete_milestone"), ActionLevel::Modify);
        assert_eq!(action_level("list_milestones"), ActionLevel::Observe);
    }

    #[test]
    fn action_level_unknown_defaults_to_observe() {
        assert_eq!(action_level("something_new"), ActionLevel::Observe);
    }

    #[test]
    fn authorize_auto_approves_low_levels() {
        assert_eq!(authorize(ActionLevel::Observe), ActionDecision::Approve);
        assert_eq!(authorize(ActionLevel::Annotate), ActionDecision::Approve);
    }

    #[test]
    fn authorize_rejects_high_levels() {
        assert!(matches!(authorize(ActionLevel::Modify), ActionDecision::Reject(_)));
        assert!(matches!(authorize(ActionLevel::Transmit), ActionDecision::Reject(_)));
        assert!(matches!(authorize(ActionLevel::Destruct), ActionDecision::Reject(_)));
    }

    #[test]
    fn action_level_ordering() {
        assert!(ActionLevel::Observe < ActionLevel::Annotate);
        assert!(ActionLevel::Annotate < ActionLevel::Modify);
        assert!(ActionLevel::Modify < ActionLevel::Transmit);
        assert!(ActionLevel::Transmit < ActionLevel::Destruct);
    }

    #[test]
    fn bubble_visual_state_variants() {
        let states = [
            BubbleVisualState::Idle,
            BubbleVisualState::ProcessingOwned,
            BubbleVisualState::ProcessingExternal,
            BubbleVisualState::Proposing,
            BubbleVisualState::Executing,
            BubbleVisualState::Suggesting,
        ];
        // Each state is distinct
        for (i, a) in states.iter().enumerate() {
            for (j, b) in states.iter().enumerate() {
                if i == j {
                    assert_eq!(a, b);
                } else {
                    assert_ne!(a, b);
                }
            }
        }
    }

    #[test]
    fn proposed_action_serializable() {
        let pa = ProposedAction {
            action: "delete_thread".into(),
            level: ActionLevel::Destruct,
            plane: Plane::Control,
            doc_id: None,
            thread_id: Some("thread:abc".into()),
            description: "Delete thread abc".into(),
            target: ActionTarget::default(),
            request_id: RequestId::default(),
            deadline: ProposedAction::NO_DEADLINE,
        };
        let json = serde_json::to_string(&pa).unwrap();
        assert!(json.contains("delete_thread"));
        assert!(json.contains("Destruct"));
    }

    #[test]
    fn action_level_list_models_observe() {
        assert_eq!(action_level("list_models"), ActionLevel::Observe);
    }

    #[test]
    fn action_level_swap_model_modify() {
        assert_eq!(action_level("swap_model"), ActionLevel::Modify);
    }

    #[test]
    fn action_level_p2p_observe() {
        assert_eq!(action_level("sync_device"), ActionLevel::Observe);
        assert_eq!(action_level("list_guardians"), ActionLevel::Observe);
        assert_eq!(action_level("sync_status"), ActionLevel::Observe);
        assert_eq!(action_level("list_devices"), ActionLevel::Observe);
    }

    #[test]
    fn action_level_p2p_transmit() {
        assert_eq!(action_level("pair_device"), ActionLevel::Transmit);
        assert_eq!(action_level("enroll_guardian"), ActionLevel::Transmit);
        assert_eq!(action_level("rotate_shards"), ActionLevel::Transmit);
    }

    #[test]
    fn action_level_p2p_destruct() {
        assert_eq!(action_level("initiate_recovery"), ActionLevel::Destruct);
        assert_eq!(action_level("revoke_guardian"), ActionLevel::Destruct);
    }

    #[test]
    fn action_decision_equality() {
        assert_eq!(ActionDecision::Approve, ActionDecision::Approve);
        assert_ne!(
            ActionDecision::Approve,
            ActionDecision::Reject("reason".into())
        );
    }
}
