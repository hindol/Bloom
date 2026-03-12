//! Block ID generation and assignment.
//!
//! IDs are vault-scoped: 5-character base36 (a-z0-9), globally unique.

use std::collections::HashSet;

use crate::BlockIdInsertion;

const ID_LEN: usize = 5;
const ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";

/// Load all known block IDs (live + retired) from the index for collision avoidance.
pub fn load_all_known_ids(conn: &rusqlite::Connection) -> HashSet<String> {
    let mut ids = HashSet::new();
    for table in &["block_ids", "retired_block_ids"] {
        if let Ok(mut stmt) = conn.prepare(&format!("SELECT block_id FROM {table}")) {
            if let Ok(rows) = stmt.query_map([], |row| row.get::<_, String>(0)) {
                for id in rows.flatten() {
                    ids.insert(id);
                }
            }
        }
    }
    ids
}

/// Generate a random 5-char base36 block ID not in `existing`.
pub fn next_block_id(existing: &HashSet<String>) -> String {
    let mut seed: u64 =
        0xcafe_f00d_dead_beef ^ (existing.len() as u64).wrapping_mul(6364136223846793005);
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

/// Compute block ID assignments from a list of blocks and existing IDs.
///
/// `blocks` describes which blocks need IDs. `existing_ids` is the set of
/// all known IDs (from the parsed document + index) for collision avoidance.
pub fn compute_assignments(
    blocks: &[crate::BlockNeedingId],
    existing_ids: &HashSet<String>,
) -> Vec<BlockIdInsertion> {
    let mut existing = existing_ids.clone();
    let mut insertions = Vec::new();

    for block in blocks {
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

/// Apply block ID insertions to text, returning the modified string.
/// Returns `None` if no insertions needed.
pub fn apply_insertions(text: &str, insertions: &[BlockIdInsertion]) -> Option<String> {
    if insertions.is_empty() {
        return None;
    }
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
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_5_char_base36() {
        let id = next_block_id(&HashSet::new());
        assert_eq!(id.len(), 5);
        assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
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
    fn compute_assignments_skips_existing() {
        let blocks = vec![
            crate::BlockNeedingId { last_line: 0, has_id: true },
            crate::BlockNeedingId { last_line: 2, has_id: false },
        ];
        let existing = HashSet::new();
        let insertions = compute_assignments(&blocks, &existing);
        assert_eq!(insertions.len(), 1);
        assert_eq!(insertions[0].line, 2);
    }

    #[test]
    fn apply_insertions_to_text() {
        let text = "Line one\nLine two\n";
        let insertions = vec![BlockIdInsertion { line: 1, id: "abc12".into() }];
        let result = apply_insertions(text, &insertions).unwrap();
        assert!(result.contains("Line two ^abc12"));
        assert!(result.contains("Line one\n"));
    }

    #[test]
    fn apply_empty_insertions_returns_none() {
        assert!(apply_insertions("text", &[]).is_none());
    }
}
