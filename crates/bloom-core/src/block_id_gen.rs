//! Universal block ID generation and assignment.
//!
//! Every content block (paragraph, heading, list item, task, blockquote) gets a
//! short, page-scoped ID appended to its last line: `Some text ^a3`.
//!
//! See `docs/lab/BLOCK_IDENTITY.md` for the full design.

use std::collections::HashSet;

use crate::parser::extensions::parse_block_id;

/// A computed block ID insertion: append ` ^{id}` to the end of the given line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockIdInsertion {
    /// Zero-based line index where the ID should be appended.
    pub line: usize,
    /// The generated block ID (without the `^` prefix).
    pub id: String,
}

// ---------------------------------------------------------------------------
// ID generation
// ---------------------------------------------------------------------------

/// Generate the shortest available block ID not in `existing`.
///
/// Sequence: `a`, `b`, …, `z`, `a0`, `a1`, …, `az`, `b0`, …, `zz`,
/// `a00`, …, `zzz`. First character is always a letter (`a`–`z`);
/// subsequent characters are alphanumeric (`0`–`9`, `a`–`z`).
pub fn next_block_id(existing: &HashSet<String>) -> String {
    // Length 1: a–z (26)
    for c in b'a'..=b'z' {
        let id = String::from(c as char);
        if !existing.contains(&id) {
            return id;
        }
    }
    // Length 2: [a-z][0-9a-z] (936)
    for first in b'a'..=b'z' {
        for second in (b'0'..=b'9').chain(b'a'..=b'z') {
            let id = format!("{}{}", first as char, second as char);
            if !existing.contains(&id) {
                return id;
            }
        }
    }
    // Length 3: [a-z][0-9a-z][0-9a-z] (33,696)
    for first in b'a'..=b'z' {
        for second in (b'0'..=b'9').chain(b'a'..=b'z') {
            for third in (b'0'..=b'9').chain(b'a'..=b'z') {
                let id = format!("{}{}{}", first as char, second as char, third as char);
                if !existing.contains(&id) {
                    return id;
                }
            }
        }
    }
    unreachable!("exhausted 34,658 block IDs — no page has this many blocks")
}

// ---------------------------------------------------------------------------
// Block detection
// ---------------------------------------------------------------------------

/// Compute block ID assignments for a page's text.
///
/// Returns a list of insertions (line index + generated ID) for blocks
/// that don't already have a `^block-id`. Returns an empty vec if all
/// blocks already have IDs.
pub fn compute_block_id_assignments(text: &str) -> Vec<BlockIdInsertion> {
    let lines: Vec<&str> = text.lines().collect();

    // Collect existing block IDs.
    let mut existing: HashSet<String> = HashSet::new();
    for (i, line) in lines.iter().enumerate() {
        if let Some(bid) = parse_block_id(line, i) {
            existing.insert(bid.id.0);
        }
    }

    let block_last_lines = find_blocks_needing_ids(&lines);

    let mut insertions = Vec::new();
    for line_idx in block_last_lines {
        let id = next_block_id(&existing);
        existing.insert(id.clone());
        insertions.push(BlockIdInsertion { line: line_idx, id });
    }

    insertions
}

/// Convenience: apply block ID assignments and return the modified text.
///
/// Returns `None` if no blocks needed IDs (text is unchanged).
pub fn assign_block_ids(text: &str) -> Option<String> {
    let insertions = compute_block_id_assignments(text);
    if insertions.is_empty() {
        return None;
    }
    Some(apply_insertions(text, &insertions))
}

/// Apply insertions to text, returning the modified string.
pub fn apply_insertions(text: &str, insertions: &[BlockIdInsertion]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut result: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    for ins in insertions {
        let trimmed = result[ins.line].trim_end().to_string();
        result[ins.line] = format!("{} ^{}", trimmed, ins.id);
    }

    let has_trailing_newline = text.ends_with('\n');
    // Preserve original line ending style.
    let sep = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = result.join(sep);
    if has_trailing_newline {
        out.push_str(sep);
    }
    out
}

// ---------------------------------------------------------------------------
// Internal: block boundary detection
// ---------------------------------------------------------------------------

/// Scan lines and return the last-line index of every block that has no
/// existing `^block-id` on any of its lines.
fn find_blocks_needing_ids(lines: &[&str]) -> Vec<usize> {
    let mut result = Vec::new();
    let mut i = 0;
    let mut in_frontmatter = false;
    let mut in_code_fence = false;

    // Frontmatter: only recognised at the very first line.
    if !lines.is_empty() && lines[0].trim() == "---" {
        in_frontmatter = true;
        i = 1;
    }

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        // Close frontmatter.
        if in_frontmatter {
            if trimmed == "---" || trimmed == "..." {
                in_frontmatter = false;
            }
            i += 1;
            continue;
        }

        // Code fences.
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            i += 1;
            continue;
        }
        if in_code_fence {
            i += 1;
            continue;
        }

        // Blank lines and horizontal rules.
        if trimmed.is_empty() || is_horizontal_rule(trimmed) {
            i += 1;
            continue;
        }

        // --- Block identification ---

        // Heading (single line).
        if is_heading(trimmed) {
            if !line_has_block_id(line) {
                result.push(i);
            }
            i += 1;
            continue;
        }

        // Blockquote: consecutive `>` lines form one block.
        if trimmed.starts_with('>') {
            let mut last = i;
            let mut has_id = line_has_block_id(line);
            i += 1;
            while i < lines.len() {
                let next = lines[i].trim();
                if next.starts_with('>') {
                    last = i;
                    if line_has_block_id(lines[i]) {
                        has_id = true;
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            if !has_id {
                result.push(last);
            }
            continue;
        }

        // List item (including tasks): `- `, `* `, `+ `, `1. `, etc.
        if is_list_item(trimmed) {
            let indent = leading_spaces(line);
            let mw = list_marker_width(trimmed);
            let content_indent = indent + mw;
            let mut last = i;
            let mut has_id = line_has_block_id(line);
            i += 1;
            // Continuation lines: indented at least to content start, not a new
            // list item, not blank.
            while i < lines.len() {
                let next = lines[i];
                let next_trimmed = next.trim();
                if next_trimmed.is_empty() {
                    break;
                }
                let next_indent = leading_spaces(next);
                if next_indent >= content_indent && !is_list_item(next_trimmed) {
                    last = i;
                    if line_has_block_id(next) {
                        has_id = true;
                    }
                    i += 1;
                } else {
                    break;
                }
            }
            if !has_id {
                result.push(last);
            }
            continue;
        }

        // Paragraph: consecutive non-blank, non-special lines.
        {
            let mut last = i;
            let mut has_id = line_has_block_id(line);
            i += 1;
            while i < lines.len() {
                let next = lines[i];
                let next_trimmed = next.trim();
                if next_trimmed.is_empty()
                    || is_heading(next_trimmed)
                    || next_trimmed.starts_with('>')
                    || is_list_item(next_trimmed)
                    || next_trimmed.starts_with("```")
                    || next_trimmed.starts_with("~~~")
                    || is_horizontal_rule(next_trimmed)
                {
                    break;
                }
                last = i;
                if line_has_block_id(next) {
                    has_id = true;
                }
                i += 1;
            }
            if !has_id {
                result.push(last);
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn line_has_block_id(line: &str) -> bool {
    parse_block_id(line, 0).is_some()
}

fn is_heading(trimmed: &str) -> bool {
    let bytes = trimmed.as_bytes();
    if bytes.is_empty() || bytes[0] != b'#' {
        return false;
    }
    let level = bytes.iter().take_while(|&&b| b == b'#').count();
    level <= 6 && bytes.get(level) == Some(&b' ')
}

fn is_list_item(trimmed: &str) -> bool {
    list_marker_width(trimmed) > 0
}

/// Returns the width of the list marker (e.g. `- ` → 2, `1. ` → 3), or 0
/// if the line is not a list item.
fn list_marker_width(trimmed: &str) -> usize {
    let bytes = trimmed.as_bytes();
    // Unordered: `- `, `* `, `+ `
    if bytes.len() >= 2 && (bytes[0] == b'-' || bytes[0] == b'*' || bytes[0] == b'+') && bytes[1] == b' ' {
        return 2;
    }
    // Ordered: digits followed by `. ` or `) `
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

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- next_block_id --

    #[test]
    fn first_id_is_a() {
        let existing = HashSet::new();
        assert_eq!(next_block_id(&existing), "a");
    }

    #[test]
    fn skips_existing() {
        let existing: HashSet<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_block_id(&existing), "d");
    }

    #[test]
    fn wraps_to_length_two() {
        let existing: HashSet<String> = (b'a'..=b'z').map(|c| String::from(c as char)).collect();
        assert_eq!(next_block_id(&existing), "a0");
    }

    #[test]
    fn skips_existing_length_two() {
        let mut existing: HashSet<String> = (b'a'..=b'z').map(|c| String::from(c as char)).collect();
        existing.insert("a0".into());
        assert_eq!(next_block_id(&existing), "a1");
    }

    #[test]
    fn respects_manually_set_ids() {
        let existing: HashSet<String> =
            ["a", "b", "my-custom-id"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_block_id(&existing), "c");
    }

    // -- is_heading --

    #[test]
    fn heading_detection() {
        assert!(is_heading("# Title"));
        assert!(is_heading("## Sub"));
        assert!(is_heading("###### Deep"));
        assert!(!is_heading("####### Too deep"));
        assert!(!is_heading("#NoSpace"));
        assert!(!is_heading("Not a heading"));
    }

    // -- is_list_item --

    #[test]
    fn list_item_detection() {
        assert!(is_list_item("- Item"));
        assert!(is_list_item("* Item"));
        assert!(is_list_item("+ Item"));
        assert!(is_list_item("1. Item"));
        assert!(is_list_item("12. Item"));
        assert!(is_list_item("- [ ] Task"));
        assert!(is_list_item("- [x] Done"));
        assert!(!is_list_item("Not a list"));
        assert!(!is_list_item("-NoSpace"));
    }

    // -- is_horizontal_rule --

    #[test]
    fn horizontal_rule_detection() {
        assert!(is_horizontal_rule("---"));
        assert!(is_horizontal_rule("***"));
        assert!(is_horizontal_rule("___"));
        assert!(is_horizontal_rule("- - -"));
        assert!(is_horizontal_rule("-----"));
        assert!(!is_horizontal_rule("--"));
        assert!(!is_horizontal_rule("- text"));
    }

    // -- find_blocks_needing_ids --

    #[test]
    fn simple_page() {
        let text = "---\nid: abc\ntitle: \"T\"\n---\n\n# Heading\n\nA paragraph.\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        assert_eq!(blocks, vec![5, 7]); // heading line 5, paragraph line 7
    }

    #[test]
    fn multiline_paragraph() {
        let text = "First line\ncontinues here\n\nSecond block\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // ID goes on last line of each paragraph
        assert_eq!(blocks, vec![1, 3]);
    }

    #[test]
    fn multiline_blockquote() {
        let text = "> Line one\n> Line two\n> Line three\n\nAfter.\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // blockquote last line=2, paragraph last line=4
        assert_eq!(blocks, vec![2, 4]);
    }

    #[test]
    fn list_with_continuations() {
        let text = "- Item one\n  continues\n- Item two\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // First item: lines 0-1, ID on line 1. Second item: line 2, ID on line 2.
        assert_eq!(blocks, vec![1, 2]);
    }

    #[test]
    fn task_items() {
        let text = "- [ ] Open task\n- [x] Done task\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        assert_eq!(blocks, vec![0, 1]);
    }

    #[test]
    fn skips_code_fences() {
        let text = "Before\n\n```\ncode line\nmore code\n```\n\nAfter\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // "Before" line 0, "After" line 7
        assert_eq!(blocks, vec![0, 7]);
    }

    #[test]
    fn skips_frontmatter() {
        let text = "---\nid: abc\ntitle: \"T\"\n---\n\nContent here\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        assert_eq!(blocks, vec![5]);
    }

    #[test]
    fn skips_blocks_with_existing_id() {
        let text = "Paragraph one ^existing\n\nParagraph two\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // First paragraph already has ID, second doesn't
        assert_eq!(blocks, vec![2]);
    }

    #[test]
    fn skips_horizontal_rules() {
        let text = "Above\n\n---\n\nBelow\n";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        assert_eq!(blocks, vec![0, 4]);
    }

    #[test]
    fn mixed_document() {
        let text = "\
---
id: 8f3a1b2c
title: \"Test\"
---

## Heading

Some paragraph
continues here.

- List item one
- [ ] Task item
- Long item that
  continues here

> Blockquote line one
> line two

```
code block
```

Another paragraph.
";
        let lines: Vec<&str> = text.lines().collect();
        let blocks = find_blocks_needing_ids(&lines);
        // heading=5, para last=8, list=10, task=11, list-cont last=13,
        // blockquote last=16, paragraph=22
        assert_eq!(blocks, vec![5, 8, 10, 11, 13, 16, 22]);
    }

    // -- assign_block_ids --

    #[test]
    fn assigns_ids_to_simple_page() {
        let text = "---\nid: abc\ntitle: \"T\"\n---\n\n# Heading\n\nA paragraph.\n";
        let result = assign_block_ids(text).unwrap();
        assert!(result.contains("# Heading ^a\n"));
        assert!(result.contains("A paragraph. ^b\n"));
    }

    #[test]
    fn no_change_when_all_have_ids() {
        let text = "# Heading ^a\n\nA paragraph. ^b\n";
        assert!(assign_block_ids(text).is_none());
    }

    #[test]
    fn preserves_existing_ids() {
        let text = "First ^existing\n\nSecond\n";
        let result = assign_block_ids(text).unwrap();
        // "existing" is preserved, new block gets "a" (not "existing" again)
        assert!(result.contains("First ^existing\n"));
        assert!(result.contains("Second ^a\n"));
    }

    #[test]
    fn multiline_gets_id_on_last_line() {
        let text = "> Quote line one\n> Quote line two\n";
        let result = assign_block_ids(text).unwrap();
        // ID on last line of blockquote
        assert!(!result.contains("> Quote line one ^"));
        assert!(result.contains("> Quote line two ^a\n"));
    }

    #[test]
    fn list_continuation_gets_id_on_last_line() {
        let text = "- Long item\n  continuation\n";
        let result = assign_block_ids(text).unwrap();
        assert!(!result.contains("- Long item ^"));
        assert!(result.contains("  continuation ^a\n"));
    }

    #[test]
    fn empty_text() {
        assert!(assign_block_ids("").is_none());
    }

    #[test]
    fn only_frontmatter() {
        let text = "---\nid: abc\ntitle: \"T\"\n---\n";
        assert!(assign_block_ids(text).is_none());
    }

    #[test]
    fn only_code_block() {
        let text = "```\nsome code\n```\n";
        assert!(assign_block_ids(text).is_none());
    }
}
