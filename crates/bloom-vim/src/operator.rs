/// Vim operators that act on a motion or text-object range.
///
/// Combined with a [`MotionType`](super::motion::MotionType) or
/// [`TextObjectType`](super::text_object::TextObjectType) to form commands
/// like `dw` (Delete + WordForward) or `ci"` (Change + Inner DoubleQuote).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    Delete,
    Change,
    Yank,
    Indent,
    Dedent,
    AutoIndent,
    Reflow,
}
