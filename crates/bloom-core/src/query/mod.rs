//! Bloom Query Language (BQL) — tokeniser, parser, compiler, executor.
//!
//! See `docs/lab/LIVE_VIEWS.md` for the full design.

mod compile;
mod execute;
mod parse;
pub mod schema;
mod validate;
mod cache;

pub use cache::QueryCache;
pub use compile::{CompiledQuery, CompileError, SqlParam, compile};
pub use execute::{CellValue, QueryContext, QueryResult, QueryResultKind, Row, RowResult, execute, run_query};
pub use parse::{
    Clause, Expr, Field, Op, ParseError, Query, SortField, Source, Token, TokenKind, Value,
    parse, tokenise,
};
pub use validate::{ValidateError, ValidatedQuery, validate};
