//! Bloom Query Language (BQL) — tokeniser, parser, compiler, executor.
//!
//! See `docs/lab/LIVE_VIEWS.md` for the full design.

mod compile;
mod execute;
mod parse;

pub use compile::{CompiledQuery, CompileError, SqlParam, compile};
pub use execute::{CellValue, QueryResult, Row, RowResult, execute, run_query};
pub use parse::{
    Clause, Expr, Field, Op, ParseError, Query, SortField, Source, Token, TokenKind, Value,
    parse, tokenise,
};
