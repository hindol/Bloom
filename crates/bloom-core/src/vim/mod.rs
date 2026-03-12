//! Vim modal editing engine.
//!
//! Implements a Vim-compatible state machine with Normal, Insert, Visual, and
//! Command modes. The grammar parser converts key sequences into operators +
//! motions; the motion resolver computes character ranges in the buffer.
//! Text-object support includes standard Vim objects plus Bloom-specific ones
//! (wiki-links, tags, timestamps, headings).

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
pub use bloom_buffer::EditOp;
