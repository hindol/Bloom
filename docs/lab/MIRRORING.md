# Block Mirroring 🪞

> Same block, same ID, real content in multiple files — kept in sync via the BufferWriter.
> Status: **Architecture ready, partially active.** Task toggle mirroring works end-to-end.
> General text mirroring requires ~30 lines of wiring.

---

## The Idea

A block `^k7m2x` exists as real text in multiple files. Not a reference, not transclusion — the actual content is duplicated. Bloom keeps copies in sync: edit one, all other copies update. Both files are equal co-owners.

```markdown
pages/Text Editor Theory.md:
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x

pages/Rust Programming.md:
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x
```

Toggle the task in a view or any pane → all copies update synchronously.

---

## What's Built

### Database schema

The `block_ids` table uses a **composite primary key** `(block_id, page_id)`, allowing the same block to appear in multiple pages:

```sql
CREATE TABLE block_ids (
    block_id TEXT NOT NULL,
    page_id  TEXT NOT NULL,
    line     INTEGER NOT NULL,
    PRIMARY KEY (block_id, page_id)
);
CREATE INDEX idx_block_ids_page  ON block_ids(page_id);
CREATE INDEX idx_block_ids_block ON block_ids(block_id);
```

`retired_block_ids` ensures deleted IDs are never reused. `block_links` tracks `[[^block_id|hint]]` references separately from content mirrors.

### Mirror lookup

```rust
Index::find_all_pages_by_block_id(&BlockId) -> Vec<(PageMeta, line)>
```

One query, returns every page that contains the block and the line number. This is the foundation for all propagation.

### BufferMessage — Edit vs MirrorEdit

The `BufferMessage` enum has two edit variants:

```rust
Edit {
    page_id, range, replacement, cursor_after, cursor_idx
}
MirrorEdit {
    page_id, range, replacement
}
```

`Edit` is the normal user-initiated mutation. `MirrorEdit` is identical in its rope operation but **does NOT emit `BlockChanged` events and does NOT trigger further mirror propagation.** This single distinction prevents circular notification loops. One flag, checked in one place.

```
User edits ^k7m2x in page A
  → writer.apply(Edit { page_id: A, ... })
  → mutate A's buffer ✅
  → emit BlockChanged("k7m2x") → views refresh
  → index lookup: which other pages contain ^k7m2x?
  → for each target page B:
      → writer.apply(MirrorEdit { page_id: B, ... })  ← no events, no cascade
      → mark dirty → queue save
```

### Task toggle mirroring (active)

`handle_view_toggle_task()` in `keys.rs` implements full mirror propagation for task toggles:

1. Load source page into buffer (if not already open)
2. Flip the checkbox (`- [ ]` ↔ `- [x]`)
3. Extract block ID from the toggled line
4. Query `find_all_pages_by_block_id()`
5. For each mirror page: load buffer, replace the line, save
6. Re-render the view with fresh results

This is the proof-of-concept for the full mirroring pipeline. It works end-to-end today.

### Event bus (wired, not yet subscribed)

```rust
pub struct BufferWriter {
    buffer_mgr: BufferManager,
    block_watchers: HashMap<String, Vec<Box<dyn Fn() + Send>>>,
}
```

The event bus exists on `BufferWriter`. Views would register watchers for block IDs in their result set. When `Edit` (not `MirrorEdit`) touches a watched block, callbacks fire and views re-query. Currently the HashMap is empty — views re-render on explicit actions (toggle, Enter) rather than subscribing to events.

---

## Sync Mechanism

**Synchronous in-memory propagation via BufferWriter.** No file watchers, no last-write-wins races.

1. User edits `^k7m2x` in buffer A (or toggles via a view).
2. `BufferWriter::apply(Edit)` mutates buffer A.
3. Writer queries the index: which other pages contain `^k7m2x`?
4. For each page B: load into buffer if needed, `apply(MirrorEdit)` — same rope op, no events.
5. Auto-save writes both A and B to disk.
6. Git commits capture the state.

This is fundamentally different from the original "patch files on disk" design. All mutations happen in-memory through the single-threaded BufferWriter. No file watcher races, no dirty-buffer prompts, no fingerprint-based loop detection.

---

## What Works

| Scenario | Behaviour |
|----------|-----------|
| Toggle task in view, mirrors exist | All copies updated synchronously. View re-renders. |
| Mirror page not open | BufferWriter loads it, applies MirrorEdit, saves, closes. |
| Mirror page open in another pane | Buffer mutated in place. Pane renders updated content on next frame. |
| Circular propagation | Impossible — `MirrorEdit` does not emit events or trigger further mirrors. |
| Block in N pages | Source edit + N−1 MirrorEdits. All synchronous, single-threaded. |
| Undo after mirror sync | Undo reverts the local buffer only. Mirror targets keep their version. Next edit re-propagates. |
| Delete block from one file | Mirror count decreases in index. Remaining copies keep their content. No cascading delete. |
| Recovery | Git has every intermediate state. |

---

## What's Not Built Yet

### General text mirroring

Task toggle mirroring works because the mutation is simple (flip `[ ]` / `[x]`). General text mirroring — edit any character on a mirrored line, propagate the new line to all copies — requires:

1. **Detect which block was edited.** After `apply(Edit)`, determine the block ID on the edited line. The line number is in the Edit message; scanning for `^xxxxx` at end-of-line is O(1).

2. **Propagate the full line.** Query mirrors, replace the corresponding line in each target buffer via `MirrorEdit`. ~20 lines in the `Edit` handler of `apply()`.

3. **Queue saves.** Mark mirror targets dirty so auto-save picks them up.

Estimated: ~30 lines in `BufferWriter::apply()`. The architecture is ready; this is wiring.

### Event bus subscriptions

Views don't subscribe to `BlockChanged` events yet. They re-render on explicit user actions. Wiring this would let views update live when a mirrored block changes in another pane.

### Multi-line blocks

Current propagation is line-level (one block = one line). Multi-line blocks (paragraphs, list trees) would need a block boundary parser to determine where the block starts and ends. Not needed for task mirroring (tasks are single-line) but required for general content mirroring.

---

## Design Decisions (from UNIFIED_BUFFER.md)

| Decision | Rationale |
|----------|-----------|
| In-memory sync, not file patching | BufferWriter owns all buffers. Synchronous mutation eliminates file watcher races and dirty-buffer prompts. |
| `MirrorEdit` as separate variant | One-flag circular prevention. Cleaner than fingerprint-based disk detection. |
| Single-threaded writer | All mutations serialized. No concurrent-edit races. Industry standard (VS Code, Neovim, Helix). |
| Event bus is block-level only | Pickers use snapshots (ephemeral). Only long-lived views need live updates. One event type (`BlockChanged`), one subscriber type (views). |
| No CRDT, no merge logic | Local-first, single-user. Last edit wins. Git is the safety net. |

---

## Remaining UX Questions

| Question | Current thinking |
|----------|-----------------|
| Should mirroring be opt-in per block? | Yes — user pastes a block preserving its `^id` into another file. Indexer detects the duplicate. No explicit "mirror this" command needed. |
| What happens when both panes edit the same mirrored block? | Last `apply(Edit)` wins. MirrorEdit overwrites the other pane's in-progress text. Acceptable for single-user — you'd have to be editing the same line in two panes simultaneously. |
| Should there be a mirror indicator in the gutter? | Nice to have. Index can report mirror count per block ID. Display `🪞2` in gutter for blocks mirrored in 2+ pages. |
| When is mirroring the wrong abstraction? | Block in 10+ files suggests tags + views. Mirroring is for 2–3 page cross-references. Bloom should suggest views when mirror count is high. |

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL views as the read-only alternative for cross-context visibility
