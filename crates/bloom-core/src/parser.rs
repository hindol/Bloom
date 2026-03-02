use std::ops::Range;

use chrono::{NaiveDate, NaiveTime};

use crate::document::{
    Block, BlockKind, Document, Embed, Frontmatter, Link, Span, Tag, Timestamp, TimestampKind,
};

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("frontmatter is missing closing delimiter")]
    UnterminatedFrontmatter,
    #[error("invalid frontmatter yaml: {0}")]
    InvalidFrontmatter(#[from] serde_yaml::Error),
}

pub fn parse(input: &str) -> Result<Document, ParseError> {
    let (frontmatter, body) = split_frontmatter(input)?;
    let blocks = parse_body(body);
    Ok(Document {
        frontmatter,
        blocks,
    })
}

pub fn serialize(doc: &Document) -> String {
    let mut yaml = serde_yaml::to_string(&doc.frontmatter).expect("frontmatter should serialize");
    if let Some(stripped) = yaml.strip_prefix("---\n") {
        yaml = stripped.to_string();
    }

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(yaml.trim_end());
    out.push_str("\n---\n");

    if !doc.blocks.is_empty() {
        out.push('\n');
        for (idx, block) in doc.blocks.iter().enumerate() {
            out.push_str(&block.content);
            if idx + 1 < doc.blocks.len() {
                out.push('\n');
            }
        }
    }

    out
}

fn split_frontmatter(input: &str) -> Result<(Frontmatter, &str), ParseError> {
    let Some(after_start) = input
        .strip_prefix("---\n")
        .or_else(|| input.strip_prefix("---\r\n"))
    else {
        return Ok((Frontmatter::new("Untitled"), input));
    };

    let mut yaml_len = 0usize;
    let mut body_start = None;

    for line in after_start.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(|c| c == '\n' || c == '\r');
        if trimmed == "---" {
            body_start = Some(yaml_len + line.len());
            break;
        }
        yaml_len += line.len();
    }

    let body_start = body_start.ok_or(ParseError::UnterminatedFrontmatter)?;
    let yaml = &after_start[..yaml_len];
    let body = &after_start[body_start..];
    let frontmatter = serde_yaml::from_str::<Frontmatter>(yaml)?;
    Ok((frontmatter, body))
}

fn parse_body(body: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut in_code_fence = false;
    let mut active_fence: Option<char> = None;

    for raw_line in body.lines() {
        let trimmed = raw_line.trim_start();

        if let Some((marker, language)) = parse_fence_line(trimmed) {
            if in_code_fence {
                if active_fence == Some(marker) {
                    in_code_fence = false;
                    active_fence = None;
                }
                blocks.push(make_code_block(raw_line, None));
            } else {
                in_code_fence = true;
                active_fence = Some(marker);
                blocks.push(make_code_block(raw_line, language));
            }
            continue;
        }

        if in_code_fence {
            blocks.push(make_code_block(raw_line, None));
            continue;
        }

        if raw_line.trim().is_empty() {
            continue;
        }

        let code_spans = inline_code_ranges(raw_line);
        let (links, embeds) = extract_links_and_embeds(raw_line, &code_spans);
        let tags = extract_tags(raw_line, &code_spans);
        let timestamps = extract_timestamps(raw_line, &code_spans);
        let id = extract_block_id(raw_line, &code_spans);
        let kind = classify_block(trimmed);

        blocks.push(Block {
            kind,
            content: raw_line.to_string(),
            id,
            links,
            embeds,
            tags,
            timestamps,
        });
    }

    blocks
}

fn make_code_block(content: &str, language: Option<String>) -> Block {
    Block {
        kind: BlockKind::CodeBlock { language },
        content: content.to_string(),
        id: None,
        links: Vec::new(),
        embeds: Vec::new(),
        tags: Vec::new(),
        timestamps: Vec::new(),
    }
}

fn parse_fence_line(line: &str) -> Option<(char, Option<String>)> {
    let marker = line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }

    let marker_len = line.chars().take_while(|c| *c == marker).count();
    if marker_len < 3 {
        return None;
    }

    let rest = line[marker_len..].trim();
    let language = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    Some((marker, language))
}

fn classify_block(line: &str) -> BlockKind {
    if let Some(level) = heading_level(line) {
        return BlockKind::Heading { level };
    }

    if let Some(checked) = list_item_checked(line) {
        return BlockKind::ListItem { checked };
    }

    if line.starts_with('>') {
        return BlockKind::BlockQuote;
    }

    BlockKind::Paragraph
}

fn heading_level(line: &str) -> Option<u8> {
    let hashes = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ') {
        Some(hashes as u8)
    } else {
        None
    }
}

fn list_item_checked(line: &str) -> Option<Option<bool>> {
    if line.starts_with("- [ ] ") {
        return Some(Some(false));
    }
    if line.starts_with("- [x] ") || line.starts_with("- [X] ") {
        return Some(Some(true));
    }
    if line.starts_with("- ") || line.starts_with("* ") {
        return Some(None);
    }

    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        return Some(None);
    }

    None
}

fn inline_code_ranges(line: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut open_start: Option<usize> = None;

    for (idx, ch) in line.char_indices() {
        if ch == '`' {
            if let Some(start) = open_start.take() {
                ranges.push(start..(idx + ch.len_utf8()));
            } else {
                open_start = Some(idx);
            }
        }
    }

    if let Some(start) = open_start {
        ranges.push(start..line.len());
    }

    ranges
}

fn in_code_span(spans: &[Range<usize>], idx: usize) -> bool {
    spans.iter().any(|span| idx >= span.start && idx < span.end)
}

fn overlaps_code_spans(spans: &[Range<usize>], start: usize, end: usize) -> bool {
    spans
        .iter()
        .any(|span| start < span.end && end > span.start)
}

fn extract_links_and_embeds(line: &str, code_spans: &[Range<usize>]) -> (Vec<Link>, Vec<Embed>) {
    let mut links = Vec::new();
    let mut embeds = Vec::new();
    let mut i = 0usize;

    while i < line.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }

        let rest = &line[i..];
        let (is_embed, open_len) = if rest.starts_with("![[") {
            (true, 3usize)
        } else if rest.starts_with("[[") {
            (false, 2usize)
        } else {
            let ch = rest
                .chars()
                .next()
                .expect("char boundary guarantees a char");
            i += ch.len_utf8();
            continue;
        };

        if in_code_span(code_spans, i) {
            i += open_len;
            continue;
        }

        let inner_start = i + open_len;
        let Some(close_rel) = line[inner_start..].find("]]") else {
            i += open_len;
            continue;
        };
        let close = inner_start + close_rel;
        let end = close + 2;

        if overlaps_code_spans(code_spans, i, end) {
            i = end;
            continue;
        }

        let inner = &line[inner_start..close];
        if let Some((page_id, sub_id, display)) = parse_link_target(inner) {
            if is_embed {
                embeds.push(Embed {
                    page_id,
                    sub_id,
                    display,
                    span: Span { start: i, end },
                });
            } else {
                links.push(Link {
                    page_id,
                    sub_id,
                    display,
                    span: Span { start: i, end },
                });
            }
        }

        i = end;
    }

    (links, embeds)
}

fn parse_link_target(inner: &str) -> Option<(String, Option<String>, Option<String>)> {
    let (target, display) = if let Some((left, right)) = inner.split_once('|') {
        (left.trim(), Some(right.trim().to_string()))
    } else {
        (inner.trim(), None)
    };

    let (page_id, sub_id) = if let Some((page, sub)) = target.split_once('#') {
        (page.trim(), Some(sub.trim().to_string()))
    } else {
        (target, None)
    };

    if page_id.is_empty() {
        return None;
    }

    let sub_id = sub_id.filter(|s| !s.is_empty());
    let display = display.filter(|s| !s.is_empty());
    Some((page_id.to_string(), sub_id, display))
}

fn extract_tags(line: &str, code_spans: &[Range<usize>]) -> Vec<Tag> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut tags = Vec::new();

    for idx in 0..chars.len() {
        let (hash_pos, ch) = chars[idx];
        if ch != '#' || in_code_span(code_spans, hash_pos) {
            continue;
        }

        let at_line_start = idx == 0;
        let preceded_by_ws = !at_line_start && chars[idx - 1].1.is_whitespace();
        if !(at_line_start || preceded_by_ws) {
            continue;
        }

        let Some((name_start, first_char)) = chars.get(idx + 1).copied() else {
            continue;
        };

        if !first_char.is_alphabetic() {
            continue;
        }

        let mut end = name_start + first_char.len_utf8();
        let mut j = idx + 2;
        while let Some((pos, c)) = chars.get(j).copied() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                end = pos + c.len_utf8();
                j += 1;
            } else {
                break;
            }
        }

        let name = &line[name_start..end];
        tags.push(Tag {
            name: name.to_string(),
            span: Span {
                start: hash_pos,
                end,
            },
        });
    }

    tags
}

fn extract_timestamps(line: &str, code_spans: &[Range<usize>]) -> Vec<Timestamp> {
    let patterns = [
        ("@due(", TimestampKind::Due),
        ("@start(", TimestampKind::Start),
        ("@at(", TimestampKind::At),
    ];

    let mut out = Vec::new();
    let mut i = 0usize;

    while i < line.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }

        let rest = &line[i..];
        let ch = rest
            .chars()
            .next()
            .expect("char boundary guarantees a char");
        if ch != '@' || in_code_span(code_spans, i) {
            i += ch.len_utf8();
            continue;
        }

        let mut consumed = false;
        for (prefix, kind) in &patterns {
            if !rest.starts_with(prefix) {
                continue;
            }

            let value_start = i + prefix.len();
            if let Some(close_rel) = line[value_start..].find(')') {
                let close = value_start + close_rel;
                let end = close + 1;
                if !overlaps_code_spans(code_spans, i, end) {
                    if let Some((date, time)) = parse_timestamp_value(&line[value_start..close]) {
                        out.push(Timestamp {
                            kind: kind.clone(),
                            date,
                            time,
                            span: Span { start: i, end },
                        });
                    }
                }
                i = end;
            } else {
                i += prefix.len();
            }
            consumed = true;
            break;
        }

        if !consumed {
            i += ch.len_utf8();
        }
    }

    out
}

fn parse_timestamp_value(raw: &str) -> Option<(NaiveDate, Option<NaiveTime>)> {
    let mut parts = raw.split_whitespace();
    let date_str = parts.next()?;
    let time_str = parts.next();

    if parts.next().is_some() {
        return None;
    }

    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
    let time = match time_str {
        Some(value) => Some(NaiveTime::parse_from_str(value, "%H:%M").ok()?),
        None => None,
    };
    Some((date, time))
}

fn extract_block_id(line: &str, code_spans: &[Range<usize>]) -> Option<String> {
    let trimmed = line.trim_end();
    let caret = trimmed.rfind('^')?;

    if in_code_span(code_spans, caret) {
        return None;
    }

    if caret == 0 {
        return None;
    }

    let prev = trimmed[..caret].chars().last()?;
    if !prev.is_whitespace() {
        return None;
    }

    let id = &trimmed[(caret + 1)..];
    if id.is_empty() {
        return None;
    }

    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }

    Some(id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::BlockKind;

    fn full_doc_input() -> &'static str {
        r#"---
id: 8f3a1b2c
title: "Parser Test"
created: 2026-02-28T00:00:00Z
tags: [rust]
---

# Heading
- [ ] Task [[deadbeef|Task Page]] #work @due(2026-03-05) ^task-1
Paragraph ![[feedcafe#sec-1|Embed]] @at(2026-03-02 14:00)
"#
    }

    #[test]
    fn parse_full_document_extensions() {
        let doc = parse(full_doc_input()).unwrap();
        assert_eq!(doc.frontmatter.id, "8f3a1b2c");
        assert_eq!(doc.frontmatter.title, "Parser Test");
        assert_eq!(doc.blocks.len(), 3);

        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::Heading { level: 1 }
        ));
        assert!(matches!(
            doc.blocks[1].kind,
            BlockKind::ListItem {
                checked: Some(false)
            }
        ));
        assert_eq!(doc.blocks[1].id.as_deref(), Some("task-1"));
        assert_eq!(doc.blocks[1].links.len(), 1);
        assert_eq!(doc.blocks[1].tags.len(), 1);
        assert_eq!(doc.blocks[1].timestamps.len(), 1);

        assert_eq!(doc.blocks[2].embeds.len(), 1);
        assert_eq!(doc.blocks[2].timestamps.len(), 1);
    }

    #[test]
    fn links_in_fenced_code_are_ignored() {
        let input = r#"```rust
let x = "[[deadbeef|skip]]";
```
outside [[feedbeef|ok]]
"#;
        let doc = parse(input).unwrap();

        let total_links: usize = doc.blocks.iter().map(|b| b.links.len()).sum();
        assert_eq!(total_links, 1);
        assert_eq!(doc.blocks.last().unwrap().links[0].page_id, "feedbeef");
    }

    #[test]
    fn tags_follow_bloom_rules() {
        let input = "hello #rust #プログラミング foo#bar #123";
        let doc = parse(input).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        let tags = &doc.blocks[0].tags;
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, "rust");
        assert_eq!(tags[1].name, "プログラミング");
    }

    #[test]
    fn timestamps_parse_due_start_and_at() {
        let input =
            "- [ ] task @due(2026-03-05) @start(2026-03-03) @at(2026-03-02 14:00) @at(2026-03-01)";
        let doc = parse(input).unwrap();
        assert_eq!(doc.blocks.len(), 1);
        assert_eq!(doc.blocks[0].timestamps.len(), 4);
        assert_eq!(doc.blocks[0].timestamps[0].kind, TimestampKind::Due);
        assert_eq!(doc.blocks[0].timestamps[1].kind, TimestampKind::Start);
        assert_eq!(doc.blocks[0].timestamps[2].kind, TimestampKind::At);
        assert!(doc.blocks[0].timestamps[2].time.is_some());
        assert!(doc.blocks[0].timestamps[3].time.is_none());
    }

    #[test]
    fn empty_document_is_supported() {
        let doc = parse("").unwrap();
        assert!(doc.blocks.is_empty());
        assert_eq!(doc.frontmatter.title, "Untitled");
        assert_eq!(doc.frontmatter.id.len(), 16);
    }

    #[test]
    fn no_frontmatter_uses_defaults() {
        let doc = parse("Plain text line").unwrap();
        assert_eq!(doc.frontmatter.title, "Untitled");
        assert_eq!(doc.blocks.len(), 1);
        assert!(matches!(doc.blocks[0].kind, BlockKind::Paragraph));
    }

    #[test]
    fn serialize_roundtrip_keeps_content() {
        let doc = parse(full_doc_input()).unwrap();
        let serialized = serialize(&doc);
        let reparsed = parse(&serialized).unwrap();

        assert_eq!(reparsed.frontmatter.id, doc.frontmatter.id);
        assert_eq!(reparsed.frontmatter.title, doc.frontmatter.title);
        assert_eq!(reparsed.blocks.len(), doc.blocks.len());
        assert_eq!(reparsed.blocks[1].content, doc.blocks[1].content);
        assert_eq!(reparsed.blocks[2].content, doc.blocks[2].content);
    }

    #[test]
    fn heading_levels_are_detected() {
        let doc = parse("# h1\n### h3\n###### h6\n####### no\n").unwrap();
        assert_eq!(doc.blocks.len(), 4);
        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::Heading { level: 1 }
        ));
        assert!(matches!(
            doc.blocks[1].kind,
            BlockKind::Heading { level: 3 }
        ));
        assert!(matches!(
            doc.blocks[2].kind,
            BlockKind::Heading { level: 6 }
        ));
        assert!(matches!(doc.blocks[3].kind, BlockKind::Paragraph));
    }

    #[test]
    fn inline_code_masks_extensions() {
        let doc = parse("`[[deadbeef|skip]] #nope @due(2026-03-01)` #yes").unwrap();
        assert_eq!(doc.blocks.len(), 1);
        assert!(doc.blocks[0].links.is_empty());
        assert!(doc.blocks[0].timestamps.is_empty());
        assert_eq!(doc.blocks[0].tags.len(), 1);
        assert_eq!(doc.blocks[0].tags[0].name, "yes");
    }

    // ── Edge-case tests ────────────────────────────────────────────

    #[test]
    fn multiple_links_on_one_line() {
        let doc = parse("See [[aaa|First]] and [[bbb|Second]] here").unwrap();
        assert_eq!(doc.blocks[0].links.len(), 2);
        assert_eq!(doc.blocks[0].links[0].page_id, "aaa");
        assert_eq!(doc.blocks[0].links[0].display.as_deref(), Some("First"));
        assert_eq!(doc.blocks[0].links[1].page_id, "bbb");
        assert_eq!(doc.blocks[0].links[1].display.as_deref(), Some("Second"));
    }

    #[test]
    fn link_without_display_text() {
        let doc = parse("bare [[abcd1234]]").unwrap();
        assert_eq!(doc.blocks[0].links.len(), 1);
        assert_eq!(doc.blocks[0].links[0].page_id, "abcd1234");
        assert!(doc.blocks[0].links[0].display.is_none());
    }

    #[test]
    fn link_with_sub_id() {
        let doc = parse("ref [[page01#sec-2|Section Two]]").unwrap();
        let link = &doc.blocks[0].links[0];
        assert_eq!(link.page_id, "page01");
        assert_eq!(link.sub_id.as_deref(), Some("sec-2"));
        assert_eq!(link.display.as_deref(), Some("Section Two"));
    }

    #[test]
    fn embed_with_sub_id() {
        let doc = parse("![[page01#block-1|Embedded Block]]").unwrap();
        assert_eq!(doc.blocks[0].embeds.len(), 1);
        let embed = &doc.blocks[0].embeds[0];
        assert_eq!(embed.page_id, "page01");
        assert_eq!(embed.sub_id.as_deref(), Some("block-1"));
        assert_eq!(embed.display.as_deref(), Some("Embedded Block"));
    }

    #[test]
    fn embed_without_display() {
        let doc = parse("![[feedcafe]]").unwrap();
        assert_eq!(doc.blocks[0].embeds.len(), 1);
        assert_eq!(doc.blocks[0].embeds[0].page_id, "feedcafe");
        assert!(doc.blocks[0].embeds[0].display.is_none());
    }

    #[test]
    fn empty_link_is_ignored() {
        let doc = parse("empty [[]] here").unwrap();
        assert!(doc.blocks[0].links.is_empty());
    }

    #[test]
    fn unclosed_link_is_ignored() {
        let doc = parse("broken [[deadbeef|no close").unwrap();
        assert!(doc.blocks[0].links.is_empty());
    }

    #[test]
    fn unclosed_embed_is_ignored() {
        let doc = parse("broken ![[deadbeef|no close").unwrap();
        assert!(doc.blocks[0].embeds.is_empty());
    }

    #[test]
    fn nested_brackets_parse_outer_only() {
        // Inner `[[` inside an already-open `[[...]]` — parser finds first `]]`
        let doc = parse("text [[outer [[inner]] rest]]").unwrap();
        // Should find `[[outer [[inner]]` as the first match (closes at first `]]`)
        assert!(!doc.blocks[0].links.is_empty());
        let first = &doc.blocks[0].links[0];
        assert_eq!(first.page_id, "outer [[inner");
    }

    #[test]
    fn tilde_code_fences_mask_extensions() {
        let input = "~~~\n[[deadbeef|skip]] #nope\n~~~\n[[feedbeef|ok]]";
        let doc = parse(input).unwrap();
        let total_links: usize = doc.blocks.iter().map(|b| b.links.len()).sum();
        assert_eq!(total_links, 1);
        assert_eq!(doc.blocks.last().unwrap().links[0].page_id, "feedbeef");
    }

    #[test]
    fn mixed_fence_markers_independent() {
        // Opening with ``` should not close with ~~~
        let input = "```\ncode [[aaa|skip]]\n~~~\nstill code [[bbb|skip]]\n```\nout [[ccc|ok]]";
        let doc = parse(input).unwrap();
        let total_links: usize = doc.blocks.iter().map(|b| b.links.len()).sum();
        assert_eq!(total_links, 1);
        assert_eq!(doc.blocks.last().unwrap().links[0].page_id, "ccc");
    }

    #[test]
    fn blockquote_with_extensions() {
        let doc = parse("> quoted text [[aaa|link]] #tag @due(2026-05-01)").unwrap();
        assert!(matches!(doc.blocks[0].kind, BlockKind::BlockQuote));
        assert_eq!(doc.blocks[0].links.len(), 1);
        assert_eq!(doc.blocks[0].tags.len(), 1);
        assert_eq!(doc.blocks[0].timestamps.len(), 1);
    }

    #[test]
    fn ordered_list_item() {
        let doc = parse("1. First item\n2. Second item\n10. Tenth").unwrap();
        assert_eq!(doc.blocks.len(), 3);
        for b in &doc.blocks {
            assert!(matches!(b.kind, BlockKind::ListItem { checked: None }));
        }
    }

    #[test]
    fn checked_task_item() {
        let doc = parse("- [x] Done task\n- [X] Also done\n- [ ] Not done").unwrap();
        assert_eq!(doc.blocks.len(), 3);
        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::ListItem {
                checked: Some(true)
            }
        ));
        assert!(matches!(
            doc.blocks[1].kind,
            BlockKind::ListItem {
                checked: Some(true)
            }
        ));
        assert!(matches!(
            doc.blocks[2].kind,
            BlockKind::ListItem {
                checked: Some(false)
            }
        ));
    }

    #[test]
    fn unordered_list_with_star() {
        let doc = parse("* star item").unwrap();
        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::ListItem { checked: None }
        ));
    }

    #[test]
    fn block_id_on_heading() {
        let doc = parse("## Section ^sec-1").unwrap();
        assert!(matches!(
            doc.blocks[0].kind,
            BlockKind::Heading { level: 2 }
        ));
        assert_eq!(doc.blocks[0].id.as_deref(), Some("sec-1"));
    }

    #[test]
    fn block_id_on_paragraph() {
        let doc = parse("Some text ^my-id").unwrap();
        assert_eq!(doc.blocks[0].id.as_deref(), Some("my-id"));
    }

    #[test]
    fn caret_without_preceding_space_is_not_block_id() {
        let doc = parse("word^nope").unwrap();
        assert!(doc.blocks[0].id.is_none());
    }

    #[test]
    fn caret_at_line_start_is_not_block_id() {
        let doc = parse("^nope").unwrap();
        assert!(doc.blocks[0].id.is_none());
    }

    #[test]
    fn block_id_with_special_chars_rejected() {
        let doc = parse("text ^no!valid").unwrap();
        assert!(doc.blocks[0].id.is_none());
    }

    #[test]
    fn invalid_timestamp_date_silently_skipped() {
        let doc = parse("@due(2026-13-01) @due(not-a-date)").unwrap();
        assert!(doc.blocks[0].timestamps.is_empty());
    }

    #[test]
    fn timestamp_with_extra_parts_skipped() {
        let doc = parse("@due(2026-03-01 14:00 extra)").unwrap();
        assert!(doc.blocks[0].timestamps.is_empty());
    }

    #[test]
    fn unclosed_timestamp_skipped() {
        // @due( greedily matches to the only `)` — value is invalid, so 0 timestamps
        let doc = parse("@due(2026-03-01 and @start(2026-03-02)").unwrap();
        assert_eq!(doc.blocks[0].timestamps.len(), 0);
        // Properly closed @start works on its own
        let doc2 = parse("text @start(2026-03-02)").unwrap();
        assert_eq!(doc2.blocks[0].timestamps.len(), 1);
        assert_eq!(doc2.blocks[0].timestamps[0].kind, TimestampKind::Start);
    }

    #[test]
    fn tag_with_hyphens_and_underscores() {
        let doc = parse("#my-tag #my_tag #a-b_c-d").unwrap();
        let tags = &doc.blocks[0].tags;
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].name, "my-tag");
        assert_eq!(tags[1].name, "my_tag");
        assert_eq!(tags[2].name, "a-b_c-d");
    }

    #[test]
    fn tag_at_line_start() {
        let doc = parse("#rust is great").unwrap();
        // Must NOT be confused with heading (heading requires `# ` with space after hashes)
        assert!(matches!(doc.blocks[0].kind, BlockKind::Paragraph));
        assert_eq!(doc.blocks[0].tags.len(), 1);
        assert_eq!(doc.blocks[0].tags[0].name, "rust");
    }

    #[test]
    fn hash_without_letter_is_not_tag() {
        let doc = parse("#123 # ## foo#bar").unwrap();
        // #123 starts with digit — not a tag
        // # alone — not a tag (nothing after)
        // ## — heading-like but no space — treated as paragraph, no tags
        // foo#bar — # not preceded by whitespace — not a tag
        let total_tags: usize = doc.blocks.iter().map(|b| b.tags.len()).sum();
        assert_eq!(total_tags, 0);
    }

    #[test]
    fn link_spans_are_correct() {
        let input = "ab [[ccc|D]] ef";
        let doc = parse(input).unwrap();
        let link = &doc.blocks[0].links[0];
        assert_eq!(link.span.start, 3); // position of first `[`
        assert_eq!(link.span.end, 12); // position after `]]`
        assert_eq!(&input[link.span.start..link.span.end], "[[ccc|D]]");
    }

    #[test]
    fn tag_spans_are_correct() {
        let input = "ab #tag cd";
        let doc = parse(input).unwrap();
        let tag = &doc.blocks[0].tags[0];
        assert_eq!(tag.span.start, 3); // position of `#`
        assert_eq!(tag.span.end, 7); // position after `tag`
        assert_eq!(&input[tag.span.start..tag.span.end], "#tag");
    }

    #[test]
    fn timestamp_spans_are_correct() {
        let input = "ab @due(2026-03-01) cd";
        let doc = parse(input).unwrap();
        let ts = &doc.blocks[0].timestamps[0];
        assert_eq!(&input[ts.span.start..ts.span.end], "@due(2026-03-01)");
    }

    #[test]
    fn windows_line_endings_handled() {
        let input = "---\r\nid: abcd1234\r\ntitle: \"Win\"\r\ncreated: 2026-01-01T00:00:00Z\r\ntags: []\r\n---\r\n\r\nLine one\r\nLine two [[aaa|link]]\r\n";
        let doc = parse(input).unwrap();
        assert_eq!(doc.frontmatter.id, "abcd1234");
        assert_eq!(doc.frontmatter.title, "Win");
        // Body lines should parse
        let total_links: usize = doc.blocks.iter().map(|b| b.links.len()).sum();
        assert_eq!(total_links, 1);
    }

    #[test]
    fn frontmatter_with_extra_keys_preserved() {
        let input = "---\nid: abcd1234\ntitle: Test\ncreated: 2026-01-01T00:00:00Z\ntags: []\ncustom_key: custom_value\nlogseq_property: something\n---\n";
        let doc = parse(input).unwrap();
        assert_eq!(doc.frontmatter.id, "abcd1234");
        assert!(doc.frontmatter.extra.contains_key("custom_key"));
        assert!(doc.frontmatter.extra.contains_key("logseq_property"));
    }

    #[test]
    fn frontmatter_missing_optional_tags_defaults_empty() {
        let input = "---\nid: abcd1234\ntitle: Minimal\ncreated: 2026-01-01T00:00:00Z\n---\n";
        let doc = parse(input).unwrap();
        assert!(doc.frontmatter.tags.is_empty());
    }

    #[test]
    fn unterminated_frontmatter_is_error() {
        let result = parse("---\nid: abcd1234\ntitle: Oops\n");
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            ParseError::UnterminatedFrontmatter
        ));
    }

    #[test]
    fn code_fence_with_language() {
        let input = "```python\nprint('hello')\n```\n";
        let doc = parse(input).unwrap();
        assert!(matches!(
            &doc.blocks[0].kind,
            BlockKind::CodeBlock {
                language: Some(lang)
            } if lang == "python"
        ));
    }

    #[test]
    fn all_extensions_on_one_line() {
        let input =
            "- [ ] Read [[aaa|Book]] and ![[bbb#ch1|Chapter 1]] #reading @due(2026-06-01) ^task-99";
        let doc = parse(input).unwrap();
        let b = &doc.blocks[0];
        assert!(matches!(
            b.kind,
            BlockKind::ListItem {
                checked: Some(false)
            }
        ));
        assert_eq!(b.links.len(), 1);
        assert_eq!(b.embeds.len(), 1);
        assert_eq!(b.tags.len(), 1);
        assert_eq!(b.timestamps.len(), 1);
        assert_eq!(b.id.as_deref(), Some("task-99"));
    }

    #[test]
    fn blank_lines_between_blocks_are_skipped() {
        let doc = parse("First\n\n\n\nSecond").unwrap();
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[0].content, "First");
        assert_eq!(doc.blocks[1].content, "Second");
    }

    #[test]
    fn serialize_preserves_block_count() {
        let input = "---\nid: abcd1234\ntitle: Test\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n# Heading\nParagraph\n- List item\n";
        let doc = parse(input).unwrap();
        let out = serialize(&doc);
        let reparsed = parse(&out).unwrap();
        assert_eq!(reparsed.blocks.len(), doc.blocks.len());
        for (a, b) in reparsed.blocks.iter().zip(doc.blocks.iter()) {
            assert_eq!(a.kind, b.kind);
            assert_eq!(a.content, b.content);
        }
    }

    #[test]
    fn embed_and_link_on_same_line() {
        let doc = parse("See [[aaa|Page]] and embed ![[bbb|Other]]").unwrap();
        assert_eq!(doc.blocks[0].links.len(), 1);
        assert_eq!(doc.blocks[0].embeds.len(), 1);
        assert_eq!(doc.blocks[0].links[0].page_id, "aaa");
        assert_eq!(doc.blocks[0].embeds[0].page_id, "bbb");
    }

    #[test]
    fn link_with_empty_sub_id_treated_as_none() {
        // [[page#|display]] — empty sub_id should be None
        let doc = parse("[[page01#|display]]").unwrap();
        let link = &doc.blocks[0].links[0];
        assert_eq!(link.page_id, "page01");
        assert!(link.sub_id.is_none());
        assert_eq!(link.display.as_deref(), Some("display"));
    }

    #[test]
    fn link_with_empty_display_treated_as_none() {
        // [[page|]] — empty display should be None
        let doc = parse("[[page01|]]").unwrap();
        let link = &doc.blocks[0].links[0];
        assert_eq!(link.page_id, "page01");
        assert!(link.display.is_none());
    }
}
