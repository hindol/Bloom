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
