use std::ops::Range;

use chrono::{NaiveDate, NaiveDateTime};

use crate::types::{BlockId, PageId, TagName, Timestamp};

use super::traits::{
    ParsedBlockId, ParsedBlockLink, ParsedLink, ParsedTag, ParsedTask, ParsedTimestamp,
};

enum ParsedLinkKind {
    Page(ParsedLink),
    Block(ParsedBlockLink),
}

/// Parse all `[[target|display]]` links from a line (not inside code spans).
pub fn parse_links(line: &str, line_number: usize) -> (Vec<ParsedLink>, Vec<ParsedBlockLink>) {
    let mut links = Vec::new();
    let mut block_links = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i + 1 < len {
        // Skip code spans
        if bytes[i] == b'`' {
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1; // skip closing `
            }
            continue;
        }

        if bytes[i] == b'[' && i + 1 < len && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            // Find closing ]]
            let content_start = i;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            if i + 1 < len {
                let content = &line[content_start..i];
                let end = i + 2;
                i = end;

                match parse_link_content(content, line_number, start..end) {
                    Some(ParsedLinkKind::Page(link)) => links.push(link),
                    Some(ParsedLinkKind::Block(bl)) => block_links.push(bl),
                    None => {}
                }
            }
            continue;
        }
        i += 1;
    }
    (links, block_links)
}

fn parse_link_content(
    content: &str,
    line: usize,
    byte_range: Range<usize>,
) -> Option<ParsedLinkKind> {
    let (target_part, display) = if let Some(pipe_pos) = content.find('|') {
        (&content[..pipe_pos], content[pipe_pos + 1..].to_string())
    } else {
        (content, content.to_string())
    };

    // Block-only link: [[^block_id|text]]
    if let Some(block_id_str) = target_part.strip_prefix('^') {
        if block_id_str.is_empty() {
            return None;
        }
        return Some(ParsedLinkKind::Block(ParsedBlockLink {
            block_id: BlockId(block_id_str.to_string()),
            display_hint: display,
            line,
            byte_range,
        }));
    }

    // Page link: [[page_id|text]] — reject [[page^block|text]] (parse error)
    if target_part.contains('^') {
        return None;
    }

    let target = PageId::from_hex(target_part)?;

    Some(ParsedLinkKind::Page(ParsedLink {
        target,
        display_hint: display,
        line,
        byte_range,
    }))
}

/// Parse `^block-id` at the end of a line.
pub fn parse_block_id(line: &str, line_number: usize) -> Option<ParsedBlockId> {
    let trimmed = line.trim_end();
    // Must be preceded by a space (or be the entire line)
    if let Some(pos) = trimmed.rfind(" ^") {
        let after_caret = &trimmed[pos + 2..];
        // Check for ^= mirror marker
        let (id_str, is_mirror) = if let Some(rest) = after_caret.strip_prefix('=') {
            (rest, true)
        } else {
            (after_caret, false)
        };
        if !id_str.is_empty()
            && id_str
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Some(ParsedBlockId {
                id: BlockId(id_str.to_string()),
                line: line_number,
                is_mirror,
            });
        }
    }
    // Line that is solely ^block-id or ^=block-id
    if let Some(rest) = trimmed.strip_prefix('^') {
        let (id_str, is_mirror) = if let Some(r) = rest.strip_prefix('=') {
            (r, true)
        } else {
            (rest, false)
        };
        if !id_str.is_empty()
            && id_str
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Some(ParsedBlockId {
                id: BlockId(id_str.to_string()),
                line: line_number,
                is_mirror,
            });
        }
    }
    None
}

/// Parse `#tag` occurrences from a line (not inside code spans).
/// Tags must start with a Unicode letter and be preceded by whitespace or start of line.
pub fn parse_tags(line: &str, line_number: usize) -> Vec<ParsedTag> {
    let mut tags = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip code spans
        if bytes[i] == b'`' {
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            continue;
        }

        // Skip [[ links (don't extract tags from inside)
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            i += 2;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            continue;
        }

        if bytes[i] == b'#' {
            // Must be preceded by whitespace or start of line
            let preceded_by_ws =
                i == 0 || line[..i].chars().last().is_none_or(|c| c.is_whitespace());
            if preceded_by_ws {
                // Check if this is a heading (# at start of line followed by space)
                if i == 0 {
                    // Check if this is a heading pattern
                    let rest = &line[i..];
                    let hash_count = rest.bytes().take_while(|&b| b == b'#').count();
                    if hash_count <= 6
                        && line.len() > i + hash_count
                        && bytes[i + hash_count] == b' '
                    {
                        // It's a heading — skip
                        i += hash_count + 1;
                        continue;
                    }
                }

                let tag_start = i;
                i += 1; // skip #
                let name_start = i;
                // First char must be a Unicode letter
                let first_char = line[name_start..].chars().next();
                if let Some(fc) = first_char {
                    if fc.is_alphabetic() {
                        i += fc.len_utf8();
                        // Subsequent chars: letters, digits, hyphens, underscores
                        while i < len {
                            let ch = line[i..].chars().next().unwrap();
                            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                                i += ch.len_utf8();
                            } else {
                                break;
                            }
                        }
                        let name = &line[name_start..i];
                        tags.push(ParsedTag {
                            name: TagName(name.to_string()),
                            line: line_number,
                            byte_range: tag_start..i,
                        });
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    tags
}

/// Parse `@due(...)`, `@start(...)`, `@at(...)` timestamps from a line.
pub fn parse_timestamps(line: &str, line_number: usize) -> Vec<ParsedTimestamp> {
    let mut timestamps = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        // Skip code spans
        if bytes[i] == b'`' {
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1;
            }
            continue;
        }

        if bytes[i] == b'@' {
            let start = i;
            if let Some(ts) = try_parse_timestamp(line, &mut i) {
                timestamps.push(ParsedTimestamp {
                    timestamp: ts,
                    line: line_number,
                    byte_range: start..i,
                });
                continue;
            }
        }
        i += 1;
    }
    timestamps
}

fn try_parse_timestamp(line: &str, pos: &mut usize) -> Option<Timestamp> {
    let rest = &line[*pos..];

    for (prefix, constructor) in &[
        ("@due(", TimestampKind::Due),
        ("@start(", TimestampKind::Start),
        ("@at(", TimestampKind::At),
    ] {
        if rest.starts_with(prefix) {
            let inner_start = *pos + prefix.len();
            if let Some(close) = line[inner_start..].find(')') {
                let inner = &line[inner_start..inner_start + close];
                let ts = match constructor {
                    TimestampKind::Due => NaiveDate::parse_from_str(inner, "%Y-%m-%d")
                        .ok()
                        .map(Timestamp::Due),
                    TimestampKind::Start => NaiveDate::parse_from_str(inner, "%Y-%m-%d")
                        .ok()
                        .map(Timestamp::Start),
                    TimestampKind::At => NaiveDateTime::parse_from_str(inner, "%Y-%m-%d %H:%M")
                        .ok()
                        .map(Timestamp::At)
                        .or_else(|| {
                            NaiveDate::parse_from_str(inner, "%Y-%m-%d")
                                .ok()
                                .map(|d| Timestamp::At(d.and_hms_opt(0, 0, 0).unwrap()))
                        }),
                };
                if let Some(ts) = ts {
                    *pos = inner_start + close + 1; // past the closing )
                    return Some(ts);
                }
            }
        }
    }
    None
}

enum TimestampKind {
    Due,
    Start,
    At,
}

/// Parse a task line: `- [ ] text` or `- [x] text`.
pub fn parse_task(line: &str, line_number: usize) -> Option<ParsedTask> {
    let trimmed = line.trim_start();
    let (done, text_start) = if trimmed.starts_with("- [ ] ") {
        (false, "- [ ] ".len())
    } else if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        (true, "- [x] ".len())
    } else {
        return None;
    };

    let text = &trimmed[text_start..];
    let timestamps = parse_timestamps(text, 0)
        .into_iter()
        .map(|pt| pt.timestamp)
        .collect();

    Some(ParsedTask {
        text: text.to_string(),
        done,
        timestamps,
        line: line_number,
    })
}

// ===========================================================================
// Unified per-line parse
// ===========================================================================

/// Structural elements extracted from a single line.
#[derive(Debug, Clone, Default)]
pub struct LineElements {
    pub links: Vec<ParsedLink>,
    pub block_links: Vec<ParsedBlockLink>,
    pub tags: Vec<ParsedTag>,
    pub timestamps: Vec<ParsedTimestamp>,
    pub task: Option<ParsedTask>,
    pub block_id: Option<ParsedBlockId>,
}

impl LineElements {
    /// True if the line is a task (`- [ ]` or `- [x]`).
    pub fn is_task(&self) -> bool {
        self.task.is_some()
    }

    /// True if the line starts with `- ` (task or plain list item).
    pub fn is_list_item(line: &str) -> bool {
        line.trim_start().starts_with("- ")
    }

    /// The block ID string (without `^` or `^=` prefix), if present.
    pub fn block_id_str(&self) -> Option<&str> {
        self.block_id.as_ref().map(|b| b.id.0.as_str())
    }

    /// Split line text into `(content, block_id_suffix)`.
    /// `block_id_suffix` includes the leading ` ^` or ` ^=`, e.g. `" ^k7m2x"`.
    /// Returns `(line, "")` if no block ID.
    pub fn split_block_id(line: &str) -> (&str, &str) {
        if let Some(pos) = line.rfind(" ^") {
            let after_caret = &line[pos + 2..];
            let id_part = after_caret.strip_prefix('=').unwrap_or(after_caret);
            if id_part.len() == 5
                && id_part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            {
                return (&line[..pos], &line[pos..]);
            }
        }
        (line, "")
    }

    /// Find the byte position of the first `@due(`, `@start(`, or `@at(`.
    pub fn first_timestamp_pos(line: &str) -> Option<usize> {
        ["@due(", "@start(", "@at("]
            .iter()
            .filter_map(|p| line.find(p))
            .min()
    }
}

/// Parse all structural elements from a single line.
/// One call replaces scattered manual parsing of block IDs, tasks,
/// timestamps, tags, and links.
pub fn parse_line(line: &str, line_number: usize) -> LineElements {
    let (links, block_links) = parse_links(line, line_number);
    LineElements {
        links,
        block_links,
        tags: parse_tags(line, line_number),
        timestamps: parse_timestamps(line, line_number),
        task: parse_task(line, line_number),
        block_id: parse_block_id(line, line_number),
    }
}

/// Extract the link target from a `[[...]]` pattern at the given column.
/// Returns either a page ID hex string or `^block_id` for block links.
pub fn extract_link_at_col(line: &str, col: usize) -> Option<String> {
    let byte_col = line
        .char_indices()
        .nth(col)
        .map(|(i, _)| i)
        .unwrap_or(line.len());
    let bytes = line.as_bytes();
    let len = bytes.len();
    if byte_col >= len {
        return None;
    }

    // Search backwards for [[
    let mut start = None;
    let mut i = byte_col.min(len.saturating_sub(1));
    while i > 0 {
        if i > 0 && bytes[i - 1] == b'[' && bytes[i] == b'[' {
            start = Some(i + 1);
            break;
        }
        if i > 0 && bytes[i - 1] == b']' && bytes[i] == b']' {
            return None;
        }
        i -= 1;
    }
    let content_start = start?;

    // Search forward for ]]
    let mut j = content_start;
    while j + 1 < len {
        if bytes[j] == b']' && bytes[j + 1] == b']' {
            let content = &line[content_start..j];
            let target = content.split('|').next().unwrap_or(content);
            return Some(target.to_string());
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{BlockId, PageId, TagName, Timestamp};
    use chrono::Timelike;

    // --- Link tests (UC-24) ---

    #[test]
    fn test_parse_link_with_display() {
        let line = "See [[8f3a1b2c|Text Editor]] for details";
        let (links, block_links) = parse_links(line, 0);
        assert_eq!(links.len(), 1);
        assert_eq!(block_links.len(), 0);
        assert_eq!(links[0].target, PageId::from_hex("8f3a1b2c").unwrap());
        assert_eq!(links[0].display_hint, "Text Editor");
    }

    #[test]
    fn test_parse_link_with_section_is_parse_error() {
        // [[page^block|text]] form is no longer supported — parse error
        let line = "[[8f3a1b2c^intro|Text Editor]]";
        let (links, block_links) = parse_links(line, 0);
        assert_eq!(links.len(), 0);
        assert_eq!(block_links.len(), 0);
    }

    #[test]
    fn test_parse_block_only_link() {
        let line = "See [[^k7m2x|the rope analysis]] for details";
        let (links, block_links) = parse_links(line, 0);
        assert_eq!(links.len(), 0);
        assert_eq!(block_links.len(), 1);
        assert_eq!(block_links[0].block_id, BlockId("k7m2x".to_string()));
        assert_eq!(block_links[0].display_hint, "the rope analysis");
    }

    #[test]
    fn test_parse_block_link_without_display() {
        let line = "[[^k7m2x]]";
        let (_, block_links) = parse_links(line, 0);
        assert_eq!(block_links.len(), 1);
        assert_eq!(block_links[0].block_id, BlockId("k7m2x".to_string()));
        assert_eq!(block_links[0].display_hint, "^k7m2x");
    }

    #[test]
    fn test_parse_multiple_links_on_one_line() {
        let line = "[[aabbccdd|A]] and [[11223344|B]]";
        let (links, _) = parse_links(line, 0);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].display_hint, "A");
        assert_eq!(links[1].display_hint, "B");
    }

    #[test]
    fn test_parse_link_without_display() {
        let line = "[[8f3a1b2c]]";
        let (links, _) = parse_links(line, 0);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].display_hint, "8f3a1b2c");
    }

    #[test]
    fn test_parse_link_invalid_id() {
        let line = "[[short|Bad]]";
        let (links, block_links) = parse_links(line, 0);
        assert_eq!(links.len(), 0);
        assert_eq!(block_links.len(), 0);
    }

    #[test]
    fn test_links_not_parsed_in_code_span() {
        let line = "Before `[[8f3a1b2c|Link]]` after";
        let (links, block_links) = parse_links(line, 0);
        assert_eq!(links.len(), 0);
        assert_eq!(block_links.len(), 0);
    }

    // --- Tag tests (UC-35) ---

    #[test]
    fn test_parse_tag() {
        let line = "This is about #rust and #editors";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].name, TagName("rust".to_string()));
        assert_eq!(tags[1].name, TagName("editors".to_string()));
    }

    #[test]
    fn test_tag_must_start_with_letter() {
        let line = "#123 should not match but #rust123 should";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, TagName("rust123".to_string()));
    }

    #[test]
    fn test_tag_not_preceded_by_word_char() {
        let line = "foo#bar is not a tag";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 0);
    }

    #[test]
    fn test_tags_not_parsed_in_code_span() {
        let line = "`#not-a-tag` but #real-tag yes";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, TagName("real-tag".to_string()));
    }

    #[test]
    fn test_tag_at_start_of_line() {
        let line = "#mytag here";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].name, TagName("mytag".to_string()));
    }

    #[test]
    fn test_heading_not_parsed_as_tag() {
        let line = "## My Heading";
        let tags = parse_tags(line, 0);
        assert_eq!(tags.len(), 0);
    }

    // --- Task tests (UC-41) ---

    #[test]
    fn test_parse_unchecked_task() {
        let task = parse_task("- [ ] Review the API", 0).unwrap();
        assert!(!task.done);
        assert_eq!(task.text, "Review the API");
    }

    #[test]
    fn test_parse_checked_task() {
        let task = parse_task("- [x] Done item", 0).unwrap();
        assert!(task.done);
        assert_eq!(task.text, "Done item");
    }

    #[test]
    fn test_parse_checked_task_uppercase() {
        let task = parse_task("- [X] Also done", 0).unwrap();
        assert!(task.done);
    }

    #[test]
    fn test_parse_task_with_timestamp() {
        let task = parse_task("- [ ] Do thing @due(2026-03-05)", 0).unwrap();
        assert!(!task.done);
        assert_eq!(task.timestamps.len(), 1);
        match &task.timestamps[0] {
            Timestamp::Due(d) => {
                assert_eq!(*d, chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap())
            }
            _ => panic!("Expected Due timestamp"),
        }
    }

    #[test]
    fn test_parse_non_task_line() {
        assert!(parse_task("Just a normal line", 0).is_none());
        assert!(parse_task("- A list item", 0).is_none());
    }

    // --- Timestamp tests (UC-47) ---

    #[test]
    fn test_parse_due_timestamp() {
        let ts = parse_timestamps("@due(2026-03-05)", 0);
        assert_eq!(ts.len(), 1);
        match &ts[0].timestamp {
            Timestamp::Due(d) => {
                assert_eq!(*d, chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap())
            }
            _ => panic!("Expected Due"),
        }
    }

    #[test]
    fn test_parse_start_timestamp() {
        let ts = parse_timestamps("@start(2026-03-01)", 0);
        assert_eq!(ts.len(), 1);
        match &ts[0].timestamp {
            Timestamp::Start(d) => {
                assert_eq!(*d, chrono::NaiveDate::from_ymd_opt(2026, 3, 1).unwrap())
            }
            _ => panic!("Expected Start"),
        }
    }

    #[test]
    fn test_parse_at_timestamp() {
        let ts = parse_timestamps("@at(2026-03-05 14:30)", 0);
        assert_eq!(ts.len(), 1);
        match &ts[0].timestamp {
            Timestamp::At(dt) => {
                assert_eq!(
                    dt.date(),
                    chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()
                );
                assert_eq!(dt.time().hour(), 14);
                assert_eq!(dt.time().minute(), 30);
            }
            _ => panic!("Expected At"),
        }
    }

    #[test]
    fn test_parse_at_timestamp_date_only() {
        let ts = parse_timestamps("@at(2026-03-05)", 0);
        assert_eq!(ts.len(), 1);
        match &ts[0].timestamp {
            Timestamp::At(dt) => {
                assert_eq!(
                    dt.date(),
                    chrono::NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()
                );
            }
            _ => panic!("Expected At"),
        }
    }

    #[test]
    fn test_timestamps_not_parsed_in_code_span() {
        let ts = parse_timestamps("`@due(2026-01-01)` visible", 0);
        assert_eq!(ts.len(), 0);
    }

    #[test]
    fn test_multiple_timestamps() {
        let ts = parse_timestamps("@due(2026-01-01) and @start(2026-02-01)", 0);
        assert_eq!(ts.len(), 2);
    }

    // --- Block ID tests ---

    #[test]
    fn test_parse_block_id() {
        let bid = parse_block_id("Some text ^my-block", 0).unwrap();
        assert_eq!(bid.id, BlockId("my-block".to_string()));
    }

    #[test]
    fn test_parse_block_id_standalone() {
        let bid = parse_block_id("^solo-block", 0).unwrap();
        assert_eq!(bid.id, BlockId("solo-block".to_string()));
    }

    #[test]
    fn test_no_block_id() {
        assert!(parse_block_id("No block id here", 0).is_none());
    }

    #[test]
    fn test_block_id_with_underscores() {
        let bid = parse_block_id("Text ^my_block_2", 0).unwrap();
        assert_eq!(bid.id, BlockId("my_block_2".to_string()));
    }
}
