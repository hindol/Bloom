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