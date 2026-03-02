use serde::{Deserialize, Serialize};

/// 16-char hex UUID for pages, sections, and blocks (64-bit entropy).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BloomId(pub String);

impl BloomId {
    pub fn new() -> Self {
        let bytes: [u8; 8] = rand::random();
        BloomId(hex::encode(&bytes))
    }
}

impl std::str::FromStr for BloomId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(BloomId(s.to_string()))
    }
}

impl std::fmt::Display for BloomId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// Inline hex encoding to avoid an extra dependency
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// YAML frontmatter for a Bloom page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Frontmatter {
    pub id: String,
    pub title: String,
    #[serde(default = "chrono::Utc::now")]
    pub created: chrono::DateTime<chrono::Utc>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Arbitrary extra keys preserved from Logseq import or user additions.
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_yaml::Value>,
}

impl Frontmatter {
    pub fn new(title: &str) -> Self {
        let id = BloomId::new();
        Self {
            id: id.0,
            title: title.to_string(),
            created: chrono::Utc::now(),
            tags: Vec::new(),
            extra: std::collections::HashMap::new(),
        }
    }
}

/// A parsed Bloom document.
#[derive(Debug, Clone)]
pub struct Document {
    pub frontmatter: Frontmatter,
    pub blocks: Vec<Block>,
}

/// A single block in a document (paragraph, heading, list item, code block, etc.).
#[derive(Debug, Clone)]
pub struct Block {
    pub kind: BlockKind,
    pub content: String,
    pub id: Option<String>,
    pub links: Vec<Link>,
    pub embeds: Vec<Embed>,
    pub tags: Vec<Tag>,
    pub timestamps: Vec<Timestamp>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BlockKind {
    Heading { level: u8 },
    Paragraph,
    ListItem { checked: Option<bool> },
    CodeBlock { language: Option<String> },
    BlockQuote,
}

/// A `[[page-id|display]]` or `[[page-id#sub-id|display]]` link.
#[derive(Debug, Clone)]
pub struct Link {
    pub page_id: String,
    pub sub_id: Option<String>,
    pub display: Option<String>,
    /// Byte offset within the block content.
    pub span: Span,
}

/// A `![[page-id|display]]` embed.
#[derive(Debug, Clone)]
pub struct Embed {
    pub page_id: String,
    pub sub_id: Option<String>,
    pub display: Option<String>,
    pub span: Span,
}

/// An inline `#tag`.
#[derive(Debug, Clone)]
pub struct Tag {
    pub name: String,
    pub span: Span,
}

/// A `@due(...)`, `@start(...)`, or `@at(...)` timestamp.
#[derive(Debug, Clone)]
pub struct Timestamp {
    pub kind: TimestampKind,
    pub date: chrono::NaiveDate,
    pub time: Option<chrono::NaiveTime>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TimestampKind {
    Due,
    Start,
    At,
}

/// Byte offset range within text.
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bloom_id_is_16_chars() {
        let id = BloomId::new();
        assert_eq!(id.0.len(), 16);
        assert!(id.0.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn frontmatter_roundtrip() {
        let fm = Frontmatter::new("Test Page");
        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: Frontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(parsed.title, "Test Page");
        assert_eq!(parsed.id.len(), 16);
    }
}
