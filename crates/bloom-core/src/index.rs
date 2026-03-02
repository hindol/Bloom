use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension, params};

use crate::document::Document;

#[derive(Debug, thiserror::Error)]
pub enum IndexError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    pub page_id: String,
    pub path: PathBuf,
    pub title: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedPage {
    pub page_id: String,
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexedPageContent {
    pub page_id: String,
    pub path: PathBuf,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backlink {
    pub source_page_id: String,
    pub source_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagCount {
    pub tag: String,
    pub count: usize,
}

pub struct SqliteIndex {
    conn: Connection,
    db_path: PathBuf,
}

impl std::fmt::Debug for SqliteIndex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteIndex").finish_non_exhaustive()
    }
}
impl SqliteIndex {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, IndexError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(path)?;
        // WAL mode enables concurrent readers + single writer safely.
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let index = Self { conn, db_path: path.to_path_buf() };
        index.init_schema()?;
        if !index.check_integrity() {
            // Database is corrupted — drop and recreate schema.
            // Content will be repopulated by the indexer on next scan.
            index.clear_and_rebuild_schema()?;
        }
        Ok(index)
    }

    /// Return the database file path (for opening secondary read-only connections).
    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// Returns true if the database passes SQLite's integrity check.
    pub fn check_integrity(&self) -> bool {
        self.conn
            .query_row("PRAGMA integrity_check", [], |row| {
                let result: String = row.get(0)?;
                Ok(result == "ok")
            })
            .unwrap_or(false)
    }

    /// Drop all tables and recreate schema. Used for recovery from corruption.
    pub fn clear_and_rebuild_schema(&self) -> Result<(), IndexError> {
        self.conn.execute_batch(
            r#"
            DROP TABLE IF EXISTS tags;
            DROP TABLE IF EXISTS backlinks;
            DROP TABLE IF EXISTS documents_fts;
            DROP TABLE IF EXISTS documents;
            "#,
        )?;
        self.init_schema()
    }

    fn init_schema(&self) -> Result<(), IndexError> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS documents (
                path TEXT PRIMARY KEY,
                page_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS documents_fts USING fts5(
                path UNINDEXED,
                page_id UNINDEXED,
                title,
                content
            );

            CREATE TABLE IF NOT EXISTS backlinks (
                source_path TEXT NOT NULL,
                source_page_id TEXT NOT NULL,
                target_page_id TEXT NOT NULL,
                PRIMARY KEY (source_path, target_page_id)
            );
            CREATE INDEX IF NOT EXISTS idx_backlinks_target ON backlinks(target_page_id);

            CREATE TABLE IF NOT EXISTS tags (
                source_path TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (source_path, tag)
            );
            CREATE INDEX IF NOT EXISTS idx_tags_tag ON tags(tag);
            "#,
        )?;
        Ok(())
    }

    pub fn index_document(&mut self, path: &Path, doc: &Document) -> Result<(), IndexError> {
        let source_path = path.to_string_lossy().into_owned();
        let page_id = doc.frontmatter.id.trim().to_string();
        let title = doc.frontmatter.title.trim().to_string();
        let content = flatten_content(doc);

        let tx = self.conn.transaction()?;

        tx.execute(
            r#"
            INSERT INTO documents (path, page_id, title, content)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(path) DO UPDATE SET
                page_id = excluded.page_id,
                title = excluded.title,
                content = excluded.content
            "#,
            params![&source_path, &page_id, &title, &content],
        )?;

        tx.execute(
            "DELETE FROM documents_fts WHERE path = ?1",
            params![&source_path],
        )?;
        tx.execute(
            "INSERT INTO documents_fts (path, page_id, title, content) VALUES (?1, ?2, ?3, ?4)",
            params![&source_path, &page_id, &title, &content],
        )?;

        tx.execute(
            "DELETE FROM backlinks WHERE source_path = ?1",
            params![&source_path],
        )?;
        for target_page_id in backlink_targets(doc) {
            tx.execute(
                "INSERT OR IGNORE INTO backlinks (source_path, source_page_id, target_page_id) VALUES (?1, ?2, ?3)",
                params![&source_path, &page_id, target_page_id],
            )?;
        }

        tx.execute(
            "DELETE FROM tags WHERE source_path = ?1",
            params![&source_path],
        )?;
        for tag in document_tags(doc) {
            tx.execute(
                "INSERT OR IGNORE INTO tags (source_path, tag) VALUES (?1, ?2)",
                params![&source_path, tag],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<SearchHit>, IndexError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        // Convert user query to FTS5 prefix query so partial words match
        // (e.g. "Ru" matches "Rust"). Each token gets a trailing `*`.
        let fts_query = fts_prefix_query(query);

        let mut stmt = self.conn.prepare(
            r#"
            SELECT
                page_id,
                path,
                title,
                snippet(documents_fts, 3, '[', ']', '…', 8)
            FROM documents_fts
            WHERE documents_fts MATCH ?1
            ORDER BY bm25(documents_fts), path
            "#,
        )?;

        let rows = stmt.query_map(params![fts_query], |row| {
            Ok(SearchHit {
                page_id: row.get(0)?,
                path: PathBuf::from(row.get::<_, String>(1)?),
                title: row.get(2)?,
                snippet: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            })
        })?;

        let mut hits = Vec::new();
        for row in rows {
            hits.push(row?);
        }
        Ok(hits)
    }

    pub fn page_for_id(&self, page_id: &str) -> Result<Option<IndexedPage>, IndexError> {
        let page_id = page_id.trim();
        if page_id.is_empty() {
            return Ok(None);
        }

        self.conn
            .query_row(
                r#"
                SELECT page_id, path, title
                FROM documents
                WHERE page_id = ?1
                LIMIT 1
                "#,
                params![page_id],
                |row| {
                    Ok(IndexedPage {
                        page_id: row.get(0)?,
                        path: PathBuf::from(row.get::<_, String>(1)?),
                        title: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(IndexError::from)
    }

    pub fn page_for_title(&self, title: &str) -> Result<Option<IndexedPage>, IndexError> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(None);
        }

        self.conn
            .query_row(
                r#"
                SELECT page_id, path, title
                FROM documents
                WHERE title = ?1 COLLATE NOCASE
                ORDER BY path
                LIMIT 1
                "#,
                params![title],
                |row| {
                    Ok(IndexedPage {
                        page_id: row.get(0)?,
                        path: PathBuf::from(row.get::<_, String>(1)?),
                        title: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(IndexError::from)
    }

    pub fn content_for_page_id(&self, page_id: &str) -> Result<Option<String>, IndexError> {
        let page_id = page_id.trim();
        if page_id.is_empty() {
            return Ok(None);
        }

        self.conn
            .query_row(
                "SELECT content FROM documents WHERE page_id = ?1 LIMIT 1",
                params![page_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(IndexError::from)
    }

    pub fn page_content_for_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<IndexedPageContent>, IndexError> {
        let path = path.as_ref().to_string_lossy();
        if path.trim().is_empty() {
            return Ok(None);
        }

        self.conn
            .query_row(
                r#"
                SELECT page_id, path, title, content
                FROM documents
                WHERE path = ?1
                LIMIT 1
                "#,
                params![path.as_ref()],
                |row| {
                    Ok(IndexedPageContent {
                        page_id: row.get(0)?,
                        path: PathBuf::from(row.get::<_, String>(1)?),
                        title: row.get(2)?,
                        content: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(IndexError::from)
    }

    pub fn list_pages(&self) -> Result<Vec<IndexedPage>, IndexError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT page_id, path, title
            FROM documents
            ORDER BY title COLLATE NOCASE, path
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(IndexedPage {
                page_id: row.get(0)?,
                path: PathBuf::from(row.get::<_, String>(1)?),
                title: row.get(2)?,
            })
        })?;

        let mut pages = Vec::new();
        for row in rows {
            pages.push(row?);
        }
        Ok(pages)
    }

    pub fn backlinks_for(&self, target_page_id: &str) -> Result<Vec<Backlink>, IndexError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT source_page_id, source_path
            FROM backlinks
            WHERE target_page_id = ?1
            ORDER BY source_path
            "#,
        )?;

        let rows = stmt.query_map(params![target_page_id], |row| {
            Ok(Backlink {
                source_page_id: row.get(0)?,
                source_path: PathBuf::from(row.get::<_, String>(1)?),
            })
        })?;

        let mut backlinks = Vec::new();
        for row in rows {
            backlinks.push(row?);
        }
        Ok(backlinks)
    }

    pub fn tags_for_path(&self, path: &str) -> Result<Vec<String>, IndexError> {
        let mut stmt = self.conn.prepare(
            "SELECT tag FROM tags WHERE source_path = ?1 ORDER BY tag COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![path], |row| row.get(0))?;
        let mut tags = Vec::new();
        for row in rows {
            tags.push(row?);
        }
        Ok(tags)
    }

    pub fn paths_for_tag(&self, tag: &str) -> Result<Vec<PathBuf>, IndexError> {
        let mut stmt = self
            .conn
            .prepare("SELECT source_path FROM tags WHERE tag = ?1 ORDER BY source_path")?;
        let rows = stmt.query_map(params![tag], |row| {
            Ok(PathBuf::from(row.get::<_, String>(0)?))
        })?;
        let mut paths = Vec::new();
        for row in rows {
            paths.push(row?);
        }
        Ok(paths)
    }

    pub fn remove_document(&mut self, path: &Path) -> Result<(), IndexError> {
        let p = path.to_string_lossy();
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM documents WHERE path = ?1", params![p.as_ref()])?;
        tx.execute(
            "DELETE FROM documents_fts WHERE path = ?1",
            params![p.as_ref()],
        )?;
        tx.execute(
            "DELETE FROM backlinks WHERE source_path = ?1",
            params![p.as_ref()],
        )?;
        tx.execute(
            "DELETE FROM tags WHERE source_path = ?1",
            params![p.as_ref()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn list_tags(&self) -> Result<Vec<TagCount>, IndexError> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT tag, COUNT(*) as count
            FROM tags
            GROUP BY tag
            ORDER BY tag COLLATE NOCASE
            "#,
        )?;

        let rows = stmt.query_map([], |row| {
            let count: i64 = row.get(1)?;
            Ok(TagCount {
                tag: row.get(0)?,
                count: count as usize,
            })
        })?;

        let mut tags = Vec::new();
        for row in rows {
            tags.push(row?);
        }
        Ok(tags)
    }
}

fn flatten_content(doc: &Document) -> String {
    doc.blocks
        .iter()
        .map(|block| block.content.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn backlink_targets(doc: &Document) -> BTreeSet<String> {
    let mut targets = BTreeSet::new();
    for block in &doc.blocks {
        for link in &block.links {
            let target = link.page_id.trim();
            if !target.is_empty() {
                targets.insert(target.to_string());
            }
        }
        for embed in &block.embeds {
            let target = embed.page_id.trim();
            if !target.is_empty() {
                targets.insert(target.to_string());
            }
        }
    }
    targets
}

fn document_tags(doc: &Document) -> BTreeSet<String> {
    let mut tags = BTreeSet::new();

    for tag in &doc.frontmatter.tags {
        if let Some(normalized) = normalize_tag(tag) {
            tags.insert(normalized);
        }
    }

    for block in &doc.blocks {
        for tag in &block.tags {
            if let Some(normalized) = normalize_tag(&tag.name) {
                tags.insert(normalized);
            }
        }
    }

    tags
}

fn normalize_tag(tag: &str) -> Option<String> {
    let normalized = tag.trim().trim_start_matches('#').to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

/// Convert a user query into an FTS5 prefix query.
/// Each word token gets a trailing `*` so partial input matches
/// (e.g. "Ru" → `"Ru" *` which matches "Rust").
fn fts_prefix_query(query: &str) -> String {
    let tokens: Vec<&str> = query.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }
    tokens
        .iter()
        .map(|tok| {
            // Escape any double-quotes inside the token.
            let escaped = tok.replace('"', "\"\"");
            format!("\"{escaped}\" *")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;

    use super::*;
    use crate::parser::parse;
    use tempfile::TempDir;

    fn make_index() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index)
    }

    fn make_doc(id: &str, title: &str, front_tags: &[&str], body: &str) -> Document {
        let tags = if front_tags.is_empty() {
            String::from("[]")
        } else {
            let joined = front_tags
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{joined}]")
        };

        let raw = format!("---\nid: {id}\ntitle: \"{title}\"\ntags: {tags}\n---\n\n{body}\n");
        parse(&raw).unwrap()
    }

    #[test]
    fn open_creates_index_db_file() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();

        assert!(db_path.exists());
        assert!(index.search("anything").unwrap().is_empty());
    }

    #[test]
    fn index_document_and_search_content() {
        let (_tmp, mut index) = make_index();
        let doc = make_doc(
            "aaaa1111",
            "Alpha",
            &["rust"],
            "SQLite indexing makes search fast.",
        );

        index
            .index_document(Path::new("pages/alpha.md"), &doc)
            .unwrap();

        let hits = index.search("sqlite").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page_id, "aaaa1111");
        assert_eq!(hits[0].title, "Alpha");
        assert_eq!(hits[0].path, Path::new("pages/alpha.md"));
    }

    #[test]
    fn search_prefix_matches_partial_word() {
        let (_tmp, mut index) = make_index();
        let doc = make_doc("bb001111", "Beta", &[], "Rust programming is great.");
        index.index_document(Path::new("pages/beta.md"), &doc).unwrap();

        // "Ru" should match "Rust" via prefix query
        let hits = index.search("Ru").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].page_id, "bb001111");

        // "prog" should match "programming"
        let hits = index.search("prog").unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn backlinks_lookup_by_target_page_id() {
        let (_tmp, mut index) = make_index();

        let target = make_doc("target01", "Target", &[], "Base page.");
        let source_a = make_doc("srca0001", "Source A", &[], "See [[target01|Target]].");
        let source_b = make_doc("srcb0001", "Source B", &[], "Embed ![[target01|Target]].");

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/a.md"), &source_a)
            .unwrap();
        index
            .index_document(Path::new("pages/b.md"), &source_b)
            .unwrap();

        let backlinks = index.backlinks_for("target01").unwrap();
        assert_eq!(backlinks.len(), 2);
        assert_eq!(backlinks[0].source_page_id, "srca0001");
        assert_eq!(backlinks[0].source_path, Path::new("pages/a.md"));
        assert_eq!(backlinks[1].source_page_id, "srcb0001");
        assert_eq!(backlinks[1].source_path, Path::new("pages/b.md"));
    }

    #[test]
    fn list_tags_returns_counts() {
        let (_tmp, mut index) = make_index();

        let doc_a = make_doc(
            "taga0001",
            "Tagged A",
            &["Rust", "docs"],
            "One #rust and one #search tag.",
        );
        let doc_b = make_doc(
            "tagb0001",
            "Tagged B",
            &["search"],
            "Body has #Rust and #misc markers.",
        );

        index
            .index_document(Path::new("pages/tag-a.md"), &doc_a)
            .unwrap();
        index
            .index_document(Path::new("pages/tag-b.md"), &doc_b)
            .unwrap();

        let tags = index.list_tags().unwrap();
        let counts: HashMap<_, _> = tags.into_iter().map(|t| (t.tag, t.count)).collect();

        assert_eq!(counts.get("rust"), Some(&2));
        assert_eq!(counts.get("search"), Some(&2));
        assert_eq!(counts.get("docs"), Some(&1));
        assert_eq!(counts.get("misc"), Some(&1));
    }

    #[test]
    fn reindex_replaces_stale_entries() {
        let (_tmp, mut index) = make_index();

        let first = make_doc(
            "reidx001",
            "Reindex",
            &["oldtag"],
            "Oldcontent token with [[target99|Target]].",
        );
        let second = make_doc(
            "reidx001",
            "Reindex",
            &["newtag"],
            "Freshcontent token only.",
        );

        index
            .index_document(Path::new("pages/reindex.md"), &first)
            .unwrap();
        index
            .index_document(Path::new("pages/reindex.md"), &second)
            .unwrap();

        assert!(index.search("oldcontent").unwrap().is_empty());
        assert_eq!(index.search("freshcontent").unwrap().len(), 1);
        assert!(index.backlinks_for("target99").unwrap().is_empty());

        let tags = index.list_tags().unwrap();
        let counts: HashMap<_, _> = tags.into_iter().map(|t| (t.tag, t.count)).collect();
        assert_eq!(counts.get("newtag"), Some(&1));
        assert!(!counts.contains_key("oldtag"));
    }

    #[test]
    fn page_lookup_by_id_and_title() {
        let (_tmp, mut index) = make_index();
        let doc = make_doc("lookup01", "Lookup Page", &[], "Lookup body.");
        index
            .index_document(Path::new("pages/lookup.md"), &doc)
            .unwrap();

        let by_id = index.page_for_id("lookup01").unwrap().unwrap();
        assert_eq!(by_id.page_id, "lookup01");
        assert_eq!(by_id.title, "Lookup Page");
        assert_eq!(by_id.path, Path::new("pages/lookup.md"));

        let by_title = index.page_for_title("lookup page").unwrap().unwrap();
        assert_eq!(by_title.page_id, "lookup01");
        assert_eq!(by_title.path, Path::new("pages/lookup.md"));

        assert!(index.page_for_id("missing01").unwrap().is_none());
        assert!(index.page_for_title("missing title").unwrap().is_none());
    }

    #[test]
    fn page_content_lookup_by_path() {
        let (_tmp, mut index) = make_index();
        let doc = make_doc(
            "content01",
            "Content Page",
            &[],
            "Line one.\nSee [[target01|Target]].",
        );
        index
            .index_document(Path::new("pages/content.md"), &doc)
            .unwrap();

        let page = index
            .page_content_for_path(Path::new("pages/content.md"))
            .unwrap()
            .unwrap();
        assert_eq!(page.page_id, "content01");
        assert_eq!(page.title, "Content Page");
        assert_eq!(page.path, Path::new("pages/content.md"));
        assert!(page.content.contains("[[target01|Target]]"));
        assert!(
            index
                .page_content_for_path(Path::new("pages/missing.md"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn list_pages_returns_all_indexed_pages() {
        let (_tmp, mut index) = make_index();
        let alpha = make_doc("list0001", "Alpha", &[], "Alpha body.");
        let beta = make_doc("list0002", "beta", &[], "Beta body.");

        index
            .index_document(Path::new("pages/alpha.md"), &alpha)
            .unwrap();
        index
            .index_document(Path::new("journal/2026-03-01.md"), &beta)
            .unwrap();

        let pages = index.list_pages().unwrap();
        let tuples: Vec<_> = pages
            .iter()
            .map(|page| {
                (
                    page.page_id.as_str(),
                    page.title.as_str(),
                    page.path.to_string_lossy().to_string(),
                )
            })
            .collect();

        assert_eq!(
            tuples,
            vec![
                ("list0001", "Alpha", "pages/alpha.md".to_string()),
                ("list0002", "beta", "journal/2026-03-01.md".to_string()),
            ]
        );
    }

    #[test]
    fn integrity_check_passes_on_fresh_db() {
        let (_tmp, index) = make_index();
        assert!(index.check_integrity());
    }
}
