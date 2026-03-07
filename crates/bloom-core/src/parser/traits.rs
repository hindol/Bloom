use std::collections::HashMap;
use std::ops::Range;

use chrono::NaiveDate;

use crate::types::{BlockId, PageId, TagName, Timestamp};

// --- Document types ---

#[derive(Debug, Clone)]
pub struct Document {
    pub frontmatter: Option<Frontmatter>,
    pub sections: Vec<Section>,
    pub links: Vec<ParsedLink>,
    pub tags: Vec<ParsedTag>,
    pub tasks: Vec<ParsedTask>,
    pub timestamps: Vec<ParsedTimestamp>,
    pub block_ids: Vec<ParsedBlockId>,
}

#[derive(Debug, Clone)]
pub struct Frontmatter {
    pub id: Option<PageId>,
    pub title: Option<String>,
    pub created: Option<NaiveDate>,
    pub tags: Vec<TagName>,
    pub extra: HashMap<String, serde_yaml::Value>,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub level: u8,
    pub title: String,
    pub block_id: Option<BlockId>,
    pub line_range: Range<usize>,
}

// --- Parsed element types ---

#[derive(Debug, Clone)]
pub struct ParsedLink {
    pub target: PageId,
    pub section: Option<BlockId>,
    pub display_hint: String,
    pub line: usize,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct ParsedTag {
    pub name: TagName,
    pub line: usize,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct ParsedTask {
    pub text: String,
    pub done: bool,
    pub timestamps: Vec<Timestamp>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct ParsedTimestamp {
    pub timestamp: Timestamp,
    pub line: usize,
    pub byte_range: Range<usize>,
}

#[derive(Debug, Clone)]
pub struct ParsedBlockId {
    pub id: BlockId,
    pub line: usize,
}

// --- Highlighting types ---

#[derive(Debug, Clone, PartialEq)]
pub struct StyledSpan {
    pub range: Range<usize>,
    pub style: Style,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Style {
    Normal,
    Heading { level: u8 },
    Bold,
    Italic,
    Code,
    CodeBlock,
    LinkText,
    LinkChrome,
    Tag,
    TimestampKeyword,
    TimestampDate,
    TimestampOverdue,
    TimestampParens,
    BlockId,
    BlockIdCaret,
    ListMarker,
    CheckboxUnchecked,
    CheckboxChecked,
    CheckedTaskText,
    Blockquote,
    BlockquoteMarker,
    TablePipe,
    TableAlignmentRow,
    Frontmatter,
    FrontmatterKey,
    FrontmatterTitle,
    FrontmatterId,
    FrontmatterDate,
    FrontmatterTags,
    BrokenLink,
    SyntaxNoise,
    SearchMatch,
    SearchMatchCurrent,
}

#[derive(Debug, Clone, Default)]
pub struct LineContext {
    pub in_code_block: bool,
    pub in_frontmatter: bool,
    pub code_fence_lang: Option<String>,
}

// --- Trait ---

pub trait DocumentParser: Send + Sync {
    fn parse(&self, text: &str) -> Document;
    fn parse_frontmatter(&self, text: &str) -> Option<Frontmatter>;
    fn highlight_line(&self, line: &str, context: &LineContext) -> Vec<StyledSpan>;
    fn serialize_frontmatter(&self, fm: &Frontmatter) -> String;
}
