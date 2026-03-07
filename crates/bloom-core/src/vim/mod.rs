mod grammar;
mod macros;
mod register;

pub mod motion;
pub mod operator;
pub mod state;
pub mod text_object;

pub use motion::MotionType;
pub use operator::Operator;
pub use state::{Mode, MotionResult, RecordedCommand, VimAction, VimState};
pub use text_object::{ObjectKind, TextObjectType};

// Re-export EditOp from buffer so consumers can access it via vim module
pub use crate::buffer::EditOp;
