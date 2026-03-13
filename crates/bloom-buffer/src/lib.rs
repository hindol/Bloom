//! bloom-buffer: Rope-based text buffer with cursor tracking and undo tree.
//!
//! The buffer owns cursors — all mutations (insert, delete, replace)
//! automatically adjust cursor positions. External code never sets raw
//! cursor offsets; it goes through `Buffer::set_cursor()` which enforces
//! bounds.

pub mod block_id;
pub mod edit;
pub mod rope;
pub mod undo;

pub use edit::EditOp;
pub use rope::Buffer;
pub use undo::{UndoNodeInfo, UndoTree};

/// Read-only buffer trait — the base interface for both mutable and immutable buffers.
///
/// Provides access to text content, line iteration, cursor state, and metadata.
/// `Buffer` implements this (with full rope + undo), and `StaticBuffer` implements
/// it as a lightweight read-only container for views, logs, and other non-editable content.
pub trait ReadBuffer {
    fn text_string(&self) -> String;
    fn len_chars(&self) -> usize;
    fn len_lines(&self) -> usize;
    fn line_text(&self, idx: usize) -> String;
    fn is_dirty(&self) -> bool;
    fn is_read_only(&self) -> bool;
}

/// A lightweight read-only buffer backed by a simple string.
/// Used for view results, log viewer, and other non-editable content.
pub struct StaticBuffer {
    content: String,
    lines: Vec<String>,
}

impl StaticBuffer {
    pub fn new(content: &str) -> Self {
        let lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        Self {
            content: content.to_string(),
            lines,
        }
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

impl ReadBuffer for StaticBuffer {
    fn text_string(&self) -> String {
        self.content.clone()
    }
    fn len_chars(&self) -> usize {
        self.content.len()
    }
    fn len_lines(&self) -> usize {
        self.lines.len()
    }
    fn line_text(&self, idx: usize) -> String {
        self.lines.get(idx).cloned().unwrap_or_default()
    }
    fn is_dirty(&self) -> bool {
        false
    }
    fn is_read_only(&self) -> bool {
        true
    }
}

impl ReadBuffer for Buffer {
    fn text_string(&self) -> String {
        self.text().to_string()
    }
    fn len_chars(&self) -> usize {
        self.text().len_chars()
    }
    fn len_lines(&self) -> usize {
        self.text().len_lines()
    }
    fn line_text(&self, idx: usize) -> String {
        self.line(idx).to_string()
    }
    fn is_dirty(&self) -> bool {
        self.is_dirty()
    }
    fn is_read_only(&self) -> bool {
        false
    }
}

/// Unique identifier for an undo tree node.
pub type UndoNodeId = u64;

/// Buffer version counter, incremented on every edit.
pub type Version = u64;

/// A cursor position tracked by the buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    /// Character offset in the rope.
    pub position: usize,
    /// Anchor for selections (Visual mode). None in Normal/Insert.
    pub anchor: Option<usize>,
}

impl Cursor {
    pub fn new(position: usize) -> Self {
        Self {
            position,
            anchor: None,
        }
    }
}

/// Data for persisting a single undo node to SQLite.
#[derive(Debug, Clone)]
pub struct UndoNodeData {
    pub node_id: i64,
    pub parent_id: Option<i64>,
    pub content: String,
    pub timestamp_ms: i64,
    pub description: String,
}

/// Data for persisting a page's undo tree to SQLite.
#[derive(Debug, Clone)]
pub struct UndoPersistData {
    pub page_id: String,
    pub nodes: Vec<UndoNodeData>,
    pub current_node_id: i64,
}

/// A block that needs a block ID assigned.
#[derive(Debug, Clone)]
pub struct BlockNeedingId {
    /// Last line of the block (zero-based) — where `^id` is appended.
    pub last_line: usize,
    /// Whether this block already has a `^block-id`.
    pub has_id: bool,
}

/// A computed block ID insertion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIdInsertion {
    /// Zero-based line index where the ID should be appended.
    pub line: usize,
    /// The generated block ID (without the `^` prefix).
    pub id: String,
}
