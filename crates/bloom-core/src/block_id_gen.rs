//! Universal block ID generation and assignment.
//!
//! Block boundaries are identified by the parser (`Document.blocks`). This
//! module handles ID generation and text insertion only — no parsing.
//!
//! IDs are vault-scoped: 5-character base36 (a-z0-9), globally unique across
//! the entire vault and all time. See `docs/lab/BLOCK_IDENTITY.md`.

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

const ID_LEN: usize = 5;
const ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Generate a random 5-char base36 block ID not in `existing`.
///
/// Uses a simple xorshift64 PRNG seeded from the existing set size plus a
/// counter, avoiding the need for `rand` as a dependency. Retries on
/// collision — at typical vault densities (<10%), this averages ~1.1 attempts.
pub fn next_block_id(existing: &HashSet<String>) -> String {
    let mut seed: u64 = 0xcafe_f00d_dead_beef ^ (existing.len() as u64).wrapping_mul(6364136223846793005);
    // Mix in a counter so repeated calls with the same set size produce different IDs.
    // The counter is derived from a thread-local to avoid needing external state.
    seed ^= thread_counter();

    loop {
        seed = xorshift64(seed);
        let id = seed_to_id(seed);
        if !existing.contains(&id) {
            return id;
        }
        seed = xorshift64(seed);
    }
}

fn xorshift64(mut x: u64) -> u64 {
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    x
}

fn seed_to_id(mut seed: u64) -> String {
    let mut buf = [0u8; ID_LEN];
    for b in &mut buf {
        *b = ALPHABET[(seed % 36) as usize];
        seed /= 36;
    }
    // Safety: all bytes are ASCII alphanumeric.
    unsafe { String::from_utf8_unchecked(buf.to_vec()) }
}

fn thread_counter() -> u64 {
    use std::cell::Cell;
    thread_local! {
        static COUNTER: Cell<u64> = const { Cell::new(0) };
    }
    COUNTER.with(|c| {
        let v = c.get().wrapping_add(1);
        c.set(v);
        v
    })
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
    fn generates_5_char_base36() {
        let id = next_block_id(&HashSet::new());
        assert_eq!(id.len(), 5, "ID should be exactly 5 chars: {id}");
        assert!(
            id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "ID should be base36: {id}"
        );
    }

    #[test]
    fn generates_unique_ids() {
        let mut existing = HashSet::new();
        for _ in 0..1000 {
            let id = next_block_id(&existing);
            assert!(existing.insert(id), "collision detected");
        }
    }

    #[test]
    fn skips_existing() {
        let mut existing = HashSet::new();
        let first = next_block_id(&existing);
        existing.insert(first.clone());
        let second = next_block_id(&existing);
        assert_ne!(first, second);
        assert_eq!(second.len(), 5);
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
        let text = "Paragraph one ^k7m2x\n\nParagraph two\n";
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
        // Verify both blocks got 5-char IDs.
        let doc2 = parse(&result);
        assert_eq!(doc2.block_ids.len(), 2);
        for bid in &doc2.block_ids {
            assert_eq!(bid.id.0.len(), 5, "ID should be 5 chars: {}", bid.id.0);
        }
    }

    #[test]
    fn no_change_when_all_have_ids() {
        let text = "# Heading ^k7m2x\n\nA paragraph. ^p3a9f\n";
        let doc = parse(text);
        assert!(assign_block_ids(text, &doc).is_none());
    }

    #[test]
    fn preserves_existing_ids() {
        let text = "First ^k7m2x\n\nSecond\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(result.contains("First ^k7m2x\n"));
        // Second block gets a new 5-char ID.
        let doc2 = parse(&result);
        assert_eq!(doc2.block_ids.len(), 2);
        let ids: Vec<&str> = doc2.block_ids.iter().map(|b| b.id.0.as_str()).collect();
        assert!(ids.contains(&"k7m2x"));
        let auto_id = ids.iter().find(|&&id| id != "k7m2x").unwrap();
        assert_eq!(auto_id.len(), 5);
    }

    #[test]
    fn multiline_gets_id_on_last_line() {
        let text = "> Quote line one\n> Quote line two\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(!result.contains("> Quote line one ^"));
        assert!(result.contains("> Quote line two ^"));
    }

    #[test]
    fn list_continuation_gets_id_on_last_line() {
        let text = "- Long item\n  continuation\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        assert!(!result.contains("- Long item ^"));
        assert!(result.contains("  continuation ^"));
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

        // All generated IDs are unique and 5 chars.
        let ids: Vec<&str> = doc2.block_ids.iter().map(|b| b.id.0.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "IDs should be unique: {:?}", ids);
        for id in &ids {
            assert_eq!(id.len(), 5, "ID should be 5 chars: {id}");
        }

        // Third pass: re-assigning should produce no changes (idempotent).
        assert!(
            assign_block_ids(&with_ids, &doc2).is_none(),
            "second assignment should be a no-op"
        );
    }
}
