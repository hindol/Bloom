use bloom_md::parser::traits::{Style, StyledSpan};
use std::ops::Range;

/// Find all occurrences of `query` in `text` (case-insensitive) and return
/// `StyledSpan`s with `Style::SearchMatch`. For multi-word queries, each
/// whitespace-separated fragment is matched independently.
pub fn highlight_matches(text: &str, query: &str) -> Vec<StyledSpan> {
    let mut spans = Vec::new();
    let text_lower = text.to_lowercase();

    for fragment in query.split_whitespace() {
        if fragment.is_empty() {
            continue;
        }
        let frag_lower = fragment.to_lowercase();
        let mut start = 0;
        while let Some(pos) = text_lower[start..].find(&frag_lower) {
            let abs = start + pos;
            spans.push(StyledSpan {
                byte_range: abs..abs + frag_lower.len(),
                style: Style::SearchMatch,
            });
            start = abs + frag_lower.len();
        }
    }

    // Sort by position and merge overlapping ranges
    spans.sort_by_key(|s| s.byte_range.start);
    merge_overlapping(&mut spans);
    spans
}

/// Merge overlapping or adjacent spans in a sorted list.
fn merge_overlapping(spans: &mut Vec<StyledSpan>) {
    if spans.len() <= 1 {
        return;
    }
    let mut merged = Vec::with_capacity(spans.len());
    merged.push(spans[0].clone());
    for s in &spans[1..] {
        let last = merged.last_mut().unwrap();
        if s.byte_range.start <= last.byte_range.end {
            last.byte_range.end = last.byte_range.end.max(s.byte_range.end);
        } else {
            merged.push(s.clone());
        }
    }
    *spans = merged;
}

/// Overlay search highlight spans on top of existing styled spans for a line.
/// Returns a new list where `SearchMatch` spans split or override base spans.
pub fn overlay_search_spans(
    base: &[StyledSpan],
    search: &[StyledSpan],
    line_len: usize,
) -> Vec<StyledSpan> {
    if search.is_empty() {
        return base.to_vec();
    }

    // Build a flag array: which byte positions are search-highlighted
    let mut is_match = vec![false; line_len];
    for s in search {
        for i in s.byte_range.clone() {
            if i < line_len {
                is_match[i] = true;
            }
        }
    }

    let mut result = Vec::new();
    for span in base {
        let Range { start, end } = span.byte_range.clone();
        let end = end.min(line_len);
        if start >= end {
            continue;
        }

        // Split this span at search match boundaries
        let mut pos = start;
        while pos < end {
            let in_search = is_match.get(pos).copied().unwrap_or(false);
            let mut run_end = pos + 1;
            while run_end < end && is_match.get(run_end).copied().unwrap_or(false) == in_search {
                run_end += 1;
            }
            result.push(StyledSpan {
                byte_range: pos..run_end,
                style: if in_search {
                    Style::SearchMatch
                } else {
                    span.style.clone()
                },
            });
            pos = run_end;
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_single_match() {
        let spans = highlight_matches("Hello World", "world");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].byte_range, 6..11);
    }

    #[test]
    fn test_highlight_multiple_fragments() {
        let spans = highlight_matches("The quick brown fox", "quick fox");
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].byte_range, 4..9);
        assert_eq!(spans[1].byte_range, 16..19);
    }

    #[test]
    fn test_highlight_case_insensitive() {
        let spans = highlight_matches("HELLO hello Hello", "hello");
        assert_eq!(spans.len(), 3);
    }

    #[test]
    fn test_highlight_no_match() {
        let spans = highlight_matches("Hello World", "xyz");
        assert!(spans.is_empty());
    }

    #[test]
    fn test_highlight_overlapping_merged() {
        // "abab" with query "ab ba" — fragments "ab" at 0..2 and 2..4, "ba" at 1..3
        // All overlap → should merge to single span
        let spans = highlight_matches("abab", "ab ba");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].byte_range, 0..4);
    }

    #[test]
    fn test_overlay_splits_base_spans() {
        let base = vec![StyledSpan {
            byte_range: 0..11,
            style: Style::Normal,
        }];
        let search = vec![StyledSpan {
            byte_range: 6..11,
            style: Style::SearchMatch,
        }];
        let result = overlay_search_spans(&base, &search, 11);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].style, Style::Normal);
        assert_eq!(result[0].byte_range, 0..6);
        assert_eq!(result[1].style, Style::SearchMatch);
        assert_eq!(result[1].byte_range, 6..11);
    }
}
