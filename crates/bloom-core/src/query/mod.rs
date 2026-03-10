//! Bloom Query Language (BQL) — tokeniser, parser, compiler, executor.
//!
//! See `docs/lab/LIVE_VIEWS.md` for the full design.

mod cache;
mod compile;
mod execute;
mod parse;
pub mod schema;
mod validate;

pub use cache::QueryCache;
pub use compile::{compile, CompileError, CompiledQuery, SqlParam};
pub use execute::{
    execute, run_query, CellValue, QueryContext, QueryResult, QueryResultKind, Row, RowResult,
};
pub use parse::{
    parse, tokenise, Clause, Expr, Field, Op, ParseError, Query, SortField, Source, Token,
    TokenKind, Value,
};
pub use validate::{validate, ValidateError, ValidatedQuery};
