// Syntax highlighting — generates StyledSpans from parsed content.
//
// This module bridges the parser output to the render layer.
// It does NOT do full document re-parsing per render; instead it
// operates on single lines with lightweight context (are we in
// frontmatter? in a code fence?).

use crate::parser;
use crate::render::{Style, StyledSpan};

/// Context carried across lines during highlighting.
#[derive(Debug, Clone)]
pub struct HighlightContext {
    /// True while inside --- frontmatter --- delimiters.
    pub in_frontmatter: bool,
    /// True while inside a fenced code block.
    pub in_code_fence: bool,
    /// The fence marker character (` or ~) if in a code fence.
    pub fence_marker: Option<char>,
    /// Whether we've seen the opening --- yet.
    pub seen_opening_fence: bool,
}

impl HighlightContext {
    pub fn new() -> Self {
        Self {
            in_frontmatter: false,
            in_code_fence: false,
            fence_marker: None,
            seen_opening_fence: false,
        }
    }
}

/// Highlight a single line, returning styled spans and updating context.
pub fn highlight_line(line: &str, ctx: &mut HighlightContext) -> Vec<StyledSpan> {
    let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
    let len = trimmed.len();

    if len == 0 {
        return vec![];
    }

    // --- Frontmatter handling ---
    if trimmed == "---" {
        if !ctx.seen_opening_fence {
            ctx.seen_opening_fence = true;
            ctx.in_frontmatter = true;
            return vec![StyledSpan {
                start: 0,
                end: len,
                style: Style::Frontmatter,
            }];
        } else if ctx.in_frontmatter {
            ctx.in_frontmatter = false;
            return vec![StyledSpan {
                start: 0,
                end: len,
                style: Style::Frontmatter,
            }];
        }
    }

    if ctx.in_frontmatter {
        return vec![StyledSpan {
            start: 0,
            end: len,
            style: Style::Frontmatter,
        }];
    }

    // --- Code fence handling ---
    let trimmed_start = trimmed.trim_start();
    if let Some(marker) = detect_fence(trimmed_start) {
        if ctx.in_code_fence {
            if ctx.fence_marker == Some(marker) {
                ctx.in_code_fence = false;
                ctx.fence_marker = None;
            }
        } else {
            ctx.in_code_fence = true;
            ctx.fence_marker = Some(marker);
        }
        return vec![StyledSpan {
            start: 0,
            end: len,
            style: Style::CodeBlock,
        }];
    }

    if ctx.in_code_fence {
        return vec![StyledSpan {
            start: 0,
            end: len,
            style: Style::CodeBlock,
        }];
    }

    // --- Normal content: parse extensions + structural elements ---
    let mut spans = Vec::new();

    // Heading detection
    let ltrimmed = trimmed.trim_start();
    let hashes = ltrimmed.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&hashes) && ltrimmed.chars().nth(hashes) == Some(' ') {
        spans.push(StyledSpan {
            start: 0,
            end: len,
            style: Style::Heading {
                level: hashes as u8,
            },
        });
        // Still extract inline extensions within the heading
    }

    // List marker / checkbox detection
    if ltrimmed.starts_with("- [x] ") || ltrimmed.starts_with("- [X] ") {
        let offset = trimmed.len() - ltrimmed.len();
        spans.push(StyledSpan {
            start: offset,
            end: offset + 6,
            style: Style::CheckboxChecked,
        });
        // Apply dim+strikethrough to the whole line content after checkbox
        if offset + 6 < len {
            spans.push(StyledSpan {
                start: offset + 6,
                end: len,
                style: Style::CheckboxChecked,
            });
        }
    } else if ltrimmed.starts_with("- [ ] ") {
        let offset = trimmed.len() - ltrimmed.len();
        spans.push(StyledSpan {
            start: offset,
            end: offset + 6,
            style: Style::CheckboxUnchecked,
        });
    } else if ltrimmed.starts_with("- ") || ltrimmed.starts_with("* ") {
        let offset = trimmed.len() - ltrimmed.len();
        spans.push(StyledSpan {
            start: offset,
            end: offset + 2,
            style: Style::ListMarker,
        });
    } else {
        // Ordered list: digits followed by ". "
        let digits = ltrimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits > 0 && ltrimmed[digits..].starts_with(". ") {
            let offset = trimmed.len() - ltrimmed.len();
            spans.push(StyledSpan {
                start: offset,
                end: offset + digits + 2,
                style: Style::ListMarker,
            });
        }
    }

    // Use parser's inline extraction for Bloom extensions
    let code_spans = parser_inline_code_ranges(trimmed);

    // Links and embeds
    extract_link_spans(trimmed, &code_spans, &mut spans);

    // Tags
    extract_tag_spans(trimmed, &code_spans, &mut spans);

    // Timestamps
    extract_timestamp_spans(trimmed, &code_spans, &mut spans);

    // Block IDs
    extract_block_id_spans(trimmed, &code_spans, &mut spans);

    // Inline code spans themselves
    for cs in &code_spans {
        spans.push(StyledSpan {
            start: cs.start,
            end: cs.end,
            style: Style::Code,
        });
    }

    spans
}

// --- Helper: detect code fence ---
fn detect_fence(line: &str) -> Option<char> {
    let first = line.chars().next()?;
    if first != '`' && first != '~' {
        return None;
    }
    let count = line.chars().take_while(|c| *c == first).count();
    if count >= 3 {
        Some(first)
    } else {
        None
    }
}

// --- Inline code ranges (simplified from parser) ---
fn parser_inline_code_ranges(line: &str) -> Vec<std::ops::Range<usize>> {
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

fn in_code_span(spans: &[std::ops::Range<usize>], idx: usize) -> bool {
    spans.iter().any(|s| idx >= s.start && idx < s.end)
}

// --- Link/embed span extraction ---
fn extract_link_spans(
    line: &str,
    code_spans: &[std::ops::Range<usize>],
    out: &mut Vec<StyledSpan>,
) {
    let mut i = 0usize;
    while i < line.len() {
        if !line.is_char_boundary(i) {
            i += 1;
            continue;
        }
        let rest = &line[i..];
        let (is_embed, open_len) = if rest.starts_with("![[") {
            (true, 3)
        } else if rest.starts_with("[[") {
            (false, 2)
        } else {
            i += rest.chars().next().map(|c| c.len_utf8()).unwrap_or(1);
            continue;
        };

        if in_code_span(code_spans, i) {
            i += open_len;
            continue;
        }

        let inner_start = i + open_len;
        if let Some(close_rel) = line[inner_start..].find("]]") {
            let end = inner_start + close_rel + 2;
            out.push(StyledSpan {
                start: i,
                end,
                style: if is_embed { Style::Embed } else { Style::Link },
            });
            i = end;
        } else {
            i += open_len;
        }
    }
}

// --- Tag span extraction ---
fn extract_tag_spans(
    line: &str,
    code_spans: &[std::ops::Range<usize>],
    out: &mut Vec<StyledSpan>,
) {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    for idx in 0..chars.len() {
        let (hash_pos, ch) = chars[idx];
        if ch != '#' || in_code_span(code_spans, hash_pos) {
            continue;
        }
        let at_start = idx == 0;
        let preceded_by_ws = !at_start && chars[idx - 1].1.is_whitespace();
        if !(at_start || preceded_by_ws) {
            continue;
        }
        let Some((_, first_char)) = chars.get(idx + 1).copied() else {
            continue;
        };
        if !first_char.is_alphabetic() {
            continue;
        }
        let mut end = hash_pos + 1 + first_char.len_utf8();
        let mut j = idx + 2;
        while let Some((pos, c)) = chars.get(j).copied() {
            if c.is_alphanumeric() || c == '_' || c == '-' {
                end = pos + c.len_utf8();
                j += 1;
            } else {
                break;
            }
        }
        out.push(StyledSpan {
            start: hash_pos,
            end,
            style: Style::Tag,
        });
    }
}

// --- Timestamp span extraction ---
fn extract_timestamp_spans(
    line: &str,
    code_spans: &[std::ops::Range<usize>],
    out: &mut Vec<StyledSpan>,
) {
    for prefix in &["@due(", "@start(", "@at("] {
        let mut search_from = 0;
        while let Some(pos) = line[search_from..].find(prefix) {
            let abs_pos = search_from + pos;
            if in_code_span(code_spans, abs_pos) {
                search_from = abs_pos + prefix.len();
                continue;
            }
            if let Some(close_rel) = line[abs_pos + prefix.len()..].find(')') {
                let end = abs_pos + prefix.len() + close_rel + 1;
                out.push(StyledSpan {
                    start: abs_pos,
                    end,
                    style: Style::Timestamp,
                });
                search_from = end;
            } else {
                break;
            }
        }
    }
}

// --- Block ID span extraction ---
fn extract_block_id_spans(
    line: &str,
    code_spans: &[std::ops::Range<usize>],
    out: &mut Vec<StyledSpan>,
) {
    let trimmed = line.trim_end();
    if let Some(caret) = trimmed.rfind('^') {
        if in_code_span(code_spans, caret) {
            return;
        }
        if caret == 0 {
            return;
        }
        if let Some(prev) = trimmed[..caret].chars().last() {
            if !prev.is_whitespace() {
                return;
            }
        }
        let id = &trimmed[(caret + 1)..];
        if !id.is_empty()
            && id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            out.push(StyledSpan {
                start: caret,
                end: trimmed.len(),
                style: Style::BlockId,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heading_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true; // past frontmatter
        let spans = highlight_line("## My Heading", &mut ctx);
        assert!(
            spans.iter().any(|s| matches!(s.style, Style::Heading { level: 2 })),
            "should have H2 span"
        );
    }

    #[test]
    fn link_and_tag_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let spans = highlight_line("See [[abc|Page]] and #tag here", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::Link), "should have link span");
        assert!(spans.iter().any(|s| s.style == Style::Tag), "should have tag span");
    }

    #[test]
    fn frontmatter_spans() {
        let mut ctx = HighlightContext::new();
        let s1 = highlight_line("---", &mut ctx);
        assert!(s1.iter().any(|s| s.style == Style::Frontmatter));
        assert!(ctx.in_frontmatter);

        let s2 = highlight_line("id: abc123", &mut ctx);
        assert!(s2.iter().any(|s| s.style == Style::Frontmatter));

        let s3 = highlight_line("---", &mut ctx);
        assert!(s3.iter().any(|s| s.style == Style::Frontmatter));
        assert!(!ctx.in_frontmatter);
    }

    #[test]
    fn code_fence_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let s1 = highlight_line("```rust", &mut ctx);
        assert!(s1.iter().any(|s| s.style == Style::CodeBlock));
        assert!(ctx.in_code_fence);

        let s2 = highlight_line("let x = 42;", &mut ctx);
        assert!(s2.iter().any(|s| s.style == Style::CodeBlock));

        let s3 = highlight_line("```", &mut ctx);
        assert!(!ctx.in_code_fence);
    }

    #[test]
    fn code_block_hides_extensions() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        highlight_line("```", &mut ctx);
        let spans = highlight_line("[[abc|link]] #tag @due(2026-01-01)", &mut ctx);
        // Everything should be CodeBlock, no Link/Tag/Timestamp
        assert!(spans.iter().all(|s| s.style == Style::CodeBlock));
    }

    #[test]
    fn checkbox_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let spans = highlight_line("- [ ] unchecked task", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::CheckboxUnchecked));

        let spans = highlight_line("- [x] done task", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::CheckboxChecked));
    }

    #[test]
    fn timestamp_and_block_id_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let spans = highlight_line("task @due(2026-03-01) ^task-1", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::Timestamp));
        assert!(spans.iter().any(|s| s.style == Style::BlockId));
    }

    #[test]
    fn embed_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let spans = highlight_line("See ![[abc|Embed]]", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::Embed));
    }

    #[test]
    fn inline_code_spans() {
        let mut ctx = HighlightContext::new();
        ctx.seen_opening_fence = true;
        let spans = highlight_line("Use `ropey` crate", &mut ctx);
        assert!(spans.iter().any(|s| s.style == Style::Code));
    }
}
