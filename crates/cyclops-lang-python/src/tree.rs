//! Small helpers for reading a tree-sitter tree.

use tree_sitter::Node;

/// A byte offset as the rest of Cyclops counts it.
pub(crate) fn offset(position: usize) -> u32 {
    u32::try_from(position).unwrap_or(u32::MAX)
}

pub(crate) fn start(node: Node<'_>) -> u32 {
    offset(node.start_byte())
}

pub(crate) fn end(node: Node<'_>) -> u32 {
    offset(node.end_byte())
}

/// The text of a node.
pub(crate) fn text<'t>(node: Node<'_>, source: &'t str) -> &'t str {
    source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or_default()
}

pub(crate) fn named_children(node: Node<'_>) -> Vec<Node<'_>> {
    let mut cursor = node.walk();
    node.named_children(&mut cursor).collect()
}

/// Visits a node and everything inside it, in the order of the code, without
/// recursing.
pub(crate) fn each_node<'n>(root: Node<'n>, mut visit: impl FnMut(Node<'n>)) {
    let mut cursor = root.walk();
    loop {
        visit(cursor.node());
        if cursor.goto_first_child() {
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
        }
    }
}
