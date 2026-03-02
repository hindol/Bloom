use ropey::Rope;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::time::Instant;

// ── Undo tree types ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Edit {
    Insert { pos: usize, text: String },
    Delete { pos: usize, text: String },
}

#[derive(Debug)]
struct UndoNode {
    edit: Edit,
    parent: Option<usize>,
    children: Vec<usize>,
    timestamp: Instant,
}

#[derive(Debug)]
struct UndoTree {
    nodes: Vec<UndoNode>,
    current: usize, // index of current node (0 = sentinel root)
}

impl UndoTree {
    fn new() -> Self {
        // Sentinel root node – never undone past this point.
        let root = UndoNode {
            edit: Edit::Insert {
                pos: 0,
                text: String::new(),
            },
            parent: None,
            children: Vec::new(),
            timestamp: Instant::now(),
        };
        UndoTree {
            nodes: vec![root],
            current: 0,
        }
    }

    fn record(&mut self, edit: Edit) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(UndoNode {
            edit,
            parent: Some(self.current),
            children: Vec::new(),
            timestamp: Instant::now(),
        });
        self.nodes[self.current].children.push(idx);
        self.current = idx;
        idx
    }

    /// Move to parent. Returns the edit to reverse, or None if at root.
    fn undo(&mut self) -> Option<&Edit> {
        if self.current == 0 {
            return None;
        }
        let parent = self.nodes[self.current].parent.unwrap();
        let cur = self.current;
        self.current = parent;
        Some(&self.nodes[cur].edit)
    }

    /// Move to the last child. Returns the edit to apply, or None if leaf.
    fn redo(&mut self) -> Option<&Edit> {
        let children = &self.nodes[self.current].children;
        if children.is_empty() {
            return None;
        }
        let child = *children.last().unwrap();
        self.current = child;
        Some(&self.nodes[child].edit)
    }

    /// List children indices of the current node's parent (sibling branches).
    fn branches(&self) -> Vec<usize> {
        match self.nodes[self.current].parent {
            Some(parent) => self.nodes[parent].children.clone(),
            None => vec![self.current],
        }
    }

    fn switch_branch(&mut self, branch_idx: usize) {
        if let Some(parent) = self.nodes[self.current].parent {
            let children = &self.nodes[parent].children;
            if branch_idx < children.len() {
                self.current = children[branch_idx];
            }
        }
    }
}

// ── Buffer ──────────────────────────────────────────────────────────

pub struct Buffer {
    pub rope: Rope,
    pub file_path: Option<PathBuf>,
    pub dirty: bool,
    undo_tree: UndoTree,
}

impl std::fmt::Debug for Buffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Buffer")
            .field("len", &self.rope.len_chars())
            .field("file_path", &self.file_path)
            .field("dirty", &self.dirty)
            .finish()
    }
}

impl Buffer {
    pub fn new() -> Self {
        Buffer {
            rope: Rope::new(),
            file_path: None,
            dirty: false,
            undo_tree: UndoTree::new(),
        }
    }

    pub fn from_str(s: &str) -> Self {
        Buffer {
            rope: Rope::from_str(s),
            file_path: None,
            dirty: false,
            undo_tree: UndoTree::new(),
        }
    }

    pub fn from_file(path: &Path) -> io::Result<Self> {
        let content = fs::read_to_string(path)?;
        Ok(Buffer {
            rope: Rope::from_str(&content),
            file_path: Some(path.to_path_buf()),
            dirty: false,
            undo_tree: UndoTree::new(),
        })
    }

    // ── Editing ──────────────────────────────────────────────────

    pub fn insert(&mut self, pos: usize, text: &str) {
        let char_pos = self.rope.byte_to_char(pos);
        self.rope.insert(char_pos, text);
        self.undo_tree.record(Edit::Insert {
            pos,
            text: text.to_string(),
        });
        self.dirty = true;
    }

    pub fn delete(&mut self, range: Range<usize>) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        let deleted: String = self.rope.slice(start..end).into();
        self.rope.remove(start..end);
        self.undo_tree.record(Edit::Delete {
            pos: range.start,
            text: deleted,
        });
        self.dirty = true;
    }

    pub fn replace(&mut self, range: Range<usize>, text: &str) {
        let start = self.rope.byte_to_char(range.start);
        let end = self.rope.byte_to_char(range.end);
        let deleted: String = self.rope.slice(start..end).into();
        self.rope.remove(start..end);
        self.rope.insert(start, text);
        // Record delete then insert, but we want a single undo entry.
        // We model replace as Delete(old) + Insert(new) recorded atomically.
        self.undo_tree.record(Edit::Delete {
            pos: range.start,
            text: deleted,
        });
        self.undo_tree.record(Edit::Insert {
            pos: range.start,
            text: text.to_string(),
        });
        self.dirty = true;
    }

    // ── Queries ──────────────────────────────────────────────────

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn line(&self, n: usize) -> Option<String> {
        if n >= self.rope.len_lines() {
            return None;
        }
        Some(self.rope.line(n).to_string())
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn byte_to_line(&self, byte: usize) -> usize {
        let char_idx = self.rope.byte_to_char(byte);
        self.rope.char_to_line(char_idx)
    }

    pub fn line_to_byte(&self, line: usize) -> usize {
        let char_idx = self.rope.line_to_char(line);
        self.rope.char_to_byte(char_idx)
    }

    // ── Undo / redo ─────────────────────────────────────────────

    pub fn undo(&mut self) -> bool {
        // For replace we recorded two edits; undo both.
        self._undo_one()
    }

    fn _undo_one(&mut self) -> bool {
        let edit = match self.undo_tree.undo() {
            Some(e) => e.clone(),
            None => return false,
        };
        self.apply_inverse(&edit);
        self.dirty = true;
        true
    }

    pub fn redo(&mut self) -> bool {
        self._redo_one()
    }

    fn _redo_one(&mut self) -> bool {
        let edit = match self.undo_tree.redo() {
            Some(e) => e.clone(),
            None => return false,
        };
        self.apply_edit(&edit);
        self.dirty = true;
        true
    }

    pub fn undo_branches(&self) -> Vec<usize> {
        self.undo_tree.branches()
    }

    /// Node index of the current undo position.
    pub fn undo_current_node(&self) -> usize {
        self.undo_tree.current
    }

    pub fn switch_branch(&mut self, branch_idx: usize) {
        // First undo current to get back to parent state.
        let cur = self.undo_tree.current;
        if let Some(parent) = self.undo_tree.nodes[cur].parent {
            // Reverse current edit to reach parent state.
            let edit = self.undo_tree.nodes[cur].edit.clone();
            self.apply_inverse(&edit);
            self.undo_tree.current = parent;

            // Now switch to the requested branch.
            let children = &self.undo_tree.nodes[parent].children;
            if branch_idx < children.len() {
                let target = children[branch_idx];
                let target_edit = self.undo_tree.nodes[target].edit.clone();
                self.apply_edit(&target_edit);
                self.undo_tree.current = target;
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────────

    fn apply_edit(&mut self, edit: &Edit) {
        match edit {
            Edit::Insert { pos, text } => {
                let char_pos = self.rope.byte_to_char(*pos);
                self.rope.insert(char_pos, text);
            }
            Edit::Delete { pos, text } => {
                let char_pos = self.rope.byte_to_char(*pos);
                let end = self.rope.byte_to_char(*pos + text.len());
                self.rope.remove(char_pos..end);
            }
        }
    }

    fn apply_inverse(&mut self, edit: &Edit) {
        match edit {
            Edit::Insert { pos, text } => {
                let char_pos = self.rope.byte_to_char(*pos);
                let end = self.rope.byte_to_char(*pos + text.len());
                self.rope.remove(char_pos..end);
            }
            Edit::Delete { pos, text } => {
                let char_pos = self.rope.byte_to_char(*pos);
                self.rope.insert(char_pos, text);
            }
        }
    }
}

impl Default for Buffer {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn insert_and_read_back() {
        let mut buf = Buffer::new();
        buf.insert(0, "hello");
        assert_eq!(buf.text(), "hello");
        buf.insert(5, " world");
        assert_eq!(buf.text(), "hello world");
        assert!(buf.dirty);
    }

    #[test]
    fn delete_and_read_back() {
        let mut buf = Buffer::from_str("hello world");
        buf.delete(5..11);
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn replace_text() {
        let mut buf = Buffer::from_str("hello world");
        buf.replace(6..11, "rust");
        assert_eq!(buf.text(), "hello rust");
    }

    #[test]
    fn undo_redo_linear() {
        let mut buf = Buffer::new();
        buf.insert(0, "aaa");
        buf.insert(3, "bbb");
        assert_eq!(buf.text(), "aaabbb");

        assert!(buf.undo());
        assert_eq!(buf.text(), "aaa");

        assert!(buf.undo());
        assert_eq!(buf.text(), "");

        // At root, undo returns false.
        assert!(!buf.undo());

        assert!(buf.redo());
        assert_eq!(buf.text(), "aaa");

        assert!(buf.redo());
        assert_eq!(buf.text(), "aaabbb");

        // At leaf, redo returns false.
        assert!(!buf.redo());
    }

    #[test]
    fn undo_branch_new_edit() {
        let mut buf = Buffer::new();
        buf.insert(0, "aaa");
        buf.insert(3, "bbb");
        assert_eq!(buf.text(), "aaabbb");

        // Undo back to "aaa"
        buf.undo();
        assert_eq!(buf.text(), "aaa");

        // New edit creates a branch
        buf.insert(3, "ccc");
        assert_eq!(buf.text(), "aaaccc");

        // Parent of current node should have 2 children (branches).
        let branches = buf.undo_branches();
        assert_eq!(branches.len(), 2);
    }

    #[test]
    fn switch_between_branches() {
        let mut buf = Buffer::new();
        buf.insert(0, "aaa");
        buf.insert(3, "bbb"); // branch 0
        assert_eq!(buf.text(), "aaabbb");

        buf.undo(); // back to "aaa"
        buf.insert(3, "ccc"); // branch 1
        assert_eq!(buf.text(), "aaaccc");

        // Switch to branch 0 (the "bbb" branch)
        buf.switch_branch(0);
        assert_eq!(buf.text(), "aaabbb");

        // Switch back to branch 1 (the "ccc" branch)
        buf.switch_branch(1);
        assert_eq!(buf.text(), "aaaccc");
    }

    #[test]
    fn from_str_works() {
        let buf = Buffer::from_str("line1\nline2\nline3\n");
        assert_eq!(buf.line_count(), 4); // trailing newline creates empty last line
        assert_eq!(buf.line(0).unwrap(), "line1\n");
        assert_eq!(buf.line(1).unwrap(), "line2\n");
        assert!(!buf.dirty);
    }

    #[test]
    fn from_file_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        {
            let mut f = fs::File::create(&path).unwrap();
            f.write_all(b"file content").unwrap();
        }
        let buf = Buffer::from_file(&path).unwrap();
        assert_eq!(buf.text(), "file content");
        assert_eq!(buf.file_path.as_deref(), Some(path.as_path()));
        assert!(!buf.dirty);
    }

    #[test]
    fn byte_to_line_and_line_to_byte() {
        let buf = Buffer::from_str("aaa\nbbb\nccc\n");
        assert_eq!(buf.byte_to_line(0), 0);
        assert_eq!(buf.byte_to_line(4), 1);
        assert_eq!(buf.byte_to_line(8), 2);
        assert_eq!(buf.line_to_byte(0), 0);
        assert_eq!(buf.line_to_byte(1), 4);
        assert_eq!(buf.line_to_byte(2), 8);
    }

    #[test]
    fn undo_delete() {
        let mut buf = Buffer::from_str("hello world");
        buf.delete(5..11);
        assert_eq!(buf.text(), "hello");
        buf.undo();
        assert_eq!(buf.text(), "hello world");
    }

    #[test]
    fn line_out_of_bounds() {
        let buf = Buffer::from_str("one\ntwo\n");
        assert!(buf.line(10).is_none());
    }
}
