use rusqlite::types::ToSql;

use super::{Index, SearchFilters, SearchResult};

impl Index {
    /// Phase 1: FTS5 prefix query to retrieve candidate pages and their content.
    /// Returns (PageMeta, full_content) pairs for pages matching the prefix terms.
    pub fn search_candidates(&self, query: &str) -> Vec<(crate::types::PageMeta, String)> {
        if query.trim().is_empty() {
            return Vec::new();
        }

        // Convert query words to FTS5 prefix terms: "rop data" → "rop* OR data*"
        // Use OR to cast a wide net; nucleo handles precision in phase 2.
        let fts_query: String = query
            .split_whitespace()
            .filter(|w| !w.is_empty())
            .map(|w| format!("{}*", w.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");

        if fts_query.is_empty() {
            return Vec::new();
        }

        let sql = "SELECT p.id, p.title, p.created, p.path, f.content \
                    FROM pages_fts f \
                    JOIN pages p ON p.id = f.page_id \
                    WHERE pages_fts MATCH ?1";

        let mut stmt = match self.conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let rows: Vec<_> = match stmt.query_map(rusqlite::params![&fts_query], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        }) {
            Ok(r) => r.filter_map(|r| r.ok()).collect(),
            Err(_) => return Vec::new(),
        };

        rows.into_iter()
            .map(|(id, title, created, path, content)| {
                (self.row_to_page_meta(&id, &title, &created, &path), content)
            })
            .collect()
    }

    pub fn search(&self, query: &str, filters: &SearchFilters) -> Vec<SearchResult> {
        let mut sql = String::from(
            "SELECT p.id, p.title, p.created, p.path, f.content, f.rank \
             FROM pages_fts f \
             JOIN pages p ON p.id = f.page_id \
             WHERE pages_fts MATCH ?1",
        );
        let mut params: Vec<Box<dyn ToSql>> = vec![Box::new(query.to_string())];
        let mut idx = 2;

        for tag in &filters.tags {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM tags WHERE page_id = p.id AND tag = ?{})",
                idx
            ));
            params.push(Box::new(tag.0.clone()));
            idx += 1;
        }

        if let Some((start, end)) = &filters.date_range {
            sql.push_str(&format!(
                " AND p.created >= ?{} AND p.created <= ?{}",
                idx,
                idx + 1
            ));
            params.push(Box::new(start.to_string()));
            params.push(Box::new(end.to_string()));
            idx += 2;
        }

        if let Some(ref target) = filters.links_to {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM links WHERE from_page = p.id AND to_page = ?{})",
                idx
            ));
            params.push(Box::new(target.to_hex()));
            idx += 1;
        }

        if let Some(done) = filters.task_status {
            sql.push_str(&format!(
                " AND EXISTS (SELECT 1 FROM tasks WHERE page_id = p.id AND done = ?{})",
                idx
            ));
            params.push(Box::new(done as i32));
            idx += 1;
        }

        let _ = idx;
        sql.push_str(" ORDER BY f.rank");

        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();

        let rows: Vec<_> = match stmt.query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, f64>(5)?,
            ))
        }) {
            Ok(r) => r.filter_map(|r| r.ok()).collect(),
            Err(_) => return Vec::new(),
        };

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        for (id, title, created, path, content, rank) in &rows {
            let page_meta = self.row_to_page_meta(id, title, created, path);
            let score = -rank;

            for (line_num, line_text) in content.lines().enumerate() {
                if line_text.to_lowercase().contains(&query_lower) {
                    results.push(SearchResult {
                        page: page_meta.clone(),
                        line: line_num,
                        line_text: line_text.to_string(),
                        score,
                    });
                }
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results
    }
}
