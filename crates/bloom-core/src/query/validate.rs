//! BQL validator — resolves fields, checks types, resolves variables.
//!
//! Takes a parsed [`Query`] AST and produces a [`ValidatedQuery`] where every
//! field reference is resolved to its SQL mapping and type. After validation,
//! the codegen can assume all fields are valid.

use std::fmt;

use super::parse::{Clause, Expr, Field, Op, Query, Source, Value};
use super::schema::{self, FieldSql, FieldType, SourceSchema};

// ---------------------------------------------------------------------------
// Validated AST types
// ---------------------------------------------------------------------------

/// A validated query where all fields are resolved and types checked.
#[derive(Debug, Clone)]
pub struct ValidatedQuery {
    pub source: Source,
    pub schema: &'static SourceSchema,
    pub clauses: Vec<ValidatedClause>,
}

#[derive(Debug, Clone)]
pub enum ValidatedClause {
    Where(ValidatedExpr),
    Sort(Vec<ValidatedSortField>),
    Group(ResolvedField),
    Limit(u64),
    Count,
}

#[derive(Debug, Clone)]
pub struct ValidatedSortField {
    pub field: ResolvedField,
    pub desc: bool,
}

/// A field reference resolved against the schema.
#[derive(Debug, Clone)]
pub struct ResolvedField {
    pub name: String,
    pub sql: &'static FieldSql,
    pub field_type: FieldType,
}

#[derive(Debug, Clone)]
pub enum ValidatedExpr {
    And(Box<ValidatedExpr>, Box<ValidatedExpr>),
    Or(Box<ValidatedExpr>, Box<ValidatedExpr>),
    Not(Box<ValidatedExpr>),
    Compare(ResolvedField, Op, ResolvedValue),
    Has(ResolvedField, String),
    InRange(ResolvedField, String),
}

/// A value with variables and dates resolved.
#[derive(Debug, Clone)]
pub enum ResolvedValue {
    Text(String),
    Int(i64),
    Float(f64),
    Date(String),
    Bool(bool),
    Null,
    /// `$page` — resolved to the current page ID at execution time.
    PageRef,
}

// ---------------------------------------------------------------------------
// Validation error
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ValidateError {
    pub message: String,
    pub position: Option<usize>,
}

impl fmt::Display for ValidateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(pos) = self.position {
            write!(f, "at position {}: {}", pos, self.message)
        } else {
            write!(f, "{}", self.message)
        }
    }
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Validate a parsed BQL query against the schema.
///
/// Resolves all field references, checks types, resolves date keywords and
/// variables. Returns a `ValidatedQuery` ready for SQL codegen.
pub fn validate(query: &Query, today: &str) -> Result<ValidatedQuery, ValidateError> {
    let schema = schema::schema_for(&query.source);

    if !schema.available {
        return Err(ValidateError {
            message: format!("'{:?}' source is not yet available", query.source),
            position: None,
        });
    }

    let mut validated_clauses = Vec::new();
    let mut seen_count = false;

    for clause in &query.clauses {
        if seen_count {
            return Err(ValidateError {
                message: "'count' must be the last clause".to_string(),
                position: None,
            });
        }

        let validated = match clause {
            Clause::Where(expr) => {
                let vexpr = validate_expr(expr, schema, today)?;
                ValidatedClause::Where(vexpr)
            }
            Clause::Sort(fields) => {
                let mut vfields = Vec::new();
                for sf in fields {
                    let resolved = resolve(&sf.field, schema)?;
                    if resolved.field_type == FieldType::TagList {
                        return Err(ValidateError {
                            message: format!("cannot sort by list field '{}'", sf.field),
                            position: None,
                        });
                    }
                    vfields.push(ValidatedSortField {
                        field: resolved,
                        desc: sf.desc,
                    });
                }
                ValidatedClause::Sort(vfields)
            }
            Clause::Group(field) => {
                let resolved = resolve(field, schema)?;
                ValidatedClause::Group(resolved)
            }
            Clause::Limit(n) => ValidatedClause::Limit(*n),
            Clause::Count => {
                seen_count = true;
                ValidatedClause::Count
            }
        };
        validated_clauses.push(validated);
    }

    Ok(ValidatedQuery {
        source: query.source.clone(),
        schema,
        clauses: validated_clauses,
    })
}

fn resolve(field: &Field, schema: &'static SourceSchema) -> Result<ResolvedField, ValidateError> {
    let name = field.to_string();
    match schema::resolve_field(schema, &name) {
        Some(def) => Ok(ResolvedField {
            name,
            sql: &def.sql,
            field_type: def.field_type,
        }),
        None => Err(ValidateError {
            message: format!(
                "unknown field '{}' (available: {})",
                name,
                schema
                    .fields
                    .iter()
                    .map(|f| f.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            position: None,
        }),
    }
}

fn validate_expr(
    expr: &Expr,
    schema: &'static SourceSchema,
    today: &str,
) -> Result<ValidatedExpr, ValidateError> {
    match expr {
        Expr::And(l, r) => {
            let vl = validate_expr(l, schema, today)?;
            let vr = validate_expr(r, schema, today)?;
            Ok(ValidatedExpr::And(Box::new(vl), Box::new(vr)))
        }
        Expr::Or(l, r) => {
            let vl = validate_expr(l, schema, today)?;
            let vr = validate_expr(r, schema, today)?;
            Ok(ValidatedExpr::Or(Box::new(vl), Box::new(vr)))
        }
        Expr::Not(inner) => {
            let vi = validate_expr(inner, schema, today)?;
            Ok(ValidatedExpr::Not(Box::new(vi)))
        }
        Expr::Compare(field, op, value) => {
            let resolved = resolve(field, schema)?;
            let rval = resolve_value(value, resolved.field_type, today)?;

            // None only works with = and !=
            if matches!(rval, ResolvedValue::Null) && !matches!(op, Op::Eq | Op::Neq) {
                return Err(ValidateError {
                    message: "only = and != work with 'none'".to_string(),
                    position: None,
                });
            }

            Ok(ValidatedExpr::Compare(resolved, op.clone(), rval))
        }
        Expr::Has(field, tag) => {
            let resolved = resolve(field, schema)?;
            if resolved.field_type != FieldType::TagList {
                return Err(ValidateError {
                    message: format!("'has' can only be used with tag fields, not '{}'", field),
                    position: None,
                });
            }
            Ok(ValidatedExpr::Has(resolved, tag.clone()))
        }
        Expr::InRange(field, range) => {
            let resolved = resolve(field, schema)?;
            if resolved.field_type != FieldType::Date {
                return Err(ValidateError {
                    message: format!("date ranges only apply to date fields, not '{}'", field),
                    position: None,
                });
            }
            Ok(ValidatedExpr::InRange(resolved, range.clone()))
        }
    }
}

fn resolve_value(
    value: &Value,
    _expected_type: FieldType,
    today: &str,
) -> Result<ResolvedValue, ValidateError> {
    match value {
        Value::String(s) => Ok(ResolvedValue::Text(s.clone())),
        Value::Number(n) => {
            if n.fract() == 0.0 && *n >= i64::MIN as f64 && *n <= i64::MAX as f64 {
                Ok(ResolvedValue::Int(*n as i64))
            } else {
                Ok(ResolvedValue::Float(*n))
            }
        }
        Value::Date(d) => {
            let resolved = resolve_date(d, today);
            Ok(ResolvedValue::Date(resolved))
        }
        Value::Bool(b) => Ok(ResolvedValue::Bool(*b)),
        Value::None => Ok(ResolvedValue::Null),
        Value::Var(name) => match name.as_str() {
            "page" => Ok(ResolvedValue::PageRef),
            "today" => Ok(ResolvedValue::Date(today.to_string())),
            _ => Err(ValidateError {
                message: format!("unknown variable '${name}'"),
                position: None,
            }),
        },
    }
}

fn resolve_date(d: &str, today: &str) -> String {
    match d {
        "today" => today.to_string(),
        "yesterday" => shift_date(today, -1),
        "tomorrow" => shift_date(today, 1),
        _ => d.to_string(),
    }
}

fn shift_date(iso: &str, days: i64) -> String {
    use chrono::NaiveDate;
    if let Ok(d) = NaiveDate::parse_from_str(iso, "%Y-%m-%d") {
        (d + chrono::Duration::days(days))
            .format("%Y-%m-%d")
            .to_string()
    } else {
        iso.to_string()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::parse;

    fn validate_str(input: &str) -> Result<ValidatedQuery, ValidateError> {
        let q = parse(input).map_err(|e| ValidateError {
            message: e.to_string(),
            position: Some(e.position),
        })?;
        validate(&q, "2026-03-08")
    }

    #[test]
    fn validate_tasks_simple() {
        let vq = validate_str("tasks").unwrap();
        assert!(matches!(vq.source, Source::Tasks));
        assert!(vq.clauses.is_empty());
    }

    #[test]
    fn validate_unknown_field() {
        let err = validate_str("tasks | where foo = 1").unwrap_err();
        assert!(err.message.contains("unknown field 'foo'"));
        assert!(err.message.contains("available:"));
    }

    #[test]
    fn validate_has_on_non_tag() {
        let err = validate_str("tasks | where due has #work").unwrap_err();
        assert!(err
            .message
            .contains("'has' can only be used with tag fields"));
    }

    #[test]
    fn validate_range_on_non_date() {
        let err = validate_str("tasks | where text this week").unwrap_err();
        assert!(err
            .message
            .contains("date ranges only apply to date fields"));
    }

    #[test]
    fn validate_blocks_unavailable() {
        let err = validate_str("blocks").unwrap_err();
        assert!(err.message.contains("not yet available"));
    }

    #[test]
    fn validate_none_with_lt() {
        let err = validate_str("tasks | where due < none").unwrap_err();
        assert!(err.message.contains("only = and !="));
    }

    #[test]
    fn validate_sort_by_tags_rejected() {
        let err = validate_str("tasks | where not done | sort tags").unwrap_err();
        assert!(err.message.contains("cannot sort by list field"));
    }

    #[test]
    fn validate_page_ref() {
        let vq = validate_str("links | where to = $page").unwrap();
        match &vq.clauses[0] {
            ValidatedClause::Where(ValidatedExpr::Compare(_, Op::Eq, ResolvedValue::PageRef)) => {}
            other => panic!("expected PageRef, got {other:?}"),
        }
    }

    #[test]
    fn validate_date_resolution() {
        let vq = validate_str("tasks | where due < today").unwrap();
        match &vq.clauses[0] {
            ValidatedClause::Where(ValidatedExpr::Compare(_, _, ResolvedValue::Date(d))) => {
                assert_eq!(d, "2026-03-08");
            }
            other => panic!("expected resolved date, got {other:?}"),
        }
    }

    #[test]
    fn validate_complex_query() {
        let vq = validate_str(
            "tasks | where not done and due this week and tags has #work | sort due | limit 10",
        )
        .unwrap();
        assert_eq!(vq.clauses.len(), 3);
        assert!(matches!(vq.clauses[0], ValidatedClause::Where(_)));
        assert!(matches!(vq.clauses[1], ValidatedClause::Sort(_)));
        assert!(matches!(vq.clauses[2], ValidatedClause::Limit(10)));
    }

    #[test]
    fn validate_unknown_variable() {
        let err = validate_str("links | where to = $unknown").unwrap_err();
        assert!(err.message.contains("unknown variable"));
    }
}
