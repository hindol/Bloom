//! Editor orchestrator — split into focused submodules.
//!
//! This is a private module; its submodules extend [`BloomEditor`](crate::BloomEditor)
//! with `impl` blocks for initialization, key handling, commands, file I/O,
//! navigation, pickers, notifications, and render-frame construction.

mod commands;
pub mod event_loop;
mod files;
mod init;
mod keys;
mod navigation;
mod notifications;
mod page_history;
mod pickers;
mod render;
