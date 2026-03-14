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

## Stress Test: Editable Views via Block Mirroring

> **Premise:** BQL returns blocks with IDs. If we make saved views editable, edits flow back to source pages through the mirroring pipeline. Journal view = mirrored blocks. Toggle is already trivial. Does this generalize?

### How it would work

```
User opens Agenda view (BQL: tasks | where not done | sort due)
  → query returns rows with (page_id, line, block_id, text)
  → view renders as mutable buffer (not frozen)
  → row_map maps each buffer line → source (page_id, line, block_id)
  → user edits a line in the view
  → reverse-map: find source page + line from row_map
  → apply(Edit) to source buffer
  → apply(MirrorEdit) to all other mirrors
  → view re-renders from fresh query
```

### Showstopper 1: Projection mismatch

The view reformats content. Source and view lines are structurally different:

```
Source:  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x
View:    [ ] Review the ropey API  @due(2026-03-10)  (Rust Programming)
```

Differences:
- View strips `- ` list prefix
- View strips `^block_id` suffix
- View adds `  (page_title)` suffix
- View normalizes whitespace around `@due`

**If the user edits the view line and we write it back to source, we corrupt the source.** The list prefix is gone. The block ID is gone. The page title suffix is foreign content. There is no clean reverse-mapping from the projected form back to source.

**Mitigation:** Render source lines verbatim in the view. But then the view loses its formatted presentation — it's just a list of raw markdown lines. The whole point of a view is formatted projection.

**Conclusion: Free-text editing of formatted views is fundamentally broken.** The projection is lossy and non-invertible.

### Showstopper 2: Row map invalidation

`row_map[line_index]` maps buffer lines to source. Any edit that adds or removes lines shifts all subsequent indices:

```
Before:  row_map[5] = Source { page_id: A, line: 42 }
User does `dd` on line 3
After:   row_map[5] still points to the OLD line 5's source
         but buffer line 5 now shows what was line 6
```

Every line add/delete silently corrupts the mapping. Propagation writes the wrong content to the wrong source line. Data loss.

**Mitigation:** Re-build row_map after every edit. But this requires re-running the BQL query and re-rendering — at which point we've replaced the buffer content, losing the user's cursor, scroll, and in-progress edit.

**Alternative:** Use block IDs embedded in line text for identity instead of positional mapping. Parse `^xxxxx` from the edited line to find the source. This works but requires block IDs to survive in the view text (back to "render verbatim" problem from Showstopper 1).

### Problem 3: New content has no home

User presses `o` (Vim: open line below) in the view. A new empty line appears. Where does it go?

- No page_id in the row_map for this line
- No block ID
- Which source file should own it?
- If it's a journal view spanning 7 days, which day's file?

Heuristic: inherit the source page from the line above. But what if the cursor is on a section header? Or between sections from different pages?

**Conclusion:** New line insertion in views is undefined without explicit routing rules.

### Problem 4: Undo incoherence

```
1. User edits task in view → propagates to source page A
2. User presses `u` (undo) in view → view buffer reverts
3. Source page A still has the edit
4. View and source are now out of sync
```

To fix: undo must also propagate to source. But `BufferMessage::Undo` operates on a single buffer. We'd need a "transaction undo" that reverts the view buffer AND the source buffer AND all mirrors atomically. This is CRDT territory — exactly what we decided not to build.

**Mitigation:** Re-render view from scratch after undo (re-query). User loses their cursor position. Acceptable? Probably not for frequent undo/redo cycles.

### Problem 5: Stale view during concurrent edits

User has source page in pane 1, view in pane 2:

```
1. User adds a new task line in pane 1 (source)
2. Source buffer now has different line numbers
3. View's row_map still points to old line numbers
4. User toggles a task in pane 2 (view)
5. row_map[cursor_line].line is now WRONG
6. Toggle hits the wrong line in source
```

Without event bus subscriptions (not wired yet), the view has no way to know the source changed. Even with the event bus, a full re-render on every source edit makes the view jumpy and unpredictable.

---

### What DOES work: Structured edits on read-only views

The existing toggle (`x` key) works perfectly because it:

1. **Reads from source, not the view.** It loads the source buffer, finds the line, flips the checkbox there.
2. **Re-renders the entire view afterward.** Fresh query, fresh row_map, no stale state.
3. **Is a bounded transformation.** Toggle knows exactly what to change — one character (`[ ]` ↔ `[x]`). No projection-reversal needed.

This pattern generalizes to other **structured edits**:

| Structured edit | What it changes in source |
|----------------|---------------------------|
| Toggle task (`x`) | `[ ]` ↔ `[x]` — already works |
| Set due date (`d`) | Replace `@due(...)` or append it |
| Add tag (`t`) | Append `#tag` to the line |
| Remove tag (`T`) | Remove `#tag` from the line |
| Set priority (`p`) | Replace `@priority(...)` |
| Snooze (`s`) | Update `@due(...)` to tomorrow/next week |
| Move to page (`m`) | Cut block from source, paste into target |

Each is a well-defined transformation on the **source line**, not the view line. The view re-renders after each operation. No projection reversal, no row_map invalidation, no undo incoherence.

**This is the correct path for BQL views.** Expand the set of structured edits rather than making the buffer freely editable.

---

### What about Journal specifically?

The journal use case is different from general BQL views because:

1. **Content IS raw markdown.** A journal "today" view shows task lines verbatim — there's no lossy projection.
2. **Single source page.** Today's journal entries all live in one file. New lines have an obvious home.
3. **Block IDs are present.** Every list item has a `^xxxxx` suffix.

This means a **journal mirror document** could work:

```
Virtual document (not a query result):
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x
  - [ ] Fix parser bug @due(2026-03-12) ^a3b4c
  - Morning standup notes ^p9q8r
```

Each line is verbatim from the journal page. Block IDs provide identity. Edits propagate via block ID lookup (not positional row_map). New lines go to today's journal page.

**But this is NOT an "editable BQL view."** It's a separate concept — a **virtual page** that concatenates blocks from source pages. It bypasses BQL entirely. The rendering is verbatim (no projection), the identity is block-ID-based (not positional), and the source routing is explicit (today's journal page).

---

### Decision Matrix

| Approach | Projection problem | Row map problem | New content | Undo | Complexity |
|----------|-------------------|-----------------|-------------|------|------------|
| **Free-text editable views** | 🔴 Showstopper | 🔴 Showstopper | 🔴 Undefined | 🔴 Incoherent | High |
| **Structured edits on read-only views** | ✅ N/A (reads source) | ✅ N/A (re-renders) | ✅ N/A | ✅ N/A (atomic) | Low |
| **Journal mirror document** | ✅ Verbatim lines | ✅ Block-ID identity | 🟡 Route to today | 🟡 Per-buffer undo | Medium |

### Recommendation

1. **BQL views:** Stay read-only. Expand structured edits (toggle, set date, add tag, snooze, move). Re-render after each. This is clean, predictable, already proven by toggle.

2. **Journal:** Build a separate virtual/mirror page concept. Not a BQL query result. Verbatim source lines with block-ID-based propagation. This is the right abstraction for "editable collection of blocks from known source pages."

3. **Don't conflate the two.** The insight "BQL returns block IDs, so views could be editable" is appealing but the projection mismatch makes it fundamentally unsound for free-text editing. The correct generalization is: BQL provides the *query*, structured edits provide the *mutations*, and the view re-renders between operations.

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL views as the read-only alternative for cross-context visibility
