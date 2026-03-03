use std::ops::Range;

use chrono::{NaiveDate, NaiveDateTime};

use crate::types::{BlockId, PageId, TagName, Timestamp};

use super::traits::{ParsedBlockId, ParsedLink, ParsedTag, ParsedTask, ParsedTimestamp};

/// Parse all `[[target|display]]` links from a line (not inside code spans).
pub fn parse_links(line: &str, line_number: usize) -> Vec<ParsedLink> {
    let mut links = Vec::new();
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

                if let Some(link) = parse_link_content(content, line_number, start..end) {
                    links.push(link);
                }
            }
            continue;
        }
        i += 1;
    }
    links
}

fn parse_link_content(content: &str, line: usize, byte_range: Range<usize>) -> Option<ParsedLink> {
    // Format: target or target|display or target#section|display
    let (target_part, display) = if let Some(pipe_pos) = content.find('|') {
        (&content[..pipe_pos], content[pipe_pos + 1..].to_string())
    } else {
        (content, content.to_string())
    };

    let (id_str, section) = if let Some(hash_pos) = target_part.find('#') {
        let section_str = &target_part[hash_pos + 1..];
        (&target_part[..hash_pos], Some(BlockId(section_str.to_string())))
    } else {
        (target_part, None)
    };

    let target = PageId::from_hex(id_str)?;

    Some(ParsedLink {
        target,
        section,
        display_hint: display,
        line,
        byte_range,
    })
}

/// Parse `^block-id` at the end of a line.
pub fn parse_block_id(line: &str, line_number: usize) -> Option<ParsedBlockId> {
    let trimmed = line.trim_end();
    // Must be preceded by a space (or be the entire line)
    if let Some(pos) = trimmed.rfind(" ^") {
        let id_str = &trimmed[pos + 2..];
        if !id_str.is_empty() && id_str.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Some(ParsedBlockId {
                id: BlockId(id_str.to_string()),
                line: line_number,
            });
        }
    }
    // Line that is solely ^block-id
    if let Some(rest) = trimmed.strip_prefix('^') {
        if !rest.is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
            return Some(ParsedBlockId {
                id: BlockId(rest.to_string()),
                line: line_number,
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
            let preceded_by_ws = i == 0 || line.as_bytes()[i - 1].is_ascii_whitespace();
            if preceded_by_ws {
                // Check if this is a heading (# at start of line followed by space)
                if i == 0 {
                    // Check if this is a heading pattern
                    let rest = &line[i..];
                    let hash_count = rest.bytes().take_while(|&b| b == b'#').count();
                    if hash_count <= 6 && line.len() > i + hash_count && bytes[i + hash_count] == b' ' {
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
                    TimestampKind::Due => {
                        NaiveDate::parse_from_str(inner, "%Y-%m-%d").ok().map(Timestamp::Due)
                    }
                    TimestampKind::Start => {
                        NaiveDate::parse_from_str(inner, "%Y-%m-%d").ok().map(Timestamp::Start)
                    }
                    TimestampKind::At => {
                        NaiveDateTime::parse_from_str(inner, "%Y-%m-%d %H:%M")
                            .ok()
                            .map(Timestamp::At)
                            .or_else(|| {
                                NaiveDate::parse_from_str(inner, "%Y-%m-%d")
                                    .ok()
                                    .map(|d| Timestamp::At(d.and_hms_opt(0, 0, 0).unwrap()))
                            })
                    }
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