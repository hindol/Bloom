//! Rope-based text buffer with branching undo/redo.
//!
//! Built on the [`ropey`] crate for O(log n) edits on large documents.
//! Tracks version numbers and dirty state; groups related edits (e.g. an
//! entire insert-mode session) into single undo nodes via edit groups.

pub mod edit;
pub mod rope;
pub mod undo;

pub use edit::EditOp;
pub use rope::Buffer;
pub use undo::{UndoNodeInfo, UndoTree};
