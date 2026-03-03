use crate::types::BlockId;

use super::extensions::{parse_block_id, parse_links, parse_tags, parse_task, parse_timestamps};
use super::frontmatter;
use super::highlight;
use super::traits::{
    Document, DocumentParser, Frontmatter, LineContext, Section, StyledSpan,
};

/// The concrete Bloom Markdown parser.
pub struct BloomMarkdownParser;

impl BloomMarkdownParser {
    pub fn new() -> Self {
        BloomMarkdownParser
    }
}

impl Default for BloomMarkdownParser {
    fn default() -> Self {
        Self::new()
    }
}

impl DocumentParser for BloomMarkdownParser {
    fn parse(&self, text: &str) -> Document {
        let fm = frontmatter::parse_frontmatter(text);
        let body_start = if fm.is_some() {
            frontmatter::extract_frontmatter_text(text)
                .map(|(_, line)| line)
                .unwrap_or(0)
        } else {
            0
        };

        let lines: Vec<&str> = text.lines().collect();
        let mut sections = Vec::new();
        let mut links = Vec::new();
        let mut tags = Vec::new();
        let mut tasks = Vec::new();
        let mut timestamps = Vec::new();
        let mut block_ids = Vec::new();

        let mut in_code_block = false;
        let mut current_section_start: Option<(u8, String, Option<BlockId>, usize)> = None;

        for (line_idx, &line) in lines.iter().enumerate() {
            // Track frontmatter region (skip for extension parsing)
            if line_idx < body_start {
                continue;
            }

            // Track code fences
            let trimmed = line.trim_start();
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                continue;
            }

            if in_code_block {
                continue;
            }

            // Parse heading
            if let Some(heading) = parse_heading(line) {
                // Close the previous section
                if let Some((level, title, block_id, start_line)) = current_section_start.take() {
                    sections.push(Section {
                        level,
                        title,
                        block_id,
                        line_range: start_line..line_idx,
                    });
                }
                let block = parse_block_id(line, line_idx);
                current_section_start = Some((
                    heading.0,
                    heading.1.to_string(),
                    block.as_ref().map(|b| b.id.clone()),
                    line_idx,
                ));
                if let Some(b) = block {
                    block_ids.push(b);
                }
                continue;
            }

            // Parse extensions
            links.extend(parse_links(line, line_idx));
            tags.extend(parse_tags(line, line_idx));
            timestamps.extend(parse_timestamps(line, line_idx));

            if let Some(task) = parse_task(line, line_idx) {
                tasks.push(task);
            }

            if let Some(bid) = parse_block_id(line, line_idx) {
                block_ids.push(bid);
            }
        }

        // Close last section
        if let Some((level, title, block_id, start_line)) = current_section_start {
            sections.push(Section {
                level,
                title,
                block_id,
                line_range: start_line..lines.len(),
            });
        }

        Document {
            frontmatter: fm,
            sections,
            links,
            tags,
            tasks,
            timestamps,
            block_ids,
        }
    }

    fn parse_frontmatter(&self, text: &str) -> Option<Frontmatter> {
        frontmatter::parse_frontmatter(text)
    }

    fn highlight_line(&self, line: &str, context: &LineContext) -> Vec<StyledSpan> {
        highlight::highlight_line(line, context)
    }

    fn serialize_frontmatter(&self, fm: &Frontmatter) -> String {
        frontmatter::serialize_frontmatter(fm)
    }
}

/// Parse a heading line, returning (level, title text).
fn parse_heading(line: &str) -> Option<(u8, &str)> {
    let bytes = line.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' {
        return None;
    }
    let level = bytes.iter().take_while(|&&b| b == b'#').count();
    if level > 6 || level >= line.len() || bytes[level] != b' ' {
        return None;
    }
    Some((level as u8, line[level + 1..].trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::traits::{DocumentParser, LineContext, Style};
    use crate::types::{BlockId, Timestamp};

    #[test]
    fn test_parse_full_document() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Test\"\ncreated: 2026-01-01\ntags: [rust]\n---\n\n# Introduction\n\nHello #world. See [[abcd1234|Other]] page.\n\n- [ ] Task @due(2026-03-05)\n\nSome text ^block-1\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert!(doc.frontmatter.is_some());
        assert!(!doc.sections.is_empty());
        assert!(!doc.links.is_empty());
        assert!(!doc.tags.is_empty());
        assert!(!doc.tasks.is_empty());
        assert!(!doc.timestamps.is_empty());
        assert!(!doc.block_ids.is_empty());
    }

    #[test]
    fn test_parse_document_without_frontmatter() {
        let text = "# Just a heading\n\nSome body text.\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert!(doc.frontmatter.is_none());
        assert_eq!(doc.sections.len(), 1);
        assert_eq!(doc.sections[0].title, "Just a heading");
    }

    #[test]
    fn test_parse_document_multiple_sections() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n# Section 1\n\nText\n\n## Section 2\n\nMore text\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.sections.len(), 2);
        assert_eq!(doc.sections[0].level, 1);
        assert_eq!(doc.sections[1].level, 2);
    }

    // FILE_FORMAT.md: Extensions ignored in code blocks
    #[test]
    fn test_links_not_parsed_in_code_block() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n```\n[[aabbccdd|Link]]\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.links.len(), 0);
    }

    #[test]
    fn test_tags_not_parsed_in_code_block() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n```\n#not-a-tag\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.tags.len(), 0);
    }

    #[test]
    fn test_timestamps_not_parsed_in_code_block() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n```\n@due(2026-01-01)\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.timestamps.len(), 0);
    }

    #[test]
    fn test_tasks_not_parsed_in_code_block() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n```\n- [ ] Not a task\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.tasks.len(), 0);
    }

    #[test]
    fn test_document_task_details() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\n- [ ] Open task\n- [x] Done task\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.tasks.len(), 2);
        assert!(!doc.tasks[0].done);
        assert!(doc.tasks[1].done);
    }

    #[test]
    fn test_document_links_with_sections() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\nSee [[abcd1234#intro|Page]]\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.links.len(), 1);
        assert_eq!(
            doc.links[0].section,
            Some(BlockId("intro".to_string()))
        );
    }

    #[test]
    fn test_parse_frontmatter_via_parser() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Via Parser\"\ncreated: 2026-06-15\ntags: [test]\n---\n";
        let parser = BloomMarkdownParser::new();
        let fm = parser.parse_frontmatter(text).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Via Parser"));
    }

    #[test]
    fn test_highlight_line_via_parser() {
        let parser = BloomMarkdownParser::new();
        let ctx = LineContext::default();
        let spans = parser.highlight_line("## Heading", &ctx);
        assert!(spans
            .iter()
            .any(|s| s.style == Style::Heading { level: 2 }));
    }

    #[test]
    fn test_serialize_frontmatter_via_parser() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Round\"\n---\n";
        let parser = BloomMarkdownParser::new();
        let fm = parser.parse_frontmatter(text).unwrap();
        let serialized = parser.serialize_frontmatter(&fm);
        assert!(serialized.starts_with("---"));
        assert!(serialized.ends_with("---"));
        assert!(serialized.contains("8f3a1b2c"));
    }

    #[test]
    fn test_parse_heading_helper() {
        assert_eq!(parse_heading("# Title"), Some((1, "Title")));
        assert_eq!(parse_heading("### Sub"), Some((3, "Sub")));
        assert!(parse_heading("Not a heading").is_none());
        assert!(parse_heading("").is_none());
        assert!(parse_heading("##NoSpace").is_none());
    }

    #[test]
    fn test_block_ids_in_document() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\nSome text ^block-1\nMore ^block-2\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.block_ids.len(), 2);
        assert_eq!(doc.block_ids[0].id, BlockId("block-1".to_string()));
        assert_eq!(doc.block_ids[1].id, BlockId("block-2".to_string()));
    }

    #[test]
    fn test_document_timestamps() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\nDo it @due(2026-03-05) and @start(2026-03-01)\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.timestamps.len(), 2);
        assert!(matches!(doc.timestamps[0].timestamp, Timestamp::Due(_)));
        assert!(matches!(doc.timestamps[1].timestamp, Timestamp::Start(_)));
    }

    #[test]
    fn test_empty_document() {
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse("");
        assert!(doc.frontmatter.is_none());
        assert!(doc.sections.is_empty());
        assert!(doc.links.is_empty());
        assert!(doc.tags.is_empty());
    }
}