use ropey::{Rope, RopeSlice};
use std::ops::Range;

use crate::buffer::undo::UndoTree;
use crate::types::{UndoNodeId, Version};

/// A rope-based text buffer with undo support.
pub struct Buffer {
    rope: Rope,
    undo_tree: UndoTree,
    version: Version,
    dirty: bool,
    clean_version: Version,
}

impl Buffer {
    pub fn from_text(text: &str) -> Self {
        let rope = Rope::from_str(text);
        let undo_tree = UndoTree::new(rope.clone());
        Buffer {
            rope,
            undo_tree,
            version: 0,
            dirty: false,
            clean_version: 0,
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

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        self.rope.insert(char_idx, text);
        self.bump_version();
        let desc = if text.len() <= 20 {
            format!("insert '{text}'")
        } else {
            format!("insert '{}...'", &text[..17])
        };
        self.undo_tree.push(self.rope.clone(), desc);
    }

    pub fn delete(&mut self, range: Range<usize>) {
        self.rope.remove(range);
        self.bump_version();
        self.undo_tree
            .push(self.rope.clone(), "delete".to_string());
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = range.start;
        self.rope.remove(range);
        self.rope.insert(start, text);
        self.bump_version();
        let desc = if text.len() <= 20 {
            format!("replace with '{text}'")
        } else {
            format!("replace with '{}...'", &text[..17])
        };
        self.undo_tree.push(self.rope.clone(), desc);
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

    pub fn restore_state(&mut self, node_id: UndoNodeId) {
        self.rope = self.undo_tree.restore(node_id);
        self.bump_version();
    }

    pub fn mark_clean(&mut self) {
        self.clean_version = self.version;
        self.dirty = false;
    }
}