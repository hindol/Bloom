use crate::error::BloomError;
use crate::types::*;

use super::{Index, IndexEntry, RebuildStats};

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
             DELETE FROM pages;",
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