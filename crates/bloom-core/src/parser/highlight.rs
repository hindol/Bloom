use super::traits::{LineContext, Style, StyledSpan};

/// Highlight a single line, producing styled spans. Respects LineContext.
pub fn highlight_line(line: &str, context: &LineContext) -> Vec<StyledSpan> {
    if line.is_empty() {
        return vec![];
    }

    // Inside frontmatter: everything is Frontmatter style
    if context.in_frontmatter {
        return vec![StyledSpan {
            range: 0..line.len(),
            style: Style::Frontmatter,
        }];
    }

    // Inside code block: everything is CodeBlock style
    if context.in_code_block {
        return vec![StyledSpan {
            range: 0..line.len(),
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
                range: 0..level + 1,
                style: Style::SyntaxNoise,
            });
            spans.push(StyledSpan {
                range: level + 1..len,
                style: Style::Heading { level: level as u8 },
            });
            return spans;
        }
    }

    // Check for list marker / checkbox at start
    let trimmed = line.trim_start();
    let indent = line.len() - trimmed.len();

    if trimmed.starts_with("- [x] ") || trimmed.starts_with("- [X] ") {
        spans.push(StyledSpan {
            range: indent..indent + 6,
            style: Style::CheckboxChecked,
        });
        highlight_inline(line, indent + 6, &mut spans);
        return spans;
    }

    if trimmed.starts_with("- [ ] ") {
        spans.push(StyledSpan {
            range: indent..indent + 6,
            style: Style::CheckboxUnchecked,
        });
        highlight_inline(line, indent + 6, &mut spans);
        return spans;
    }

    if trimmed.starts_with("- ") {
        spans.push(StyledSpan {
            range: indent..indent + 2,
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
                range: indent..indent + dot_pos + 2,
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
                range: code_start..i,
                style: Style::Code,
            });
            normal_start = i;
            continue;
        }

        // Wiki-links [[...]]
        if i + 1 < len && bytes[i] == b'[' && bytes[i + 1] == b'[' {
            flush_normal(normal_start, i, spans);
            let _link_start = i;
            // Opening [[ is noise
            spans.push(StyledSpan {
                range: i..i + 2,
                style: Style::SyntaxNoise,
            });
            i += 2;
            let content_start = i;
            while i + 1 < len && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            let content_end = i;

            // Determine if link target is valid (8-char hex)
            let content = &line[content_start..content_end];
            let target_str = content.split('|').next().unwrap_or(content);
            let target_str = target_str.split('#').next().unwrap_or(target_str);
            let link_style = if crate::types::PageId::from_hex(target_str).is_some() {
                Style::Link
            } else {
                Style::BrokenLink
            };

            spans.push(StyledSpan {
                range: content_start..content_end,
                style: link_style,
            });

            if i + 1 < len {
                spans.push(StyledSpan {
                    range: i..i + 2,
                    style: Style::SyntaxNoise,
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
                range: i..i + 2,
                style: Style::SyntaxNoise,
            });
            i += 2;
            let bold_start = i;
            while i + 1 < len && !(bytes[i] == b'*' && bytes[i + 1] == b'*') {
                i += 1;
            }
            spans.push(StyledSpan {
                range: bold_start..i,
                style: Style::Bold,
            });
            if i + 1 < len {
                spans.push(StyledSpan {
                    range: i..i + 2,
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
                range: i..i + 1,
                style: Style::SyntaxNoise,
            });
            i += 1;
            let italic_start = i;
            while i < len && bytes[i] != b'*' {
                i += 1;
            }
            spans.push(StyledSpan {
                range: italic_start..i,
                style: Style::Italic,
            });
            if i < len {
                spans.push(StyledSpan {
                    range: i..i + 1,
                    style: Style::SyntaxNoise,
                });
                i += 1;
            }
            normal_start = i;
            continue;
        }

        // Tags #tagname
        if bytes[i] == b'#' {
            let preceded_by_ws = i == 0 || bytes[i - 1].is_ascii_whitespace();
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
                            range: tag_start..i,
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
            if let Some(end) = try_match_timestamp(line, i) {
                flush_normal(normal_start, ts_start, spans);
                spans.push(StyledSpan {
                    range: ts_start..end,
                    style: Style::Timestamp,
                });
                i = end;
                normal_start = i;
                continue;
            }
        }

        // Block ID ^id at end of line (after space)
        if bytes[i] == b'^' && (i == 0 || bytes[i - 1] == b' ') {
            let rest = &line[i + 1..];
            if !rest.is_empty()
                && rest.trim_end().chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                let block_end = line.trim_end().len();
                flush_normal(normal_start, i, spans);
                spans.push(StyledSpan {
                    range: i..block_end,
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
            range: start..end,
            style: Style::Normal,
        });
    }
}

/// Try to match a timestamp pattern starting at `pos`. Returns the byte position after the closing `)`.
fn try_match_timestamp(line: &str, pos: usize) -> Option<usize> {
    let rest = &line[pos..];
    for prefix in &["@due(", "@start(", "@at("] {
        if rest.starts_with(prefix) {
            let inner_start = pos + prefix.len();
            if let Some(close) = line[inner_start..].find(')') {
                return Some(inner_start + close + 1);
            }
        }
    }
    None
}