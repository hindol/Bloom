//! BQL source schemas — single source of truth for field metadata.
//!
//! Each BQL source (pages, tasks, tags, links, journal, blocks) has a
//! [`SourceSchema`] defining its base SQL, available fields, and how each
//! field maps to SQL. Consumed by the validator and codegen.

use super::parse::Source;

// ---------------------------------------------------------------------------
// Field metadata
// ---------------------------------------------------------------------------

/// How a BQL field maps to SQL.
#[derive(Debug, Clone)]
pub enum FieldSql {
    /// Direct column reference, e.g. `"p.title"`.
    Column(&'static str),
    /// Scalar subquery, e.g. `"(SELECT COUNT(*) FROM links WHERE to_page = {id})"`.
    /// `id_col` is substituted for `{id}` during codegen.
    Subquery {
        template: &'static str,
        id_col: &'static str,
    },
    /// EXISTS/IN subquery for list membership (used with `has`).
    ListSubquery {
        table: &'static str,
        match_col: &'static str,
        fk_col: &'static str,
        id_col: &'static str,
    },
    /// Aggregate expression, valid only in HAVING (after GROUP BY).
    Aggregate(&'static str),
}

/// Semantic type of a BQL field, used for validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Text,
    Date,
    Bool,
    Int,
    TagList,
}

/// Metadata for one field on a BQL source.
#[derive(Debug, Clone)]
pub struct FieldDef {
    pub name: &'static str,
    pub sql: FieldSql,
    pub field_type: FieldType,
}

// ---------------------------------------------------------------------------
// Source schema
// ---------------------------------------------------------------------------

/// Complete schema for a BQL source.
#[derive(Debug, Clone)]
pub struct SourceSchema {
    /// Base SELECT columns (default projection).
    pub select: &'static str,
    /// FROM clause (table + alias).
    pub from: &'static str,
    /// JOINs always applied for this source.
    pub base_joins: &'static [&'static str],
    /// Implicit WHERE filter (e.g., journal restricts to journal paths).
    pub implicit_where: Option<&'static str>,
    /// Whether this source requires a GROUP BY in its base form.
    pub base_group_by: Option<&'static str>,
    /// Available fields.
    pub fields: &'static [FieldDef],
    /// Whether this source is currently available (has backing tables).
    pub available: bool,
}

// ---------------------------------------------------------------------------
// Schema definitions
// ---------------------------------------------------------------------------

pub static PAGES_FIELDS: &[FieldDef] = &[
    FieldDef { name: "title",           sql: FieldSql::Column("p.title"),   field_type: FieldType::Text },
    FieldDef { name: "created",         sql: FieldSql::Column("p.created"), field_type: FieldType::Date },
    FieldDef { name: "path",            sql: FieldSql::Column("p.path"),    field_type: FieldType::Text },
    FieldDef { name: "tags",            sql: FieldSql::ListSubquery { table: "tags", match_col: "tag", fk_col: "page_id", id_col: "p.id" }, field_type: FieldType::TagList },
    FieldDef { name: "backlinks.count", sql: FieldSql::Subquery { template: "(SELECT COUNT(*) FROM links WHERE to_page = {})", id_col: "p.id" }, field_type: FieldType::Int },
];

pub static TASKS_FIELDS: &[FieldDef] = &[
    FieldDef { name: "text",  sql: FieldSql::Column("t.text"),       field_type: FieldType::Text },
    FieldDef { name: "done",  sql: FieldSql::Column("t.done"),       field_type: FieldType::Bool },
    FieldDef { name: "due",   sql: FieldSql::Column("t.due_date"),   field_type: FieldType::Date },
    FieldDef { name: "start", sql: FieldSql::Column("t.start_date"), field_type: FieldType::Date },
    FieldDef { name: "page",  sql: FieldSql::Column("p.title"),      field_type: FieldType::Text },
    FieldDef { name: "tags",  sql: FieldSql::ListSubquery { table: "tags", match_col: "tag", fk_col: "page_id", id_col: "t.page_id" }, field_type: FieldType::TagList },
    FieldDef { name: "line",  sql: FieldSql::Column("t.line"),       field_type: FieldType::Int },
];

pub static TAGS_FIELDS: &[FieldDef] = &[
    FieldDef { name: "name",  sql: FieldSql::Column("tag"),           field_type: FieldType::Text },
    FieldDef { name: "count", sql: FieldSql::Aggregate("cnt"),        field_type: FieldType::Int },
];

pub static LINKS_FIELDS: &[FieldDef] = &[
    FieldDef { name: "from",    sql: FieldSql::Column("l.from_page"),    field_type: FieldType::Text },
    FieldDef { name: "to",      sql: FieldSql::Column("l.to_page"),      field_type: FieldType::Text },
    FieldDef { name: "display", sql: FieldSql::Column("l.display_hint"), field_type: FieldType::Text },
];

pub static JOURNAL_FIELDS: &[FieldDef] = &[
    FieldDef { name: "date",  sql: FieldSql::Column("p.created"), field_type: FieldType::Date },
    FieldDef { name: "title", sql: FieldSql::Column("p.title"),   field_type: FieldType::Text },
    FieldDef { name: "tags",  sql: FieldSql::ListSubquery { table: "tags", match_col: "tag", fk_col: "page_id", id_col: "p.id" }, field_type: FieldType::TagList },
];

pub static BLOCKS_FIELDS: &[FieldDef] = &[];

// ---------------------------------------------------------------------------

static PAGES_SCHEMA: SourceSchema = SourceSchema {
    select: "p.id, p.title, p.created, p.path",
    from: "pages p",
    base_joins: &[],
    implicit_where: None,
    base_group_by: None,
    fields: PAGES_FIELDS,
    available: true,
};

static TASKS_SCHEMA: SourceSchema = SourceSchema {
    select: "t.page_id, t.line, t.text, t.done, t.due_date, t.start_date, p.title as page_title",
    from: "tasks t",
    base_joins: &["JOIN pages p ON p.id = t.page_id"],
    implicit_where: None,
    base_group_by: None,
    fields: TASKS_FIELDS,
    available: true,
};

static TAGS_SCHEMA: SourceSchema = SourceSchema {
    select: "tag as name, COUNT(*) as cnt",
    from: "tags",
    base_joins: &[],
    implicit_where: None,
    base_group_by: Some("tag"),
    fields: TAGS_FIELDS,
    available: true,
};

static LINKS_SCHEMA: SourceSchema = SourceSchema {
    select: "l.from_page, l.to_page, l.display_hint, l.line",
    from: "links l",
    base_joins: &[],
    implicit_where: None,
    base_group_by: None,
    fields: LINKS_FIELDS,
    available: true,
};

static JOURNAL_SCHEMA: SourceSchema = SourceSchema {
    select: "p.id, p.title, p.created, p.path",
    from: "pages p",
    base_joins: &[],
    implicit_where: Some("p.path LIKE 'journal/%'"),
    base_group_by: None,
    fields: JOURNAL_FIELDS,
    available: true,
};

static BLOCKS_SCHEMA: SourceSchema = SourceSchema {
    select: "",
    from: "",
    base_joins: &[],
    implicit_where: None,
    base_group_by: None,
    fields: BLOCKS_FIELDS,
    available: false,
};

/// Look up the schema for a BQL source.
pub fn schema_for(source: &Source) -> &'static SourceSchema {
    match source {
        Source::Pages   => &PAGES_SCHEMA,
        Source::Tasks   => &TASKS_SCHEMA,
        Source::Tags    => &TAGS_SCHEMA,
        Source::Links   => &LINKS_SCHEMA,
        Source::Journal => &JOURNAL_SCHEMA,
        Source::Blocks  => &BLOCKS_SCHEMA,
    }
}

/// Look up a field by name within a source schema.
pub fn resolve_field<'a>(schema: &'a SourceSchema, field_name: &str) -> Option<&'a FieldDef> {
    schema.fields.iter().find(|f| f.name == field_name)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_tasks_has_expected_fields() {
        let s = schema_for(&Source::Tasks);
        assert!(s.available);
        assert!(resolve_field(s, "text").is_some());
        assert!(resolve_field(s, "done").is_some());
        assert!(resolve_field(s, "due").is_some());
        assert!(resolve_field(s, "tags").is_some());
        assert!(resolve_field(s, "nonexistent").is_none());
    }

    #[test]
    fn schema_tags_has_aggregate() {
        let s = schema_for(&Source::Tags);
        let count = resolve_field(s, "count").unwrap();
        assert!(matches!(count.sql, FieldSql::Aggregate(_)));
    }

    #[test]
    fn schema_blocks_unavailable() {
        let s = schema_for(&Source::Blocks);
        assert!(!s.available);
    }

    #[test]
    fn schema_pages_backlinks_is_subquery() {
        let s = schema_for(&Source::Pages);
        let bl = resolve_field(s, "backlinks.count").unwrap();
        assert!(matches!(bl.sql, FieldSql::Subquery { .. }));
        assert_eq!(bl.field_type, FieldType::Int);
    }

    #[test]
    fn schema_tags_field_is_list_subquery() {
        let s = schema_for(&Source::Tasks);
        let tags = resolve_field(s, "tags").unwrap();
        assert!(matches!(tags.sql, FieldSql::ListSubquery { .. }));
        assert_eq!(tags.field_type, FieldType::TagList);
    }
}
