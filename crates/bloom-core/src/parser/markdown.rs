use crate::types::BlockId;

use super::extensions::{parse_block_id, parse_links, parse_tags, parse_task, parse_timestamps};
use super::frontmatter;
use super::highlight;
use super::traits::{Document, DocumentParser, Frontmatter, LineContext, Section, StyledSpan};

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
        let mut blocks = Vec::new();
        let mut bql_blocks = Vec::new();

        let mut in_code_block = false;
        let mut bql_fence_start: Option<usize> = None; // line of opening ```bql
        let mut bql_query_lines: Vec<&str> = Vec::new();
        let mut current_section_start: Option<(u8, String, Option<BlockId>, usize)> = None;

        // Block tracking state: (first_line, last_line, has_id)
        let mut block_start: Option<(usize, usize, bool)> = None;

        // Flush the current block into `blocks`.
        let flush_block =
            |block_start: &mut Option<(usize, usize, bool)>, blocks: &mut Vec<super::traits::ParsedBlock>| {
                if let Some((first, last, has_id)) = block_start.take() {
                    blocks.push(super::traits::ParsedBlock {
                        first_line: first,
                        last_line: last,
                        has_id,
                    });
                }
            };

        let mut line_idx = body_start;
        while line_idx < lines.len() {
            let line = lines[line_idx];
            let trimmed = line.trim_start();

            // Track code fences — flush any open block, skip contents.
            if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
                flush_block(&mut block_start, &mut blocks);
                if in_code_block {
                    // Closing fence — check if this was a BQL block.
                    if let Some(fence_start) = bql_fence_start.take() {
                        let query = bql_query_lines.join("\n").trim().to_string();
                        if !query.is_empty() {
                            bql_blocks.push(super::traits::BqlBlock {
                                query,
                                fence_start,
                                fence_end: line_idx,
                            });
                        }
                        bql_query_lines.clear();
                    }
                    in_code_block = false;
                } else {
                    // Opening fence — check for ```bql language hint.
                    in_code_block = true;
                    let lang = trimmed.trim_start_matches('`').trim_start_matches('~').trim();
                    if lang.eq_ignore_ascii_case("bql") {
                        bql_fence_start = Some(line_idx);
                    }
                }
                line_idx += 1;
                continue;
            }

            if in_code_block {
                // Collect BQL query lines if inside a ```bql block.
                if bql_fence_start.is_some() {
                    bql_query_lines.push(line);
                }
                line_idx += 1;
                continue;
            }

            let trimmed_end = line.trim();

            // Blank lines and horizontal rules end the current block.
            if trimmed_end.is_empty() || is_horizontal_rule(trimmed_end) {
                flush_block(&mut block_start, &mut blocks);
                line_idx += 1;
                continue;
            }

            let line_bid = parse_block_id(line, line_idx);
            let has_bid = line_bid.is_some();

            // Parse heading — always a single-line block.
            if let Some(heading) = parse_heading(line) {
                flush_block(&mut block_start, &mut blocks);

                // Section tracking (existing logic).
                if let Some((level, title, block_id, start_line)) = current_section_start.take() {
                    sections.push(Section {
                        level,
                        title,
                        block_id,
                        line_range: start_line..line_idx,
                    });
                }
                current_section_start = Some((
                    heading.0,
                    heading.1.to_string(),
                    line_bid.as_ref().map(|b| b.id.clone()),
                    line_idx,
                ));
                if let Some(b) = line_bid {
                    block_ids.push(b);
                }

                // Heading = single-line block.
                blocks.push(super::traits::ParsedBlock {
                    first_line: line_idx,
                    last_line: line_idx,
                    has_id: has_bid,
                });

                line_idx += 1;
                continue;
            }

            // Parse extensions (always, for any non-heading body line).
            links.extend(parse_links(line, line_idx));
            tags.extend(parse_tags(line, line_idx));
            timestamps.extend(parse_timestamps(line, line_idx));
            if let Some(task) = parse_task(line, line_idx) {
                tasks.push(task);
            }
            if let Some(b) = line_bid {
                block_ids.push(b);
            }

            // Blockquote block: consecutive `>` lines.
            if trimmed_end.starts_with('>') {
                if let Some((_, _, _)) = &block_start {
                    // We were in a different block type — flush it.
                    if !is_blockquote_block(&block_start, &lines) {
                        flush_block(&mut block_start, &mut blocks);
                    }
                }
                match &mut block_start {
                    Some((_, last, hid)) if is_blockquote_line(&lines, *last) => {
                        // Continue existing blockquote.
                        *last = line_idx;
                        *hid = *hid || has_bid;
                    }
                    _ => {
                        flush_block(&mut block_start, &mut blocks);
                        block_start = Some((line_idx, line_idx, has_bid));
                    }
                }
                line_idx += 1;
                continue;
            }

            // List item: starts a new block. Continuation lines (indented past
            // the marker, not themselves list items) extend the block.
            if is_list_item(trimmed_end) {
                flush_block(&mut block_start, &mut blocks);
                let indent = line.len() - trimmed.len();
                let mw = list_marker_width(trimmed_end);
                let content_indent = indent + mw;
                let mut last = line_idx;
                let mut hid = has_bid;

                // Scan ahead for continuation lines.
                let mut j = line_idx + 1;
                while j < lines.len() {
                    let next = lines[j];
                    let next_trimmed = next.trim();
                    if next_trimmed.is_empty() {
                        break;
                    }
                    let next_indent = next.len() - next.trim_start().len();
                    if next_indent >= content_indent && !is_list_item(next_trimmed) {
                        // Continuation: parse extensions for this line too.
                        links.extend(parse_links(next, j));
                        tags.extend(parse_tags(next, j));
                        timestamps.extend(parse_timestamps(next, j));
                        let next_bid = parse_block_id(next, j);
                        if let Some(b) = &next_bid {
                            block_ids.push(b.clone());
                        }
                        hid = hid || next_bid.is_some();
                        last = j;
                        j += 1;
                    } else {
                        break;
                    }
                }

                blocks.push(super::traits::ParsedBlock {
                    first_line: line_idx,
                    last_line: last,
                    has_id: hid,
                });
                line_idx = j;
                continue;
            }

            // Paragraph: extend current block or start a new one.
            match &mut block_start {
                Some((_, last, hid)) => {
                    *last = line_idx;
                    *hid = *hid || has_bid;
                }
                None => {
                    block_start = Some((line_idx, line_idx, has_bid));
                }
            }

            line_idx += 1;
        }

        // Flush any remaining block.
        flush_block(&mut block_start, &mut blocks);

        // Close last section.
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
            blocks,
            bql_blocks,
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

fn is_list_item(trimmed: &str) -> bool {
    list_marker_width(trimmed) > 0
}

fn list_marker_width(trimmed: &str) -> usize {
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && (bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+')
        && bytes[1] == b' '
    {
        return 2;
    }
    let digit_count = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digit_count > 0 && bytes.len() > digit_count + 1 {
        let delim = bytes[digit_count];
        if (delim == b'.' || delim == b')') && bytes.get(digit_count + 1) == Some(&b' ') {
            return digit_count + 2;
        }
    }
    0
}

fn is_horizontal_rule(trimmed: &str) -> bool {
    if trimmed.len() < 3 {
        return false;
    }
    let chars: Vec<char> = trimmed.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() < 3 {
        return false;
    }
    let first = chars[0];
    (first == '-' || first == '*' || first == '_') && chars.iter().all(|&c| c == first)
}

fn is_blockquote_line(lines: &[&str], idx: usize) -> bool {
    lines.get(idx).map_or(false, |l| l.trim().starts_with('>'))
}

fn is_blockquote_block(
    block_start: &Option<(usize, usize, bool)>,
    lines: &[&str],
) -> bool {
    block_start
        .as_ref()
        .map_or(false, |(first, _, _)| is_blockquote_line(lines, *first))
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
        let text = "---\nid: 8f3a1b2c\ntitle: \"T\"\n---\n\nSee [[abcd1234^intro|Page]]\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.links.len(), 1);
        assert_eq!(doc.links[0].section, Some(BlockId("intro".to_string())));
    }

    #[test]
    fn test_parse_frontmatter_via_parser() {
        let text =
            "---\nid: 8f3a1b2c\ntitle: \"Via Parser\"\ncreated: 2026-06-15\ntags: [test]\n---\n";
        let parser = BloomMarkdownParser::new();
        let fm = parser.parse_frontmatter(text).unwrap();
        assert_eq!(fm.title.as_deref(), Some("Via Parser"));
    }

    #[test]
    fn test_highlight_line_via_parser() {
        let parser = BloomMarkdownParser::new();
        let ctx = LineContext::default();
        let spans = parser.highlight_line("## Heading", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Heading { level: 2 }));
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

    #[test]
    fn test_bql_block_detection() {
        let text = "# Dashboard\n\n```bql\ntasks | where not done\n```\n\nSome text\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.bql_blocks.len(), 1);
        assert_eq!(doc.bql_blocks[0].query, "tasks | where not done");
        assert_eq!(doc.bql_blocks[0].fence_start, 2);
        assert_eq!(doc.bql_blocks[0].fence_end, 4);
    }

    #[test]
    fn test_bql_block_multiple() {
        let text = "```bql\ntasks\n```\n\ntext\n\n```bql\npages | sort title\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.bql_blocks.len(), 2);
        assert_eq!(doc.bql_blocks[0].query, "tasks");
        assert_eq!(doc.bql_blocks[1].query, "pages | sort title");
    }

    #[test]
    fn test_bql_block_not_regular_code() {
        let text = "```rust\nfn main() {}\n```\n\n```bql\ntasks\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.bql_blocks.len(), 1);
        assert_eq!(doc.bql_blocks[0].query, "tasks");
    }

    #[test]
    fn test_bql_block_multiline_query() {
        let text = "```bql\ntasks | where not done\n  and due < today\n```\n";
        let parser = BloomMarkdownParser::new();
        let doc = parser.parse(text);
        assert_eq!(doc.bql_blocks.len(), 1);
        assert!(doc.bql_blocks[0].query.contains("not done"));
        assert!(doc.bql_blocks[0].query.contains("due < today"));
    }
}
