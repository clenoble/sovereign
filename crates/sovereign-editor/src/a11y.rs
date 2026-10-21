//! The editor's AccessKit tree (guarantee 10): a container holding one text
//! input whose text runs are the document's units, with the caret and the
//! selection as an AccessKit text selection. A password field exposes bullets
//! only.

use accesskit::{Action, Node, NodeId, Role, TextPosition, TextSelection, Tree, TreeId, TreeUpdate};
use unicode_segmentation::UnicodeSegmentation;

pub(crate) const ROOT_ID: NodeId = NodeId(1);
pub(crate) const TEXT_ID: NodeId = NodeId(2);
/// The run of an empty document.
pub(crate) const EMPTY_RUN_ID: NodeId = NodeId(3);
/// A document editor's title field (title contract T6), and its one run.
pub(crate) const TITLE_ID: NodeId = NodeId(4);
pub(crate) const TITLE_RUN_ID: NodeId = NodeId(5);
/// What the title field's empty value is announced as (T1).
pub(crate) const TITLE_PLACEHOLDER: &str = "Untitled";
/// Run ids are allocated from here on.
pub(crate) const RUN_BASE: u64 = 16;

/// One text run: the displayed text of a unit (and the line break after it).
pub(crate) struct Run {
    pub text: String,
    /// Length in bytes of each character (grapheme cluster; split when longer
    /// than 255 bytes).
    pub lens: Vec<u8>,
}

impl Run {
    pub(crate) fn new(text: String) -> Run {
        let mut lens = Vec::new();
        for g in text.graphemes(true) {
            if g.len() <= 255 {
                lens.push(g.len() as u8);
            } else {
                let mut acc = 0usize;
                for c in g.chars() {
                    if acc + c.len_utf8() > 255 {
                        lens.push(acc as u8);
                        acc = 0;
                    }
                    acc += c.len_utf8();
                }
                if acc > 0 {
                    lens.push(acc as u8);
                }
            }
        }
        Run { text, lens }
    }

    /// The character index of the boundary at or before byte `d`.
    pub(crate) fn char_index(&self, d: usize) -> usize {
        let mut at = 0usize;
        for (i, &l) in self.lens.iter().enumerate() {
            if at + l as usize > d {
                return i;
            }
            at += l as usize;
        }
        self.lens.len()
    }
}

/// The title field's nodes, when they are sent.
pub(crate) struct TitleNode<'a> {
    /// The title as shown (tokens as their labels).
    pub run: &'a Run,
    /// A field (editable) or, read-only, text (T7).
    pub editable: bool,
    /// (anchor, focus) characters into `run`.
    pub selection: (usize, usize),
}

pub(crate) struct TreeInput<'a> {
    pub tree_id: TreeId,
    pub role: Role,
    pub label: Option<&'a str>,
    pub read_only: bool,
    pub focused: bool,
    /// The text input's children, in order.
    pub children: Vec<NodeId>,
    /// Runs to send (all of them for a full tree; new ones for an update).
    pub runs: Vec<(NodeId, &'a Run)>,
    /// (run node, character) of the anchor and of the focus.
    pub selection: Option<((NodeId, usize), (NodeId, usize))>,
    /// A full tree (initial request) rather than an update.
    pub full: bool,
    /// The editor has a title field (a document editor).
    pub has_title: bool,
    /// The title's nodes to send (always in a full tree).
    pub title: Option<TitleNode<'a>>,
    /// The title field holds the caret.
    pub title_focused: bool,
}

pub(crate) fn build(input: TreeInput<'_>) -> TreeUpdate {
    let mut nodes = Vec::with_capacity(input.runs.len() + 2);
    if input.full {
        let mut root = Node::new(Role::GenericContainer);
        root.set_children(if input.has_title { vec![TITLE_ID, TEXT_ID] } else { vec![TEXT_ID] });
        nodes.push((ROOT_ID, root));
    }
    if let Some(t) = &input.title {
        let shown = t.run.text.as_str();
        let mut n = if t.editable {
            let mut n = Node::new(Role::TextInput);
            n.set_label("Title");
            n.set_value(shown);
            if shown.is_empty() {
                n.set_placeholder(TITLE_PLACEHOLDER);
            }
            n.add_action(Action::Focus);
            let (a, f) = t.selection;
            n.set_text_selection(TextSelection {
                anchor: TextPosition { node: TITLE_RUN_ID, character_index: a },
                focus: TextPosition { node: TITLE_RUN_ID, character_index: f },
            });
            n
        } else {
            // Read-only: the title is text, not a field (T7).
            let mut n = Node::new(Role::Heading);
            n.set_label(if shown.is_empty() { TITLE_PLACEHOLDER } else { shown });
            n
        };
        n.set_children(vec![TITLE_RUN_ID]);
        nodes.push((TITLE_ID, n));
        let mut r = Node::new(Role::TextRun);
        r.set_value(shown);
        r.set_character_lengths(t.run.lens.clone());
        nodes.push((TITLE_RUN_ID, r));
    }
    let mut text = Node::new(input.role);
    if let Some(l) = input.label {
        text.set_label(l);
    }
    if input.read_only {
        text.set_read_only();
    }
    text.add_action(Action::Focus);
    text.set_children(input.children);
    if let Some(((an, ac), (fnode, fc))) = input.selection {
        text.set_text_selection(TextSelection {
            anchor: TextPosition { node: an, character_index: ac },
            focus: TextPosition { node: fnode, character_index: fc },
        });
    }
    nodes.push((TEXT_ID, text));
    for (id, r) in input.runs {
        let mut n = Node::new(Role::TextRun);
        n.set_value(r.text.clone());
        n.set_character_lengths(r.lens.clone());
        nodes.push((id, n));
    }
    TreeUpdate {
        nodes,
        tree: input.full.then(|| Tree::new(ROOT_ID)),
        tree_id: input.tree_id,
        focus: match (input.focused, input.title_focused) {
            (true, true) => TITLE_ID,
            (true, false) => TEXT_ID,
            (false, _) => ROOT_ID,
        },
    }
}
