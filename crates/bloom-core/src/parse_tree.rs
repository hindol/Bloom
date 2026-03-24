//! Persistent, incrementally-updated parse tree for each buffer.
//!
//! Stores per-line structural data (headings, block IDs, links, tags, tasks)
//! and line-end context (code block / frontmatter state). Enables O(1) context
//! lookup for rendering and instant structural queries for features like
//! section mirroring, jump-to-heading, and link validation.
//!
//! See `docs/PARSE_TREE.md` for design rationale.

use std::ops::Range;

use bloom_md::parser::extensions::{parse_heading, parse_line, LineElements};
use bloom_md::parser::traits::{LineContext, ParsedBlockId, Section};
use bloom_md::types::BlockId;

/// Per-line parse result: structural elements + context flowing to the next line.
#[derive(Clone, Debug)]
pub struct LineData {
    pub elements: LineElements,
    pub heading: Option<(u8, String)>,
    pub context_out: LineContext,
}

/// Persistent parse tree for one buffer.
///
/// Built on buffer open (full parse), incrementally invalidated on edits.
/// Bundled with the buffer in `ManagedBuffer` — same lifecycle.
pub struct ParseTree {
    lines: Vec<LineData>,
    dirty: Option<Range<usize>>,
}

impl ParseTree {
    /// Build a full parse tree from buffer text.
    pub fn build(text: &str) -> Self {
        let raw_lines: Vec<&str> = text.split('\n').collect();
        let mut lines = Vec::with_capacity(raw_lines.len());

        let mut ctx = LineContext::default();
        let mut seen_first_fm_delimiter = false;

        for (idx, raw) in raw_lines.iter().enumerate() {
            let line = raw.trim_end_matches('\r');
            let trimmed = line.trim();

            // Track frontmatter state
            let mut next_ctx = ctx.clone();
            if idx == 0 && trimmed == "---" {
                next_ctx.in_frontmatter = true;
                seen_first_fm_delimiter = true;
            } else if ctx.in_frontmatter && seen_first_fm_delimiter && trimmed == "---" {
                next_ctx.in_frontmatter = false;
            }

            // Track code fence state (only outside frontmatter)
            if !ctx.in_frontmatter
                && !next_ctx.in_frontmatter
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                if ctx.in_code_block {
                    next_ctx.in_code_block = false;
                    next_ctx.code_fence_lang = None;
                } else {
                    next_ctx.in_code_block = true;
                    let lang = trimmed
                        .trim_start_matches('`')
                        .trim_start_matches('~')
                        .trim();
                    next_ctx.code_fence_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
                }
            }

            // Parse structural elements (skip inside code blocks and frontmatter)
            let elements = if ctx.in_code_block || ctx.in_frontmatter {
                LineElements::default()
            } else {
                parse_line(line, idx)
            };

            let heading = if !ctx.in_code_block && !ctx.in_frontmatter {
                parse_heading(line).map(|(level, title)| (level, title.to_string()))
            } else {
                None
            };

            lines.push(LineData {
                elements,
                heading,
                context_out: next_ctx.clone(),
            });

            ctx = next_ctx;
        }

        Self { lines, dirty: None }
    }

    /// Number of parsed lines.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Whether this parse tree has dirty lines needing re-parse.
    pub fn is_dirty(&self) -> bool {
        self.dirty.is_some()
    }

    /// Get the parse data for a specific line.
    pub fn line(&self, idx: usize) -> Option<&LineData> {
        self.lines.get(idx)
    }

    /// Get the `LineContext` that flows INTO `line_idx`.
    /// For line 0, this is the default context. For line N, it's line N-1's `context_out`.
    pub fn context_before(&self, line_idx: usize) -> LineContext {
        if line_idx == 0 {
            LineContext::default()
        } else {
            self.lines
                .get(line_idx - 1)
                .map(|ld| ld.context_out.clone())
                .unwrap_or_default()
        }
    }

    // --- Incremental invalidation ---

    /// Mark a range of lines as dirty (needing re-parse).
    /// Called after buffer edits. `new_line_count` is the buffer's line count
    /// after the edit (the ParseTree may need to grow or shrink).
    pub fn mark_dirty(&mut self, start: usize, old_end: usize, new_end: usize) {
        // Resize the lines vec to match new buffer length
        let delta = new_end as isize - old_end as isize;
        if delta > 0 {
            // Lines inserted — add placeholders
            for _ in 0..delta {
                self.lines.insert(
                    new_end.min(self.lines.len()),
                    LineData {
                        elements: LineElements::default(),
                        heading: None,
                        context_out: LineContext::default(),
                    },
                );
            }
        } else if delta < 0 {
            // Lines removed
            let remove_count = (-delta) as usize;
            let remove_start = start;
            let remove_end = (remove_start + remove_count).min(self.lines.len());
            if remove_start < remove_end {
                self.lines.drain(remove_start..remove_end);
            }
        }

        // Merge with existing dirty range
        let new_dirty = start..new_end.max(start + 1);
        self.dirty = Some(match self.dirty.take() {
            Some(existing) => {
                existing.start.min(new_dirty.start)..existing.end.max(new_dirty.end)
            }
            None => new_dirty,
        });
    }

    /// Re-parse dirty lines using current buffer content.
    /// Returns true if any context cascaded beyond the dirty range.
    pub fn refresh(&mut self, get_line: impl Fn(usize) -> String, line_count: usize) {
        // Ensure lines vec matches buffer
        while self.lines.len() < line_count {
            self.lines.push(LineData {
                elements: LineElements::default(),
                heading: None,
                context_out: LineContext::default(),
            });
        }
        self.lines.truncate(line_count);

        let Some(dirty) = self.dirty.take() else {
            return;
        };

        let start = dirty.start.min(line_count);
        let mut end = dirty.end.min(line_count);

        // Re-parse dirty lines + cascade if context changes
        let mut idx = start;
        while idx < line_count {
            let ctx_in = self.context_before(idx);
            let line_text = get_line(idx);
            let line = line_text.trim_end_matches(['\n', '\r']);
            let trimmed = line.trim();

            // Compute new context_out
            let mut next_ctx = ctx_in.clone();
            if idx == 0 && trimmed == "---" {
                next_ctx.in_frontmatter = true;
            } else if ctx_in.in_frontmatter && trimmed == "---" {
                next_ctx.in_frontmatter = false;
            }

            if !ctx_in.in_frontmatter
                && !next_ctx.in_frontmatter
                && (trimmed.starts_with("```") || trimmed.starts_with("~~~"))
            {
                if ctx_in.in_code_block {
                    next_ctx.in_code_block = false;
                    next_ctx.code_fence_lang = None;
                } else {
                    next_ctx.in_code_block = true;
                    let lang = trimmed
                        .trim_start_matches('`')
                        .trim_start_matches('~')
                        .trim();
                    next_ctx.code_fence_lang = if lang.is_empty() {
                        None
                    } else {
                        Some(lang.to_string())
                    };
                }
            }

            let elements = if ctx_in.in_code_block || ctx_in.in_frontmatter {
                LineElements::default()
            } else {
                parse_line(line, idx)
            };

            let heading = if !ctx_in.in_code_block && !ctx_in.in_frontmatter {
                parse_heading(line).map(|(level, title)| (level, title.to_string()))
            } else {
                None
            };

            let old_context = self.lines.get(idx).map(|ld| &ld.context_out).cloned();

            if idx < self.lines.len() {
                self.lines[idx] = LineData {
                    elements,
                    heading,
                    context_out: next_ctx.clone(),
                };
            }

            idx += 1;

            // Past the dirty range — check if context changed (cascade)
            if idx > end {
                if old_context.as_ref() == Some(&next_ctx) {
                    break; // context stable — stop cascading
                }
                end = (idx + 1).min(line_count); // cascade one more line
            }
        }
    }

    // --- Structural queries ---

    /// All sections derived from heading lines in the parse tree.
    /// A section extends from its heading until the next heading of equal or
    /// higher level (lower number), matching the parser's behavior.
    pub fn sections(&self) -> Vec<Section> {
        let mut sections = Vec::new();
        let mut stack: Vec<(u8, String, Option<BlockId>, usize)> = Vec::new();

        for (idx, ld) in self.lines.iter().enumerate() {
            if let Some((level, title)) = &ld.heading {
                // Close all sections at same or deeper level
                while let Some(top) = stack.last() {
                    if top.0 >= *level {
                        let (s_level, s_title, s_bid, s_start) = stack.pop().unwrap();
                        sections.push(Section {
                            level: s_level,
                            title: s_title,
                            block_id: s_bid,
                            line_range: s_start..idx,
                        });
                    } else {
                        break;
                    }
                }
                stack.push((
                    *level,
                    title.clone(),
                    ld.elements.block_id.as_ref().map(|b| b.id.clone()),
                    idx,
                ));
            }
        }
        // Close remaining sections
        while let Some((level, title, bid, start)) = stack.pop() {
            sections.push(Section {
                level,
                title,
                block_id: bid,
                line_range: start..self.lines.len(),
            });
        }
        // Sort by start line (stack pops in reverse order)
        sections.sort_by_key(|s| s.line_range.start);
        sections
    }

    /// All block IDs in the buffer.
    pub fn block_ids(&self) -> Vec<&ParsedBlockId> {
        self.lines
            .iter()
            .filter_map(|ld| ld.elements.block_id.as_ref())
            .collect()
    }

    /// Find all outermost `^=` heading sections (Rule 5: nested ^= = leaf only).
    pub fn mirror_sections(&self) -> Vec<Section> {
        let sections = self.sections();
        let mut result = Vec::new();
        let mut outer_end: usize = 0;

        for section in &sections {
            let is_mirror = section
                .block_id
                .as_ref()
                .and_then(|bid| {
                    self.lines.get(section.line_range.start).and_then(|ld| {
                        ld.elements
                            .block_id
                            .as_ref()
                            .filter(|b| b.id == *bid && b.is_mirror)
                    })
                })
                .is_some();

            if !is_mirror {
                continue;
            }

            // Rule 5: skip nested ^= headings inside an outer ^= section
            if section.line_range.start < outer_end {
                continue;
            }

            outer_end = section.line_range.end;
            result.push(section.clone());
        }
        result
    }

    /// Find the section (with ^= marker) enclosing a given line, if any.
    pub fn enclosing_mirror_section(&self, line: usize) -> Option<Section> {
        self.mirror_sections()
            .into_iter()
            .find(|s| s.line_range.contains(&line))
    }

    /// Find a section by its heading block ID.
    pub fn section_by_block_id(&self, block_id: &BlockId) -> Option<Section> {
        self.sections()
            .into_iter()
            .find(|s| s.block_id.as_ref() == Some(block_id))
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_simple_document() {
        let text = "---\ntitle: Test\n---\n\n## Heading ^=abc01\n\n- [ ] Task ^t0001\n";
        let tree = ParseTree::build(text);
        // split('\n') on trailing-\n text produces an extra empty element
        assert!(tree.len() >= 7);

        // Frontmatter lines have context
        assert!(tree.line(0).unwrap().context_out.in_frontmatter);
        assert!(!tree.line(2).unwrap().context_out.in_frontmatter);

        // Heading detected
        let h = tree.line(4).unwrap();
        assert_eq!(h.heading.as_ref().unwrap().0, 2);

        // Block IDs found
        let bids = tree.block_ids();
        assert_eq!(bids.len(), 2);
    }

    #[test]
    fn sections_from_tree() {
        let text = "## Alpha ^a0001\n\nSome text\n\n## Beta ^b0001\n\nMore text\n";
        let tree = ParseTree::build(text);
        let sections = tree.sections();
        assert_eq!(sections.len(), 2);
        assert_eq!(sections[0].title, "Alpha ^a0001");
        assert_eq!(sections[0].level, 2);
        assert_eq!(sections[1].title, "Beta ^b0001");
    }

    #[test]
    fn mirror_sections_outermost_only() {
        let text = "## Outer ^=out01\n\n### Inner ^=inn01\n\n- Task ^=t0001\n\n## Regular ^reg01\n";
        let tree = ParseTree::build(text);
        let mirrors = tree.mirror_sections();
        // Only the outer ^= heading, not the nested one
        assert_eq!(mirrors.len(), 1);
        assert_eq!(
            mirrors[0].block_id.as_ref().unwrap(),
            &BlockId("out01".to_string())
        );
    }

    #[test]
    fn code_block_suppresses_elements() {
        let text = "Normal #tag1\n\n```\n#tag2 inside code\n```\n\nNormal #tag3\n";
        let tree = ParseTree::build(text);
        // tag1 and tag3 should be found, tag2 inside code block should not
        let all_tags: Vec<_> = tree
            .lines
            .iter()
            .flat_map(|ld| &ld.elements.tags)
            .collect();
        assert_eq!(all_tags.len(), 2);
    }

    #[test]
    fn context_before_line() {
        let text = "---\ntitle: X\n---\nContent\n";
        let tree = ParseTree::build(text);
        // Line 0: default context flowing in
        let ctx0 = tree.context_before(0);
        assert!(!ctx0.in_frontmatter);
        // Line 1: after "---", should be in frontmatter
        let ctx1 = tree.context_before(1);
        assert!(ctx1.in_frontmatter);
        // Line 3: after closing "---", should be out
        let ctx3 = tree.context_before(3);
        assert!(!ctx3.in_frontmatter);
    }

    #[test]
    fn refresh_after_dirty() {
        let text = "## Heading\n\nSome text\n";
        let mut tree = ParseTree::build(text);
        assert!(tree.line(0).unwrap().heading.is_some());

        // Simulate editing line 0 to no longer be a heading
        tree.mark_dirty(0, 1, 1);
        tree.refresh(
            |idx| {
                match idx {
                    0 => "Plain text\n".to_string(),
                    1 => "\n".to_string(),
                    2 => "Some text\n".to_string(),
                    _ => String::new(),
                }
            },
            3,
        );
        assert!(tree.line(0).unwrap().heading.is_none());
    }
}
