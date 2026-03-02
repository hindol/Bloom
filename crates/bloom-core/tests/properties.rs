//! Property-based tests for Bloom-core invariants.

use proptest::prelude::*;

use bloom_core::parser::{parse, serialize};
use bloom_core::store::sanitize_filename;

// ── Parser: never panics on arbitrary input ────────────────────────────────

proptest! {
    #[test]
    fn parse_never_panics(input in "\\PC{0,2000}") {
        // Parser must return Ok or Err, never panic
        let _ = parse(&input);
    }
}

// ── Parser: roundtrip preserves frontmatter and block content ──────────────

proptest! {
    #[test]
    fn roundtrip_preserves_identity(input in valid_document_strategy()) {
        let doc = parse(&input).unwrap();
        let serialized = serialize(&doc);
        let reparsed = parse(&serialized).unwrap();

        prop_assert_eq!(reparsed.frontmatter.id, doc.frontmatter.id);
        prop_assert_eq!(reparsed.frontmatter.title, doc.frontmatter.title);
        prop_assert_eq!(reparsed.blocks.len(), doc.blocks.len());
        for (a, b) in reparsed.blocks.iter().zip(doc.blocks.iter()) {
            prop_assert_eq!(&a.content, &b.content);
        }
    }
}

/// Strategy that generates valid Bloom documents with frontmatter.
fn valid_document_strategy() -> impl Strategy<Value = String> {
    let id = "[0-9a-f]{8}";
    let title = "[A-Za-z ]{1,40}";
    let lines = prop::collection::vec("[A-Za-z0-9 #@\\[\\]!|^()-]{0,80}", 0..10);

    (id, title, lines).prop_map(|(id, title, lines)| {
        let mut doc = format!(
            "---\nid: {id}\ntitle: \"{title}\"\ncreated: 2026-01-01T00:00:00Z\ntags: []\n---\n\n"
        );
        for line in lines {
            doc.push_str(&line);
            doc.push('\n');
        }
        doc
    })
}

// ── sanitize_filename: always returns a valid, bounded filename ────────────

proptest! {
    #[test]
    fn sanitize_produces_valid_filename(title in "\\PC{0,500}") {
        let name = sanitize_filename(&title);

        // Must not exceed 200 chars + 6 char hash suffix
        prop_assert!(name.len() <= 206, "name too long: {} chars", name.len());

        // Must not contain filesystem-invalid characters
        let invalid = ['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
        for c in invalid {
            prop_assert!(!name.contains(c), "name contains invalid char '{c}': {name}");
        }

        // Must not start or end with dots or whitespace (after trimming)
        if !name.is_empty() {
            prop_assert!(!name.starts_with('.'), "starts with dot: {name}");
            prop_assert!(!name.ends_with('.'), "ends with dot: {name}");
            prop_assert!(!name.starts_with(' '), "starts with space: {name}");
            prop_assert!(!name.ends_with(' '), "ends with space: {name}");
        }
    }
}

// ── sanitize_filename: deterministic ───────────────────────────────────────

proptest! {
    #[test]
    fn sanitize_is_deterministic(title in "\\PC{0,200}") {
        let a = sanitize_filename(&title);
        let b = sanitize_filename(&title);
        prop_assert_eq!(a, b);
    }
}
