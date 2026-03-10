//! BQL tokeniser and parser.
//!
//! Tokeniser: input string → Vec<Token> with position tracking.
//! Parser: Vec<Token> → Query AST with position-aware errors.

use std::fmt;

// ---------------------------------------------------------------------------
// Tokens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Sources
    Source, // pages, tasks, journal, blocks, tags, links

    // Clause keywords
    Where,
    Sort,
    Group,
    Limit,
    Count,

    // Logical
    And,
    Or,
    Not,

    // Operators
    Eq,  // =
    Neq, // !=
    Lt,  // <
    Gt,  // >
    Lte, // <=
    Gte, // >=
    Has, // has

    // Sort direction
    Asc,
    Desc,

    // Ranges
    Range, // "this week", "last month", etc. — stored as text

    // Values
    String, // "..." or '...'
    Number, // 123
    Date,   // 2026-03-08
    True,
    False,
    None,

    // Variables
    Var, // $page, $today

    // Tag literal
    Tag, // #rust, #work

    // Identifiers (field names)
    Ident,

    // Punctuation
    Pipe,   // |
    LParen, // (
    RParen, // )
    Comma,  // ,
    Dot,    // .
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TokenKind::Source => write!(f, "source"),
            TokenKind::Where => write!(f, "'where'"),
            TokenKind::Sort => write!(f, "'sort'"),
            TokenKind::Group => write!(f, "'group'"),
            TokenKind::Limit => write!(f, "'limit'"),
            TokenKind::Count => write!(f, "'count'"),
            TokenKind::And => write!(f, "'and'"),
            TokenKind::Or => write!(f, "'or'"),
            TokenKind::Not => write!(f, "'not'"),
            TokenKind::Eq => write!(f, "'='"),
            TokenKind::Neq => write!(f, "'!='"),
            TokenKind::Lt => write!(f, "'<'"),
            TokenKind::Gt => write!(f, "'>'"),
            TokenKind::Lte => write!(f, "'<='"),
            TokenKind::Gte => write!(f, "'>='"),
            TokenKind::Has => write!(f, "'has'"),
            TokenKind::Asc => write!(f, "'asc'"),
            TokenKind::Desc => write!(f, "'desc'"),
            TokenKind::Range => write!(f, "range"),
            TokenKind::String => write!(f, "string"),
            TokenKind::Number => write!(f, "number"),
            TokenKind::Date => write!(f, "date"),
            TokenKind::True => write!(f, "'true'"),
            TokenKind::False => write!(f, "'false'"),
            TokenKind::None => write!(f, "'none'"),
            TokenKind::Var => write!(f, "variable"),
            TokenKind::Tag => write!(f, "tag"),
            TokenKind::Ident => write!(f, "identifier"),
            TokenKind::Pipe => write!(f, "'|'"),
            TokenKind::LParen => write!(f, "'('"),
            TokenKind::RParen => write!(f, "')'"),
            TokenKind::Comma => write!(f, "','"),
            TokenKind::Dot => write!(f, "'.'"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tokeniser
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ParseError {
    pub message: String,
    pub position: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "at position {}: {}", self.position, self.message)
    }
}

const SOURCES: &[&str] = &["pages", "tasks", "journal", "blocks", "tags", "links"];

const RANGE_PREFIXES: &[&str] = &[
    "this week",
    "last week",
    "next week",
    "this month",
    "last month",
    "next month",
];

pub fn tokenise(input: &str) -> Result<Vec<Token>, ParseError> {
    let mut tokens = Vec::new();
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        // Skip whitespace.
        if bytes[i].is_ascii_whitespace() {
            i += 1;
            continue;
        }

        let start = i;

        // Single-char punctuation.
        match bytes[i] {
            b'|' => {
                tokens.push(tok(TokenKind::Pipe, start, start + 1, "|"));
                i += 1;
                continue;
            }
            b'(' => {
                tokens.push(tok(TokenKind::LParen, start, start + 1, "("));
                i += 1;
                continue;
            }
            b')' => {
                tokens.push(tok(TokenKind::RParen, start, start + 1, ")"));
                i += 1;
                continue;
            }
            b',' => {
                tokens.push(tok(TokenKind::Comma, start, start + 1, ","));
                i += 1;
                continue;
            }
            b'.' => {
                tokens.push(tok(TokenKind::Dot, start, start + 1, "."));
                i += 1;
                continue;
            }
            _ => {}
        }

        // Operators: =, !=, <, >, <=, >=
        if bytes[i] == b'!' && i + 1 < bytes.len() && bytes[i + 1] == b'=' {
            tokens.push(tok(TokenKind::Neq, start, start + 2, "!="));
            i += 2;
            continue;
        }
        if bytes[i] == b'<' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                tokens.push(tok(TokenKind::Lte, start, start + 2, "<="));
                i += 2;
            } else {
                tokens.push(tok(TokenKind::Lt, start, start + 1, "<"));
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'>' {
            if i + 1 < bytes.len() && bytes[i + 1] == b'=' {
                tokens.push(tok(TokenKind::Gte, start, start + 2, ">="));
                i += 2;
            } else {
                tokens.push(tok(TokenKind::Gt, start, start + 1, ">"));
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'=' {
            tokens.push(tok(TokenKind::Eq, start, start + 1, "="));
            i += 1;
            continue;
        }

        // Strings: "..." or '...'
        if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let str_start = i;
            while i < bytes.len() && bytes[i] != quote {
                i += 1;
            }
            if i >= bytes.len() {
                return Err(ParseError {
                    message: "unterminated string".to_string(),
                    position: start,
                });
            }
            let text = &input[str_start..i];
            tokens.push(tok(TokenKind::String, start, i + 1, text));
            i += 1; // skip closing quote
            continue;
        }

        // Tag: #identifier (Unicode-aware single-pass scan)
        if bytes[i] == b'#' {
            i += 1;
            let tag_start = i;
            while i < bytes.len() {
                let ch = input[i..].chars().next().unwrap();
                if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                    i += ch.len_utf8();
                } else {
                    break;
                }
            }
            if i == tag_start {
                return Err(ParseError {
                    message: "expected tag name after '#'".to_string(),
                    position: start,
                });
            }
            let text = &input[tag_start..i];
            tokens.push(tok(TokenKind::Tag, start, i, text));
            continue;
        }

        // Variable: $identifier
        if bytes[i] == b'$' {
            i += 1;
            let var_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if i == var_start {
                return Err(ParseError {
                    message: "expected variable name after '$'".to_string(),
                    position: start,
                });
            }
            let text = &input[var_start..i];
            tokens.push(tok(TokenKind::Var, start, i, text));
            continue;
        }

        // Number or date (YYYY-MM-DD).
        if bytes[i].is_ascii_digit() {
            let num_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            // Check for date: NNNN-NN-NN
            if i - num_start == 4
                && i + 6 <= bytes.len()
                && bytes[i] == b'-'
                && bytes[i + 1].is_ascii_digit()
                && bytes[i + 2].is_ascii_digit()
                && bytes[i + 3] == b'-'
                && bytes[i + 4].is_ascii_digit()
                && bytes[i + 5].is_ascii_digit()
            {
                i += 6; // consume -MM-DD
                let text = &input[num_start..i];
                tokens.push(tok(TokenKind::Date, start, i, text));
            } else {
                let text = &input[num_start..i];
                tokens.push(tok(TokenKind::Number, start, i, text));
            }
            continue;
        }

        // Word: identifier, keyword, or range prefix.
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            // Check for range prefixes first ("this week", "last month", etc.)
            let remaining = &input[i..];
            let mut matched_range = false;
            for &range in RANGE_PREFIXES {
                if remaining.starts_with(range) {
                    let after = i + range.len();
                    // Must be followed by end-of-input or non-alphanumeric.
                    if after >= bytes.len() || !bytes[after].is_ascii_alphanumeric() {
                        tokens.push(tok(TokenKind::Range, start, after, range));
                        i = after;
                        matched_range = true;
                        break;
                    }
                }
            }
            if matched_range {
                continue;
            }

            // Regular word.
            let word_start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &input[word_start..i];
            let kind = match word {
                "where" => TokenKind::Where,
                "sort" => TokenKind::Sort,
                "group" => TokenKind::Group,
                "limit" => TokenKind::Limit,
                "count" => TokenKind::Count,
                "and" => TokenKind::And,
                "or" => TokenKind::Or,
                "not" => TokenKind::Not,
                "has" => TokenKind::Has,
                "asc" => TokenKind::Asc,
                "desc" => TokenKind::Desc,
                "true" => TokenKind::True,
                "false" => TokenKind::False,
                "none" => TokenKind::None,
                "today" | "yesterday" | "tomorrow" => TokenKind::Date,
                w if SOURCES.contains(&w) => TokenKind::Source,
                _ => TokenKind::Ident,
            };
            tokens.push(tok(kind, start, i, word));
            continue;
        }

        return Err(ParseError {
            message: format!(
                "unexpected character '{}'",
                input[i..].chars().next().unwrap()
            ),
            position: i,
        });
    }

    Ok(tokens)
}

fn tok(kind: TokenKind, start: usize, end: usize, text: &str) -> Token {
    Token {
        kind,
        span: Span { start, end },
        text: text.to_string(),
    }
}

// ---------------------------------------------------------------------------
// AST
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub source: Source,
    pub clauses: Vec<Clause>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Pages,
    Tasks,
    Journal,
    Blocks,
    Tags,
    Links,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Clause {
    Where(Expr),
    Sort(Vec<SortField>),
    Group(Field),
    Limit(u64),
    Count,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SortField {
    pub field: Field,
    pub desc: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub segments: Vec<String>, // e.g. ["backlinks", "count"] or ["due"]
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.segments.join("."))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Compare(Field, Op, Value),
    Has(Field, String),     // field has #tag
    InRange(Field, String), // field this_week / last_month / etc.
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Op {
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    String(String),
    Number(f64),
    Date(String), // ISO date or "today"/"yesterday"/"tomorrow"
    Bool(bool),
    None,
    Var(String), // "page", "today"
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<&Token> {
        let tok = self.tokens.get(self.pos);
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, kind: TokenKind) -> Result<&Token, ParseError> {
        match self.peek() {
            Some(tok) if tok.kind == kind => {
                self.pos += 1;
                Ok(&self.tokens[self.pos - 1])
            }
            Some(tok) => Err(ParseError {
                message: format!("expected {kind}, got {}", tok.kind),
                position: tok.span.start,
            }),
            None => Err(ParseError {
                message: format!("expected {kind}, got end of input"),
                position: self.tokens.last().map(|t| t.span.end).unwrap_or(0),
            }),
        }
    }

    fn current_position(&self) -> usize {
        self.peek()
            .map(|t| t.span.start)
            .unwrap_or_else(|| self.tokens.last().map(|t| t.span.end).unwrap_or(0))
    }

    fn parse_query(&mut self) -> Result<Query, ParseError> {
        let source_tok = self.expect(TokenKind::Source)?;
        let source = match source_tok.text.as_str() {
            "pages" => Source::Pages,
            "tasks" => Source::Tasks,
            "journal" => Source::Journal,
            "blocks" => Source::Blocks,
            "tags" => Source::Tags,
            "links" => Source::Links,
            _ => unreachable!(),
        };

        let mut clauses = Vec::new();
        while self.peek().is_some_and(|t| t.kind == TokenKind::Pipe) {
            self.advance(); // consume |
            let clause = self.parse_clause()?;

            // count must be last
            if let Some(prev) = clauses.last() {
                if matches!(prev, Clause::Count) {
                    return Err(ParseError {
                        message: "'count' must be the last clause".to_string(),
                        position: self.current_position(),
                    });
                }
            }

            clauses.push(clause);
        }

        if self.peek().is_some() {
            return Err(ParseError {
                message: format!("unexpected token '{}'", self.peek().unwrap().text),
                position: self.peek().unwrap().span.start,
            });
        }

        Ok(Query { source, clauses })
    }

    fn parse_clause(&mut self) -> Result<Clause, ParseError> {
        let tok = self.peek().ok_or_else(|| ParseError {
            message: "expected clause after '|'".to_string(),
            position: self.current_position(),
        })?;

        match tok.kind {
            TokenKind::Where => {
                self.advance();
                let expr = self.parse_expr()?;
                Ok(Clause::Where(expr))
            }
            TokenKind::Sort => {
                self.advance();
                let fields = self.parse_sort_fields()?;
                Ok(Clause::Sort(fields))
            }
            TokenKind::Group => {
                self.advance();
                let field = self.parse_field()?;
                Ok(Clause::Group(field))
            }
            TokenKind::Limit => {
                self.advance();
                let num_tok = self.expect(TokenKind::Number)?;
                let n: u64 = num_tok.text.parse().map_err(|_| ParseError {
                    message: "invalid limit number".to_string(),
                    position: num_tok.span.start,
                })?;
                Ok(Clause::Limit(n))
            }
            TokenKind::Count => {
                self.advance();
                Ok(Clause::Count)
            }
            _ => Err(ParseError {
                message: format!("expected clause keyword, got '{}'", tok.text),
                position: tok.span.start,
            }),
        }
    }

    fn parse_expr(&mut self) -> Result<Expr, ParseError> {
        self.parse_or()
    }

    /// Or has lower precedence: `a or b and c` parses as `a or (b and c)`.
    fn parse_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and()?;

        while self.peek().is_some_and(|t| t.kind == TokenKind::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::Or(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    /// And has higher precedence than or.
    fn parse_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pred()?;

        while self.peek().is_some_and(|t| t.kind == TokenKind::And) {
            self.advance();
            let right = self.parse_pred()?;
            left = Expr::And(Box::new(left), Box::new(right));
        }

        Ok(left)
    }

    fn parse_pred(&mut self) -> Result<Expr, ParseError> {
        // not ATOM
        if self.peek().is_some_and(|t| t.kind == TokenKind::Not) {
            self.advance();
            // Disallow double negation.
            if self.peek().is_some_and(|t| t.kind == TokenKind::Not) {
                return Err(ParseError {
                    message: "double negation is not allowed".to_string(),
                    position: self.peek().unwrap().span.start,
                });
            }
            let atom = self.parse_atom()?;
            return Ok(Expr::Not(Box::new(atom)));
        }
        self.parse_atom()
    }

    fn parse_atom(&mut self) -> Result<Expr, ParseError> {
        // ( expr )
        if self.peek().is_some_and(|t| t.kind == TokenKind::LParen) {
            self.advance();
            let expr = self.parse_expr()?;
            self.expect(TokenKind::RParen)?;
            return Ok(expr);
        }

        // field OP value | field has #tag | field RANGE
        let field = self.parse_field()?;

        // field has #tag
        if self.peek().is_some_and(|t| t.kind == TokenKind::Has) {
            self.advance();
            let tag_tok = self.expect(TokenKind::Tag)?;
            return Ok(Expr::Has(field, tag_tok.text.clone()));
        }

        // field RANGE (this week, last month, etc.)
        if self.peek().is_some_and(|t| t.kind == TokenKind::Range) {
            let range_tok = self.advance().unwrap();
            return Ok(Expr::InRange(field, range_tok.text.clone()));
        }

        // Bare boolean field: `done` or `not done` (no operator following).
        // Treated as `field = true`.
        let is_op = self.peek().is_some_and(|t| {
            matches!(
                t.kind,
                TokenKind::Eq
                    | TokenKind::Neq
                    | TokenKind::Lt
                    | TokenKind::Gt
                    | TokenKind::Lte
                    | TokenKind::Gte
            )
        });
        if !is_op {
            return Ok(Expr::Compare(field, Op::Eq, Value::Bool(true)));
        }

        // field OP value
        let op = self.parse_op()?;
        let value = self.parse_value()?;

        Ok(Expr::Compare(field, op, value))
    }

    fn parse_field(&mut self) -> Result<Field, ParseError> {
        let first = self.expect_field_name()?;
        let mut segments = vec![first];

        while self.peek().is_some_and(|t| t.kind == TokenKind::Dot) {
            self.advance(); // consume .
            let next = self.expect_field_name()?;
            segments.push(next);
        }

        Ok(Field { segments })
    }

    /// Accept an identifier or a keyword that can also be a field name
    /// (e.g., `tags`, `count`, `links`, `text`, `date`).
    fn expect_field_name(&mut self) -> Result<String, ParseError> {
        let tok = self.peek().ok_or_else(|| ParseError {
            message: "expected field name".to_string(),
            position: self.current_position(),
        })?;
        match tok.kind {
            TokenKind::Ident
            | TokenKind::Source
            | TokenKind::Count
            | TokenKind::Date
            | TokenKind::True
            | TokenKind::False => {
                let text = tok.text.clone();
                self.advance();
                Ok(text)
            }
            _ => Err(ParseError {
                message: format!("expected field name, got {}", tok.kind),
                position: tok.span.start,
            }),
        }
    }

    fn parse_op(&mut self) -> Result<Op, ParseError> {
        let tok = self.peek().ok_or_else(|| ParseError {
            message: "expected operator".to_string(),
            position: self.current_position(),
        })?;
        let op = match tok.kind {
            TokenKind::Eq => Op::Eq,
            TokenKind::Neq => Op::Neq,
            TokenKind::Lt => Op::Lt,
            TokenKind::Gt => Op::Gt,
            TokenKind::Lte => Op::Lte,
            TokenKind::Gte => Op::Gte,
            _ => {
                return Err(ParseError {
                    message: format!("expected operator, got '{}'", tok.text),
                    position: tok.span.start,
                });
            }
        };
        self.advance();
        Ok(op)
    }

    fn parse_value(&mut self) -> Result<Value, ParseError> {
        let tok = self.peek().ok_or_else(|| ParseError {
            message: "expected value".to_string(),
            position: self.current_position(),
        })?;
        let val = match tok.kind {
            TokenKind::String => Value::String(tok.text.clone()),
            TokenKind::Number => {
                let n: f64 = tok.text.parse().map_err(|_| ParseError {
                    message: "invalid number".to_string(),
                    position: tok.span.start,
                })?;
                Value::Number(n)
            }
            TokenKind::Date => Value::Date(tok.text.clone()),
            TokenKind::True => Value::Bool(true),
            TokenKind::False => Value::Bool(false),
            TokenKind::None => Value::None,
            TokenKind::Var => Value::Var(tok.text.clone()),
            _ => {
                return Err(ParseError {
                    message: format!("expected value, got '{}'", tok.text),
                    position: tok.span.start,
                });
            }
        };
        self.advance();
        Ok(val)
    }

    fn parse_sort_fields(&mut self) -> Result<Vec<SortField>, ParseError> {
        let mut fields = vec![self.parse_sort_field()?];
        while self.peek().is_some_and(|t| t.kind == TokenKind::Comma) {
            self.advance(); // consume ,
            fields.push(self.parse_sort_field()?);
        }
        Ok(fields)
    }

    fn parse_sort_field(&mut self) -> Result<SortField, ParseError> {
        let field = self.parse_field()?;
        let desc = if self.peek().is_some_and(|t| t.kind == TokenKind::Desc) {
            self.advance();
            true
        } else if self.peek().is_some_and(|t| t.kind == TokenKind::Asc) {
            self.advance();
            false
        } else {
            false
        };
        Ok(SortField { field, desc })
    }
}

/// Parse a BQL query string into an AST.
pub fn parse(input: &str) -> Result<Query, ParseError> {
    let tokens = tokenise(input)?;
    if tokens.is_empty() {
        return Err(ParseError {
            message: "empty query".to_string(),
            position: 0,
        });
    }
    let mut parser = Parser::new(tokens);
    parser.parse_query()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Tokeniser tests --

    #[test]
    fn tokenise_simple() {
        let tokens = tokenise("tasks").unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Source);
        assert_eq!(tokens[0].text, "tasks");
    }

    #[test]
    fn tokenise_pipe_and_keywords() {
        let tokens = tokenise("tasks | where not done | sort due desc").unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Source,
                &TokenKind::Pipe,
                &TokenKind::Where,
                &TokenKind::Not,
                &TokenKind::Ident,
                &TokenKind::Pipe,
                &TokenKind::Sort,
                &TokenKind::Ident,
                &TokenKind::Desc,
            ]
        );
    }

    #[test]
    fn tokenise_operators() {
        let tokens = tokenise("due < today").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Lt);
        let tokens = tokenise("due != none").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Neq);
        let tokens = tokenise("count >= 5").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Gte);
    }

    #[test]
    fn tokenise_string() {
        let tokens = tokenise(r#"title = "Hello World""#).unwrap();
        assert_eq!(tokens[2].kind, TokenKind::String);
        assert_eq!(tokens[2].text, "Hello World");
    }

    #[test]
    fn tokenise_tag() {
        let tokens = tokenise("tags has #rust").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::Tag);
        assert_eq!(tokens[2].text, "rust");
    }

    #[test]
    fn tokenise_tag_with_hyphen() {
        let tokens = tokenise("tags has #data-structures").unwrap();
        assert_eq!(tokens[2].text, "data-structures");
    }

    #[test]
    fn tokenise_variable() {
        let tokens = tokenise("to = $page").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::Var);
        assert_eq!(tokens[2].text, "page");
    }

    #[test]
    fn tokenise_date() {
        let tokens = tokenise("due < 2026-03-08").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::Date);
        assert_eq!(tokens[2].text, "2026-03-08");
    }

    #[test]
    fn tokenise_date_keyword() {
        let tokens = tokenise("due < today").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::Date);
        assert_eq!(tokens[2].text, "today");
    }

    #[test]
    fn tokenise_range() {
        let tokens = tokenise("due this week").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Range);
        assert_eq!(tokens[1].text, "this week");
    }

    #[test]
    fn tokenise_range_last_month() {
        let tokens = tokenise("created last month").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Range);
        assert_eq!(tokens[1].text, "last month");
    }

    #[test]
    fn tokenise_number() {
        let tokens = tokenise("limit 20").unwrap();
        assert_eq!(tokens[1].kind, TokenKind::Number);
        assert_eq!(tokens[1].text, "20");
    }

    #[test]
    fn tokenise_dotted_field() {
        let tokens = tokenise("backlinks.count = 0").unwrap();
        let kinds: Vec<_> = tokens.iter().map(|t| &t.kind).collect();
        // `count` tokenises as Count keyword; the parser accepts it as a field name.
        assert_eq!(
            kinds,
            vec![
                &TokenKind::Ident,
                &TokenKind::Dot,
                &TokenKind::Count,
                &TokenKind::Eq,
                &TokenKind::Number
            ]
        );
    }

    #[test]
    fn tokenise_unterminated_string() {
        let err = tokenise(r#"title = "hello"#).unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    // -- Parser tests --

    #[test]
    fn parse_source_only() {
        let q = parse("tasks").unwrap();
        assert_eq!(q.source, Source::Tasks);
        assert!(q.clauses.is_empty());
    }

    #[test]
    fn parse_where_not_done() {
        let q = parse("tasks | where not done").unwrap();
        assert_eq!(q.source, Source::Tasks);
        assert_eq!(q.clauses.len(), 1);
        match &q.clauses[0] {
            Clause::Where(Expr::Not(inner)) => match inner.as_ref() {
                Expr::Compare(f, Op::Eq, Value::Bool(true)) => {
                    assert_eq!(f.segments, vec!["done"]);
                }
                _ => panic!("expected bare field 'done' parsed as compare with true"),
            },
            _ => panic!("expected Where(Not(...))"),
        }
    }

    #[test]
    fn parse_where_and_or() {
        let q = parse("tasks | where not done and due < today").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::And(left, right)) => {
                assert!(matches!(left.as_ref(), Expr::Not(_)));
                assert!(matches!(
                    right.as_ref(),
                    Expr::Compare(_, Op::Lt, Value::Date(_))
                ));
            }
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn parse_where_with_parens() {
        let q = parse("tasks | where not done and (tags has #work or tags has #urgent)").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::And(_, right)) => {
                assert!(matches!(right.as_ref(), Expr::Or(_, _)));
            }
            other => panic!("expected And(..., Or(...)), got {other:?}"),
        }
    }

    #[test]
    fn parse_sort() {
        let q = parse("pages | sort created desc, title").unwrap();
        match &q.clauses[0] {
            Clause::Sort(fields) => {
                assert_eq!(fields.len(), 2);
                assert_eq!(fields[0].field.segments, vec!["created"]);
                assert!(fields[0].desc);
                assert_eq!(fields[1].field.segments, vec!["title"]);
                assert!(!fields[1].desc);
            }
            other => panic!("expected Sort, got {other:?}"),
        }
    }

    #[test]
    fn parse_group() {
        let q = parse("tasks | where not done | group page").unwrap();
        assert_eq!(q.clauses.len(), 2);
        match &q.clauses[1] {
            Clause::Group(f) => assert_eq!(f.segments, vec!["page"]),
            other => panic!("expected Group, got {other:?}"),
        }
    }

    #[test]
    fn parse_limit() {
        let q = parse("pages | sort created desc | limit 20").unwrap();
        assert!(matches!(q.clauses[1], Clause::Limit(20)));
    }

    #[test]
    fn parse_count() {
        let q = parse("tasks | where not done | count").unwrap();
        assert!(matches!(q.clauses[1], Clause::Count));
    }

    #[test]
    fn parse_count_must_be_last() {
        let err = parse("tasks | count | sort due").unwrap_err();
        assert!(err.message.contains("last"));
    }

    #[test]
    fn parse_dotted_field() {
        let q = parse("pages | where backlinks.count = 0").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::Compare(f, Op::Eq, Value::Number(n))) => {
                assert_eq!(f.segments, vec!["backlinks", "count"]);
                assert_eq!(*n, 0.0);
            }
            other => panic!("expected Compare with dotted field, got {other:?}"),
        }
    }

    #[test]
    fn parse_has_tag() {
        let q = parse("pages | where tags has #rust").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::Has(f, tag)) => {
                assert_eq!(f.segments, vec!["tags"]);
                assert_eq!(tag, "rust");
            }
            other => panic!("expected Has, got {other:?}"),
        }
    }

    #[test]
    fn parse_range() {
        let q = parse("tasks | where due this week").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::InRange(f, range)) => {
                assert_eq!(f.segments, vec!["due"]);
                assert_eq!(range, "this week");
            }
            other => panic!("expected InRange, got {other:?}"),
        }
    }

    #[test]
    fn parse_none_value() {
        let q = parse("tasks | where due = none").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::Compare(_, Op::Eq, Value::None)) => {}
            other => panic!("expected Compare with None, got {other:?}"),
        }
    }

    #[test]
    fn parse_variable() {
        let q = parse("links | where to = $page").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::Compare(_, Op::Eq, Value::Var(v))) => {
                assert_eq!(v, "page");
            }
            other => panic!("expected Compare with Var, got {other:?}"),
        }
    }

    #[test]
    fn parse_double_negation_error() {
        let err = parse("tasks | where not not done").unwrap_err();
        assert!(err.message.contains("double negation"));
    }

    #[test]
    fn parse_empty_error() {
        let err = parse("").unwrap_err();
        assert!(err.message.contains("empty"));
    }

    #[test]
    fn parse_complex_query() {
        let q = parse(
            "tasks | where not done and due this week and tags has #work | sort due | limit 10",
        )
        .unwrap();
        assert_eq!(q.source, Source::Tasks);
        assert_eq!(q.clauses.len(), 3);
        assert!(matches!(q.clauses[0], Clause::Where(_)));
        assert!(matches!(q.clauses[1], Clause::Sort(_)));
        assert!(matches!(q.clauses[2], Clause::Limit(10)));
    }

    #[test]
    fn parse_all_sources() {
        for source in &["pages", "tasks", "journal", "blocks", "tags", "links"] {
            let q = parse(source).unwrap();
            assert_eq!(q.clauses.len(), 0);
        }
    }

    #[test]
    fn parse_group_with_count() {
        let q = parse("tasks | where not done | group page | count").unwrap();
        assert_eq!(q.clauses.len(), 3);
        assert!(matches!(q.clauses[1], Clause::Group(_)));
        assert!(matches!(q.clauses[2], Clause::Count));
    }

    #[test]
    fn parse_and_binds_tighter_than_or() {
        // `a or b and c` should parse as `a or (b and c)`, not `(a or b) and c`.
        let q = parse("tasks | where done or tags has #work and due < today").unwrap();
        match &q.clauses[0] {
            Clause::Where(Expr::Or(left, right)) => {
                // left = done (bare field)
                assert!(matches!(
                    left.as_ref(),
                    Expr::Compare(_, Op::Eq, Value::Bool(true))
                ));
                // right = tags has #work and due < today
                assert!(matches!(right.as_ref(), Expr::And(_, _)));
            }
            other => panic!("expected Or(done, And(...)), got {other:?}"),
        }
    }

    #[test]
    fn tokenise_unicode_tag() {
        let tokens = tokenise("tags has #café").unwrap();
        assert_eq!(tokens[2].kind, TokenKind::Tag);
        assert_eq!(tokens[2].text, "café");
    }
}
