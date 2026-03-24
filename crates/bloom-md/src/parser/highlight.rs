use super::traits::{LineContext, Style, StyledSpan};

/// Highlight a single line, producing styled spans. Respects LineContext.
pub fn highlight_line(line: &str, context: &LineContext) -> Vec<StyledSpan> {
    if line.is_empty() {
        return vec![];
    }

    // Inside frontmatter: parse YAML key/value pairs with per-field styles
    if context.in_frontmatter {
        return highlight_frontmatter_line(line);
    }

    // Code fence lines (``` or ~~~): always SyntaxNoise, whether opening or closing.
    let trimmed_for_fence = line.trim_start();
    if trimmed_for_fence.starts_with("```") || trimmed_for_fence.starts_with("~~~") {
        return vec![StyledSpan {
            byte_range: 0..line.len(),
            style: Style::SyntaxNoise,
        }];
    }

    // Inside code block: everything is CodeBlock style
    if context.in_code_block {
        return vec![StyledSpan {
            byte_range: 0..line.len(),
            style: Style::CodeBlock,
        }];
    }

    let mut spans = Vec::new();
    let bytes = line.as_bytes();
    let len = bytes.len();

    // Check for heading
    if bytes[0] == b'#' {
        let level = bytes.iter().take_while(|&&b| b == b'#').count();
        if level <= 6 && len > level && bytes[level] == b' ' {
            // Heading markers are SyntaxNoise
            spans.push(StyledSpan {
                byte_range: 0..level + 1,
                style: Style::SyntaxNoise,
            });
            // Check for trailing block ID (e.g. "## Heading ^block-id")
            let content = &line[level + 1..];
            if let Some(caret_rel) = content.rfind(" ^") {
                let after_caret = &content[caret_rel + 2..];
                if !after_caret.is_empty()
                    && after_caret
                        .trim_end()
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '=')
                {
                    let abs_caret = level + 1 + caret_rel;
                    let block_end = line.trim_end().len();
                    // Heading text before block ID
                    if abs_caret > level + 1 {
                        spans.push(StyledSpan {
                            byte_range: level + 1..abs_caret,
                            style: Style::Heading { level: level as u8 },
                        });
                    }
                    // Space before caret
                    spans.push(StyledSpan {
                        byte_range: abs_caret..abs_caret + 1,
                        style: Style::Heading { level: level as u8 },
                    });
                    // Caret
                    spans.push(StyledSpan {
                        byte_range: abs_caret + 1..abs_caret + 2,
                        style: Style::BlockIdCaret,
                    });
                    // ID text
                    spans.push(StyledSpan {
                        byte_range: abs_caret + 2..block_end,
                        style: Style::BlockId,
                    });
                    return spans;
                }
            }
            spans.push(StyledSpan {
                byte_range: level + 1..len,
                style: Style::Heading { level: level as u8 },
            });
            return spans;
        }
    }

    // Check for blockquote
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if trimmed.starts_with("> ") || trimmed == ">" {
        spans.push(StyledSpan {
            byte_range: indent..indent + 2.min(trimmed.len()),
            style: Style::BlockquoteMarker,
        });
        if trimmed.len() > 2 {
            let content_start = indent + 2;
            let mut inline_spans = Vec::new();
            highlight_inline(line, content_start, &mut inline_spans);
            // Replace Normal spans with Blockquote — keep special styles (BlockId, Tag, etc.)
            for s in inline_spans {
                spans.push(StyledSpan {
                    byte_range: s.byte_range,
                    style: if s.style == Style::Normal {
                        Style::Blockquote
                    } else {
                        s.style
                    },
                });
            }
        }
        return spans;
    }

    // Check for table alignment row (e.g. |---|:---:|---:|)
    if is_table_alignment_row(trimmed) {
        spans.push(StyledSpan {
            byte_range: 0..len,
            style: Style::TableAlignmentRow,
        });
        return spans;
    }

    // Check for table row (starts with |)
    if trimmed.starts_with('|') && trimmed.ends_with('|') && trimmed.len() > 1 {
        highlight_table_row(line, &mut spans);
        return spans;
    }

    // Check for list marker / checkbox at start
    if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        // "-" is ListMarker, " " normal, "[x]" is CheckboxChecked, rest is CheckedTaskText
        spans.push(StyledSpan {
            byte_range: indent..indent + 1,
            style: Style::ListMarker,
        });
        spans.push(StyledSpan {
            byte_range: indent + 1..indent + 2,
            style: Style::Normal,
        });
        spans.push(StyledSpan {
            byte_range: indent + 2..indent + 5,
            style: Style::CheckboxChecked,
        });
        if len > indent + 6 {
            // Check for trailing block ID: " ^xxxxx" or " ^=xxxxx"
            let rest = &line[indent + 6..];
            if let Some(caret_pos) = rest.rfind(" ^") {
                let text_end = indent + 6 + caret_pos;
                let block_start = text_end + 1; // position of ^
                                                // CheckedTaskText for the body, then BlockId spans for the ID
                if text_end > indent + 6 {
                    spans.push(StyledSpan {
                        byte_range: indent + 6..text_end,
                        style: Style::CheckedTaskText,
                    });
                }
                // Space before block ID
                spans.push(StyledSpan {
                    byte_range: text_end..block_start,
                    style: Style::CheckedTaskText,
                });
                // Caret (^ or ^=)
                let id_start = if line.as_bytes().get(block_start + 1) == Some(&b'=') {
                    spans.push(StyledSpan {
                        byte_range: block_start..block_start + 2,
                        style: Style::BlockIdCaret,
                    });
                    block_start + 2
                } else {
                    spans.push(StyledSpan {
                        byte_range: block_start..block_start + 1,
                        style: Style::BlockIdCaret,
                    });
                    block_start + 1
                };
                if id_start < len {
                    spans.push(StyledSpan {
                        byte_range: id_start..len,
                        style: Style::BlockId,
                    });
                }
            } else {
                spans.push(StyledSpan {
                    byte_range: indent + 6..len,
                    style: Style::CheckedTaskText,
                });
            }
        }
        return spans;
    }

    if trimmed.starts_with("- [ ] ") {
        // "-" is ListMarker, " " normal, "[ ]" is CheckboxUnchecked
        spans.push(StyledSpan {
            byte_range: indent..indent + 1,
            style: Style::ListMarker,
        });
        spans.push(StyledSpan {
            byte_range: indent + 1..indent + 2,
            style: Style::Normal,
        });
        spans.push(StyledSpan {
            byte_range: indent + 2..indent + 5,
            style: Style::CheckboxUnchecked,
        });
        highlight_inline(line, indent + 6, &mut spans);
        return spans;
    }

    if trimmed.starts_with("- ") {
        spans.push(StyledSpan {
            byte_range: indent..indent + 2,
            style: Style::ListMarker,
        });
        highlight_inline(line, indent + 2, &mut spans);
        return spans;
    }

    // Check for numbered list
    if let Some(dot_pos) = trimmed.find(". ") {
        let num_part = &trimmed[..dot_pos];
        if !num_part.is_empty() && num_part.chars().all(|c| c.is_ascii_digit()) {
            spans.push(StyledSpan {
                byte_range: indent..indent + dot_pos + 2,
                style: Style::ListMarker,
            });
            highlight_inline(line, indent + dot_pos + 2, &mut spans);
            return spans;
        }
    }

    // Default: inline highlighting from the start
    highlight_inline(line, 0, &mut spans);
    spans
}

/// Highlight inline elements (bold, italic, code, links, tags, timestamps, block-ids)
/// starting from `offset` in the line.
fn highlight_inline(line: &str, offset: usize, spans: &mut Vec<StyledSpan>) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = offset;
    let mut normal_start = i;

    while i < len {
        // Inline code
        if bytes[i] == b'`' {
            flush_normal(normal_start, i, spans);
            let code_start = i;
            i += 1;
            while i < len && bytes[i] != b'`' {
                i += 1;
            }
            if i < len {
                i += 1; // closing `
            }
            spans.push(StyledSpan {
                byte_range: code_start..i,
                style: Style::Code,
            });
            normal_start = i;
            continue;
        }

        // Wiki-links [[...]]
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            flush_normal(normal_start, i, spans);
            // Opening [[ is link chrome
            spans.push(StyledSpan {
                byte_range: i..i + 2,
                style: Style::LinkChrome,
            });
            i += 2;
            let content_start = i;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            let content_end = i;

            // Parse link content: page_id|display or ^block_id|display
            let content = &line[content_start..content_end];
            let target_str = content.split('|').next().unwrap_or(content);
            let is_block_link = target_str.starts_with('^');
            let is_valid = if is_block_link {
                target_str.len() > 1 // ^id must have at least one char after ^
            } else {
                crate::types::PageId::from_hex(target_str).is_some()
            };

            // Split into uuid (chrome) and display text
            if let Some(pipe_pos) = content.find('|') {
                let uuid_end = content_start + pipe_pos;
                // UUID part — chrome
                spans.push(StyledSpan {
                    byte_range: content_start..uuid_end,
                    style: Style::LinkChrome,
                });
                // Pipe — chrome
                spans.push(StyledSpan {
                    byte_range: uuid_end..uuid_end + 1,
                    style: Style::LinkChrome,
                });
                // Display text
                spans.push(StyledSpan {
                    byte_range: uuid_end + 1..content_end,
                    style: if is_valid {
                        Style::LinkText
                    } else {
                        Style::BrokenLink
                    },
                });
            } else {
                // No pipe — whole content is the link
                spans.push(StyledSpan {
                    byte_range: content_start..content_end,
                    style: if is_valid {
                        Style::LinkText
                    } else {
                        Style::BrokenLink
                    },
                });
            }

            if i + 1 < len {
                spans.push(StyledSpan {
                    byte_range: i..i + 2,
                    style: Style::LinkChrome,
                });
                i += 2;
            }
            normal_start = i;
            continue;
        }

        // Bold **...**
        if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'*' {
            flush_normal(normal_start, i, spans);
            spans.push(StyledSpan {
                byte_range: i..i + 2,
                style: Style::SyntaxNoise,
            });
            i += 2;
            let bold_start = i;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'*') {
                i += 1;
            }
            spans.push(StyledSpan {
                byte_range: bold_start..i,
                style: Style::Bold,
            });
            if i + 1 < len {
                spans.push(StyledSpan {
                    byte_range: i..i + 2,
                    style: Style::SyntaxNoise,
                });
                i += 2;
            }
            normal_start = i;
            continue;
        }

        // Italic *...*
        if bytes[i] == b'*' && (i + 1 >= len || bytes[i + 1] != b'*') {
            flush_normal(normal_start, i, spans);
            spans.push(StyledSpan {
                byte_range: i..i + 1,
                style: Style::SyntaxNoise,
            });
            i += 1;
            let italic_start = i;
            while i < len && bytes[i] != b'*' {
                i += 1;
            }
            spans.push(StyledSpan {
                byte_range: italic_start..i,
                style: Style::Italic,
            });
            if i < len {
                spans.push(StyledSpan {
                    byte_range: i..i + 1,
                    style: Style::SyntaxNoise,
                });
                i += 1;
            }
            normal_start = i;
            continue;
        }

        // Tags #tagname
        if bytes[i] == b'#' {
            let preceded_by_ws =
                i == 0 || line[..i].chars().last().is_none_or(|c| c.is_whitespace());
            if preceded_by_ws {
                let tag_start = i;
                i += 1;
                if i < len {
                    let first_char = line[i..].chars().next().unwrap();
                    if first_char.is_alphabetic() {
                        i += first_char.len_utf8();
                        while i < len {
                            let ch = line[i..].chars().next().unwrap();
                            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                                i += ch.len_utf8();
                            } else {
                                break;
                            }
                        }
                        flush_normal(normal_start, tag_start, spans);
                        spans.push(StyledSpan {
                            byte_range: tag_start..i,
                            style: Style::Tag,
                        });
                        normal_start = i;
                        continue;
                    }
                }
                // Not a valid tag, continue
                i = tag_start + 1;
                continue;
            }
        }

        // Timestamps @due(...), @start(...), @at(...)
        if bytes[i] == b'@' {
            let ts_start = i;
            if let Some((keyword_end, date_start, date_end, close)) =
                try_match_timestamp_parts(line, i)
            {
                flush_normal(normal_start, ts_start, spans);
                let keyword = &line[ts_start..keyword_end];
                // Keyword: @due, @start, @at
                spans.push(StyledSpan {
                    byte_range: ts_start..keyword_end,
                    style: Style::TimestampKeyword,
                });
                // Opening paren
                spans.push(StyledSpan {
                    byte_range: keyword_end..date_start,
                    style: Style::TimestampParens,
                });
                // Date value — only @due can be overdue
                let date_str = &line[date_start..date_end];
                let is_overdue = keyword == "@due" && is_overdue_date(date_str);
                spans.push(StyledSpan {
                    byte_range: date_start..date_end,
                    style: if is_overdue {
                        Style::TimestampOverdue
                    } else {
                        Style::TimestampDate
                    },
                });
                // Closing paren
                spans.push(StyledSpan {
                    byte_range: date_end..close,
                    style: Style::TimestampParens,
                });
                i = close;
                normal_start = i;
                continue;
            }
        }

        // Block ID ^id or ^=id at end of line (after space)
        if bytes[i] == b'^' && (i == 0 || bytes[i - 1] == b' ') {
            let rest = &line[i + 1..];
            if !rest.is_empty()
                && rest
                    .trim_end()
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_' || c == '=')
            {
                let block_end = line.trim_end().len();
                flush_normal(normal_start, i, spans);
                // Caret
                spans.push(StyledSpan {
                    byte_range: i..i + 1,
                    style: Style::BlockIdCaret,
                });
                // ID text
                spans.push(StyledSpan {
                    byte_range: i + 1..block_end,
                    style: Style::BlockId,
                });
                i = len;
                normal_start = i;
                continue;
            }
        }

        i += 1;
    }

    flush_normal(normal_start, len, spans);
}

fn flush_normal(start: usize, end: usize, spans: &mut Vec<StyledSpan>) {
    if start < end {
        spans.push(StyledSpan {
            byte_range: start..end,
            style: Style::Normal,
        });
    }
}

/// Try to match a timestamp and return (keyword_end, date_start, date_end, close).
/// E.g. for `@due(2026-03-05)`: keyword_end=4, date_start=5, date_end=15, close=16.
fn try_match_timestamp_parts(line: &str, pos: usize) -> Option<(usize, usize, usize, usize)> {
    let rest = &line[pos..];
    for prefix in &["@due(", "@start(", "@at("] {
        if rest.starts_with(prefix) {
            let keyword_end = pos + prefix.len() - 1; // position of '('
            let date_start = pos + prefix.len();
            if let Some(close_offset) = line[date_start..].find(')') {
                let date_end = date_start + close_offset;
                let close = date_end + 1;
                return Some((keyword_end, date_start, date_end, close));
            }
        }
    }
    None
}

/// Check if a date string is in the past (overdue). Only applies to @due() dates.
fn is_overdue_date(date_str: &str) -> bool {
    // Only check pure date strings (YYYY-MM-DD)
    if date_str.len() >= 10 {
        if let Ok(date) = chrono::NaiveDate::parse_from_str(&date_str[..10], "%Y-%m-%d") {
            return date < chrono::Local::now().date_naive();
        }
    }
    false
}

/// Highlight frontmatter lines with per-field styling.
fn highlight_frontmatter_line(line: &str) -> Vec<StyledSpan> {
    let len = line.len();

    // --- delimiters
    if line.trim() == "---" {
        return vec![StyledSpan {
            byte_range: 0..len,
            style: Style::SyntaxNoise,
        }];
    }

    // key: value lines
    if let Some(colon_pos) = line.find(": ") {
        let key = line[..colon_pos].trim();
        let value_start = colon_pos + 2;

        // Key span (including colon and space)
        let key_span = StyledSpan {
            byte_range: 0..value_start,
            style: Style::FrontmatterKey,
        };

        // Value span — style depends on the key
        let value_style = match key {
            "title" => Style::FrontmatterTitle,
            "id" => Style::FrontmatterId,
            "created" => Style::FrontmatterDate,
            "tags" => Style::FrontmatterTags,
            _ => Style::Frontmatter,
        };
        let value_span = StyledSpan {
            byte_range: value_start..len,
            style: value_style,
        };

        return vec![key_span, value_span];
    }

    // Fallback: entire line as generic frontmatter
    vec![StyledSpan {
        byte_range: 0..len,
        style: Style::Frontmatter,
    }]
}

/// Check if a line is a table alignment row (e.g. |---|:---:|---:|)
fn is_table_alignment_row(trimmed: &str) -> bool {
    if !trimmed.starts_with('|') || !trimmed.ends_with('|') || trimmed.len() < 3 {
        return false;
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    inner.split('|').all(|cell| {
        let c = cell.trim();
        !c.is_empty() && c.chars().all(|ch| ch == '-' || ch == ':')
    })
}

/// Highlight a table row, splitting pipes from cell content.
fn highlight_table_row(line: &str, spans: &mut Vec<StyledSpan>) {
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'|' {
            spans.push(StyledSpan {
                byte_range: i..i + 1,
                style: Style::TablePipe,
            });
            i += 1;
        } else {
            let start = i;
            while i < len && bytes[i] != b'|' {
                i += 1;
            }
            if start < i {
                spans.push(StyledSpan {
                    byte_range: start..i,
                    style: Style::Normal,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::traits::{LineContext, Style};

    // UC-16: Syntax highlighting

    #[test]
    fn test_highlight_heading() {
        let ctx = LineContext::default();
        let spans = highlight_line("## My Heading", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Heading { level: 2 }));
    }

    #[test]
    fn test_highlight_heading_level_1() {
        let ctx = LineContext::default();
        let spans = highlight_line("# Title", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Heading { level: 1 }));
    }

    #[test]
    fn test_highlight_link() {
        let ctx = LineContext::default();
        let spans = highlight_line("See [[8f3a1b2c|Link]]", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::LinkText));
        assert!(spans.iter().any(|s| s.style == Style::LinkChrome));
    }

    #[test]
    fn test_highlight_broken_link() {
        let ctx = LineContext::default();
        let spans = highlight_line("[[bad|Link]]", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::BrokenLink));
    }

    #[test]
    fn test_highlight_tag() {
        let ctx = LineContext::default();
        let spans = highlight_line("text #mytag here", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Tag));
    }

    #[test]
    fn test_highlight_timestamp() {
        let ctx = LineContext::default();
        let spans = highlight_line("task @due(2099-01-01)", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::TimestampKeyword));
        assert!(spans.iter().any(|s| s.style == Style::TimestampDate));
        assert!(spans.iter().any(|s| s.style == Style::TimestampParens));
    }

    #[test]
    fn test_highlight_timestamp_overdue() {
        let ctx = LineContext::default();
        let spans = highlight_line("task @due(2020-01-01)", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::TimestampOverdue));
    }

    #[test]
    fn test_highlight_block_id() {
        let ctx = LineContext::default();
        let spans = highlight_line("text ^my-block", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::BlockId));
        assert!(spans.iter().any(|s| s.style == Style::BlockIdCaret));
    }

    #[test]
    fn test_highlight_block_id_in_heading() {
        let ctx = LineContext::default();
        let spans = highlight_line("## My Heading ^sect-1", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Heading { level: 2 }));
        assert!(spans.iter().any(|s| s.style == Style::BlockIdCaret));
        assert!(spans.iter().any(|s| s.style == Style::BlockId));
    }

    #[test]
    fn test_highlight_block_id_in_blockquote() {
        let ctx = LineContext::default();
        let spans = highlight_line("> Some quoted text ^r4d8n", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::BlockquoteMarker));
        assert!(spans.iter().any(|s| s.style == Style::Blockquote));
        assert!(
            spans.iter().any(|s| s.style == Style::BlockIdCaret),
            "block ID caret should be styled in blockquote, spans: {:?}",
            spans
                .iter()
                .map(|s| format!("{:?} {:?}", s.byte_range, s.style))
                .collect::<Vec<_>>()
        );
        assert!(spans.iter().any(|s| s.style == Style::BlockId));
    }

    #[test]
    fn test_highlight_bold() {
        let ctx = LineContext::default();
        let spans = highlight_line("some **bold** text", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Bold));
    }

    #[test]
    fn test_highlight_italic() {
        let ctx = LineContext::default();
        let spans = highlight_line("some *italic* text", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Italic));
    }

    #[test]
    fn test_highlight_inline_code() {
        let ctx = LineContext::default();
        let spans = highlight_line("some `code` text", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::Code));
    }

    #[test]
    fn test_highlight_in_code_block_context() {
        let ctx = LineContext {
            in_code_block: true,
            in_frontmatter: false,
            code_fence_lang: None,
        };
        let spans = highlight_line("[[8f3a1b2c|Link]] #tag @due(2026-01-01)", &ctx);
        assert!(spans.iter().all(|s| s.style == Style::CodeBlock));
    }

    #[test]
    fn test_highlight_in_frontmatter_context() {
        let ctx = LineContext {
            in_code_block: false,
            in_frontmatter: true,
            code_fence_lang: None,
        };
        // Key-value line gets split into key + value styles
        let spans = highlight_line("title: \"My Page\"", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::FrontmatterKey));
        assert!(spans.iter().any(|s| s.style == Style::FrontmatterTitle));
    }

    #[test]
    fn test_highlight_frontmatter_id() {
        let ctx = LineContext {
            in_code_block: false,
            in_frontmatter: true,
            code_fence_lang: None,
        };
        let spans = highlight_line("id: 8f3a1b2c", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::FrontmatterKey));
        assert!(spans.iter().any(|s| s.style == Style::FrontmatterId));
    }

    #[test]
    fn test_highlight_frontmatter_delimiter() {
        let ctx = LineContext {
            in_code_block: false,
            in_frontmatter: true,
            code_fence_lang: None,
        };
        let spans = highlight_line("---", &ctx);
        assert!(spans.iter().all(|s| s.style == Style::SyntaxNoise));
    }

    #[test]
    fn test_highlight_checkbox_checked_splits_marker() {
        let ctx = LineContext::default();
        let spans = highlight_line("- [x] Done task", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
        assert!(spans.iter().any(|s| s.style == Style::CheckboxChecked));
        assert!(spans.iter().any(|s| s.style == Style::CheckedTaskText));
    }

    #[test]
    fn test_highlight_checkbox_unchecked_splits_marker() {
        let ctx = LineContext::default();
        let spans = highlight_line("- [ ] Open task", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
        assert!(spans.iter().any(|s| s.style == Style::CheckboxUnchecked));
    }

    #[test]
    fn test_highlight_blockquote() {
        let ctx = LineContext::default();
        let spans = highlight_line("> Quoted text", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::BlockquoteMarker));
        assert!(spans.iter().any(|s| s.style == Style::Blockquote));
    }

    #[test]
    fn test_highlight_table_row() {
        let ctx = LineContext::default();
        let spans = highlight_line("| Cell A | Cell B |", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::TablePipe));
        assert!(spans.iter().any(|s| s.style == Style::Normal));
    }

    #[test]
    fn test_highlight_table_alignment() {
        let ctx = LineContext::default();
        let spans = highlight_line("|---|:---:|", &ctx);
        assert!(spans.iter().all(|s| s.style == Style::TableAlignmentRow));
    }

    #[test]
    fn test_highlight_empty_line() {
        let ctx = LineContext::default();
        let spans = highlight_line("", &ctx);
        assert!(spans.is_empty());
    }

    #[test]
    fn test_highlight_checkbox_unchecked() {
        let ctx = LineContext::default();
        let spans = highlight_line("- [ ] Task", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::CheckboxUnchecked));
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
    }

    #[test]
    fn test_highlight_checkbox_checked() {
        let ctx = LineContext::default();
        let spans = highlight_line("- [x] Done", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::CheckboxChecked));
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
    }

    #[test]
    fn test_highlight_list_marker() {
        let ctx = LineContext::default();
        let spans = highlight_line("- Item", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
    }

    #[test]
    fn test_highlight_numbered_list() {
        let ctx = LineContext::default();
        let spans = highlight_line("1. First", &ctx);
        assert!(spans.iter().any(|s| s.style == Style::ListMarker));
    }
}
