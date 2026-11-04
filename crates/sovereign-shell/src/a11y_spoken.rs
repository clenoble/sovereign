//! Test helper: the name a screen reader on Windows receives for a node of a
//! tree the shell builds (v0.0.12 rc2, `doc/plans/V0012-rc2-labels-spoken.md`).
//!
//! Shared by the shell's tests that read what Narrator hears. Moved here from
//! `setup_step_announced_tests::Snap::spoken_name` (b4f78d1), unchanged in
//! substance, so every test reads names by the same rule.
//!
//! The rule is the one of the versions in `Cargo.lock`, read in their
//! sources, not assumed:
//! - `accesskit_windows` 0.32.1, `src/node.rs` l. 398–407 (`Node::name`):
//!   `if label_comes_from_value() { write_value } else { write_label }`;
//! - `accesskit_consumer` 0.35.0, `src/node.rs`:
//!   - l. 714–716, `label_comes_from_value() == (role == Label)`;
//!   - l. 732–751, `write_label`: the node's own label when it has one (even
//!     empty), else each `labelled_by` target joined by spaces, a target
//!     written by its **value** when its role is `Label`, by its own label
//!     otherwise;
//!   - l. 687–712, `labelled_by`: with no explicit `labelled_by`, a button,
//!     check box, default button, link, menu item (and its check box / radio
//!     forms) or radio button is labelled by its descendants that pass
//!     `descendant_label_filter` (l. 678–684): `Label` and `Image` count,
//!     `GenericContainer`s are looked through, any other subtree is skipped;
//!   - l. 798–808, `write_value`: the value property; a node with text runs
//!     would give its text, but the shell's `Label`s have none.
//!
//! So a node of role `Label` is spoken by its **value**, never its label,
//! both when it is focused and as the `labelled_by` target of another node.

use accesskit::{Node, NodeId, Role};

fn node(nodes: &[(NodeId, Node)], id: NodeId) -> Option<&Node> {
    nodes.iter().find(|(i, _)| *i == id).map(|(_, n)| n)
}

/// The name Narrator receives for node `id` of `nodes` (a `TreeUpdate`'s
/// nodes); empty when it has none or is not there.
pub(crate) fn spoken_name(nodes: &[(NodeId, Node)], id: NodeId) -> String {
    let Some(n) = node(nodes, id) else { return String::new() };
    if n.role() == Role::Label {
        return n.value().unwrap_or_default().to_string();
    }
    if let Some(l) = n.label() {
        return l.to_string();
    }
    let explicit: Vec<NodeId> = n.labelled_by().to_vec();
    let implicit = explicit.is_empty()
        && matches!(
            n.role(),
            Role::Button
                | Role::CheckBox
                | Role::DefaultButton
                | Role::Link
                | Role::MenuItem
                | Role::MenuItemCheckBox
                | Role::MenuItemRadio
                | Role::RadioButton
        );
    let targets = if implicit { label_descendants(nodes, id) } else { explicit };
    targets
        .into_iter()
        .filter_map(|t| {
            let tn = node(nodes, t)?;
            if tn.role() == Role::Label { tn.value() } else { tn.label() }.map(str::to_string)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `descendant_label_filter`: `Label` and `Image` children count,
/// `GenericContainer`s are looked through, any other subtree is skipped.
fn label_descendants(nodes: &[(NodeId, Node)], id: NodeId) -> Vec<NodeId> {
    let mut out = Vec::new();
    for c in node(nodes, id).map(|n| n.children().to_vec()).unwrap_or_default() {
        match node(nodes, c).map(|n| n.role()) {
            Some(Role::Label | Role::Image) => out.push(c),
            Some(Role::GenericContainer) => out.extend(label_descendants(nodes, c)),
            _ => {}
        }
    }
    out
}
