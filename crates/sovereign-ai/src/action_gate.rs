//! Action gate — authorization layer between intent classification and execution.
//!
//! Checks plane violations (data-plane content triggering control-plane actions)
//! and builds proposals for high-gravity actions that need user confirmation.

use sovereign_core::interfaces::UserIntent;
use sovereign_core::security::{action_level, ActionLevel, Plane, ProposedAction};

/// Check if a data-plane intent is trying to trigger a control-plane action.
/// Returns a reason string if the violation is detected.
pub fn check_plane_violation(intent: &UserIntent) -> Option<String> {
    if intent.origin == Plane::Data {
        let level = action_level(&intent.action);
        if level >= ActionLevel::Modify {
            return Some(format!(
                "Data-plane content attempted control-plane action '{}' (level {:?})",
                intent.action, level
            ));
        }
    }
    None
}

/// Wrap a classified intent into a ProposedAction with computed level.
pub fn build_proposal(intent: &UserIntent) -> ProposedAction {
    let level = action_level(&intent.action);
    let target = intent.target.as_deref().unwrap_or("?");
    let description = match intent.action.as_str() {
        "create_thread" => format!("Create thread '{}'", target),
        "rename_thread" => format!("Rename thread '{}'", target),
        "delete_thread" => format!("Delete thread '{}'", target),
        "move_document" => format!("Move document: {}", target),
        "create_document" => format!("Create document '{}'", target),
        "delete_document" => format!("Delete document '{}'", target),
        _ => format!("{} → {}", intent.action, target),
    };
    ProposedAction {
        action: intent.action.clone(),
        level,
        plane: intent.origin,
        doc_id: None,
        thread_id: intent.target.clone(),
        description,
    }
}

/// Determine whether this action level requires user confirmation.
pub fn requires_confirmation(level: ActionLevel) -> bool {
    level >= ActionLevel::Modify
}

#[cfg(test)]
mod tests {
    use super::*;
    use sovereign_core::security::Plane;

    fn make_intent(action: &str, origin: Plane) -> UserIntent {
        UserIntent {
            action: action.into(),
            target: Some("test".into()),
            confidence: 0.9,
            entities: vec![],
            origin,
        }
    }

    #[test]
    fn no_violation_for_control_plane() {
        let intent = make_intent("delete_thread", Plane::Control);
        assert!(check_plane_violation(&intent).is_none());
    }

    #[test]
    fn no_violation_for_data_plane_observe() {
        let intent = make_intent("search", Plane::Data);
        assert!(check_plane_violation(&intent).is_none());
    }

    #[test]
    fn violation_for_data_plane_modify() {
        let intent = make_intent("rename_thread", Plane::Data);
        assert!(check_plane_violation(&intent).is_some());
    }

    #[test]
    fn violation_for_data_plane_destruct() {
        let intent = make_intent("delete_thread", Plane::Data);
        let result = check_plane_violation(&intent);
        assert!(result.is_some());
        assert!(result.unwrap().contains("Data-plane"));
    }

    #[test]
    fn build_proposal_computes_level() {
        let intent = make_intent("search", Plane::Control);
        let proposal = build_proposal(&intent);
        assert_eq!(proposal.level, ActionLevel::Observe);
        assert_eq!(proposal.action, "search");
    }

    #[test]
    fn build_proposal_for_destruct() {
        let intent = make_intent("delete_thread", Plane::Control);
        let proposal = build_proposal(&intent);
        assert_eq!(proposal.level, ActionLevel::Destruct);
    }

    #[test]
    fn requires_confirmation_levels() {
        assert!(!requires_confirmation(ActionLevel::Observe));
        assert!(!requires_confirmation(ActionLevel::Annotate));
        assert!(requires_confirmation(ActionLevel::Modify));
        assert!(requires_confirmation(ActionLevel::Transmit));
        assert!(requires_confirmation(ActionLevel::Destruct));
    }

    #[test]
    fn build_proposal_description_includes_target() {
        let intent = make_intent("delete_thread", Plane::Control);
        let proposal = build_proposal(&intent);
        assert!(proposal.description.contains("test"));
    }

    #[test]
    fn build_proposal_description_no_target() {
        let mut intent = make_intent("search", Plane::Control);
        intent.target = None;
        let proposal = build_proposal(&intent);
        assert!(proposal.description.contains("?"));
    }

    #[test]
    fn data_plane_annotate_no_violation() {
        let intent = make_intent("annotate", Plane::Data);
        assert!(check_plane_violation(&intent).is_none());
    }

    #[test]
    fn data_plane_transmit_violation() {
        let intent = make_intent("export", Plane::Data);
        assert!(check_plane_violation(&intent).is_some());
    }
}
