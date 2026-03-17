//! UI-agnostic render frame types.
//!
//! Defines [`RenderFrame`] — the complete snapshot of everything a frontend
//! needs to paint one frame: panes, cursor, status bar, picker overlay,
//! which-key drawer, notifications, and modal dialogs. Core produces frames;
//! TUI and GUI consume them without any editor logic.

mod frame;
mod layout;
mod measure;
pub mod search_highlight;
mod viewport;

pub use frame::*;
pub use layout::*;
pub use measure::*;
pub use viewport::*;
