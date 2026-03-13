//! BQL → SQL compiler.
//!
//! Takes a [`ValidatedQuery`] (all fields resolved, types checked) and
//! produces SQL with bind parameters. No validation logic — if it reaches
//! this stage, the query is known-good.

use super::parse::{Op, Source};
use super::schema::{FieldSql, FieldType};
use super::validate::{ResolvedValue, ValidatedClause, ValidatedExpr, ValidatedQuery};

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
    pub group_field: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SqlParam {
    Text(String),
    Int(i64),
    Float(f64),
    Null,
    /// `$page` — resolved to the current page ID at execution time.
    PageRef,
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
// Compiler
// ---------------------------------------------------------------------------

/// Compile a validated BQL query into SQL.
pub fn compile(validated: &ValidatedQuery) -> CompiledQuery {
    compile_with_limit(validated, 100)
}

/// Compile with a configurable default row limit.
/// If the query has an explicit `limit` clause, that takes precedence.
pub fn compile_with_limit(validated: &ValidatedQuery, default_limit: u64) -> CompiledQuery {
    let schema = validated.schema;

    // Determine structural query shape.
    let has_count = validated
        .clauses
        .iter()
        .any(|c| matches!(c, ValidatedClause::Count));
    let group_clause = validated.clauses.iter().find_map(|c| match c {
        ValidatedClause::Group(f) => Some(f),
        _ => None,
    });
    let group_field = group_clause.map(|f| f.name.clone());

    let mut params: Vec<SqlParam> = Vec::new();

    // Build the base query (SELECT ... FROM ... JOIN ... WHERE ...).
    let base_sql = build_base_query(validated, &mut params);

    if has_count {
        if let Some(gc) = group_clause {
            // Shape: SELECT group_col, COUNT(*) FROM (...base...) GROUP BY group_col
            let group_col = field_sql_expr(gc.sql);
            let sql = format!(
                "SELECT {group_col} as group_key, COUNT(*) as cnt FROM ({base_sql}) GROUP BY {group_col}"
            );
            CompiledQuery {
                sql,
                params,
                source: validated.source.clone(),
                has_count,
                group_field,
            }
        } else {
            // Shape: SELECT COUNT(*) FROM (...base...)
            let sql = format!("SELECT COUNT(*) as cnt FROM ({base_sql})");
            CompiledQuery {
                sql,
                params,
                source: validated.source.clone(),
                has_count,
                group_field,
            }
        }
    } else {
        // Shape: base query + GROUP BY + ORDER BY + LIMIT
        let mut sql = base_sql;

        // GROUP BY (non-tags sources — tags already have GROUP BY in base)
        if let Some(gf) = group_clause {
            if schema.base_group_by.is_none() {
                let col = field_sql_expr(gf.sql);
                sql.push_str(&format!(" GROUP BY {col}"));
            }
        }

        // ORDER BY
        for clause in &validated.clauses {
            if let ValidatedClause::Sort(fields) = clause {
                let parts: Vec<String> = fields
                    .iter()
                    .map(|sf| {
                        let col = field_sql_expr(sf.field.sql);
                        let dir = if sf.desc { "DESC" } else { "ASC" };
                        format!("{col} {dir}")
                    })
                    .collect();
                sql.push_str(" ORDER BY ");
                sql.push_str(&parts.join(", "));
            }
        }

        // LIMIT — use explicit limit if present, otherwise inject default
        let explicit_limit = validated.clauses.iter().find_map(|c| match c {
            ValidatedClause::Limit(n) => Some(*n),
            _ => None,
        });
        let limit = explicit_limit.unwrap_or(default_limit);
        if !has_count {
            sql.push_str(&format!(" LIMIT {limit}"));
        }

        CompiledQuery {
            sql,
            params,
            source: validated.source.clone(),
            has_count,
            group_field,
        }
    }
}

/// Build SELECT ... FROM ... JOIN ... WHERE (including base_group_by for tags).
fn build_base_query(validated: &ValidatedQuery, params: &mut Vec<SqlParam>) -> String {
    let schema = validated.schema;
    let mut sql = format!("SELECT DISTINCT {} FROM {}", schema.select, schema.from);

    // Base JOINs
    for join in schema.base_joins {
        sql.push(' ');
        sql.push_str(join);
    }

    // Collect WHERE and HAVING parts.
    let mut where_parts: Vec<String> = Vec::new();
    let mut having_parts: Vec<String> = Vec::new();

    // Implicit WHERE filter (e.g., journal).
    if let Some(implicit) = schema.implicit_where {
        where_parts.push(implicit.to_string());
    }

    // User WHERE clauses.
    for clause in &validated.clauses {
        if let ValidatedClause::Where(expr) = clause {
            collect_where_having(expr, params, &mut where_parts, &mut having_parts);
        }
    }

    if !where_parts.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&where_parts.join(" AND "));
    }

    // GROUP BY (tags source has an inherent GROUP BY).
    if let Some(group_by) = schema.base_group_by {
        sql.push_str(&format!(" GROUP BY {group_by}"));
        if !having_parts.is_empty() {
            sql.push_str(" HAVING ");
            sql.push_str(&having_parts.join(" AND "));
        }
    }

    sql
}

/// Split a validated expression into WHERE parts and HAVING parts.
/// Predicates on Aggregate fields go to HAVING; everything else to WHERE.
fn collect_where_having(
    expr: &ValidatedExpr,
    params: &mut Vec<SqlParam>,
    where_parts: &mut Vec<String>,
    having_parts: &mut Vec<String>,
) {
    // For top-level AND, split parts. For everything else, determine
    // the destination based on whether any field is Aggregate.
    match expr {
        ValidatedExpr::And(left, right) => {
            collect_where_having(left, params, where_parts, having_parts);
            collect_where_having(right, params, where_parts, having_parts);
        }
        _ => {
            let sql_fragment = compile_expr(expr, params);
            if expr_has_aggregate(expr) {
                having_parts.push(sql_fragment);
            } else {
                where_parts.push(sql_fragment);
            }
        }
    }
}

fn expr_has_aggregate(expr: &ValidatedExpr) -> bool {
    match expr {
        ValidatedExpr::Compare(field, _, _) => matches!(field.sql, FieldSql::Aggregate(_)),
        ValidatedExpr::And(l, r) | ValidatedExpr::Or(l, r) => {
            expr_has_aggregate(l) || expr_has_aggregate(r)
        }
        ValidatedExpr::Not(inner) => expr_has_aggregate(inner),
        _ => false,
    }
}

fn compile_expr(expr: &ValidatedExpr, params: &mut Vec<SqlParam>) -> String {
    match expr {
        ValidatedExpr::And(left, right) => {
            let l = compile_expr(left, params);
            let r = compile_expr(right, params);
            format!("({l} AND {r})")
        }
        ValidatedExpr::Or(left, right) => {
            let l = compile_expr(left, params);
            let r = compile_expr(right, params);
            format!("({l} OR {r})")
        }
        ValidatedExpr::Not(inner) => {
            let i = compile_expr(inner, params);
            format!("NOT ({i})")
        }
        ValidatedExpr::Compare(field, op, value) => {
            // NULL handling
            if matches!(value, ResolvedValue::Null) {
                let col = field_sql_expr(field.sql);
                return match op {
                    Op::Eq => format!("{col} IS NULL"),
                    Op::Neq => format!("{col} IS NOT NULL"),
                    _ => unreachable!("validator rejects non-eq/neq with none"),
                };
            }

            // Bool field: done = true → done = 1
            if field.field_type == FieldType::Bool {
                let col = field_sql_expr(field.sql);
                let sql_op = op_to_sql(op);
                let val = match value {
                    ResolvedValue::Bool(true) => "1".to_string(),
                    ResolvedValue::Bool(false) => "0".to_string(),
                    _ => push_param(value, params),
                };
                return format!("{col} {sql_op} {val}");
            }

            // Subquery field (e.g., backlinks.count)
            if let FieldSql::Subquery { template, id_col } = &field.sql {
                let subquery = template.replace("{}", id_col);
                let sql_op = op_to_sql(op);
                let val = push_param(value, params);
                return format!("{subquery} {sql_op} {val}");
            }

            let col = field_sql_expr(field.sql);
            let sql_op = op_to_sql(op);
            let val = push_param(value, params);
            format!("{col} {sql_op} {val}")
        }
        ValidatedExpr::Has(field, tag) => {
            // Tags via IN-subquery — no JOIN, no cardinality change.
            if let FieldSql::ListSubquery {
                table,
                match_col,
                fk_col,
                id_col,
            } = &field.sql
            {
                params.push(SqlParam::Text(tag.clone()));
                let idx = params.len();
                format!("{id_col} IN (SELECT {fk_col} FROM {table} WHERE {match_col} = ?{idx})")
            } else {
                unreachable!("validator ensures has is only on TagList fields")
            }
        }
        ValidatedExpr::InRange(field, range) => {
            let col = field_sql_expr(field.sql);
            let (start, end) = compute_range(range);
            params.push(SqlParam::Text(start));
            let start_idx = params.len();
            params.push(SqlParam::Text(end));
            let end_idx = params.len();
            format!("({col} >= ?{start_idx} AND {col} < ?{end_idx})")
        }
    }
}

fn field_sql_expr(sql: &FieldSql) -> &str {
    match sql {
        FieldSql::Column(col) => col,
        FieldSql::Aggregate(col) => col,
        FieldSql::Subquery {
            template: _,
            id_col,
        } => {
            // For ORDER BY / GROUP BY usage, we use the id_col as placeholder.
            // Subquery fields shouldn't appear in ORDER BY normally.
            id_col
        }
        FieldSql::ListSubquery { id_col, .. } => id_col,
    }
}

fn op_to_sql(op: &Op) -> &'static str {
    match op {
        Op::Eq => "=",
        Op::Neq => "!=",
        Op::Lt => "<",
        Op::Gt => ">",
        Op::Lte => "<=",
        Op::Gte => ">=",
    }
}

fn push_param(value: &ResolvedValue, params: &mut Vec<SqlParam>) -> String {
    match value {
        ResolvedValue::Text(s) => {
            params.push(SqlParam::Text(s.clone()));
            format!("?{}", params.len())
        }
        ResolvedValue::Int(n) => {
            params.push(SqlParam::Int(*n));
            format!("?{}", params.len())
        }
        ResolvedValue::Float(n) => {
            params.push(SqlParam::Float(*n));
            format!("?{}", params.len())
        }
        ResolvedValue::Date(d) => {
            params.push(SqlParam::Text(d.clone()));
            format!("?{}", params.len())
        }
        ResolvedValue::Bool(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        ResolvedValue::Null => "NULL".to_string(),
        ResolvedValue::PageRef => {
            params.push(SqlParam::PageRef);
            format!("?{}", params.len())
        }
    }
}

/// Compute date range boundaries. The today date is already resolved in the
/// range literal by the validator (range predicates carry the range name,
/// and the today date was captured at validation time).
fn compute_range(range: &str) -> (String, String) {
    // The validator already resolved "today" into the actual date during
    // validation. But the range itself ("this week", "last month") needs
    // to be computed relative to today. We get today from the range field's
    // InRange expr — but since compute_range doesn't receive today, we need
    // to use chrono::Local::now(). This is acceptable because:
    // 1. The validator already resolved date *values* (today/yesterday/tomorrow)
    // 2. Range boundaries are computed at compile time, same as before
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    compute_range_from(range, &today)
}

fn compute_range_from(range: &str, today: &str) -> (String, String) {
    use chrono::{Datelike, NaiveDate};

    let d = NaiveDate::parse_from_str(today, "%Y-%m-%d")
        .unwrap_or_else(|_| chrono::Local::now().date_naive());

    match range {
        "this week" => {
            let start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let end = start + chrono::Duration::days(7);
            (fmt_date(start), fmt_date(end))
        }
        "last week" => {
            let this_start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let start = this_start - chrono::Duration::days(7);
            (fmt_date(start), fmt_date(this_start))
        }
        "next week" => {
            let this_start = d - chrono::Duration::days(d.weekday().num_days_from_monday() as i64);
            let start = this_start + chrono::Duration::days(7);
            let end = start + chrono::Duration::days(7);
            (fmt_date(start), fmt_date(end))
        }
        "this month" => {
            let start = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
            let end = if d.month() == 12 {
                NaiveDate::from_ymd_opt(d.year() + 1, 1, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() + 1, 1).unwrap()
            };
            (fmt_date(start), fmt_date(end))
        }
        "last month" => {
            let this_start = NaiveDate::from_ymd_opt(d.year(), d.month(), 1).unwrap();
            let start = if d.month() == 1 {
                NaiveDate::from_ymd_opt(d.year() - 1, 12, 1).unwrap()
            } else {
                NaiveDate::from_ymd_opt(d.year(), d.month() - 1, 1).unwrap()
            };
            (fmt_date(start), fmt_date(this_start))
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
            (fmt_date(start), fmt_date(end))
        }
        _ => (today.to_string(), today.to_string()),
    }
}

fn fmt_date(d: chrono::NaiveDate) -> String {
    d.format("%Y-%m-%d").to_string()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::{parse, validate};

    fn compile_str(input: &str) -> Result<CompiledQuery, String> {
        let query = parse(input).map_err(|e| e.to_string())?;
        let validated = validate(&query, "2026-03-08").map_err(|e| e.to_string())?;
        Ok(compile(&validated))
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
        // Verify no JOIN on tags table (subquery only)
        assert!(!c.sql.contains("JOIN tags"));
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
        assert!(c
            .sql
            .contains("SELECT COUNT(*) FROM links WHERE to_page = p.id"));
    }

    #[test]
    fn compile_tags_source() {
        let c = compile_str("tags | sort count desc").unwrap();
        assert!(c.sql.contains("GROUP BY tag"));
        assert!(c.sql.contains("ORDER BY cnt DESC"));
    }

    #[test]
    fn compile_links_page_ref() {
        let c = compile_str("links | where to = $page").unwrap();
        assert!(c.sql.contains("l.to_page ="));
        assert!(matches!(&c.params[0], SqlParam::PageRef));
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
            "tasks | where not done and due this week and tags has #work | sort due | limit 10",
        )
        .unwrap();
        assert!(c.sql.contains("NOT (t.done = 1)"));
        assert!(c.sql.contains("t.due_date >="));
        assert!(c.sql.contains("tags WHERE tag ="));
        assert!(c.sql.contains("ORDER BY t.due_date ASC"));
        assert!(c.sql.contains("LIMIT 10"));
    }
}
