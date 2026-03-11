//! SQLite-backed index for full-text search, backlinks, tags, and tasks.
//!
//! Each markdown page is parsed into an [`IndexEntry`] and stored in SQLite
//! with FTS5 for search. Provides queries for backlinks, unlinked mentions,
//! tag management, task agendas, and page metadata lookups.

mod fts;
pub mod indexer;
mod query;
mod schema;
mod writer;

use std::ops::Range;
use std::path::Path;

use chrono::NaiveDate;
use rusqlite::Connection;

use crate::error::BloomError;
use crate::types::*;

/// SQLite-backed index for search, backlinks, tags, and task queries.
///
/// Wraps a [`rusqlite::Connection`] with FTS5 full-text search. Pages are
/// indexed via [`IndexEntry`] objects produced by the markdown parser;
/// queries return [`SearchResult`]s, [`Backlink`]s, tags, and task agendas.
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
    pub block_links: Vec<(BlockId, String)>,
}

#[derive(Debug, Clone)]
pub struct RebuildStats {
    pub pages: usize,
    pub links: usize,
    pub tags: usize,
}

#[derive(Debug, Clone)]
pub struct FileFingerprint {
    pub mtime_secs: i64,
    pub size_bytes: u64,
}

impl Index {
    /// Open the index with full read-write access. Used by the indexer thread.
    /// Creates tables if they don't exist.
    pub fn open(path: &Path) -> Result<Self, BloomError> {
        let conn = Connection::open(path).map_err(|e| BloomError::IndexError(e.to_string()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        schema::create_tables(&conn)?;
        Ok(Index { conn })
    }

    /// Open the index as read-only. Used by the UI thread for BQL queries,
    /// backlink lookups, etc. Enforced at the SQLite level — write attempts
    /// fail immediately. The indexer thread owns the single write connection.
    pub fn open_readonly(path: &Path) -> Result<Self, BloomError> {
        use rusqlite::OpenFlags;
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let conn = Connection::open_with_flags(path, flags)
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        conn.execute_batch("PRAGMA foreign_keys=ON;")
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
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

    /// Read-only access to the underlying SQLite connection (for BQL queries).
    pub fn connection(&self) -> &Connection {
        &self.conn
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn make_entry(id_hex: &str, title: &str, content: &str, tags: &[&str]) -> IndexEntry {
        let id = PageId::from_hex(id_hex).unwrap();
        IndexEntry {
            meta: PageMeta {
                id: id.clone(),
                title: title.to_string(),
                created: NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                tags: tags.iter().map(|t| TagName(t.to_string())).collect(),
                path: std::path::PathBuf::from(format!("pages/{}.md", title.to_lowercase())),
            },
            content: content.to_string(),
            links: vec![],
            tags: tags.iter().map(|t| TagName(t.to_string())).collect(),
            tasks: vec![],
            block_ids: vec![],
            block_links: vec![],
        }
    }

    // UC-76: Rebuild index
    #[test]
    fn test_rebuild_index() {
        let mut idx = Index::open_in_memory().unwrap();
        let entries = vec![
            make_entry("aabbccdd", "Page One", "Hello world", &["rust"]),
            make_entry("11223344", "Page Two", "Goodbye world", &["python"]),
        ];
        let stats = idx.rebuild(&entries).unwrap();
        assert_eq!(stats.pages, 2);
        assert_eq!(stats.tags, 2);
    }

    // UC-08: Find page by title
    #[test]
    fn test_find_page_by_title() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry("aabbccdd", "Rust Notes", "content", &[]))
            .unwrap();
        let result = idx.find_page_by_title("Rust Notes");
        assert!(result.is_some());
        assert_eq!(result.unwrap().title, "Rust Notes");
    }

    #[test]
    fn test_find_page_by_id() {
        let mut idx = Index::open_in_memory().unwrap();
        let id = PageId::from_hex("aabbccdd").unwrap();
        idx.index_page(&make_entry("aabbccdd", "Test", "content", &[]))
            .unwrap();
        let result = idx.find_page_by_id(&id);
        assert!(result.is_some());
    }

    // UC-34: Tags
    #[test]
    fn test_all_tags_with_counts() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry("aabbccdd", "P1", "c", &["rust", "editors"]))
            .unwrap();
        idx.index_page(&make_entry("11223344", "P2", "c", &["rust"]))
            .unwrap();
        let tags = idx.all_tags();
        let rust_count = tags.iter().find(|(t, _)| t.0 == "rust").map(|(_, c)| *c);
        assert_eq!(rust_count, Some(2));
    }

    #[test]
    fn test_pages_with_tag() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry("aabbccdd", "P1", "c", &["rust"]))
            .unwrap();
        idx.index_page(&make_entry("11223344", "P2", "c", &["python"]))
            .unwrap();
        let pages = idx.pages_with_tag(&TagName("rust".into()));
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].title, "P1");
    }

    // UC-36: Rename tag
    #[test]
    fn test_rename_tag() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry("aabbccdd", "P1", "c", &["editors"]))
            .unwrap();
        let count = idx
            .rename_tag(&TagName("editors".into()), &TagName("text-editors".into()))
            .unwrap();
        assert_eq!(count, 1);
        let pages = idx.pages_with_tag(&TagName("text-editors".into()));
        assert_eq!(pages.len(), 1);
    }

    // UC-10: Remove page
    #[test]
    fn test_remove_page() {
        let mut idx = Index::open_in_memory().unwrap();
        let id = PageId::from_hex("aabbccdd").unwrap();
        idx.index_page(&make_entry("aabbccdd", "Test", "c", &["rust"]))
            .unwrap();
        idx.remove_page(&id).unwrap();
        assert!(idx.find_page_by_id(&id).is_none());
    }

    // UC-37: Full-text search
    #[test]
    fn test_fts_search() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry(
            "aabbccdd",
            "Rust Notes",
            "Rope data structures are fast",
            &[],
        ))
        .unwrap();
        let filters = SearchFilters {
            tags: vec![],
            date_range: None,
            links_to: None,
            task_status: None,
        };
        let results = idx.search("rope", &filters);
        assert!(!results.is_empty());
    }

    // UC-08: List pages
    #[test]
    fn test_list_pages() {
        let mut idx = Index::open_in_memory().unwrap();
        idx.index_page(&make_entry("aabbccdd", "P1", "c", &[]))
            .unwrap();
        idx.index_page(&make_entry("11223344", "P2", "c", &[]))
            .unwrap();
        let pages = idx.list_pages(None);
        assert_eq!(pages.len(), 2);
    }

    // UC-27: Backlinks
    #[test]
    fn test_backlinks() {
        let mut idx = Index::open_in_memory().unwrap();
        let target_id = PageId::from_hex("aabbccdd").unwrap();
        let mut entry = make_entry("11223344", "Source", "links to target", &[]);
        entry.links.push(LinkTarget {
            page: target_id.clone(),
            display_hint: "Target".into(),
        });
        idx.index_page(&make_entry("aabbccdd", "Target", "content", &[]))
            .unwrap();
        idx.index_page(&entry).unwrap();
        let backlinks = idx.backlinks_to(&target_id);
        assert_eq!(backlinks.len(), 1);
    }

    // UC-43: Tasks for agenda
    #[test]
    fn test_open_tasks() {
        let mut idx = Index::open_in_memory().unwrap();
        let page_id = PageId::from_hex("aabbccdd").unwrap();
        let mut entry = make_entry("aabbccdd", "Tasks", "content", &[]);
        entry.tasks.push(Task {
            text: "Do thing".into(),
            done: false,
            timestamps: vec![Timestamp::Due(NaiveDate::from_ymd_opt(2026, 3, 5).unwrap())],
            source_page: page_id,
            line: 5,
        });
        idx.index_page(&entry).unwrap();
        let tasks = idx.all_open_tasks();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "Do thing");
    }

    // Block ID lookup
    #[test]
    fn test_find_page_by_block_id() {
        let mut idx = Index::open_in_memory().unwrap();
        let mut entry = make_entry("aabbccdd", "Test Page", "content", &[]);
        entry.block_ids = vec![
            (BlockId("k7m2x".into()), 5),
            (BlockId("p3a9f".into()), 10),
        ];
        idx.index_page(&entry).unwrap();

        let result = idx.find_page_by_block_id(&BlockId("k7m2x".into()));
        assert!(result.is_some());
        let (meta, line) = result.unwrap();
        assert_eq!(meta.title, "Test Page");
        assert_eq!(line, 5);

        let result2 = idx.find_page_by_block_id(&BlockId("p3a9f".into()));
        assert!(result2.is_some());
        assert_eq!(result2.unwrap().1, 10);

        // Nonexistent block
        assert!(idx.find_page_by_block_id(&BlockId("zzzzz".into())).is_none());
    }

    // Block links indexed
    #[test]
    fn test_block_links_stored() {
        let mut idx = Index::open_in_memory().unwrap();
        // Page with a block
        let mut target = make_entry("aabbccdd", "Target", "content", &[]);
        target.block_ids = vec![(BlockId("k7m2x".into()), 3)];
        idx.index_page(&target).unwrap();

        // Page that links to the block
        let mut source = make_entry("11223344", "Source", "links to block", &[]);
        source.block_links = vec![(BlockId("k7m2x".into()), "the analysis".into())];
        idx.index_page(&source).unwrap();

        // Block lookup works
        let (meta, line) = idx.find_page_by_block_id(&BlockId("k7m2x".into())).unwrap();
        assert_eq!(meta.title, "Target");
        assert_eq!(line, 3);
    }

    // Retired block IDs table exists
    #[test]
    fn test_retired_block_ids_table() {
        let idx = Index::open_in_memory().unwrap();
        // Just verify the table exists by inserting
        idx.conn
            .execute(
                "INSERT INTO retired_block_ids (block_id, retired_at) VALUES ('old1', '2026-03-01')",
                [],
            )
            .unwrap();
        let count: i64 = idx
            .conn
            .query_row("SELECT COUNT(*) FROM retired_block_ids", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }
}
