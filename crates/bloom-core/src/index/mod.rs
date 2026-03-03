mod fts;
mod query;
mod schema;
mod writer;

use std::ops::Range;
use std::path::Path;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::error::BloomError;
use crate::types::*;

/// SQLite-backed index for search, backlinks, tags, and unlinked mentions.
pub struct Index {
    conn: Connection,
}

#[derive(Debug, Clone)]
pub struct Backlink {
    pub source_page: PageMeta,
    pub context: String,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct UnlinkedMention {
    pub source_page: PageMeta,
    pub context: String,
    pub line: usize,
    pub match_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct OrphanedLink {
    pub display_hint: String,
    pub line: usize,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub page: PageMeta,
    pub line: usize,
    pub line_text: String,
    pub score: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SearchFilters {
    pub tags: Vec<TagName>,
    pub date_range: Option<(NaiveDate, NaiveDate)>,
    pub links_to: Option<PageId>,
    pub task_status: Option<bool>,
}

#[derive(Debug, Clone, Default)]
pub struct AgendaFilters {
    pub tags: Vec<TagName>,
    pub page: Option<PageId>,
    pub date_range: Option<(NaiveDate, NaiveDate)>,
}

#[derive(Debug, Clone)]
pub struct IndexEntry {
    pub meta: PageMeta,
    pub content: String,
    pub links: Vec<LinkTarget>,
    pub tags: Vec<TagName>,
    pub tasks: Vec<Task>,
    pub block_ids: Vec<(BlockId, usize)>,
}

#[derive(Debug, Clone)]
pub struct RebuildStats {
    pub pages: usize,
    pub links: usize,
    pub tags: usize,
}

impl Index {
    pub fn open(path: &Path) -> Result<Self, BloomError> {
        let conn =
            Connection::open(path).map_err(|e| BloomError::IndexError(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        schema::create_tables(&conn)?;
        Ok(Index { conn })
    }

    pub fn open_in_memory() -> Result<Self, BloomError> {
        let conn =
            Connection::open_in_memory().map_err(|e| BloomError::IndexError(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        schema::create_tables(&conn)?;
        Ok(Index { conn })
    }
}