//! BQL executor — runs compiled SQL against the SQLite index.

use rusqlite::Connection;

use super::compile::{CompiledQuery, SqlParam};
use super::parse::Source;

// ---------------------------------------------------------------------------
// Query context
// ---------------------------------------------------------------------------

/// Runtime context for BQL query execution.
pub struct QueryContext {
    pub page_id: Option<String>,
}

// ---------------------------------------------------------------------------
// Query result types
// ---------------------------------------------------------------------------

/// Result of executing a BQL query.
#[derive(Debug, Clone)]
pub struct QueryResult {
    pub source: Source,
    pub kind: QueryResultKind,
}

#[derive(Debug, Clone)]
pub enum QueryResultKind {
    /// A list of rows (the common case).
    Rows(RowResult),
    /// A single count (from `| count` without group).
    Count(u64),
    /// Per-group counts (from `| group X | count`).
    GroupCounts(Vec<(String, u64)>),
}

#[derive(Debug, Clone)]
pub struct RowResult {
    pub columns: Vec<String>,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone)]
pub struct Row {
    pub values: Vec<CellValue>,
}

#[derive(Debug, Clone)]
pub enum CellValue {
    Text(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl std::fmt::Display for CellValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CellValue::Text(s) => write!(f, "{s}"),
            CellValue::Int(n) => write!(f, "{n}"),
            CellValue::Float(n) => write!(f, "{n}"),
            CellValue::Bool(b) => write!(f, "{b}"),
            CellValue::Null => write!(f, ""),
        }
    }
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

/// Execute a compiled BQL query against the index.
pub fn execute(
    compiled: &CompiledQuery,
    conn: &Connection,
    context: &QueryContext,
) -> Result<QueryResult, String> {
    let mut stmt = conn
        .prepare(&compiled.sql)
        .map_err(|e| format!("SQL prepare error: {e}"))?;

    // Bind parameters, resolving PageRef at runtime.
    let params: Vec<Box<dyn rusqlite::types::ToSql>> = compiled
        .params
        .iter()
        .map(|p| -> Box<dyn rusqlite::types::ToSql> {
            match p {
                SqlParam::Text(s) => Box::new(s.clone()),
                SqlParam::Int(n) => Box::new(*n),
                SqlParam::Float(n) => Box::new(*n),
                SqlParam::Null => Box::new(rusqlite::types::Null),
                SqlParam::PageRef => Box::new(context.page_id.clone().unwrap_or_default()),
            }
        })
        .collect();

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|p| p.as_ref()).collect();

    // Count queries
    if compiled.has_count && compiled.group_field.is_none() {
        let count: u64 = stmt
            .query_row(param_refs.as_slice(), |row| row.get(0))
            .map_err(|e| format!("SQL query error: {e}"))?;
        return Ok(QueryResult {
            source: compiled.source.clone(),
            kind: QueryResultKind::Count(count),
        });
    }

    if compiled.has_count && compiled.group_field.is_some() {
        let mut rows = stmt
            .query(param_refs.as_slice())
            .map_err(|e| format!("SQL query error: {e}"))?;
        let mut groups = Vec::new();
        while let Some(row) = rows.next().map_err(|e| format!("SQL row error: {e}"))? {
            let key: String = row.get(0).unwrap_or_default();
            let count: u64 = row.get(1).unwrap_or(0);
            groups.push((key, count));
        }
        return Ok(QueryResult {
            source: compiled.source.clone(),
            kind: QueryResultKind::GroupCounts(groups),
        });
    }

    // Row queries
    let column_count = stmt.column_count();
    let columns: Vec<String> = (0..column_count)
        .map(|i| stmt.column_name(i).unwrap_or("?").to_string())
        .collect();

    let mut result_rows = Vec::new();
    let mut rows = stmt
        .query(param_refs.as_slice())
        .map_err(|e| format!("SQL query error: {e}"))?;

    while let Some(row) = rows.next().map_err(|e| format!("SQL row error: {e}"))? {
        let mut values = Vec::with_capacity(column_count);
        for i in 0..column_count {
            let val = row_to_cell(row, i);
            values.push(val);
        }
        result_rows.push(Row { values });
    }

    Ok(QueryResult {
        source: compiled.source.clone(),
        kind: QueryResultKind::Rows(RowResult {
            columns,
            rows: result_rows,
        }),
    })
}

/// Read a SQLite cell using its actual runtime type.
fn row_to_cell(row: &rusqlite::Row, idx: usize) -> CellValue {
    use rusqlite::types::ValueRef;
    match row.get_ref(idx) {
        Ok(ValueRef::Null) => CellValue::Null,
        Ok(ValueRef::Integer(n)) => CellValue::Int(n),
        Ok(ValueRef::Real(n)) => CellValue::Float(n),
        Ok(ValueRef::Text(bytes)) => CellValue::Text(String::from_utf8_lossy(bytes).to_string()),
        Ok(ValueRef::Blob(_)) => CellValue::Text("<blob>".to_string()),
        Err(_) => CellValue::Null,
    }
}

// ---------------------------------------------------------------------------
// Convenience: parse + validate + compile + execute
// ---------------------------------------------------------------------------

/// Parse, validate, compile, and execute a BQL query string.
pub fn run_query(
    input: &str,
    conn: &Connection,
    today: &str,
    page_id: Option<&str>,
) -> Result<QueryResult, String> {
    let query = super::parse(input).map_err(|e| e.to_string())?;
    let validated = super::validate(&query, today).map_err(|e| e.to_string())?;
    let compiled = super::compile(&validated);
    let context = QueryContext {
        page_id: page_id.map(String::from),
    };
    execute(&compiled, conn, &context)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn setup_test_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE pages (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created TEXT NOT NULL,
                path TEXT NOT NULL UNIQUE
            );
            CREATE TABLE tags (
                page_id TEXT NOT NULL,
                tag TEXT NOT NULL,
                PRIMARY KEY (page_id, tag)
            );
            CREATE TABLE tasks (
                page_id TEXT NOT NULL,
                line INTEGER NOT NULL,
                text TEXT NOT NULL,
                done INTEGER NOT NULL DEFAULT 0,
                due_date TEXT,
                start_date TEXT
            );
            CREATE TABLE links (
                from_page TEXT NOT NULL,
                to_page TEXT NOT NULL,
                display_hint TEXT,
                section TEXT,
                line INTEGER
            );
            CREATE TABLE block_ids (
                page_id TEXT NOT NULL,
                block_id TEXT NOT NULL,
                line INTEGER NOT NULL,
                PRIMARY KEY (page_id, block_id)
            );

            -- Test data
            INSERT INTO pages VALUES ('p1', 'Rust Programming', '2026-01-15', 'pages/rust.md');
            INSERT INTO pages VALUES ('p2', 'Text Editor Theory', '2026-02-01', 'pages/editor.md');
            INSERT INTO pages VALUES ('p3', 'Meeting Notes', '2026-03-08', 'pages/meeting.md');

            INSERT INTO tags VALUES ('p1', 'rust');
            INSERT INTO tags VALUES ('p1', 'programming');
            INSERT INTO tags VALUES ('p2', 'editors');
            INSERT INTO tags VALUES ('p2', 'rust');

            INSERT INTO tasks VALUES ('p1', 10, 'Review ropey API', 0, '2026-03-05', NULL);
            INSERT INTO tasks VALUES ('p1', 11, 'Write benchmarks', 0, '2026-03-10', NULL);
            INSERT INTO tasks VALUES ('p2', 20, 'Read Xi source', 1, '2026-03-01', NULL);
            INSERT INTO tasks VALUES ('p3', 5, 'Follow up', 0, NULL, NULL);

            INSERT INTO links VALUES ('p1', 'p2', 'Text Editor Theory', NULL, 5);
            INSERT INTO links VALUES ('p3', 'p1', 'Rust Programming', NULL, 3);
            ",
        )
        .unwrap();
        conn
    }

    #[test]
    fn execute_tasks_all() {
        let conn = setup_test_db();
        let result = run_query("tasks", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => assert_eq!(r.rows.len(), 4),
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_tasks_not_done() {
        let conn = setup_test_db();
        let result = run_query("tasks | where not done", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => assert_eq!(r.rows.len(), 3),
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_tasks_overdue() {
        let conn = setup_test_db();
        let result = run_query(
            "tasks | where not done and due < today",
            &conn,
            "2026-03-08",
            None,
        )
        .unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert_eq!(r.rows.len(), 1);
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_tasks_with_tag() {
        let conn = setup_test_db();
        let result = run_query("tasks | where tags has #rust", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => assert_eq!(r.rows.len(), 3),
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_tasks_count() {
        let conn = setup_test_db();
        let result =
            run_query("tasks | where not done | count", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Count(n) => assert_eq!(n, 3),
            _ => panic!("expected Count"),
        }
    }

    #[test]
    fn execute_tasks_due_none() {
        let conn = setup_test_db();
        let result = run_query("tasks | where due = none", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => assert_eq!(r.rows.len(), 1),
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_pages_all() {
        let conn = setup_test_db();
        let result = run_query("pages", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => assert_eq!(r.rows.len(), 3),
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_pages_sort_limit() {
        let conn = setup_test_db();
        let result = run_query(
            "pages | sort created desc | limit 2",
            &conn,
            "2026-03-08",
            None,
        )
        .unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert_eq!(r.rows.len(), 2);
                match &r.rows[0].values[1] {
                    CellValue::Text(t) => assert_eq!(t, "Meeting Notes"),
                    other => panic!("expected text, got {other:?}"),
                }
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_pages_backlinks_zero() {
        let conn = setup_test_db();
        let result = run_query(
            "pages | where backlinks.count = 0",
            &conn,
            "2026-03-08",
            None,
        )
        .unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert_eq!(r.rows.len(), 1);
                match &r.rows[0].values[1] {
                    CellValue::Text(t) => assert_eq!(t, "Meeting Notes"),
                    other => panic!("expected text, got {other:?}"),
                }
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_tags_all() {
        let conn = setup_test_db();
        let result = run_query("tags | sort count desc", &conn, "2026-03-08", None).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert!(r.rows.len() >= 2);
                match &r.rows[0].values[0] {
                    CellValue::Text(t) => assert_eq!(t, "rust"),
                    other => panic!("expected text, got {other:?}"),
                }
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_links_to_page() {
        let conn = setup_test_db();
        let result =
            run_query("links | where to = $page", &conn, "2026-03-08", Some("p2")).unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert_eq!(r.rows.len(), 1);
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_complex_query() {
        let conn = setup_test_db();
        let result = run_query(
            "tasks | where not done and tags has #rust | sort due | limit 10",
            &conn,
            "2026-03-08",
            None,
        )
        .unwrap();
        match result.kind {
            QueryResultKind::Rows(r) => {
                assert_eq!(r.rows.len(), 2);
            }
            _ => panic!("expected Rows"),
        }
    }

    #[test]
    fn execute_result_carries_source() {
        let conn = setup_test_db();
        let result = run_query("tasks", &conn, "2026-03-08", None).unwrap();
        assert!(matches!(result.source, Source::Tasks));
        let result = run_query("pages", &conn, "2026-03-08", None).unwrap();
        assert!(matches!(result.source, Source::Pages));
    }
}
