use crate::error::BloomError;
use crate::types::*;

use super::{FileFingerprint, Index, IndexEntry, RebuildStats};

impl Index {
    pub fn index_page(&mut self, entry: &IndexEntry) -> Result<(), BloomError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        let page_id = entry.meta.id.to_hex();

        remove_page_data(&tx, &page_id)?;
        insert_page_data(&tx, &page_id, entry)?;

        tx.commit()
            .map_err(|e| BloomError::IndexError(e.to_string()))
    }

    pub fn remove_page(&mut self, id: &PageId) -> Result<(), BloomError> {
        remove_page_data(&self.conn, &id.to_hex())
    }

    pub fn rename_tag(&mut self, old: &TagName, new: &TagName) -> Result<usize, BloomError> {
        self.conn
            .execute(
                "UPDATE tags SET tag = ?1 WHERE tag = ?2",
                rusqlite::params![new.0, old.0],
            )
            .map_err(|e| BloomError::IndexError(e.to_string()))
    }

    pub fn rebuild(&mut self, pages: &[IndexEntry]) -> Result<RebuildStats, BloomError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| BloomError::IndexError(e.to_string()))?;

        tx.execute_batch(
            "DELETE FROM pages_fts;
             DELETE FROM block_ids;
             DELETE FROM tasks;
             DELETE FROM links;
             DELETE FROM tags;
             DELETE FROM pages;
             DELETE FROM file_fingerprints;",
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;

        let mut stats = RebuildStats {
            pages: 0,
            links: 0,
            tags: 0,
        };

        for entry in pages {
            let page_id = entry.meta.id.to_hex();
            insert_page_data(&tx, &page_id, entry)?;
            stats.pages += 1;
            stats.links += entry.links.len();
            stats.tags += entry.tags.len();
        }

        tx.commit()
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        Ok(stats)
    }

    /// Incremental update: process only changed and deleted files in a single transaction.
    pub fn incremental_update(
        &mut self,
        changed: &[IndexEntry],
        deleted_paths: &[String],
    ) -> Result<RebuildStats, BloomError> {
        let tx = self
            .conn
            .transaction()
            .map_err(|e| BloomError::IndexError(e.to_string()))?;

        // Remove deleted pages (look up page ID by path)
        for path in deleted_paths {
            let page_id: Option<String> = tx
                .query_row(
                    "SELECT id FROM pages WHERE path = ?1",
                    rusqlite::params![path],
                    |row| row.get(0),
                )
                .ok();
            if let Some(pid) = page_id {
                remove_page_data(&tx, &pid)?;
            }
            tx.execute(
                "DELETE FROM file_fingerprints WHERE path = ?1",
                rusqlite::params![path],
            )
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        }

        let mut stats = RebuildStats {
            pages: 0,
            links: 0,
            tags: 0,
        };

        // Upsert changed pages
        for entry in changed {
            let page_id = entry.meta.id.to_hex();
            remove_page_data(&tx, &page_id)?;
            insert_page_data(&tx, &page_id, entry)?;
            stats.pages += 1;
            stats.links += entry.links.len();
            stats.tags += entry.tags.len();
        }

        tx.commit()
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        Ok(stats)
    }

    // Fingerprint methods

    pub fn get_fingerprint(&self, path: &str) -> Option<FileFingerprint> {
        self.conn
            .query_row(
                "SELECT mtime_secs, size_bytes FROM file_fingerprints WHERE path = ?1",
                rusqlite::params![path],
                |row| {
                    Ok(FileFingerprint {
                        mtime_secs: row.get(0)?,
                        size_bytes: row.get(1)?,
                    })
                },
            )
            .ok()
    }

    pub fn set_fingerprint(&self, path: &str, fp: &FileFingerprint) {
        let _ = self.conn.execute(
            "INSERT OR REPLACE INTO file_fingerprints (path, mtime_secs, size_bytes) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, fp.mtime_secs, fp.size_bytes],
        );
    }

    /// Batch-set fingerprints within an existing transaction scope.
    pub fn set_fingerprints_batch(&self, fingerprints: &[(String, FileFingerprint)]) {
        for (path, fp) in fingerprints {
            let _ = self.conn.execute(
                "INSERT OR REPLACE INTO file_fingerprints (path, mtime_secs, size_bytes) VALUES (?1, ?2, ?3)",
                rusqlite::params![path, fp.mtime_secs, fp.size_bytes],
            );
        }
    }

    /// Get all stored fingerprints as a map.
    pub fn all_fingerprints(&self) -> std::collections::HashMap<String, FileFingerprint> {
        let mut map = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT path, mtime_secs, size_bytes FROM file_fingerprints") {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    FileFingerprint {
                        mtime_secs: row.get(1)?,
                        size_bytes: row.get(2)?,
                    },
                ))
            }) {
                for row in rows.flatten() {
                    map.insert(row.0, row.1);
                }
            }
        }
        map
    }
}

fn insert_page_data(
    conn: &rusqlite::Connection,
    page_id: &str,
    entry: &IndexEntry,
) -> Result<(), BloomError> {
    let path_str = entry.meta.path.display().to_string();
    let created_str = entry.meta.created.to_string();

    conn.execute(
        "INSERT INTO pages (id, title, created, path) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params![page_id, entry.meta.title, created_str, path_str],
    )
    .map_err(|e| BloomError::IndexError(e.to_string()))?;

    for tag in &entry.tags {
        conn.execute(
            "INSERT INTO tags (page_id, tag) VALUES (?1, ?2)",
            rusqlite::params![page_id, tag.0],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for link in &entry.links {
        conn.execute(
            "INSERT INTO links (from_page, to_page, display_hint, section) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![
                page_id,
                link.page.to_hex(),
                link.display_hint,
                link.section.as_ref().map(|s| &s.0),
            ],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for task in &entry.tasks {
        let (due_date, start_date) = extract_task_dates(task);
        conn.execute(
            "INSERT INTO tasks (page_id, line, text, done, due_date, start_date) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                page_id,
                task.line as i64,
                task.text,
                task.done as i32,
                due_date,
                start_date,
            ],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for (block_id, line) in &entry.block_ids {
        conn.execute(
            "INSERT INTO block_ids (page_id, block_id, line) VALUES (?1, ?2, ?3)",
            rusqlite::params![page_id, block_id.0, *line as i64],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    conn.execute(
        "INSERT INTO pages_fts (page_id, title, content) VALUES (?1, ?2, ?3)",
        rusqlite::params![page_id, entry.meta.title, entry.content],
    )
    .map_err(|e| BloomError::IndexError(e.to_string()))?;

    Ok(())
}

fn remove_page_data(conn: &rusqlite::Connection, page_id: &str) -> Result<(), BloomError> {
    for sql in &[
        "DELETE FROM pages_fts WHERE page_id = ?1",
        "DELETE FROM block_ids WHERE page_id = ?1",
        "DELETE FROM tasks WHERE page_id = ?1",
        "DELETE FROM links WHERE from_page = ?1",
        "DELETE FROM tags WHERE page_id = ?1",
        "DELETE FROM pages WHERE id = ?1",
    ] {
        conn.execute(sql, rusqlite::params![page_id])
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }
    Ok(())
}

fn extract_task_dates(task: &Task) -> (Option<String>, Option<String>) {
    let mut due = None;
    let mut start = None;
    for ts in &task.timestamps {
        match ts {
            Timestamp::Due(d) => due = Some(d.to_string()),
            Timestamp::Start(d) => start = Some(d.to_string()),
            Timestamp::At(_) => {}
        }
    }
    (due, start)
}