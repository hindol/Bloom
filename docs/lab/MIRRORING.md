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

## Stress Test: Editable Agenda via Block Mirroring

> **Premise:** BQL returns task blocks with IDs. The Agenda view is a collection of tasks from across the vault. If we make it editable, edits on a task line propagate back to the source page via block ID. Toggle is already trivial. Can we generalize to free-text editing?

### The user workflow

```
1. SPC a a → open Agenda
2. j/k to navigate tasks
3. x to toggle (already works)
4. i to edit task text inline → "fix typo, change description"
5. f@ ci( to change due date → propagates to source
6. Esc → propagate, re-render
```

No context-switch to the source page. Edit where you see it.

### How it would work

**BQL returns flat rows. No `group` clause.** Grouping is a view concern — the Agenda renderer buckets tasks into regions based on `@due` values.

```
BQL query: tasks | where not done | sort due

Returns flat rows:
  (page_id, line, block_id, text, due, done)
  (page_id, line, block_id, text, due, done)
  ...

Agenda renderer groups into target regions:
  ┌─── Overdue ───────────────────────────┐
  │ - [ ] Review ropey API ^k7m2x         │ ← editable
  │ - [ ] Fix parser bug ^a3b4c           │ ← editable
  ├─── Today · Mar 14 ────────────────────┤
  │ - [ ] Ship v2.0 ^b5c6d               │ ← editable
  ├─── Upcoming ──────────────────────────┤
  │ - [ ] Plan Q2 goals ^e7f8g           │ ← editable
  └───────────────────────────────────────┘
```

Each **target region** is a section of the buffer bounded by fence lines. Task lines within a region are editable. Fences are structural.

```
User edits line, changes "ropey" to "ropey + petgraph"
  → on Insert→Normal transition (Esc):
  → parse ^k7m2x from the edited line
  → find_all_pages_by_block_id("k7m2x") → [(page A, line 42)]
  → replace source line with the edited line via MirrorEdit
  → save page A
  → refresh: re-query, rebuild regions, restore cursor to ^k7m2x
```

---

### What works

#### ✅ Block-ID identity for propagation

Every task has a `^xxxxx` suffix. After editing, parse the block ID from the line. Look up source page via index. Replace source line with MirrorEdit. This is exactly what toggle does — generalized to any within-line edit.

No positional row_map needed for propagation. Block ID is the stable identity across re-renders.

#### ✅ Propagation on Insert→Normal transition

Auto-save is already deferred during Insert mode. Same trigger: when user presses Esc, the edit group ends, propagation fires. The line is in its final form (not mid-keystroke). Clean.

#### ✅ Cursor preservation across re-render

After propagation, the view re-renders (fresh BQL query). Find the line containing the same `^block_id`. Restore cursor row and column. Block IDs are unique — O(n) scan of the new buffer.

#### ✅ Undo drives propagation

View buffer has its own undo stack. Undo in view → view line reverts → on next propagation trigger, the reverted line propagates to source via MirrorEdit. Source stays in sync. The user's mental model: "I'm editing in the Agenda, undo works in the Agenda."

#### ✅ Toggle is a special case of this

Current toggle: intercept `x`, read source, flip checkbox, MirrorEdit mirrors, re-render. Editable Agenda: user edits anything, Esc triggers propagation, MirrorEdit mirrors, re-render. Same pipeline, toggle becomes just another edit.

---

### What's hard

#### 🟡 Target regions and fence lines

The Agenda buffer has two kinds of content: **task lines** (editable, have block IDs) and **fence lines** (structural, computed from `@due` values). These are fundamentally different and the buffer must know which is which.

**Design: Fence lines are buffer lines rebuilt on every refresh.**

```
Buffer content (what's in the rope):
  ── Overdue ──                     ← fence line (no block ID)
  - [ ] Review ropey API ^k7m2x    ← task line (has block ID)
  - [ ] Fix parser bug ^a3b4c      ← task line
                                    ← fence line (blank separator)
  ── Today · Mar 14 ──             ← fence line
  - [ ] Ship v2.0 ^b5c6d          ← task line
```

**Rules:**
1. Lines with `^xxxxx` = task lines. Editable. Propagate on Esc.
2. Lines without block ID = fence lines. Non-propagating. Rebuilt on refresh.
3. If user edits a fence line → no propagation happens (no block ID). On next refresh, the fence is rebuilt correctly. Harmless.
4. If user deletes a fence line (`dd`) → same: rebuilt on refresh. Tasks above and below are still correct because identity is block-ID, not position.

**Refresh cycle:**

```
Refresh:
  1. Record cursor block ID (parse ^xxxxx from current line)
  2. Re-run BQL query
  3. Group results by due-date category → build regions
  4. Render: fence + tasks + fence + tasks + ...
  5. Find line containing cursor block ID → restore cursor (row, col)
```

**When to refresh:**
- After propagation (Esc in Normal mode after an edit)
- After structured edit (toggle, set date, etc.)
- After event bus notification (source changed in another pane)
- NOT during Insert mode (buffer is stable while user types)

**Key insight:** Fence lines are ephemeral. They exist in the buffer for cursor navigation (j/k moves through them) but carry no identity. They're rebuilt from scratch on every refresh. This means the buffer doesn't need "protected regions" or "virtual decorations" — fences are just lines that happen to not have block IDs. The propagation logic naturally ignores them.

**This replaces BQL `group`.** The grouping logic lives in the Agenda renderer, not the query language. BQL stays simple (flat rows). The view groups by due-date category, tags, page, or whatever the view type requires.

#### 🟡 Cross-line Vim commands

A mutable buffer means ALL Vim commands work. Some need Agenda-specific semantics:

| Command | What it does | Agenda semantic |
|---------|-------------|----------------|
| `dd` | Delete line | On task line: mark done (toggle `[x]`), task disappears on refresh. On fence line: no-op (rebuilt on refresh). |
| `o` / `O` | Open line below/above | Quick capture — new task routed to today's journal page. Gets a block ID on save. |
| `J` | Join lines | No-op — merging two tasks into one line is never correct. |
| `p` / `P` | Paste | If pasted text has a `^block_id` → mirror. If not → new task, route to today's journal. |

The key insight: **cross-line commands don't corrupt the Agenda because the refresh cycle rebuilds the structure.** `dd` on a fence line? Rebuilt on refresh. `o` inserts a new line? It either gets a block ID (and becomes a real task) or is swept away on refresh.

The question is whether these semantics feel natural or magical. `dd` = mark done is a semantic leap, but in an Agenda context it's defensible — you're removing a task from your todo list. `o` = new task is useful. `J` = blocked is correct.

#### 🟡 @due change moves the task between regions

User changes `@due(2026-03-10)` to `@due(2026-03-20)` on a task in the "Overdue" region. After propagation and refresh, the task moves to "Upcoming." The cursor follows (by block ID). The fence lines are rebuilt to reflect the new grouping.

This is correct behavior but potentially surprising — the line "jumps" to a different part of the view. Mitigation: brief notification — "Task moved to Upcoming." This is a feature, not a bug.

#### 🟡 Stale Agenda when source changes in another pane

User edits source page in pane 1 (adds/removes tasks). Agenda in pane 2 doesn't know. If user then edits a task in the stale Agenda, propagation writes based on the old block ID → still correct (block ID identity, not line position). But the Agenda display is out of date.

Fix: Event bus subscription. Source `Edit` on a task block → `BlockChanged` → Agenda re-renders. This is exactly what the event bus was designed for. Currently not wired, but the infrastructure exists.

Without event bus: Agenda is stale until the user explicitly re-opens it (SPC a a). Acceptable for v1.

---

### Pre-conditions

#### Tasks must have block IDs

If a task line lacks a `^xxxxx` (from a file that hasn't had `EnsureBlockIds` run), there's no identity for propagation. An edit on this line would have nowhere to go.

Solution: `EnsureBlockIds` runs on all source pages when the Agenda opens. This is already a post-save hook for edited pages — extend it to run on Agenda load for all pages in the query result. Cost: one pass over each source buffer, skipping lines that already have IDs. Typically ~0ms (IDs already present).

#### Single-user assumption

If Bloom ever supports multi-user (shared vaults), editable Agenda creates conflicts. Two users editing the same task simultaneously. Last-write-wins via MirrorEdit is not sufficient.

Not a problem today (single-user app). Worth noting as a future constraint.

---

### Comparison: Structured edits vs Editable Agenda

| Dimension | Structured edits (current path) | Editable Agenda (target regions) |
|-----------|-------------------------------|----------------------------------|
| **Toggle task** | `x` key — works today | Same (or `dd` = mark done) |
| **Edit task text** | Enter → jump to source → edit → come back | Edit inline, Esc to propagate |
| **Change due date** | `d` key → mini-prompt (not built) | Edit `@due(...)` inline with Vim motions |
| **Add tag** | `t` key → mini-prompt (not built) | Type `#tag` inline with Vim motions |
| **New task** | SPC x a (quick capture) | `o` routes to today's journal |
| **Vim grammar** | Full — buffer is read-only, all navigation works | Within-line: full. Cross-line: Agenda semantics. |
| **Section headers** | In buffer, read-only — not a problem | Fence lines — ephemeral, rebuilt on refresh |
| **Undo** | N/A — structured edits are atomic | Works via view undo → propagation |
| **BQL** | Returns flat rows, view formats | Returns flat rows, view groups into regions |
| **Implementation cost** | ~50 lines per structured edit | ~200 lines (fence rebuild, propagation, cursor restore) |
| **Risk** | Low — proven by toggle | Medium — cross-line semantics, undo coherence |

### Where each wins

**Structured edits win when:** The operation is bounded and well-defined (toggle, set date, add tag). The user doesn't need to think about buffer mechanics. The view stays clean and predictable.

**Editable Agenda wins when:** The user wants to do something unanticipated — fix a typo, reword a task, add inline notes. These are the "long tail" of edits that structured commands can't cover. The target region model makes this safe: block-ID identity for propagation, fence lines rebuilt on refresh, cursor restored by block ID.

### A hybrid path

They're not mutually exclusive:

1. **Phase 1:** Structured edits on read-only Agenda — `x` toggle (done), `d` set date, `t` add tag, `s` snooze. Low risk, immediate value.

2. **Phase 2:** Editable Agenda with target regions. Verbatim task lines + ephemeral fence lines. Structured edit keys still work as shortcuts. Inline Vim editing for everything else. Propagation on Insert→Normal. Refresh rebuilds regions. ~200 lines.

Phase 1 is valuable on its own. Phase 2 is additive — structured edit keys become convenient shortcuts for common operations that also work via inline editing.

---

### Verdict

The editable Agenda via block mirroring **holds under stress.** The target region model solves the section header problem cleanly: fence lines are ephemeral buffer lines without block IDs, rebuilt on every refresh. Task lines are verbatim source content with block IDs, propagated via MirrorEdit on Esc.

**The model is: flat BQL query → view groups into target regions → buffer has fence lines + task lines → edits propagate by block ID → refresh rebuilds regions, restores cursor by block ID.**

The hard problems (cross-line semantics, undo coherence) are design choices, not blockers. `dd` = mark done and `o` = quick capture are defensible Agenda semantics.

BQL `group` clause can be dropped. Grouping is a view-level concern — different views group differently (by due date, by tag, by page). The query returns flat rows. The renderer knows how to bucket them.

**Recommended path:** Phase 1 structured edits → Phase 2 editable Agenda with target regions. Phase 1 keys become shortcuts in Phase 2.

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL views as the read-only alternative for cross-context visibility
