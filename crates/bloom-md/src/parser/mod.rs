//! Markdown parser with Bloom extensions.
//!
//! Parses documents into structured metadata: frontmatter, sections, wiki-links
//! (`[[id|text]]`), tags (`#tag`), tasks, timestamps, and block IDs. Skips
//! extension syntax inside fenced code blocks. Provides line-level syntax
//! highlighting via [`StyledSpan`].

pub mod extensions;
pub mod frontmatter;
pub mod highlight;
pub mod markdown;
pub mod traits;

pub use markdown::BloomMarkdownParser;
pub use traits::*;
