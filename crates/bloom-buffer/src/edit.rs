use std::ops::Range;

/// Represents a single edit operation on a buffer.
#[derive(Debug, Clone)]
pub struct EditOp {
    pub range: Range<usize>,
    pub replacement: String,
    pub cursor_after: usize,
}
