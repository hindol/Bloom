use chrono::NaiveDate;
use serde::Serialize;
use std::path::PathBuf;

// Markdown-domain types re-exported from bloom-md.
pub use bloom_md::types::{BlockId, PageId, TagName, Timestamp};

// Input types re-exported from bloom-vim.
pub use bloom_vim::input::{KeyCode, KeyEvent, Modifiers};

pub type Version = u64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize)]
pub struct PaneId(pub u64);

pub type UndoNodeId = u64;

#[derive(Debug, Clone)]
pub struct PageMeta {
    pub id: PageId,
    pub title: String,
    pub created: NaiveDate,
    pub tags: Vec<TagName>,
    pub path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct LinkTarget {
    pub page: PageId,
    pub display_hint: String,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub text: String,
    pub done: bool,
    pub timestamps: Vec<Timestamp>,
    pub source_page: PageId,
    pub line: usize,
}
