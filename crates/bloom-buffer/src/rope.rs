use ropey::{Rope, RopeSlice};
use std::ops::Range;

use crate::undo::UndoTree;
use crate::{Cursor, UndoNodeId, Version};

/// A rope-based text buffer with cursor tracking and branching undo/redo.
///
/// **The buffer owns cursors.** All mutations (insert, delete, replace)
/// automatically adjust every tracked cursor. External code accesses
/// cursors via `cursor(idx)` / `set_cursor(idx, pos)` which enforce bounds.
pub struct Buffer {
    rope: Rope,
    undo_tree: UndoTree,
    cursors: Vec<Cursor>,
    version: Version,
    dirty: bool,
    clean_version: Version,
    edit_group_checkpoint: Option<Rope>,
}

impl Buffer {
    /// Create a buffer from a string with one cursor at position 0.
    pub fn from_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let undo_tree = UndoTree::new(rope.clone());
        Buffer {
            rope,
            undo_tree,
            cursors: vec![Cursor::new(0)],
            version: 0,
            dirty: false,
            clean_version: 0,
            edit_group_checkpoint: None,
        }
    }

    // -- Accessors --

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

    // -- Cursor management --

    /// Get the position of cursor at index `idx`.
    pub fn cursor(&self, idx: usize) -> usize {
        self.cursors.get(idx).map(|c| c.position).unwrap_or(0)
    }

    /// Get the full cursor state at index `idx`.
    pub fn cursor_state(&self, idx: usize) -> Option<&Cursor> {
        self.cursors.get(idx)
    }

    /// Set cursor position, clamped to valid bounds.
    pub fn set_cursor(&mut self, idx: usize, pos: usize) {
        let max = self.rope.len_chars();
        let clamped = pos.min(max);
        if idx < self.cursors.len() {
            self.cursors[idx].position = clamped;
        }
    }

    /// Set the anchor for selections (Visual mode).
    pub fn set_anchor(&mut self, idx: usize, anchor: Option<usize>) {
        if idx < self.cursors.len() {
            self.cursors[idx].anchor = anchor;
        }
    }

    /// Ensure at least `count` cursors exist.
    pub fn ensure_cursors(&mut self, count: usize) {
        while self.cursors.len() < count {
            self.cursors.push(Cursor::new(0));
        }
    }

    // -- Mutations (auto-adjust cursors) --

    fn bump_version(&mut self) {
        self.version += 1;
        self.dirty = self.version != self.clean_version;
    }

    /// Adjust all cursors after an insertion at `at` of `len` chars.
    fn adjust_cursors_after_insert(&mut self, at: usize, len: usize) {
        for c in &mut self.cursors {
            if c.position >= at {
                c.position += len;
            }
            if let Some(ref mut a) = c.anchor {
                if *a >= at {
                    *a += len;
                }
            }
        }
    }

    /// Adjust all cursors after a deletion of `range`.
    fn adjust_cursors_after_delete(&mut self, range: &Range<usize>) {
        let removed = range.end - range.start;
        for c in &mut self.cursors {
            if c.position >= range.end {
                c.position -= removed;
            } else if c.position > range.start {
                c.position = range.start;
            }
            if let Some(ref mut a) = c.anchor {
                if *a >= range.end {
                    *a -= removed;
                } else if *a > range.start {
                    *a = range.start;
                }
            }
        }
    }

    /// Insert `text` at the given character index.
    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let len = text.chars().count();
        self.rope.insert(char_idx, text);
        self.adjust_cursors_after_insert(char_idx, len);
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

    /// Delete the character range.
    pub fn delete(&mut self, range: Range<usize>) {
        self.adjust_cursors_after_delete(&range);
        self.rope.remove(range);
        self.bump_version();
        if self.edit_group_checkpoint.is_none() {
            self.undo_tree.push(self.rope.clone(), "delete".to_string());
        }
    }

    /// Replace the character range with `text`.
    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let insert_len = text.chars().count();
        self.adjust_cursors_after_delete(&range);
        let start = range.start;
        self.rope.remove(range);
        self.rope.insert(start, text);
        self.adjust_cursors_after_insert(start, insert_len);
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

    // -- Undo/Redo --

    pub fn undo(&mut self) -> bool {
        if let Some(snapshot) = self.undo_tree.undo() {
            self.rope = snapshot;
            self.bump_version();
            true
        } else {
            false
        }
    }

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

    pub fn set_undo_tree(&mut self, tree: UndoTree) {
        self.undo_tree = tree;
    }

    pub fn restore_state(&mut self, node_id: UndoNodeId) {
        self.rope = self.undo_tree.restore(node_id);
        self.bump_version();
    }

    pub fn mark_clean(&mut self) {
        self.clean_version = self.version;
        self.dirty = false;
    }

    pub fn begin_edit_group(&mut self) {
        self.edit_group_checkpoint = Some(self.rope.clone());
    }

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

    #[test]
    fn from_text_basic() {
        let buf = Buffer::from_text("hello");
        assert_eq!(buf.len_chars(), 5);
        assert!(!buf.is_dirty());
    }

    #[test]
    fn insert_adjusts_cursor() {
        let mut buf = Buffer::from_text("hello world");
        buf.set_cursor(0, 6); // cursor at 'w'
        buf.insert(0, "say "); // insert before cursor
        assert_eq!(buf.cursor(0), 10); // cursor shifted by 4
        assert_eq!(buf.text().to_string(), "say hello world");
    }

    #[test]
    fn insert_after_cursor_no_shift() {
        let mut buf = Buffer::from_text("hello world");
        buf.set_cursor(0, 3); // cursor at second 'l'
        buf.insert(6, "beautiful "); // insert after cursor
        assert_eq!(buf.cursor(0), 3); // cursor unchanged
    }

    #[test]
    fn delete_adjusts_cursor_after_range() {
        let mut buf = Buffer::from_text("hello world");
        buf.set_cursor(0, 8); // cursor at 'r'
        buf.delete(0..6); // delete "hello "
        assert_eq!(buf.cursor(0), 2); // shifted left by 6
        assert_eq!(buf.text().to_string(), "world");
    }

    #[test]
    fn delete_cursor_inside_range_collapses() {
        let mut buf = Buffer::from_text("hello world");
        buf.set_cursor(0, 3); // cursor at second 'l' (inside "hello")
        buf.delete(0..5); // delete "hello"
        assert_eq!(buf.cursor(0), 0); // collapsed to range.start
    }

    #[test]
    fn replace_adjusts_cursor() {
        let mut buf = Buffer::from_text("hello world");
        buf.set_cursor(0, 8); // cursor at 'r'
        buf.replace(6..11, "rust"); // "world" → "rust" (5 → 4 chars)
        assert_eq!(buf.text().to_string(), "hello rust");
        // cursor was after the replaced range, adjusted for size change
        assert_eq!(buf.cursor(0), 10); // end of "rust"
    }

    #[test]
    fn set_cursor_clamps_to_bounds() {
        let mut buf = Buffer::from_text("hi");
        buf.set_cursor(0, 999);
        assert_eq!(buf.cursor(0), 2); // clamped to len_chars
    }

    #[test]
    fn multiple_cursors() {
        let mut buf = Buffer::from_text("abcdef");
        buf.ensure_cursors(3);
        buf.set_cursor(0, 1); // 'b'
        buf.set_cursor(1, 3); // 'd'
        buf.set_cursor(2, 5); // 'f'
        buf.insert(2, "XX"); // insert at 'c'
        assert_eq!(buf.cursor(0), 1); // before insert — unchanged
        assert_eq!(buf.cursor(1), 5); // after insert — shifted by 2
        assert_eq!(buf.cursor(2), 7); // after insert — shifted by 2
    }

    #[test]
    fn undo_redo() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert_eq!(buf.text().to_string(), "hello world");
        buf.undo();
        assert_eq!(buf.text().to_string(), "hello");
        buf.redo();
        assert_eq!(buf.text().to_string(), "hello world");
    }

    #[test]
    fn dirty_tracking() {
        let mut buf = Buffer::from_text("hello");
        assert!(!buf.is_dirty());
        buf.insert(5, "!");
        assert!(buf.is_dirty());
        buf.mark_clean();
        assert!(!buf.is_dirty());
    }
}
