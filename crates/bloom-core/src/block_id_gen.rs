//! Universal block ID generation and assignment.
//!
//! Block boundaries are identified by the parser (`Document.blocks`). This
//! module handles ID generation and text insertion only — no parsing.
//!
//! See `docs/lab/BLOCK_IDENTITY.md` for the full design.

use std::collections::HashSet;

use crate::parser::traits::Document;

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
// Assignment from parsed Document
// ---------------------------------------------------------------------------

/// Compute block ID assignments for a parsed document.
///
/// Returns a list of insertions (line index + generated ID) for blocks
/// that don't already have a `^block-id`. Returns an empty vec if all
/// blocks already have IDs.
pub fn compute_block_id_assignments(doc: &Document) -> Vec<BlockIdInsertion> {
    let mut existing: HashSet<String> =
        doc.block_ids.iter().map(|b| b.id.0.clone()).collect();

    let mut insertions = Vec::new();
    for block in &doc.blocks {
        if block.has_id {
            continue;
        }
        let id = next_block_id(&existing);
        existing.insert(id.clone());
        insertions.push(BlockIdInsertion {
            line: block.last_line,
            id,
        });
    }

    insertions
}

/// Convenience: compute assignments and apply them to the text.
///
/// Returns `None` if no blocks needed IDs (text is unchanged).
pub fn assign_block_ids(text: &str, doc: &Document) -> Option<String> {
    let insertions = compute_block_id_assignments(doc);
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
    let sep = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out = result.join(sep);
    if has_trailing_newline {
        out.push_str(sep);
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::markdown::BloomMarkdownParser;
    use crate::parser::traits::DocumentParser;

    fn parse(text: &str) -> Document {
        BloomMarkdownParser::new().parse(text)
    }

    // -- next_block_id --

    #[test]
    fn first_id_is_a() {
        assert_eq!(next_block_id(&HashSet::new()), "a");
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
        let mut existing: HashSet<String> =
            (b'a'..=b'z').map(|c| String::from(c as char)).collect();
        existing.insert("a0".into());
        assert_eq!(next_block_id(&existing), "a1");
    }

    #[test]
    fn respects_manually_set_ids() {
        let existing: HashSet<String> =
            ["a", "b", "my-custom-id"].iter().map(|s| s.to_string()).collect();
        assert_eq!(next_block_id(&existing), "c");
    }

    // -- parser block detection (via Document.blocks) --

    #[test]
    fn simple_page_blocks() {
        let text = "---\nid: abc12345\ntitle: \"T\"\n---\n\n# Heading\n\nA paragraph.\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![5, 7]);
    }

    #[test]
    fn multiline_paragraph_blocks() {
        let text = "First line\ncontinues here\n\nSecond block\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![1, 3]);
    }

    #[test]
    fn multiline_blockquote_blocks() {
        let text = "> Line one\n> Line two\n> Line three\n\nAfter.\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![2, 4]);
    }

    #[test]
    fn list_with_continuations_blocks() {
        let text = "- Item one\n  continues\n- Item two\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![1, 2]);
    }

    #[test]
    fn task_items_blocks() {
        let text = "- [ ] Open task\n- [x] Done task\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![0, 1]);
    }

    #[test]
    fn skips_code_fences_blocks() {
        let text = "Before\n\n```\ncode line\nmore code\n```\n\nAfter\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![0, 7]);
    }

    #[test]
    fn skips_frontmatter_blocks() {
        let text = "---\nid: abc12345\ntitle: \"T\"\n---\n\nContent here\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![5]);
    }

    #[test]
    fn blocks_with_existing_id_marked() {
        let text = "Paragraph one ^existing\n\nParagraph two\n";
        let doc = parse(text);
        assert!(doc.blocks[0].has_id);
        assert!(!doc.blocks[1].has_id);
    }

    #[test]
    fn skips_horizontal_rules_blocks() {
        let text = "Above\n\n---\n\nBelow\n";
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![0, 4]);
    }

    #[test]
    fn mixed_document_blocks() {
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
        let doc = parse(text);
        let lasts: Vec<usize> = doc.blocks.iter().map(|b| b.last_line).collect();
        assert_eq!(lasts, vec![5, 8, 10, 11, 13, 16, 22]);
    }

    // -- assign_block_ids --

    #[test]
    fn assigns_ids_to_simple_page() {
        let text = "---\nid: abc12345\ntitle: \"T\"\n---\n\n# Heading\n\nA paragraph.\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(result.contains("# Heading ^a\n"));
        assert!(result.contains("A paragraph. ^b\n"));
    }

    #[test]
    fn no_change_when_all_have_ids() {
        let text = "# Heading ^a\n\nA paragraph. ^b\n";
        let doc = parse(text);
        assert!(assign_block_ids(text, &doc).is_none());
    }

    #[test]
    fn preserves_existing_ids() {
        let text = "First ^existing\n\nSecond\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(result.contains("First ^existing\n"));
        assert!(result.contains("Second ^a\n"));
    }

    #[test]
    fn multiline_gets_id_on_last_line() {
        let text = "> Quote line one\n> Quote line two\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(!result.contains("> Quote line one ^"));
        assert!(result.contains("> Quote line two ^a\n"));
    }

    #[test]
    fn list_continuation_gets_id_on_last_line() {
        let text = "- Long item\n  continuation\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(!result.contains("- Long item ^"));
        assert!(result.contains("  continuation ^a\n"));
    }

    #[test]
    fn empty_text() {
        let doc = parse("");
        assert!(assign_block_ids("", &doc).is_none());
    }

    #[test]
    fn only_frontmatter() {
        let text = "---\nid: abc12345\ntitle: \"T\"\n---\n";
        let doc = parse(text);
        assert!(assign_block_ids(text, &doc).is_none());
    }

    #[test]
    fn only_code_block() {
        let text = "```\nsome code\n```\n";
        let doc = parse(text);
        assert!(assign_block_ids(text, &doc).is_none());
    }

    // -- round-trip: assign IDs, re-parse, verify parser sees them --

    #[test]
    fn round_trip_ids_parse_back() {
        let text = "\
---
id: 8f3a1b2c
title: \"Round Trip\"
---

# Heading

A paragraph
continues here.

- List item one
- [ ] Open task
- [x] Done task

> Blockquote line one
> line two

Another paragraph.
";
        // First pass: assign IDs.
        let doc1 = parse(text);
        assert!(doc1.blocks.iter().all(|b| !b.has_id), "no IDs initially");
        let with_ids = assign_block_ids(text, &doc1).expect("should assign IDs");

        // Second pass: re-parse the modified text.
        let doc2 = parse(&with_ids);

        // Every block should now have an ID.
        for (i, block) in doc2.blocks.iter().enumerate() {
            assert!(block.has_id, "block {i} (lines {}–{}) missing ID after round-trip", block.first_line, block.last_line);
        }

        // The parser's block_ids should match the blocks count.
        assert_eq!(
            doc2.block_ids.len(),
            doc2.blocks.len(),
            "block_ids count should equal blocks count"
        );

        // All generated IDs are unique.
        let ids: Vec<&str> = doc2.block_ids.iter().map(|b| b.id.0.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "IDs should be unique: {:?}", ids);

        // Third pass: re-assigning should produce no changes (idempotent).
        assert!(
            assign_block_ids(&with_ids, &doc2).is_none(),
            "second assignment should be a no-op"
        );
    }

    #[test]
    fn round_trip_preserves_manually_set_ids() {
        let text = "# Heading ^my-heading\n\nParagraph one\n\nParagraph two ^custom\n";
        let doc1 = parse(text);
        let with_ids = assign_block_ids(text, &doc1).expect("should assign to paragraph one");
        let doc2 = parse(&with_ids);

        // Manual IDs preserved.
        let ids: Vec<&str> = doc2.block_ids.iter().map(|b| b.id.0.as_str()).collect();
        assert!(ids.contains(&"my-heading"), "manual heading ID preserved");
        assert!(ids.contains(&"custom"), "manual paragraph ID preserved");

        // Auto-assigned ID didn't collide with manual ones.
        let auto_id = ids.iter().find(|&&id| id != "my-heading" && id != "custom").unwrap();
        assert_eq!(*auto_id, "a", "auto-assigned gets 'a'");
    }
}
