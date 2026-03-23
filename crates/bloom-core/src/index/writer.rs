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

        // Clear all index-derived tables. Do NOT clear page_access —
        // it contains user-accumulated frecency data that must survive rebuilds.
        // Do NOT clear retired_block_ids — it's an append-only log.
        tx.execute_batch(
            "DELETE FROM pages_fts;
             DELETE FROM block_ids;
             DELETE FROM block_links;
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

        // Phase A: insert structured data (pages, tags, links, tasks, block_ids)
        let t_structured = std::time::Instant::now();
        for entry in pages {
            let page_id = entry.meta.id.to_hex();
            insert_page_data_no_fts(&tx, &page_id, entry)?;
            stats.pages += 1;
            stats.links += entry.links.len();
            stats.tags += entry.tags.len();
        }
        let structured_ms = t_structured.elapsed().as_millis() as u64;

        // Phase B: insert FTS content
        let t_fts = std::time::Instant::now();
        for entry in pages {
            let page_id = entry.meta.id.to_hex();
            tx.execute(
                "INSERT INTO pages_fts (page_id, title, content) VALUES (?1, ?2, ?3)",
                rusqlite::params![page_id, entry.meta.title, entry.content],
            )
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        }
        let fts_ms = t_fts.elapsed().as_millis() as u64;

        tracing::info!(
            structured_ms,
            fts_ms,
            pages = pages.len(),
            "rebuild write phase breakdown"
        );

        // Phase C: derive retired IDs from broken block links
        let retired = retire_from_broken_links(&tx).unwrap_or(0);
        if retired > 0 {
            tracing::info!(retired, "retired block IDs recovered from broken links");
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
                retire_missing_block_ids(&tx, &pid, &[])?;
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
            // Detect missing block IDs before wiping old data
            let new_ids: Vec<&str> = entry
                .block_ids
                .iter()
                .map(|(id, _, _)| id.0.as_str())
                .collect();
            retire_missing_block_ids(&tx, &page_id, &new_ids)?;
            remove_page_data(&tx, &page_id)?;
            insert_page_data(&tx, &page_id, entry)?;
            // Clean stale rows: if this page now owns a block_id that also
            // appears in another page, the other page's row may be stale.
            // We only clean if the other page was NOT in this batch (it will
            // handle its own cleanup).
            clean_stale_block_ids(&tx, &page_id, entry)?;
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
    pub fn set_fingerprints_batch(&mut self, fingerprints: &[(String, FileFingerprint)]) {
        let tx = match self.conn.transaction() {
            Ok(tx) => tx,
            Err(_) => return,
        };
        for (path, fp) in fingerprints {
            let _ = tx.execute(
                "INSERT OR REPLACE INTO file_fingerprints (path, mtime_secs, size_bytes) VALUES (?1, ?2, ?3)",
                rusqlite::params![path, fp.mtime_secs, fp.size_bytes],
            );
        }
        let _ = tx.commit();
    }

    /// Get all stored fingerprints as a map.
    pub fn all_fingerprints(&self) -> std::collections::HashMap<String, FileFingerprint> {
        let mut map = std::collections::HashMap::new();
        if let Ok(mut stmt) = self
            .conn
            .prepare("SELECT path, mtime_secs, size_bytes FROM file_fingerprints")
        {
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

    /// Clear all fingerprints (forces full re-scan on next incremental run).
    pub fn clear_fingerprints(&self) -> Result<(), BloomError> {
        self.conn
            .execute("DELETE FROM file_fingerprints", [])
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        Ok(())
    }

    /// Remove page_access rows for pages that no longer exist in the index.
    pub fn prune_orphaned_access(&self) -> Result<usize, BloomError> {
        self.conn
            .execute(
                "DELETE FROM page_access WHERE page_id NOT IN (SELECT id FROM pages)",
                [],
            )
            .map_err(|e| BloomError::IndexError(e.to_string()))
    }
}

fn insert_page_data_no_fts(
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
            "INSERT OR IGNORE INTO tags (page_id, tag) VALUES (?1, ?2)",
            rusqlite::params![page_id, tag.0],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for link in &entry.links {
        conn.execute(
            "INSERT INTO links (from_page, to_page, display_hint) VALUES (?1, ?2, ?3)",
            rusqlite::params![page_id, link.page.to_hex(), link.display_hint,],
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

    for (block_id, line, is_mirror) in &entry.block_ids {
        conn.execute(
            "INSERT OR REPLACE INTO block_ids (block_id, page_id, line, is_mirror) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![block_id.0, page_id, *line as i64, *is_mirror as i64],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for (block_id, display_hint) in &entry.block_links {
        conn.execute(
            "INSERT INTO block_links (from_page, to_block_id, display_hint) VALUES (?1, ?2, ?3)",
            rusqlite::params![page_id, block_id.0, display_hint],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    Ok(())
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
            "INSERT OR IGNORE INTO tags (page_id, tag) VALUES (?1, ?2)",
            rusqlite::params![page_id, tag.0],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for link in &entry.links {
        conn.execute(
            "INSERT INTO links (from_page, to_page, display_hint) VALUES (?1, ?2, ?3)",
            rusqlite::params![page_id, link.page.to_hex(), link.display_hint,],
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

    for (block_id, line, is_mirror) in &entry.block_ids {
        conn.execute(
            "INSERT OR REPLACE INTO block_ids (block_id, page_id, line, is_mirror) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![block_id.0, page_id, *line as i64, *is_mirror as i64],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    }

    for (block_id, display_hint) in &entry.block_links {
        conn.execute(
            "INSERT INTO block_links (from_page, to_block_id, display_hint) VALUES (?1, ?2, ?3)",
            rusqlite::params![page_id, block_id.0, display_hint],
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
        "DELETE FROM block_links WHERE from_page = ?1",
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

/// Compare old block IDs in the index with the new set from a re-parsed page.
/// Any IDs that disappeared are inserted into `retired_block_ids`.
fn retire_missing_block_ids(
    conn: &rusqlite::Connection,
    page_id: &str,
    new_ids: &[&str],
) -> Result<(), BloomError> {
    let mut stmt = conn
        .prepare("SELECT block_id FROM block_ids WHERE page_id = ?1")
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    let old_ids: Vec<String> = stmt
        .query_map(rusqlite::params![page_id], |row| row.get(0))
        .map_err(|e| BloomError::IndexError(e.to_string()))?
        .filter_map(|r| r.ok())
        .collect();

    let now = chrono::Local::now().to_rfc3339();
    for old_id in &old_ids {
        if !new_ids.contains(&old_id.as_str()) {
            // Check if this ID still exists in another page (cross-page move, not deletion)
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM block_ids WHERE block_id = ?1 AND page_id != ?2",
                    rusqlite::params![old_id, page_id],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            if count == 0 {
                // ID is truly gone from the vault — retire it
                let _ = conn.execute(
                    "INSERT OR IGNORE INTO retired_block_ids (block_id, retired_at) VALUES (?1, ?2)",
                    rusqlite::params![old_id, now],
                );
            }
        }
    }
    Ok(())
}

/// After inserting block_ids for a page, clean up stale rows where the same
/// block_id appears in other pages that no longer contain it.
/// Only cleans rows for block_ids that this page now owns.
fn clean_stale_block_ids(
    conn: &rusqlite::Connection,
    page_id: &str,
    entry: &IndexEntry,
) -> Result<(), BloomError> {
    // For solo blocks (not mirrored), there should be exactly one row.
    // Delete any other page's claim to the same block_id.
    for (block_id, _, is_mirror) in &entry.block_ids {
        if !is_mirror {
            conn.execute(
                "DELETE FROM block_ids WHERE block_id = ?1 AND page_id != ?2",
                rusqlite::params![block_id.0, page_id],
            )
            .map_err(|e| BloomError::IndexError(e.to_string()))?;
        }
    }
    Ok(())
}

/// During full rebuild, derive retired IDs from broken links:
/// block_links that reference IDs not in block_ids.
pub(crate) fn retire_from_broken_links(conn: &rusqlite::Connection) -> Result<usize, BloomError> {
    let now = chrono::Local::now().to_rfc3339();
    let count = conn
        .execute(
            "INSERT OR IGNORE INTO retired_block_ids (block_id, retired_at)
             SELECT DISTINCT bl.to_block_id, ?1
             FROM block_links bl
             WHERE NOT EXISTS (
                 SELECT 1 FROM block_ids bi WHERE bi.block_id = bl.to_block_id
             )",
            rusqlite::params![now],
        )
        .map_err(|e| BloomError::IndexError(e.to_string()))?;
    Ok(count)
}
