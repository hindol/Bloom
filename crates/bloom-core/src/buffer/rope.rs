use ropey::{Rope, RopeSlice};
use std::ops::Range;

use crate::buffer::undo::UndoTree;
use crate::types::{UndoNodeId, Version};

/// A rope-based text buffer with branching undo/redo.
///
/// Wraps a [`ropey::Rope`] with version tracking, dirty-state management,
/// and an [`UndoTree`] that supports branching history. Edit groups collapse
/// multiple edits (e.g. an insert-mode session) into a single undo node.
pub struct Buffer {
    rope: Rope,
    undo_tree: UndoTree,
    version: Version,
    dirty: bool,
    clean_version: Version,
    /// Rope snapshot taken when an edit group (insert session) began.
    /// While Some, individual edits do not push undo nodes.
    edit_group_checkpoint: Option<Rope>,
}

impl Buffer {
    /// Create a buffer from a string, initializing the undo tree root.
    pub fn from_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let undo_tree = UndoTree::new(rope.clone());
        Buffer {
            rope,
            undo_tree,
            version: 0,
            dirty: false,
            clean_version: 0,
            edit_group_checkpoint: None,
        }
    }

    pub fn text(&self) -> &Rope {
        &self.rope
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, idx: usize) -> RopeSlice<'_> {
        self.rope.line(idx)
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    fn bump_version(&mut self) {
        self.version += 1;
        self.dirty = self.version != self.clean_version;
    }

    /// Insert `text` at the given character index, pushing an undo node
    /// (unless inside an edit group).
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
        self.bump_version();
        if self.edit_group_checkpoint.is_none() {
            let desc = if text.len() <= 20 {
                format!("insert '{text}'")
            } else {
                format!("insert '{}...'", &text[..17])
            };
            self.undo_tree.push(self.rope.clone(), desc);
        }
    }

    /// Delete the character range, pushing an undo node.
    pub fn delete(&mut self, range: Range<usize>) {
        self.rope.remove(range);
        self.bump_version();
        if self.edit_group_checkpoint.is_none() {
            self.undo_tree.push(self.rope.clone(), "delete".to_string());
        }
    }

    /// Replace the character range with `text`, pushing an undo node.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = range.start;
        self.rope.remove(range);
        self.rope.insert(start, text);
        self.bump_version();
        if self.edit_group_checkpoint.is_none() {
            let desc = if text.len() <= 20 {
                format!("replace with '{text}'")
            } else {
                format!("replace with '{}...'", &text[..17])
            };
            self.undo_tree.push(self.rope.clone(), desc);
        }
    }

    pub fn find_text(&self, needle: &str) -> Vec<Range<usize>> {
        if needle.is_empty() {
            return Vec::new();
        }
        let mut results = Vec::new();
        let text = self.rope.to_string();
        let mut start = 0;
        while let Some(pos) = text[start..].find(needle) {
            let char_start = self.rope.byte_to_char(start + pos);
            let char_end = self.rope.byte_to_char(start + pos + needle.len());
            results.push(char_start..char_end);
            start += pos + needle.len();
        }
        results
    }

    /// Undo the last edit. Returns `true` if the tree moved, `false` at root.
    pub fn undo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_tree.undo() {
            self.rope = snapshot;
            self.bump_version();
            true
        } else {
            false
        }
    }

    /// Redo a previously undone edit. Returns `true` if the tree moved.
    pub fn redo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_tree.redo() {
            self.rope = snapshot;
            self.bump_version();
            true
        } else {
            false
        }
    }

    pub fn undo_tree(&self) -> &UndoTree {
        &self.undo_tree
    }

    /// Replace the undo tree (used when restoring from persistent storage).
    pub fn set_undo_tree(&mut self, tree: UndoTree) {
        self.undo_tree = tree;
    }

    pub fn restore_state(&mut self, node_id: UndoNodeId) {
        self.rope = self.undo_tree.restore(node_id);
        self.bump_version();
    }

    /// Mark the buffer as clean (matches on-disk state). Resets the dirty flag.
    pub fn mark_clean(&mut self) {
        self.clean_version = self.version;
        self.dirty = false;
    }

    /// Begin an edit group (e.g., entering Insert mode). Saves a checkpoint.
    /// All edits until `end_edit_group` are grouped into one undo node.
    pub fn begin_edit_group(&mut self) {
        self.edit_group_checkpoint = Some(self.rope.clone());
    }

    /// End the edit group (e.g., leaving Insert mode). If edits were made,
    /// pushes one undo node for the entire group.
    pub fn end_edit_group(&mut self) {
        if let Some(checkpoint) = self.edit_group_checkpoint.take() {
            if self.rope != checkpoint {
                self.undo_tree
                    .push(self.rope.clone(), "insert session".to_string());
            }
        }
    }

    /// Restore the edit group checkpoint (Ctrl+U in Insert mode).
    /// Reverts to the state when the edit group began, but keeps the group open.
    /// Returns true if restored, false if no group is active.
    pub fn restore_edit_group_checkpoint(&mut self) -> bool {
        if let Some(checkpoint) = &self.edit_group_checkpoint {
            self.rope = checkpoint.clone();
            self.bump_version();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // UC-14: Basic editing
    #[test]
    fn test_from_text_and_read_back() {
        let buf = Buffer::from_text("hello world");
        assert_eq!(buf.len_chars(), 11);
        assert_eq!(buf.len_lines(), 1);
        assert!(!buf.is_dirty());
        assert_eq!(buf.version(), 0);
    }

    #[test]
    fn test_insert_makes_dirty() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert!(buf.is_dirty());
        assert_eq!(buf.text().to_string(), "hello world");
        assert!(buf.version() > 0);
    }

    #[test]
    fn test_delete_range() {
        let mut buf = Buffer::from_text("hello world");
        buf.delete(5..11);
        assert_eq!(buf.text().to_string(), "hello");
    }

    #[test]
    fn test_replace_range() {
        let mut buf = Buffer::from_text("hello world");
        buf.replace(6..11, "rust");
        assert_eq!(buf.text().to_string(), "hello rust");
    }

    // UC-18: Undo and redo
    #[test]
    fn test_undo_reverses_insert() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert_eq!(buf.text().to_string(), "hello world");
        assert!(buf.undo());
        assert_eq!(buf.text().to_string(), "hello");
    }

    #[test]
    fn test_redo_after_undo() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        buf.undo();
        assert!(buf.redo());
        assert_eq!(buf.text().to_string(), "hello world");
    }

    #[test]
    fn test_undo_at_root_returns_false() {
        let mut buf = Buffer::from_text("hello");
        assert!(!buf.undo());
    }

    #[test]
    fn test_redo_at_tip_returns_false() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert!(!buf.redo());
    }

    // UC-18 step 5: Branching undo tree
    #[test]
    fn test_undo_tree_branches() {
        let mut buf = Buffer::from_text("");
        buf.insert(0, "alpha"); // node 1
        buf.insert(5, " beta"); // node 2
        buf.insert(10, " gamma"); // node 3
        buf.undo(); // back to node 2: "alpha beta"
        buf.undo(); // back to node 1: "alpha"
        buf.insert(5, " delta"); // NEW branch: node 4
        assert_eq!(buf.text().to_string(), "alpha delta");

        // The undo tree should have branches
        let tree = buf.undo_tree();
        // Node 1 (id=1) is the child of root; it should have 2 children (node 2 and node 4)
        let node1_children = tree.children(1);
        assert!(node1_children.len() >= 2, "expected branching undo tree");
    }

    // UC-86: mark_clean
    #[test]
    fn test_mark_clean_resets_dirty() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert!(buf.is_dirty());
        buf.mark_clean();
        assert!(!buf.is_dirty());
    }

    #[test]
    fn test_dirty_after_clean_then_edit() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        buf.mark_clean();
        buf.insert(11, "!");
        assert!(buf.is_dirty());
    }

    // UC-68: find_text for MCP search-and-replace
    #[test]
    fn test_find_text_single_match() {
        let buf = Buffer::from_text("hello world hello");
        let matches = buf.find_text("world");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], 6..11);
    }

    #[test]
    fn test_find_text_multiple_matches() {
        let buf = Buffer::from_text("hello world hello");
        let matches = buf.find_text("hello");
        assert_eq!(matches.len(), 2);
    }

    #[test]
    fn test_find_text_no_match() {
        let buf = Buffer::from_text("hello world");
        let matches = buf.find_text("xyz");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_line_access() {
        let buf = Buffer::from_text("line one\nline two\nline three");
        assert_eq!(buf.len_lines(), 3);
        assert_eq!(buf.line(0).to_string(), "line one\n");
        assert_eq!(buf.line(1).to_string(), "line two\n");
    }

    // UC-19: restore_state navigates undo tree
    #[test]
    fn test_restore_state() {
        let mut buf = Buffer::from_text("");
        buf.insert(0, "first"); // node 1
        let node_after_first = buf.undo_tree().current();
        buf.insert(5, " second"); // node 2
        assert_eq!(buf.text().to_string(), "first second");
        buf.restore_state(node_after_first);
        assert_eq!(buf.text().to_string(), "first");
    }
}
