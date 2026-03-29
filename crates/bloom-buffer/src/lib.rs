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

/// A frozen, read-only wrapper around a `Buffer`.
///
/// Exposes only read methods — no insert, delete, replace, undo, or cursor mutation.
/// Same rope internally, same rendering path, compile-time enforcement.
/// Created via `Buffer::freeze()`, reversed via `ReadOnly::thaw()`.
///
/// Exception: cursor movement IS allowed (it's a viewport concern, not content mutation).
pub struct ReadOnly<T>(T);

impl ReadOnly<Buffer> {
    pub fn text(&self) -> &ropey::Rope {
        self.0.text()
    }

    pub fn len_chars(&self) -> usize {
        self.0.len_chars()
    }

    pub fn len_lines(&self) -> usize {
        self.0.len_lines()
    }

    pub fn line(&self, idx: usize) -> ropey::RopeSlice<'_> {
        self.0.line(idx)
    }

    pub fn cursor(&self, idx: usize) -> usize {
        self.0.cursor(idx)
    }

    pub fn is_dirty(&self) -> bool {
        false // frozen buffers are never dirty
    }

    pub fn find_text(&self, needle: &str) -> Vec<std::ops::Range<usize>> {
        self.0.find_text(needle)
    }

    /// Thaw back to a mutable buffer (e.g., if the user wants to edit).
    pub fn thaw(self) -> Buffer {
        self.0
    }

    /// Borrow the inner buffer for read-only access (e.g., Vim motion computation).
    pub fn as_buffer(&self) -> &Buffer {
        &self.0
    }

    /// Access inner buffer mutably for cursor positioning.
    /// Cursor is a viewport concern — not a content mutation.
    pub fn set_cursor(&mut self, idx: usize, pos: usize) {
        self.0.ensure_cursors(idx + 1);
        self.0.set_cursor(idx, pos);
    }
}

impl Buffer {
    /// Freeze this buffer into a read-only wrapper.
    pub fn freeze(self) -> ReadOnly<Buffer> {
        ReadOnly(self)
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

/// An edit delta: the minimal edit that transforms a parent node's text into this node's text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditDelta {
    /// Character offset in the parent's text where the edit starts.
    pub offset: usize,
    /// Number of characters deleted from the parent at `offset`.
    pub delete_len: usize,
    /// Text inserted at `offset` (after deletion).
    pub insert_text: String,
}

/// Data for persisting a single undo node to SQLite.
#[derive(Debug, Clone)]
pub struct UndoNodeData {
    pub node_id: i64,
    pub parent_id: Option<i64>,
    pub content: Option<String>,
    pub delta_offset: Option<i64>,
    pub delta_del_len: Option<i64>,
    pub delta_insert: Option<String>,
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
