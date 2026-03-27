//! Syntax highlight span cache — avoids redundant `highlight_line()` calls
//! for unchanged lines during cursor movement and scrolling.

use std::collections::HashMap;

use bloom_md::parser::traits::{DocumentParser, LineContext, StyledSpan};
use bloom_md::types::PageId;

struct CachedSpans {
    line_text: String,
    context: LineContext,
    spans: Vec<StyledSpan>,
}

pub(crate) struct SpanCache {
    entries: HashMap<(PageId, usize), CachedSpans>,
}

impl SpanCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Return cached spans if the line text and context haven't changed,
    /// otherwise re-highlight and cache the result.
    pub fn get_or_highlight(
        &mut self,
        page_id: &PageId,
        line_idx: usize,
        line_text: &str,
        ctx: &LineContext,
        parser: &dyn DocumentParser,
    ) -> Vec<StyledSpan> {
        let key = (page_id.clone(), line_idx);
        if let Some(cached) = self.entries.get(&key) {
            if cached.line_text == line_text && cached.context == *ctx {
                return cached.spans.clone();
            }
        }
        let spans = parser.highlight_line(line_text, ctx);
        self.entries.insert(
            key,
            CachedSpans {
                line_text: line_text.to_string(),
                context: ctx.clone(),
                spans: spans.clone(),
            },
        );
        spans
    }

    /// Drop all cached spans for a specific page (e.g. on page close).
    #[allow(dead_code)]
    pub fn invalidate_page(&mut self, page_id: &PageId) {
        self.entries.retain(|(pid, _), _| pid != page_id);
    }

    /// Drop everything.
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
