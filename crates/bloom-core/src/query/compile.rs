//! BQL → SQL compiler.
//!
//! Validates fields and types per source, then produces a SQL query string
//! with bind parameters ready for execution against the SQLite index.

use super::parse::{Clause, Expr, Field, Op, Query, Source, Value};

// ---------------------------------------------------------------------------
// Compiled output
// ---------------------------------------------------------------------------

/// A compiled BQL query ready for execution.
#[derive(Debug, Clone)]
pub struct CompiledQuery {
    pub sql: String,
    pub params: Vec<SqlParam>,
    pub source: Source,
    pub has_count: bool,
    pub has_group: bool,
    pub group_field: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SqlParam {
    Text(String),
    Int(i64),
    Float(f64),
    Null,
}

// ---------------------------------------------------------------------------
// Compilation error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct CompileError {
    pub message: String,
}

impl std::fmt::Display for CompileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

// ---------------------------------------------------------------------------
// Source field definitions
// ---------------------------------------------------------------------------

struct FieldDef {
    sql_column: &'static str,
    field_type: FieldType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum FieldType {
    Text,
    Date,
    Bool,
    Int,
    Tags, // special: list field, only supports `has`
}

fn source_fields(source: &Source) -> &'static [(&'static str, &'static str, FieldType)] {
    match source {
        // (bql_field, sql_column, type)
        Source::Pages => &[
            ("title", "p.title", FieldType::Text),
            ("created", "p.created", FieldType::Date),
            ("path", "p.path", FieldType::Text),
            ("tags", "", FieldType::Tags),
            ("backlinks.count", "", FieldType::Int), // computed via subquery
        ],
        Source::Tasks => &[
            ("text", "t.text", FieldType::Text),
            ("done", "t.done", FieldType::Bool),
            ("due", "t.due_date", FieldType::Date),
            ("start", "t.start_date", FieldType::Date),
            ("page", "p.title", FieldType::Text),
            ("tags", "", FieldType::Tags),
            ("line", "t.line", FieldType::Int),
        ],
        Source::Tags => &[
            ("name", "tag", FieldType::Text),
            ("count", "cnt", FieldType::Int),
        ],
        Source::Links => &[
            ("from", "l.from_page", FieldType::Text),
            ("to", "l.to_page", FieldType::Text),
            ("display", "l.display_hint", FieldType::Text),
        ],
        Source::Journal => &[
            ("date", "p.created", FieldType::Date),
            ("title", "p.title", FieldType::Text),
            ("tags", "", FieldType::Tags),
        ],
        Source::Blocks => &[
            ("text", "t.text", FieldType::Text),
            ("page", "p.title", FieldType::Text),
            ("line", "t.line", FieldType::Int),
            ("tags", "", FieldType::Tags),
        ],
    }
}

fn resolve_field(
    source: &Source,
    field: &Field,
) -> Result<(&'static str, FieldType), CompileError> {
    let field_name = field.to_string();
    let fields = source_fields(source);
    for &(bql, sql, ft) in fields {
        if bql == field_name {
            return Ok((sql, ft));
        }
    }
    Err(CompileError {
        message: format!("unknown field '{}' on source '{:?}'", field_name, source),
    })
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

/// Compile a parsed BQL query into SQL.
pub fn compile(query: &Query, today: &str) -> Result<CompiledQuery, CompileError> {
    let mut params: Vec<SqlParam> = Vec::new();
    let mut needs_tag_join = false;
    let mut needs_page_join = false;

    // Check if any clause uses tags
    for clause in &query.clauses {
        if let Clause::Where(expr) = clause {
            if expr_uses_tags(expr) {
                needs_tag_join = true;
            }
        }
    }

    // Base SELECT + FROM
    let (select, from) = match &query.source {
        Source::Pages => (
            "SELECT DISTINCT p.id, p.title, p.created, p.path",
            "FROM pages p",
        ),
        Source::Tasks => {
            needs_page_join = true;
            (
                "SELECT DISTINCT t.page_id, t.line, t.text, t.done, t.due_date, t.start_date, p.title as page_title",
                "FROM tasks t",
            )
        }
        Source::Tags => (
            "SELECT tag as name, COUNT(*) as cnt FROM tags",
            "",
        ),
        Source::Links => (
            "SELECT l.from_page, l.to_page, l.display_hint, l.line",
            "FROM links l",
        ),
        Source::Journal => (
            "SELECT DISTINCT p.id, p.title, p.created, p.path",
            "FROM pages p",
        ),
        Source::Blocks => {
            // blocks = tasks + paragraphs; for now map to FTS content
            needs_page_join = true;
            (
                "SELECT DISTINCT t.page_id, t.line, t.text, t.done, t.due_date, t.start_date, p.title as page_title",
                "FROM tasks t",
            )
        }
    };

    let mut sql = String::new();
    sql.push_str(select);
    if !from.is_empty() {
        sql.push(' ');
        sql.push_str(from);
    }

    // JOINs
    if needs_page_join && !matches!(&query.source, Source::Tags | Source::Links) {
        sql.push_str(" JOIN pages p ON p.id = t.page_id");
    }
    if needs_tag_join {
        let alias_table = match &query.source {
            Source::Pages | Source::Journal => "p.id",
            Source::Tasks | Source::Blocks => "t.page_id",
            _ => "p.id",
        };
        sql.push_str(&format!(" JOIN tags tg ON tg.page_id = {alias_table}"));
    }

    // WHERE clauses
    let mut where_parts: Vec<String> = Vec::new();

    // Journal source: restrict to journal/ path
    if matches!(&query.source, Source::Journal) {
        where_parts.push("p.path LIKE 'journal/%'".to_string());
    }

    // Tags source: special handling (GROUP BY)
    let is_tags_source = matches!(&query.source, Source::Tags);

    for clause in &query.clauses {
        if let Clause::Where(expr) = clause {
            let sql_expr = compile_expr(expr, &query.source, &mut params, today)?;
            where_parts.push(sql_expr);
        }
    }

    if !where_parts.is_empty() {
        if is_tags_source {
            // Tags source uses HAVING, not WHERE
            sql.push_str(" GROUP BY tag HAVING ");
        } else {
            sql.push_str(" WHERE ");
        }
        sql.push_str(&where_parts.join(" AND "));
    } else if is_tags_source {
        sql.push_str(" GROUP BY tag");
    }

    // GROUP BY (non-tags sources)
    let mut has_group = false;
    let mut group_field: Option<String> = None;
    for clause in &query.clauses {
        if let Clause::Group(field) = clause {
            let (col, _) = resolve_field(&query.source, field)?;
            if !is_tags_source {
                sql.push_str(&format!(" GROUP BY {col}"));
            }
            has_group = true;
            group_field = Some(field.to_string());
        }
    }

    // ORDER BY
    for clause in &query.clauses {
        if let Clause::Sort(fields) = clause {
            let mut order_parts = Vec::new();
            for sf in fields {
                let (col, _) = resolve_field(&query.source, &sf.field)?;
                let dir = if sf.desc { "DESC" } else { "ASC" };
                order_parts.push(format!("{col} {dir}"));
            }
            sql.push_str(" ORDER BY ");
            sql.push_str(&order_parts.join(", "));
        }
    }

    // COUNT
    let mut has_count = false;
    for clause in &query.clauses {
        if matches!(clause, Clause::Count) {
            has_count = true;
        }
    }

    // LIMIT
    for clause in &query.clauses {
        if let Clause::Limit(n) = clause {
            sql.push_str(&format!(" LIMIT {n}"));
        }
    }

    // Wrap in COUNT if needed
    if has_count && !has_group {
        sql = format!("SELECT COUNT(*) as cnt FROM ({sql})");
    } else if has_count && has_group {
        // Per-group count: the GROUP BY is already in the inner query
        // We need to restructure: SELECT group_col, COUNT(*) GROUP BY group_col
        let group_col = group_field.as_deref().unwrap_or("id");
        let (col, _) = resolve_field(&query.source, &Field {
            segments: group_col.split('.').map(|s| s.to_string()).collect(),
        })?;
        sql = format!(
            "SELECT {col} as group_key, COUNT(*) as cnt FROM ({}) GROUP BY {col}",
            sql.replace(&format!(" GROUP BY {col}"), ""),
        );
    }

    Ok(CompiledQuery {
        sql,
        params,
        source: query.source.clone(),
        has_count,
        has_group,
        group_field: group_field.map(|s| s.to_string()),
    })
}

fn compile_expr(
    expr: &Expr,
    source: &Source,
    params: &mut Vec<SqlParam>,
    today: &str,
) -> Result<String, CompileError> {
    match expr {
        Expr::And(left, right) => {
            let l = compile_expr(left, source, params, today)?;
            let r = compile_expr(right, source, params, today)?;
            Ok(format!("({l} AND {r})"))
        }
        Expr::Or(left, right) => {
            let l = compile_expr(left, source, params, today)?;
            let r = compile_expr(right, source, params, today)?;
            Ok(format!("({l} OR {r})"))
        }
        Expr::Not(inner) => {
            let i = compile_expr(inner, source, params, today)?;
            Ok(format!("NOT ({i})"))
        }
        Expr::Compare(field, op, value) => {
            let field_name = field.to_string();

            // Special case: backlinks.count
            if field_name == "backlinks.count" {
                let table_id = match source {
                    Source::Pages | Source::Journal => "p.id",
                    _ => {
                        return Err(CompileError {
                            message: format!("'backlinks.count' not available on source '{source:?}'"),
                        })
                    }
                };
                let sql_op = compile_op(op);
                let val = compile_value(value, FieldType::Int, params, today)?;
                return Ok(format!(
                    "(SELECT COUNT(*) FROM links WHERE to_page = {table_id}) {sql_op} {val}"
                ));
            }

            let (col, ft) = resolve_field(source, field)?;

            // Bool field: `done = true` → `done = 1`
            if ft == FieldType::Bool {
                let sql_op = compile_op(op);
                let val = match value {
                    Value::Bool(true) => "1".to_string(),
                    Value::Bool(false) => "0".to_string(),
                    _ => compile_value(value, ft, params, today)?,
                };
                return Ok(format!("{col} {sql_op} {val}"));
            }

            // None handling
            if matches!(value, Value::None) {
                return match op {
                    Op::Eq => Ok(format!("{col} IS NULL")),
                    Op::Neq => Ok(format!("{col} IS NOT NULL")),
                    _ => Err(CompileError {
                        message: "only = and != work with 'none'".to_string(),
                    }),
                };
            }

            let sql_op = compile_op(op);
            let val = compile_value(value, ft, params, today)?;
            Ok(format!("{col} {sql_op} {val}"))
        }
        Expr::Has(field, tag) => {
            let (_, ft) = resolve_field(source, field)?;
            if ft != FieldType::Tags {
                return Err(CompileError {
                    message: format!("'has' only works on tag fields, not '{}'", field),
                });
            }
            params.push(SqlParam::Text(tag.clone()));
            let param_idx = params.len();
            let id_col = match source {
                Source::Pages | Source::Journal => "p.id",
                Source::Tasks | Source::Blocks => "t.page_id",
                _ => "p.id",
            };
            Ok(format!(
                "{id_col} IN (SELECT page_id FROM tags WHERE tag = ?{param_idx})"
            ))
        }
        Expr::InRange(field, range) => {
            let (col, ft) = resolve_field(source, field)?;
            if ft != FieldType::Date {
                return Err(CompileError {
                    message: format!("range '{}' only applies to date fields", range),
                });
            }
            // Compute range dates relative to today
            let (start, end) = compute_range(range, today)?;
            params.push(SqlParam::Text(start));
            let start_idx = params.len();
            params.push(SqlParam::Text(end));
            let end_idx = params.len();
            Ok(format!("({col} >= ?{start_idx} AND {col} < ?{end_idx})"))
        }
    }
}

fn compile_op(op: &Op) -> &'static str {
    match op {
        Op::Eq => "=",
        Op::Neq => "!=",
        Op::Lt => "<",
        Op::Gt => ">",
        Op::Lte => "<=",
        Op::Gte => ">=",
    }
}

fn compile_value(
    value: &Value,
    expected_type: FieldType,
    params: &mut Vec<SqlParam>,
    today: &str,
) -> Result<String, CompileError> {
    match value {
        Value::String(s) => {
            params.push(SqlParam::Text(s.clone()));
            Ok(format!("?{}", params.len()))
        }
        Value::Number(n) => {
            if expected_type == FieldType::Int {
                params.push(SqlParam::Int(*n as i64));
            } else {
                params.push(SqlParam::Float(*n));
            }
            Ok(format!("?{}", params.len()))
        }
        Value::Date(d) => {
            let resolved = resolve_date(d, today);
            params.push(SqlParam::Text(resolved));
            Ok(format!("?{}", params.len()))
        }
        Value::Bool(b) => Ok(if *b { "1".to_string() } else { "0".to_string() }),
        Value::None => Ok("NULL".to_string()),
        Value::Var(name) => match name.as_str() {
            "page" => {
                params.push(SqlParam::Text(String::new())); // placeholder, resolved at runtime
                Ok(format!("?{}", params.len()))
            }
            "today" => {
                params.push(SqlParam::Text(today.to_string()));
                Ok(format!("?{}", params.len()))
            }
            _ => Err(CompileError {
                message: format!("unknown variable '${name}'"),
            }),
        },
    }
}

fn resolve_date(d: &str, today: &str) -> String {
    match d {
        "today" => today.to_string(),
        "yesterday" => shift_date(today, -1),
        "tomorrow" => shift_date(today, 1),
        _ => d.to_string(), // ISO date passthrough
    }
}

fn shift_date(iso: &str, days: i64) -> String {
    use chrono::NaiveDate;
    if let Ok(d) = NaiveDate::parse_from_str(iso, "%Y-%m-%d") {
        (d + chrono::Duration::days(days)).format("%Y-%m-%d").to_string()
    } else {
        iso.to_string()
    }
}

fn compute_range(range: &str, today: &str) -> Result<(String, String), CompileError> {
    use chrono::{Datelike, NaiveDate, Weekday};

    let d = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .map_err(|_| CompileError { message: "invalid today date".to_string() })?;

    match range {
        "this week" => {
            let start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let end = start + chrono::Duration::days(7);
            Ok((fmt_date(start), fmt_date(end)))
        }
        "last week" => {
            let this_start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let start = this_start - chrono::Duration::days(7);
            Ok((fmt_date(start), fmt_date(this_start)))
        }
        "next week" => {
            let this_start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let start = this_start + chrono::Duration::days(7);
            let end = start + chrono::Duration::days(7);
            Ok((fmt_date(start), fmt_date(end)))
        }
        "this month" => {
            let start = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
            let end = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
            };
            Ok((fmt_date(start), fmt_date(end)))
        }
        "last month" => {
            let this_start = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
            let start = if d.month() == 1 {
                NaiveDate::from_ymd_opt(d.year() - 1, 12, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() - 1, 1).unwrap()
            };
            Ok((fmt_date(start), fmt_date(this_start)))
        }
        "next month" => {
            let start = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
            };
            let end = if start.month() == 12 {
                NaiveDate::from_ymd_opt(start.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(start.year(), start.month() + 1, 1).unwrap()
            };
            Ok((fmt_date(start), fmt_date(end)))
        }
        _ => Err(CompileError {
            message: format!("unknown range '{range}'"),
        }),
    }
}

fn fmt_date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

fn expr_uses_tags(expr: &Expr) -> bool {
    match expr {
        Expr::Has(_, _) => true,
        Expr::And(l, r) | Expr::Or(l, r) => expr_uses_tags(l) || expr_uses_tags(r),
        Expr::Not(inner) => expr_uses_tags(inner),
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse;

    fn compile_str(input: &str) -> Result<CompiledQuery, CompileError> {
        let query = parse(input).map_err(|e| CompileError { message: e.to_string() })?;
        compile(&query, "2026-03-08")
    }

    #[test]
    fn compile_tasks_simple() {
        let c = compile_str("tasks").unwrap();
        assert!(c.sql.contains("FROM tasks"));
        assert!(!c.has_count);
    }

    #[test]
    fn compile_tasks_where_not_done() {
        let c = compile_str("tasks | where not done").unwrap();
        assert!(c.sql.contains("NOT (t.done = 1)"));
    }

    #[test]
    fn compile_tasks_where_due_lt_today() {
        let c = compile_str("tasks | where due < today").unwrap();
        assert!(c.sql.contains("t.due_date <"));
        assert!(matches!(&c.params[0], SqlParam::Text(d) if d == "2026-03-08"));
    }

    #[test]
    fn compile_tasks_has_tag() {
        let c = compile_str("tasks | where tags has #work").unwrap();
        assert!(c.sql.contains("IN (SELECT page_id FROM tags WHERE tag ="));
        assert!(matches!(&c.params[0], SqlParam::Text(t) if t == "work"));
    }

    #[test]
    fn compile_tasks_range() {
        let c = compile_str("tasks | where due this week").unwrap();
        assert!(c.sql.contains("t.due_date >="));
        assert!(c.sql.contains("t.due_date <"));
        assert_eq!(c.params.len(), 2);
    }

    #[test]
    fn compile_tasks_sort() {
        let c = compile_str("tasks | sort due desc").unwrap();
        assert!(c.sql.contains("ORDER BY t.due_date DESC"));
    }

    #[test]
    fn compile_tasks_limit() {
        let c = compile_str("tasks | sort due | limit 10").unwrap();
        assert!(c.sql.contains("LIMIT 10"));
    }

    #[test]
    fn compile_tasks_count() {
        let c = compile_str("tasks | where not done | count").unwrap();
        assert!(c.sql.starts_with("SELECT COUNT(*)"));
        assert!(c.has_count);
    }

    #[test]
    fn compile_pages_simple() {
        let c = compile_str("pages | sort created desc | limit 20").unwrap();
        assert!(c.sql.contains("FROM pages"));
        assert!(c.sql.contains("ORDER BY p.created DESC"));
        assert!(c.sql.contains("LIMIT 20"));
    }

    #[test]
    fn compile_pages_backlinks_count() {
        let c = compile_str("pages | where backlinks.count = 0").unwrap();
        assert!(c.sql.contains("SELECT COUNT(*) FROM links WHERE to_page = p.id"));
    }

    #[test]
    fn compile_tags_source() {
        let c = compile_str("tags | sort count desc").unwrap();
        assert!(c.sql.contains("GROUP BY tag"));
        assert!(c.sql.contains("ORDER BY cnt DESC"));
    }

    #[test]
    fn compile_links_var() {
        let c = compile_str("links | where to = $page").unwrap();
        assert!(c.sql.contains("l.to_page ="));
    }

    #[test]
    fn compile_none_value() {
        let c = compile_str("tasks | where due = none").unwrap();
        assert!(c.sql.contains("IS NULL"));
    }

    #[test]
    fn compile_none_neq() {
        let c = compile_str("tasks | where due != none").unwrap();
        assert!(c.sql.contains("IS NOT NULL"));
    }

    #[test]
    fn compile_complex() {
        let c = compile_str(
            "tasks | where not done and due this week and tags has #work | sort due | limit 10"
        ).unwrap();
        assert!(c.sql.contains("NOT (t.done = 1)"));
        assert!(c.sql.contains("t.due_date >="));
        assert!(c.sql.contains("tags WHERE tag ="));
        assert!(c.sql.contains("ORDER BY t.due_date ASC"));
        assert!(c.sql.contains("LIMIT 10"));
    }

    #[test]
    fn compile_unknown_field_error() {
        let err = compile_str("tasks | where foo = 1").unwrap_err();
        assert!(err.message.contains("unknown field 'foo'"));
    }

    #[test]
    fn compile_has_on_non_tag_error() {
        let err = compile_str("tasks | where due has #work").unwrap_err();
        assert!(err.message.contains("'has' only works on tag fields"));
    }

    #[test]
    fn compile_range_on_non_date_error() {
        let err = compile_str("tasks | where text this week").unwrap_err();
        assert!(err.message.contains("range"));
    }
}
