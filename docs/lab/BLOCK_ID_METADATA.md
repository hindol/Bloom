# Block IDs: Pre-Edit Transform Model

## Problem

Block IDs (` ^k7m2x`) are structural metadata. The editing buffer should
contain only content. IDs live as metadata, serialized to disk/undo/git.

## Core Principle

**Block ID metadata is computed alongside each edit, not reconciled after.**

When we have an edit op AND the unedited buffer, we have maximum information.
We use both to deterministically compute the new block ID state. After the
edit is applied, the metadata is already correct. No heuristic matching.

```
┌──────────────────────────────────────────────────────────┐
│  1. PRE-EDIT: transform entries (line arithmetic)         │
│     buffer (clean) + edit op → shifted/pruned entries     │
│                                                           │
│  2. APPLY EDIT: mutate rope                               │
│                                                           │
│  3. PARSE: refresh parse tree (synchronous, ~µs)          │
│     → definitive new block boundaries                     │
│                                                           │
│  4. PLACE: entries into parser's blocks                   │
│     - entry.first_line falls in block → entry gets span   │
│     - entry.first_line in no block → entry orphaned, gone │
│     - two entries in same block → merge, keep first       │
│     - block has no entry → assign new ID immediately      │
│                                                           │
│  Result: entries always complete and correct.              │
└──────────────────────────────────────────────────────────┘
```

**The parser is the authority on block boundaries.** The pre-edit transform
preserves ID→position associations through line arithmetic. The parser then
provides exact block boundaries. Placement is deterministic — not matching.

**ID assignment happens during parsing, not during save.** When the parser
finds a block with no entry, an ID is generated immediately. This is
metadata-only — no buffer mutation, no cursor jump, no undo node.

## Data Model

### BlockIdEntry (stored in Buffer, bloom-buffer)

```rust
#[derive(Debug, Clone)]
pub struct BlockIdEntry {
    pub id: BlockId,
    pub first_line: usize,   // block span: first line (inclusive)
    pub last_line: usize,    // block span: last line (inclusive)
    pub is_mirror: bool,
}
```

Entries track the **full block span**, not a single point. This is what the
parser gives us (ParsedBlock { first_line, last_line, has_id }). We maintain
it through edits.

### The Edit Pipeline

Each edit follows this exact sequence:

```rust
fn apply_edit_with_block_ids(
    &mut self,
    edit_range: Range<usize>,
    replacement: &str,
) {
    // ── STEP 1: Pre-edit transform (line arithmetic) ──
    // Uses pre-edit rope + edit op to shift/prune entries
    let shifted = transform_entries(
        &self.block_ids, &self.rope,
        edit_range.clone(), replacement,
    );

    // ── STEP 2: Apply edit to rope ──
    self.rope.replace(edit_range, replacement);  // (or insert/delete)
    self.adjust_cursors(...);

    // ── STEP 3: Refresh parse tree (synchronous) ──
    self.parse_tree.mark_dirty(affected_lines);
    self.parse_tree.refresh(&self.rope.to_string());
    let new_blocks = self.parse_tree.blocks();  // definitive boundaries

    // ── STEP 4: Place entries into parser's blocks ──
    self.block_ids = place_entries_in_blocks(shifted, new_blocks);
}
```

#### Step 1: `transform_entries` (pre-edit, deterministic)

Shifts entry line numbers based on the edit's line delta. Removes entries
whose entire span is within the deleted range.

```rust
fn transform_entries(
    entries: &[BlockIdEntry],
    rope: &Rope,
    edit_range: Range<usize>,
    replacement: &str,
) -> Vec<BlockIdEntry> {
    let edit_start_line = rope.char_to_line(edit_range.start);
    let edit_end_line = if edit_range.end > edit_range.start {
        rope.char_to_line((edit_range.end - 1).min(rope.len_chars() - 1))
    } else {
        edit_start_line
    };
    let old_line_span = if edit_range.end > edit_range.start {
        edit_end_line - edit_start_line + 1
    } else {
        0
    };
    let new_line_span = if replacement.is_empty() {
        0
    } else {
        1 + replacement.chars().filter(|c| *c == '\n').count()
    };
    let delta = new_line_span as isize - old_line_span as isize;

    entries.iter().filter_map(|entry| {
        // Fully above edit
        if entry.last_line < edit_start_line {
            return Some(entry.clone());
        }
        // Fully below edit
        if entry.first_line > edit_end_line {
            return Some(BlockIdEntry {
                first_line: (entry.first_line as isize + delta).max(0) as usize,
                last_line: (entry.last_line as isize + delta).max(0) as usize,
                ..entry.clone()
            });
        }
        // Fully within deleted range → destroyed
        if entry.first_line >= edit_start_line && entry.last_line <= edit_end_line {
            return None;
        }
        // Partial overlap → shift endpoints
        let new_first = entry.first_line.min(edit_start_line);
        let new_last = if entry.last_line > edit_end_line {
            (entry.last_line as isize + delta).max(0) as usize
        } else {
            (edit_start_line + new_line_span).saturating_sub(1)
        };
        Some(BlockIdEntry {
            first_line: new_first,
            last_line: new_last.max(new_first),
            ..entry.clone()
        })
    }).collect()
}
```

This is pure line arithmetic. No heuristics. The resulting entries have
approximate positions — Step 4 snaps them to exact block boundaries.

#### Step 4: `place_entries_in_blocks` (post-parse, definitive)

The parser has refreshed. It knows the exact block boundaries. We place
each shifted entry into its block, and assign new IDs to unmatched blocks.

```rust
fn place_entries_in_blocks(
    shifted: Vec<BlockIdEntry>,
    blocks: &[ParsedBlock],
    existing_ids: &HashSet<String>,
) -> Vec<BlockIdEntry> {
    let mut result = Vec::new();
    let mut claimed_blocks: HashSet<usize> = HashSet::new(); // by block index

    // Place existing entries
    for entry in &shifted {
        // Find the block containing this entry's first_line
        if let Some((i, block)) = blocks.iter().enumerate()
            .find(|(_, b)| entry.first_line >= b.first_line
                        && entry.first_line <= b.last_line)
        {
            if !claimed_blocks.contains(&i) {
                // Snap entry to parser's exact block boundaries
                result.push(BlockIdEntry {
                    id: entry.id.clone(),
                    first_line: block.first_line,
                    last_line: block.last_line,
                    is_mirror: entry.is_mirror,
                });
                claimed_blocks.insert(i);
            }
            // If block already claimed → merge (drop this entry, keep first)
        }
        // If no block contains entry → orphaned, silently dropped
    }

    // Assign new IDs to unclaimed blocks
    let mut all_ids: HashSet<String> = existing_ids.clone();
    all_ids.extend(result.iter().map(|e| e.id.0.clone()));
    for (i, block) in blocks.iter().enumerate() {
        if !claimed_blocks.contains(&i) {
            let id = next_block_id(&all_ids);
            all_ids.insert(id.clone());
            result.push(BlockIdEntry {
                id: BlockId(id),
                first_line: block.first_line,
                last_line: block.last_line,
                is_mirror: false,
            });
        }
    }

    result
}
```

**This replaces `ensure_block_ids` entirely.** ID assignment is part of
the edit pipeline, not a separate post-Insert step.

### What the pipeline handles

| Operation | Step 1 (transform) | Step 4 (place) |
|-----------|-------------------|-----------------|
| Char edit (no newline) | Entries unchanged | Blocks unchanged → entries stay |
| `dd` single-line block | Entry pruned | N/A — entry gone |
| `dd` multi-line block line | Entry shrunk | Parser confirms smaller block |
| `o` / `O` | Entries shifted | New empty line → block grows or new block gets ID |
| `J` removes separator | Entries shifted | Parser sees merged block → second entry dropped |
| Enter creates blank line | Entry grows | Parser sees split → entry snapped to top half, bottom gets new ID |
| Paste with blank lines | Entries shifted | Parser finds new blocks → assigned IDs |
| Code fence insertion | Entries unchanged | Parser sees code block → entries inside it orphaned |
| Heading insertion | Entries unchanged | Parser splits paragraph → entry snapped to one part |

## Serialize / Deserialize

### Serialize (buffer → file text)

Uses entries directly. **No parser needed** — entries already know each
block's last_line.

```rust
fn serialize(rope: &Rope, entries: &[BlockIdEntry]) -> String {
    let text = rope.to_string();
    let lines: Vec<&str> = text.split('\n').collect();

    // Map: last_line → entry (first entry wins if multiple)
    let mut id_at_line: BTreeMap<usize, &BlockIdEntry> = BTreeMap::new();
    for entry in entries {
        id_at_line.entry(entry.last_line).or_insert(entry);
    }

    let mut result = String::new();
    for (idx, line_text) in lines.iter().enumerate() {
        result.push_str(line_text);
        if let Some(entry) = id_at_line.get(&idx) {
            if entry.is_mirror {
                write!(result, " ^={}", entry.id.0).unwrap();
            } else {
                write!(result, " ^{}", entry.id.0).unwrap();
            }
        }
        if idx < lines.len() - 1 {
            result.push('\n');
        }
    }
    result
}
```

### Deserialize (file text → clean buffer + entries)

Uses the parser to extract block IDs and block boundaries.

```rust
fn deserialize(text: &str, parser: &BloomMarkdownParser) -> (String, Vec<BlockIdEntry>) {
    let doc = parser.parse(text);

    // Build entries from parsed block IDs + block boundaries
    let mut entries = Vec::new();
    for parsed_bid in &doc.block_ids {
        if let Some(block) = doc.blocks.iter()
            .find(|b| parsed_bid.line >= b.first_line && parsed_bid.line <= b.last_line)
        {
            entries.push(BlockIdEntry {
                id: parsed_bid.id.clone(),
                first_line: block.first_line,
                last_line: block.last_line,
                is_mirror: parsed_bid.is_mirror,
            });
        }
    }

    // Strip IDs from text
    let clean = text.split('\n').enumerate().map(|(idx, line)| {
        strip_block_id_suffix(line)
    }).collect::<Vec<_>>().join("\n");

    (clean, entries)
}
```

---

## Phases

### Phase 1: BlockIdEntry + transform + place

**bloom-buffer** (`rope.rs`, `lib.rs`):
1. Add `BlockIdEntry` struct (id, first_line, last_line, is_mirror)
2. Add `block_ids: Vec<BlockIdEntry>` field to `Buffer`
3. Add `transform_entries()` — pre-edit line arithmetic
4. Wire into `insert()`, `delete()`, `replace()`:
   - Call `transform_entries()` BEFORE rope mutation
   - Set transformed entries AFTER rope mutation
5. Accessors: `block_ids()`, `set_block_ids()`

**bloom-core** (`lib.rs` or `editor/block_ids.rs`):
1. Add `place_entries_in_blocks()` — takes shifted entries + parser blocks
   → final entries with exact spans + new IDs for unclaimed blocks
2. Wire into the post-edit pipeline:
   ```rust
   // In BufferWriter::apply(BufferMessage::Edit { ... })
   buf.transform_and_apply(range, replacement); // Steps 1+2
   mb.parse_tree.refresh(...);                  // Step 3
   buf.set_block_ids(place_entries_in_blocks(   // Step 4
       buf.block_ids(), mb.parse_tree.blocks(),
   ));
   ```
3. ID generation (`next_block_id`) for new blocks is inside `place_entries_in_blocks`

**This replaces `ensure_block_ids`.** No separate post-Insert step needed.
ID assignment is part of the edit pipeline — metadata only, no cursor jump.

### Phase 2: Serialize / Deserialize (bloom-core)

**Files**: `crates/bloom-core/src/editor/block_id_serde.rs` (new)

1. `serialize(rope, entries) -> String` — entries provide last_line, no parser
2. `deserialize(text, parser) -> (String, Vec<BlockIdEntry>)` — parser extracts blocks + IDs
3. `strip_block_id_suffix(line) -> &str`
4. Tests: round-trip identity, edge cases (empty, headings, code blocks, mirrors)

### Phase 3: Open / Save / Reload

**Open**: `read file → deserialize → Buffer::from_text(clean) + set_block_ids`
Then `place_entries_in_blocks` assigns IDs to blocks without entries.

**Save**: `serialize(rope, entries) → atomic_write`

**Reload**: same as Open (full deserialize).

Parse tree parses clean text. `parse_block_id()` returns None for buffer text.

### Phase 4: Undo (Rope + entries + delta persistence)

**Runtime**: Rope snapshots with structural sharing (current approach).
Each UndoNode gets `block_ids` and `edit_delta` fields:

```rust
struct UndoNode {
    snapshot: ropey::Rope,              // clean text (shared B-tree nodes)
    block_ids: Vec<BlockIdEntry>,       // ~200 entries × ~40 bytes ≈ 8KB
    cursor_pos: usize,
    edit_delta: Option<EditDelta>,      // the edit that produced this node
    // ... timestamp, description, parent, children unchanged
}

struct EditDelta {
    offset: usize,          // char position in parent's text
    delete_len: usize,      // chars removed from parent
    insert_text: String,    // text inserted at offset
}
```

**Push**: record the edit that just happened (bloom-core knows the edit op):
```rust
undo_tree.push(rope.clone(), entries.clone(), cursor_pos, Some(EditDelta {
    offset: range.start, delete_len: range.len(), insert_text: replacement.into(),
}), description);
```

**Undo/Redo**: returns `(Rope, Vec<BlockIdEntry>, usize)`.

**Persistence (SQLite)**: Delta storage — only root stores full text:

```sql
-- Root node: content = full text, delta fields = NULL
-- Other nodes: content = NULL, delta fields populated
ALTER TABLE undo_tree ADD COLUMN delta_offset INTEGER;
ALTER TABLE undo_tree ADD COLUMN delta_del_len INTEGER;
ALTER TABLE undo_tree ADD COLUMN delta_insert TEXT;
ALTER TABLE undo_tree ADD COLUMN block_ids_json TEXT;
```

Storage per non-root node: ~50 bytes (vs ~100KB full text). **~2000x reduction.**

**Restore (load_from_db)**: Reconstruct shared Ropes from deltas:
```rust
// 1. Build root Rope from full text
// 2. BFS through tree:
//    child_rope = parent_rope.clone()   // O(log n), structural sharing!
//    child_rope.remove(delta.offset..delta.offset + delta.delete_len)
//    child_rope.insert(delta.offset, &delta.insert_text)
//    // child shares most B-tree nodes with parent
```

After restore: **same structural sharing as during original session.**

**Edit group deltas**: Insert mode batches many edits → one undo node.
Delta = diff(checkpoint, final). Compute at `end_edit_group` by finding
first/last differing char positions between checkpoint and current Rope.

`undo_auto_push` flag on Buffer: when false, bloom-core drives checkpoints
so entries + deltas are included.

### Phase 5: Mirror / Indexer compatibility

Mirror sync: serialize/deserialize at sync boundaries.
Indexer: reads disk files (serialized format) — unchanged.

---

## Edge Cases

| Scenario | Step 1 (transform) | Step 4 (place) | Result |
|----------|--------------------|-----------------| -------|
| `dd` single-line block | Entry pruned | N/A | ID gone ✅ |
| `dd` multi-line block | Entry shrunk | Parser confirms | ID survives ✅ |
| `dd` entire block | Entry pruned | N/A | ID gone ✅ |
| `J` removes separator | Entries shifted | Parser sees merge → first wins | Merge ✅ |
| Enter → blank line | Entry grows | Parser sees split → snapped to top half | Split ✅ |
| `o` below block | Entries shifted | Parser confirms | Shift ✅ |
| `cc` change line | Unchanged | Parser confirms | Unchanged ✅ |
| `p` paste lines | Entries shifted | New blocks get IDs | Assigned ✅ |
| Code fence insertion | Unchanged | Parser sees code block → orphaned | ID gone ✅ |
| Heading insertion | Unchanged | Parser sees heading → split | Split ✅ |
| `yy`+`p` duplicate | Shifted | Pasted block gets new ID | Assigned ✅ |
| Undo | Deserialize snapshot | N/A | Full restore ✅ |

---

## Stress Test: Failure Mode Analysis

The transform computes new entries from (old entries + rope + edit op) BEFORE
the edit. Here we systematically test where it works, where it needs help,
and where it fails.

### ✅ Works perfectly (simple line arithmetic)

**S1: `dd` — single-line block**
```
Entry: [5, 5] (task line)
Edit:  delete line 5
Check: entry fully within [5, 5] → GONE
```
Correct. Single-line block destroyed. ✅

**S2: `dd` — last line of multi-line block**
```
Entry: [3, 7]
Edit:  delete line 7
Check: partial overlap. new_last = 6. Result: [3, 6]
```
Block shrinks, ID survives. ✅

**S3: `dd` — first line of multi-line block**
```
Entry: [3, 7]
Edit:  delete line 3, delta = -1
Check: first_line(3) == edit_start(3), not fully within (7 > 3).
       new_first = 3 (collapsed to edit start). new_last = 7 - 1 = 6.
       Result: [3, 6]
```
Block shrinks from top. What was line 4 is now line 3. ID survives. ✅

**S4: `dd` — middle line of multi-line block**
```
Entry: [3, 7]
Edit:  delete line 5, delta = -1
Check: partial overlap (3 < 5, 7 > 5). new_last = 6. Result: [3, 6]
```
Block shrinks by one line. ✅

**S5: `3dd` — delete entire block**
```
Entry: [5, 7]
Edit:  delete lines 5-7, delta = -3
Check: entry fully within [5, 7] → GONE
```
Entire block deleted. ✅

**S6: `o` — open line below cursor**
```
Entries: A [3, 5], B [8, 10]
Edit:  insert "\n" at end of line 5, delta = +1
Check: A contains edit → grows to [3, 6]. B below → shifts to [9, 11].
```
New empty line 6 is part of A's block. Cursor enters Insert mode on line 6.
On Insert exit, if the new line is blank AND is at block boundary,
ensure_block_ids handles it. ✅

**S7: `cc` — change line content**
```
Entry: [3, 7], cursor on line 5
Edit:  delete line 5 content + insert replacement (same line count)
       edit_start_line = 5, edit_end_line = 5, replacement has 0 newlines
       delta = 0
Check: partial overlap. new_first = 3, new_last = 7.
```
Entry unchanged (line count didn't change). ✅

**S8: `p` — paste 3 lines below line 5**
```
Entries: A [3, 5], B [8, 10]
Edit:  insert text with 2 newlines at start of line 6, delta = +3
Check: A above edit → unchanged [3, 5]. B below → shifts to [11, 13].
```
Pasted text has no entries. ensure_block_ids assigns IDs to new blocks. ✅

**S9: Single character edits (typing, `x`, `r`)**
```
Any entry, any char-level edit: delta = 0
```
No line count change → no entry change. Always correct. ✅

**S10: `dj` / `dk` — delete two lines**
```
Entry: [3, 7], dj on line 4 (delete lines 4-5), delta = -2
Check: partial overlap. new_last = 7 - 2 = 5. Result: [3, 5]
```
Block shrinks by 2 lines. ✅

**S11: Visual line delete spanning entire block + neighbors**
```
Entries: A [2, 4], B [6, 8], C [10, 12]
Edit:  delete lines 4-10 (covers separator + all of B + separator)
Check: A partial overlap (2 < 4), new_last = 3.
       B fully within [4, 10] → GONE.
       C partial overlap (10 in range, 12 > 10), shifts.
       Result: A [2, 3], C adjusted.
```
B destroyed, A and C shrink. ✅

### ⚠️ Split/merge: handled by parser (Step 4)

The old plan used blank-line heuristics. Now the parser handles these
definitively. The pre-edit transform (Step 1) produces approximate entries.
The parser (Step 3) provides exact block boundaries. Step 4 snaps entries
to blocks and handles all structural changes.

**S12: Enter creating blank line → block split**
```
Entry: [3, 7] (paragraph). User types Enter twice at end of line 4.
```
After each Enter edit:
- Step 1: entry grows (line arithmetic): [3, 8], then [3, 9]
- Step 2: rope mutated
- Step 3: parser refreshes dirty lines → sees blank line → reports TWO blocks: [3, 4] and [6, 9]
- Step 4: entry.first_line(3) falls in block [3, 4] → snapped to [3, 4]. Block [6, 9] has no entry → assigned new ID.

**Parser catches the split. No heuristic needed.** ✅

**S13: `J` joining two blocks → merge**
```
Entries: A [3, 5], blank line 6, B [7, 9]. J on line 5.
```
- Step 1: transform shifts lines. A might become [3, 4], B becomes [6, 8].
- Step 3: parser sees lines 3-8 as one block (no separator).
- Step 4: A.first_line(3) falls in [3, 8] → A snapped to [3, 8]. B.first_line(6) also falls in [3, 8] → block already claimed → B dropped.

**Parser catches the merge. First entry wins.** ✅

**S14: `dd` on blank separator line → merge**
```
Entries: A [3, 5], blank line 6, B [7, 9]. dd on line 6.
```
Same as S13. After deletion + parse, one block [3, 8]. A claims it. B dropped. ✅

**S15: Paste with blank lines inside existing block**
```
Entry: [3, 7]. Paste text with blank line inside block.
```
- Step 1: entry grows to accommodate inserted lines
- Step 3: parser sees the blank line → reports multiple blocks
- Step 4: entry snapped to the block containing its first_line. Other blocks get new IDs.
✅

**S16: Code fence insertion changes block semantics**
```
Entry: A [3, 5], B [7, 9]. User types "```" at start of line 3.
```
- Step 1: entries unchanged (same line count)
- Step 3: parser sees code block starting at line 3 → lines 3-5 are code, not a paragraph. Block structure changes completely.
- Step 4: A.first_line(3) falls inside the code block. The parser reports code blocks differently than content blocks — code blocks do NOT appear in `doc.blocks` (they're not content blocks that get IDs). A's first_line maps to no content block → **A is orphaned and dropped**.

**Parser catches the semantic change.** ID correctly lost. ✅

**S17: Heading insertion splits paragraph**
```
Entry: [3, 7] (paragraph). User types "# " at start of line 5.
```
- Step 1: entries unchanged (same line count)
- Step 3: parser sees heading at line 5. Reports blocks: [3, 4] (paragraph), [5, 5] (heading), [6, 7] (paragraph).
- Step 4: entry.first_line(3) falls in [3, 4] → snapped. [5, 5] and [6, 7] get new IDs.

**Parser catches the heading split.** ID stays with top paragraph. ✅

### ✅ Comprehensive: all previous limitations are now handled

The parser-based Step 4 eliminates ALL the "acceptable limitations" from
the heuristic approach:

| Previously "acceptable limitation" | Now handled by |
|---------------------------------------|---------------|
| Code fence insertion (S16) | Parser knows code blocks |
| Heading insertion (S17) | Parser knows single-line blocks |
| List item restructuring | Parser knows list item spans |
| Any structural markdown change | Parser is markdown-aware |

The only remaining imprecision is in Step 1 (transform) which uses simple
line arithmetic. But Step 4 corrects any imprecision using the parser's
definitive block boundaries. Step 1's purpose is to preserve the ID→position
association through the edit, not to be perfectly accurate about spans.

---

## Performance

Reference vault: 10,000 pages, ~25 MB, 10 years, ~18K commits (from HISTORY.md).

### Per-edit overhead (edit pipeline Steps 1-4)

| Step | Cost | Time (2.5 KB page, ~100 entries) |
|------|------|----------------------------------|
| 1. transform_entries | O(entries) | ~1-5 µs |
| 3. parse refresh (dirty lines) | O(dirty lines) | ~5-50 µs |
| 4. place_entries_in_blocks | O(entries × blocks) | ~5-20 µs |
| **Total per edit** | | **~10-75 µs** |

Well within the <3ms render budget (from getting-started.md).

### Serialize/Deserialize

| Operation | Cost | Time (2.5 KB page) | Frequency |
|-----------|------|---------------------|-----------|
| serialize | O(lines) | ~50-100 µs | Save (300ms debounce) |
| deserialize | O(lines) + parse | ~200-500 µs | Open / reload |

### Undo storage (delta persistence, 20 active pages × 500 nodes)

| Metric | Current (full text) | Delta approach |
|--------|--------------------|-----------------| 
| Storage per page (2.5 KB, 500 nodes) | ~1.25 MB | ~27 KB |
| Total undo SQLite (20 pages) | **~25 MB** ❌ | **~540 KB** ✅ |
| Memory after restore (20 pages) | **~25 MB** | **~1 MB** |
| Restore time (20 pages) | ~40-100 ms | ~40-60 ms |
| Storage budget (HISTORY.md) | 1-5 MB | 1-5 MB |

See `docs/lab/UNDO_DELTA_PERSISTENCE.md` for full analysis.

---

## File Format: Unchanged

Serialized output identical to current `.md` files. No migration.