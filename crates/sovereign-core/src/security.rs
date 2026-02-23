//! Security type system for Sovereign OS.
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

/// A proposed action awaiting authorization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedAction {
    pub action: String,
    pub level: ActionLevel,
    pub plane: Plane,
    pub doc_id: Option<String>,
    pub thread_id: Option<String>,
    pub description: String,
}

/// The user's decision on a proposed action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActionDecision {
    Approve,
    Reject(String),
}

/// Map an intent action string to its gravity level.
pub fn action_level(action: &str) -> ActionLevel {
    match action {
        "search" | "open" | "navigate" | "history" | "summarize" | "word_count"
        | "list_models" | "list_milestones" | "chat"
        | "sync_device" | "list_guardians" | "sync_status" | "list_devices" => ActionLevel::Observe,
        "annotate" | "tag" | "bookmark" => ActionLevel::Annotate,
        "create_document" | "create_thread" | "rename_thread" | "move_document"
        | "restore" | "edit" | "find_replace" | "duplicate" | "import_file"
        | "swap_model" | "merge_threads" | "split_thread" | "adopt"
        | "create_milestone" | "delete_milestone" => ActionLevel::Modify,
        "export" | "share" | "transmit"
        | "pair_device" | "enroll_guardian" | "rotate_shards" => ActionLevel::Transmit,
        "delete_thread" | "delete_document" | "purge"
        | "initiate_recovery" | "revoke_guardian" => ActionLevel::Destruct,
        _ => ActionLevel::Observe,
    }
}

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
    fn action_level_merge_split_modify() {
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
