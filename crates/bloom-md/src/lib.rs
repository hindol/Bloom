//! bloom-md: Markdown parser, highlighter, and Bloom extensions.
//!
//! Pure parsing — no state, no I/O. This crate provides:
//! - Markdown parsing with Bloom extensions (links, tags, timestamps, block IDs)
//! - Semantic highlighting
//! - Frontmatter (YAML) parsing and serialization
//! - Theme palettes and style resolution

pub mod parser;
pub mod theme;
pub mod types;

pub use types::{BlockId, PageId, TagName, Timestamp};
