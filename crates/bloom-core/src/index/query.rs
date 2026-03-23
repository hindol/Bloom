use std::path::PathBuf;

use chrono::NaiveDate;
use rusqlite::types::ToSql;

use crate::types::*;

use super::{AgendaFilters, Backlink, Index, OrphanedLink, UnlinkedMention};

fn parse_date(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .unwrap_or_else(|_| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
}

fn row_to_task(
    page_id: &str,
    line: i64,
    text: &str,
    done: i32,
    due: Option<&str>,
    start: Option<&str>,
) -> Task {
    let mut timestamps = Vec::new();
    if let Some(d) = due {
        if let Ok(date) = NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            timestamps.push(Timestamp::Due(date));
        }
    }
    if let Some(s) = start {
        if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
            timestamps.push(Timestamp::Start(date));
        }
    }
    Task {
        text: text.to_string(),
        done: done != 0,
        timestamps,
        source_page: PageId::from_hex(page_id).unwrap_or(PageId([0; 4])),
        line: line as usize,
    }
}

impl Index {
    pub(crate) fn row_to_page_meta(
        &self,
        id: &str,
        title: &str,
        created: &str,
        path: &str,
    ) -> PageMeta {
        PageMeta {
            id: PageId::from_hex(id).unwrap_or(PageId([0; 4])),
            title: title.to_string(),
            created: parse_date(created),
            tags: self.tags_for_page(id),
            path: PathBuf::from(path),
        }
    }

    fn tags_for_page(&self, page_id: &str) -> Vec<TagName> {
        let mut stmt = match self.conn.prepare("SELECT tag FROM tags WHERE page_id = ?1") {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![page_id], |row| row.get::<_, String>(0)) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).map(TagName).collect()
    }

    pub fn find_page_by_title(&self, title: &str) -> Option<PageMeta> {
        self.conn
            .prepare("SELECT id, title, created, path FROM pages WHERE title = ?1")
            .ok()?
            .query_row(rusqlite::params![title], |row| {
                Ok(self.row_to_page_meta(
                    &row.get::<_, String>(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                ))
            })
            .ok()
    }

    pub fn find_page_by_id(&self, id: &PageId) -> Option<PageMeta> {
        let hex = id.to_hex();
        self.conn
            .prepare("SELECT id, title, created, path FROM pages WHERE id = ?1")
            .ok()?
            .query_row(rusqlite::params![hex], |row| {
                Ok(self.row_to_page_meta(
                    &row.get::<_, String>(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                ))
            })
            .ok()
    }

    /// Look up a block ID → (page, line). Returns the first match.
    pub fn find_page_by_block_id(&self, block_id: &BlockId) -> Option<(PageMeta, usize)> {
        self.conn
            .prepare(
                "SELECT p.id, p.title, p.created, p.path, b.line \
                 FROM block_ids b JOIN pages p ON p.id = b.page_id \
                 WHERE b.block_id = ?1 LIMIT 1",
            )
            .ok()?
            .query_row(rusqlite::params![block_id.0], |row| {
                let meta = self.row_to_page_meta(
                    &row.get::<_, String>(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                );
                let line: i64 = row.get(4)?;
                Ok((meta, line as usize))
            })
            .ok()
    }

    /// Find ALL pages containing a block ID (for mirror propagation).
    pub fn find_all_pages_by_block_id(&self, block_id: &BlockId) -> Vec<(PageMeta, usize)> {
        let mut stmt = match self.conn.prepare(
            "SELECT p.id, p.title, p.created, p.path, b.line \
             FROM block_ids b JOIN pages p ON p.id = b.page_id \
             WHERE b.block_id = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![block_id.0], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| {
            let (id, title, created, path, line) = r.ok()?;
            Some((
                self.row_to_page_meta(&id, &title, &created, &path),
                line as usize,
            ))
        })
        .collect()
    }

    pub fn find_page_fuzzy(&self, query: &str) -> Vec<PageMeta> {
        let mut stmt = match self
            .conn
            .prepare("SELECT id, title, created, path FROM pages")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let pages: Vec<_> = rows.filter_map(|r| r.ok()).collect();

        let pattern = nucleo::pattern::Pattern::parse(query, nucleo::pattern::CaseMatching::Ignore);
        let mut matcher = nucleo::Matcher::new(nucleo::Config::DEFAULT);

        let mut scored: Vec<(u32, PageMeta)> = pages
            .iter()
            .filter_map(|(id, title, created, path)| {
                let mut buf = Vec::new();
                let haystack = nucleo::Utf32Str::new(title, &mut buf);
                let score = pattern.score(haystack, &mut matcher)?;
                Some((score, self.row_to_page_meta(id, title, created, path)))
            })
            .collect();

        scored.sort_by(|a, b| b.0.cmp(&a.0));
        scored.into_iter().map(|(_, p)| p).collect()
    }

    pub fn list_pages(&self, filter: Option<&TagName>) -> Vec<PageMeta> {
        if let Some(tag) = filter {
            let mut stmt = match self.conn.prepare(
                "SELECT p.id, p.title, p.created, p.path FROM pages p \
                 JOIN tags t ON t.page_id = p.id WHERE t.tag = ?1 ORDER BY p.title",
            ) {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = match stmt.query_map(rusqlite::params![tag.0], |row| {
                Ok(self.row_to_page_meta(
                    &row.get::<_, String>(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                ))
            }) {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };
            rows.filter_map(|r| r.ok()).collect()
        } else {
            let mut stmt = match self
                .conn
                .prepare("SELECT id, title, created, path FROM pages ORDER BY title")
            {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = match stmt.query_map([], |row| {
                Ok(self.row_to_page_meta(
                    &row.get::<_, String>(0)?,
                    &row.get::<_, String>(1)?,
                    &row.get::<_, String>(2)?,
                    &row.get::<_, String>(3)?,
                ))
            }) {
                Ok(r) => r,
                Err(_) => return Vec::new(),
            };
            rows.filter_map(|r| r.ok()).collect()
        }
    }

    pub fn backlinks_to(&self, page: &PageId) -> Vec<Backlink> {
        let hex = page.to_hex();
        let mut stmt = match self.conn.prepare(
            "SELECT p.id, p.title, p.created, p.path, l.line, l.display_hint, f.content \
             FROM links l \
             JOIN pages p ON p.id = l.from_page \
             LEFT JOIN pages_fts f ON f.page_id = l.from_page \
             WHERE l.to_page = ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map(rusqlite::params![hex], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(id, title, created, path, line, hint, content)| {
                let line_num = line.unwrap_or(0) as usize;
                let context = content
                    .as_deref()
                    .and_then(|c| c.lines().nth(line_num))
                    .or(hint.as_deref())
                    .unwrap_or("")
                    .to_string();
                Backlink {
                    source_page: self.row_to_page_meta(&id, &title, &created, &path),
                    context,
                    line: line_num,
                }
            })
            .collect()
    }

    pub fn forward_links_from(&self, page: &PageId) -> Vec<LinkTarget> {
        let hex = page.to_hex();
        let mut stmt = match self
            .conn
            .prepare("SELECT to_page, display_hint FROM links WHERE from_page = ?1")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map(rusqlite::params![hex], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(to_page, hint)| LinkTarget {
                page: PageId::from_hex(&to_page).unwrap_or(PageId([0; 4])),
                display_hint: hint.unwrap_or_default(),
            })
            .collect()
    }

    pub fn orphaned_links(&self, page: &PageId) -> Vec<OrphanedLink> {
        let hex = page.to_hex();
        let mut stmt = match self.conn.prepare(
            "SELECT l.display_hint, l.line FROM links l \
             WHERE l.from_page = ?1 \
             AND NOT EXISTS (SELECT 1 FROM pages p WHERE p.id = l.to_page)",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map(rusqlite::params![hex], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<i64>>(1)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(hint, line)| OrphanedLink {
                display_hint: hint.unwrap_or_default(),
                line: line.unwrap_or(0) as usize,
                byte_range: 0..0,
            })
            .collect()
    }

    pub fn unlinked_mentions(&self, page_title: &str) -> Vec<UnlinkedMention> {
        let target_hex = match self.conn.query_row(
            "SELECT id FROM pages WHERE title = ?1",
            rusqlite::params![page_title],
            |row| row.get::<_, String>(0),
        ) {
            Ok(id) => id,
            Err(_) => return Vec::new(),
        };

        let lower_title = page_title.to_lowercase();

        let mut stmt = match self.conn.prepare(
            "SELECT f.page_id, f.content, p.id, p.title, p.created, p.path \
             FROM pages_fts f \
             JOIN pages p ON p.id = f.page_id \
             WHERE f.page_id != ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows: Vec<_> = match stmt.query_map(rusqlite::params![target_hex], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        }) {
            Ok(r) => r.filter_map(|r| r.ok()).collect(),
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();

        for (from_id, content, id, title, created, path) in &rows {
            let has_link: bool = self
                .conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM links WHERE from_page = ?1 AND to_page = ?2)",
                    rusqlite::params![from_id, target_hex],
                    |row| row.get(0),
                )
                .unwrap_or(false);

            if has_link {
                continue;
            }

            for (line_num, line_text) in content.lines().enumerate() {
                let lower_line = line_text.to_lowercase();
                let mut search_start = 0;
                while let Some(pos) = lower_line[search_start..].find(&lower_title) {
                    let abs_pos = search_start + pos;
                    results.push(UnlinkedMention {
                        source_page: self.row_to_page_meta(id, title, created, path),
                        context: line_text.to_string(),
                        line: line_num,
                        match_range: abs_pos..abs_pos + page_title.len(),
                    });
                    search_start = abs_pos + 1;
                }
            }
        }

        results
    }

    pub fn all_tags(&self) -> Vec<(TagName, usize)> {
        let mut stmt = match self
            .conn
            .prepare("SELECT tag, COUNT(*) FROM tags GROUP BY tag ORDER BY tag")
        {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, usize>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(tag, count)| (TagName(tag), count))
            .collect()
    }

    pub fn pages_with_tag(&self, tag: &TagName) -> Vec<PageMeta> {
        let mut stmt = match self.conn.prepare(
            "SELECT p.id, p.title, p.created, p.path FROM pages p \
             JOIN tags t ON t.page_id = p.id WHERE t.tag = ?1 ORDER BY p.title",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![tag.0], |row| {
            Ok(self.row_to_page_meta(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    pub fn all_open_tasks(&self) -> Vec<Task> {
        let mut stmt = match self.conn.prepare(
            "SELECT page_id, line, text, done, due_date, start_date FROM tasks WHERE done = 0",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows = match stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(page_id, line, text, done, due, start)| {
                row_to_task(
                    &page_id,
                    line,
                    &text,
                    done,
                    due.as_deref(),
                    start.as_deref(),
                )
            })
            .collect()
    }

    pub fn tasks_filtered(&self, filters: &AgendaFilters) -> Vec<Task> {
        let mut sql = String::from(
            "SELECT DISTINCT t.page_id, t.line, t.text, t.done, t.due_date, t.start_date \
             FROM tasks t",
        );
        let mut conditions: Vec<String> = Vec::new();
        let mut params: Vec<Box<dyn ToSql>> = Vec::new();
        let mut idx = 1;

        if !filters.tags.is_empty() {
            sql.push_str(" JOIN tags tg ON tg.page_id = t.page_id");
            let mut placeholders = Vec::new();
            for tag in &filters.tags {
                placeholders.push(format!("?{}", idx));
                params.push(Box::new(tag.0.clone()));
                idx += 1;
            }
            conditions.push(format!("tg.tag IN ({})", placeholders.join(",")));
        }

        if let Some(ref page) = filters.page {
            conditions.push(format!("t.page_id = ?{}", idx));
            params.push(Box::new(page.to_hex()));
            idx += 1;
        }

        if let Some((start, end)) = &filters.date_range {
            conditions.push(format!(
                "(t.due_date BETWEEN ?{} AND ?{} OR t.start_date BETWEEN ?{} AND ?{})",
                idx,
                idx + 1,
                idx + 2,
                idx + 3
            ));
            params.push(Box::new(start.to_string()));
            params.push(Box::new(end.to_string()));
            params.push(Box::new(start.to_string()));
            params.push(Box::new(end.to_string()));
            idx += 4;
        }

        let _ = idx;

        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows = match stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i32>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok())
            .map(|(page_id, line, text, done, due, start)| {
                row_to_task(
                    &page_id,
                    line,
                    &text,
                    done,
                    due.as_deref(),
                    start.as_deref(),
                )
            })
            .collect()
    }

    /// Record a page access for frecency scoring.
    /// Updates visit_count, last_accessed_ms, and recomputes frecency_score.
    pub fn record_access(&self, page_id: &PageId) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let hex = page_id.to_hex();

        // Upsert: increment visit_count, update timestamp, recompute score
        let _ = self.conn.execute(
            "INSERT INTO page_access (page_id, visit_count, last_accessed_ms, frecency_score)
             VALUES (?1, 1, ?2, 100.0)
             ON CONFLICT(page_id) DO UPDATE SET
               visit_count = visit_count + 1,
               last_accessed_ms = ?2,
               frecency_score = (visit_count + 1) * (
                 CASE
                   WHEN (?2 - last_accessed_ms) < 14400000 THEN 100  -- 4 hours
                   WHEN (?2 - last_accessed_ms) < 86400000 THEN 70   -- 1 day
                   WHEN (?2 - last_accessed_ms) < 604800000 THEN 50  -- 1 week
                   WHEN (?2 - last_accessed_ms) < 2592000000 THEN 30 -- 1 month
                   ELSE 10
                 END
               )",
            rusqlite::params![hex, now_ms],
        );
    }

    /// Get top N pages by frecency score (for zero-query state).
    pub fn frecency_top(&self, limit: usize) -> Vec<PageMeta> {
        let mut stmt = match self.conn.prepare(
            "SELECT p.id, p.title, p.created, p.path
             FROM page_access a
             JOIN pages p ON p.id = a.page_id
             ORDER BY a.frecency_score DESC
             LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![limit as i64], |row| {
            Ok(self.row_to_page_meta(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Like [`frecency_top`] but also returns `last_accessed_ms` for each page.
    pub fn frecency_top_with_time(&self, limit: usize) -> Vec<(PageMeta, i64)> {
        let mut stmt = match self.conn.prepare(
            "SELECT p.id, p.title, p.created, p.path, a.last_accessed_ms
             FROM page_access a
             JOIN pages p ON p.id = a.page_id
             ORDER BY a.frecency_score DESC
             LIMIT ?1",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(rusqlite::params![limit as i64], |row| {
            let meta = self.row_to_page_meta(
                &row.get::<_, String>(0)?,
                &row.get::<_, String>(1)?,
                &row.get::<_, String>(2)?,
                &row.get::<_, String>(3)?,
            );
            let ts: i64 = row.get(4)?;
            Ok((meta, ts))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }

    /// Get frecency score for a page (0.0 if never accessed).
    pub fn frecency_score(&self, page_id: &PageId) -> f64 {
        let hex = page_id.to_hex();
        self.conn
            .query_row(
                "SELECT frecency_score FROM page_access WHERE page_id = ?1",
                rusqlite::params![hex],
                |row| row.get(0),
            )
            .unwrap_or(0.0)
    }
}

// --- Mirror promotion / demotion queries ---

/// Block IDs that appear in multiple pages but aren't marked ^= yet.
/// Returns (block_id, page_id, path) tuples needing ^ → ^= promotion.
pub(crate) struct MirrorAction {
    pub block_id: String,
    pub page_id: String,
    pub path: PathBuf,
    pub line: usize,
}

impl Index {
    /// Find block IDs that appear in >1 page with is_mirror = 0.
    /// These need promotion from ^ to ^=.
    pub(crate) fn find_blocks_needing_promotion(&self) -> Vec<MirrorAction> {
        let sql = "
            SELECT b.block_id, b.page_id, p.path, b.line
            FROM block_ids b
            JOIN pages p ON p.id = b.page_id
            WHERE b.is_mirror = 0
              AND (SELECT COUNT(*) FROM block_ids b2 WHERE b2.block_id = b.block_id) > 1
        ";
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok(MirrorAction {
                block_id: row.get(0)?,
                page_id: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                line: row.get::<_, i64>(3)? as usize,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }

    /// Find block IDs in only 1 page but marked is_mirror = 1.
    /// These need demotion from ^= to ^.
    pub(crate) fn find_blocks_needing_demotion(&self) -> Vec<MirrorAction> {
        let sql = "
            SELECT b.block_id, b.page_id, p.path, b.line
            FROM block_ids b
            JOIN pages p ON p.id = b.page_id
            WHERE b.is_mirror = 1
              AND (SELECT COUNT(*) FROM block_ids b2 WHERE b2.block_id = b.block_id) = 1
        ";
        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        stmt.query_map([], |row| {
            Ok(MirrorAction {
                block_id: row.get(0)?,
                page_id: row.get(1)?,
                path: PathBuf::from(row.get::<_, String>(2)?),
                line: row.get::<_, i64>(3)? as usize,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}
