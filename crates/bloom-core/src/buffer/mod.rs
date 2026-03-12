//! Re-exports from the `bloom-buffer` crate.
//!
//! bloom-core's `buffer` module is a thin re-export layer. The actual
//! implementation lives in the standalone `bloom-buffer` crate, which
//! owns the Buffer, cursors, undo tree, and block ID generation.

pub use bloom_buffer::edit;
pub use bloom_buffer::edit::EditOp;
pub use bloom_buffer::rope;
pub use bloom_buffer::rope::Buffer;
pub use bloom_buffer::undo;
pub use bloom_buffer::undo::{UndoNodeInfo, UndoTree};
pub use bloom_buffer::{
    BlockIdInsertion, BlockNeedingId, Cursor, UndoNodeData, UndoNodeId, UndoPersistData, Version,
};
