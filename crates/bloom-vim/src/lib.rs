//! bloom-vim: Vim modal editing engine.
//!
//! Implements a Vim-compatible state machine with Normal, Insert, Visual, and
//! Command modes. Produces `EditOp` descriptors — never mutates buffers directly.

pub mod input;

mod grammar;
mod macros;
mod register;

pub mod motion;
pub mod operator;
pub mod state;
pub mod text_object;

pub use bloom_buffer::EditOp;
pub use input::{KeyCode, KeyEvent, Modifiers};
pub use motion::MotionType;
pub use operator::Operator;
pub use state::{Mode, MotionResult, RecordedCommand, VimAction, VimState};
pub use text_object::{ObjectKind, TextObjectType};
