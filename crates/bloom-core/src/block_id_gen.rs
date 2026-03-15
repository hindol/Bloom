//! Block ID generation adapter.
//!
//! Delegates to `bloom_buffer::block_id` for pure generation and applies
//! it to parsed `Document` structures. Also provides `load_all_known_ids`
//! for vault-wide collision avoidance via SQLite.

use std::collections::HashSet;

use rusqlite::Connection;

use bloom_buffer::{BlockIdInsertion, BlockNeedingId};
use bloom_md::parser::traits::Document;

/// Load all known block IDs (live + retired) from the index.
pub fn load_all_known_ids(conn: &Connection) -> HashSet<String> {
    bloom_buffer::block_id::load_all_known_ids(conn)
}

/// Compute block ID assignments for a parsed document.
pub fn compute_block_id_assignments(doc: &Document) -> Vec<BlockIdInsertion> {
    let existing: HashSet<String> = doc.block_ids.iter().map(|b| b.id.0.clone()).collect();
    let blocks: Vec<BlockNeedingId> = doc
        .blocks
        .iter()
        .map(|b| BlockNeedingId {
            last_line: b.last_line,
            has_id: b.has_id,
        })
        .collect();
    bloom_buffer::block_id::compute_assignments(&blocks, &existing)
}

/// Compute assignments and apply them to text.
/// Returns `None` if no blocks needed IDs.
pub fn assign_block_ids(text: &str, doc: &Document) -> Option<String> {
    let insertions = compute_block_id_assignments(doc);
    bloom_buffer::block_id::apply_insertions(text, &insertions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_md::parser::markdown::BloomMarkdownParser;
    use bloom_md::parser::traits::DocumentParser;

    fn parse(text: &str) -> Document {
        BloomMarkdownParser::new().parse(text)
    }

    #[test]
    fn assigns_ids_to_simple_page() {
        let text = "---\nid: abc12345\ntitle: \"T\"\n---\n\n# Heading\n\nA paragraph.\n";
        let doc = parse(text);
        let result = assign_block_ids(text, &doc).unwrap();
        let doc2 = parse(&result);
        assert_eq!(doc2.block_ids.len(), 2);
    }

    #[test]
    fn no_change_when_all_have_ids() {
        let text = "# Heading ^k7m2x\n\nA paragraph. ^p3a9f\n";
        let doc = parse(text);
        assert!(assign_block_ids(text, &doc).is_none());
    }

    #[test]
    fn round_trip_idempotent() {
        let text = "---\nid: 8f3a1b2c\ntitle: \"Test\"\n---\n\n# Heading\n\nContent.\n";
        let doc1 = parse(text);
        let with_ids = assign_block_ids(text, &doc1).unwrap();
        let doc2 = parse(&with_ids);
        assert!(assign_block_ids(&with_ids, &doc2).is_none());
    }

    #[test]
    fn mirror_marker_not_double_assigned() {
        let text = "- [ ] Task with mirror ^=mir01\n- [ ] Task without id\n";
        let doc = parse(text);
        // Line 0 has ^=mir01 — should be recognized as having an ID
        assert!(
            doc.blocks.iter().any(|b| b.last_line == 0 && b.has_id),
            "block on line 0 should have has_id=true for ^=mir01, blocks: {:?}",
            doc.blocks.iter().map(|b| format!("lines {}-{} has_id={}", b.first_line, b.last_line, b.has_id)).collect::<Vec<_>>()
        );
        // Only line 1 should need an ID assignment
        let insertions = compute_block_id_assignments(&doc);
        assert_eq!(
            insertions.len(), 1,
            "only task without ID should get assigned, got {} insertions: {:?}",
            insertions.len(),
            insertions.iter().map(|i| format!("line {} id={}", i.line, i.id)).collect::<Vec<_>>()
        );
        assert_eq!(insertions[0].line, 1, "insertion should be on line 1 (task without ID)");
    }
}
