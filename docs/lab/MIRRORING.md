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

```
Agenda buffer (mutable, not frozen):
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x
  - [ ] Fix parser bug @due(2026-03-12) ^a3b4c
  - [x] Ship v2.0 @due(2026-03-14) ^b5c6d

User edits line 1, changes "ropey" to "ropey + petgraph"
  → on Insert→Normal transition (Esc):
  → parse ^k7m2x from the edited line
  → find_all_pages_by_block_id("k7m2x") → [(page A, line 42)]
  → read source line from page A, line 42
  → replace source line with the edited line via MirrorEdit
  → save page A
  → re-render Agenda from fresh BQL query
  → restore cursor to line containing ^k7m2x
```

**Key difference from general views:** Render verbatim source lines (with `- [ ]`, `@due(...)`, `^block_id`). No lossy projection. The buffer IS the source content.

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

#### 🟡 Section headers in the buffer

Current Agenda has section headers ("Overdue", "Today · Mar 14", "Upcoming") as buffer lines. If the buffer is mutable, the user can edit/delete them.

**Option A — Virtual decorations.** Headers are not in the buffer. The TUI injects them at render time based on `@due` values in the task lines. Buffer contains only task lines.

- Pro: No header-editing problem. Buffer is a clean list of tasks.
- Con: Requires new TUI infrastructure (virtual lines injected during render). We don't have this yet. ~100 lines of new code.

**Option B — Protected regions.** Headers stay in the buffer. Edits on header lines are silently dropped (same mechanism as read-only filter in `translate_vim_action`).

- Pro: No TUI changes. Reuses existing read-only filtering.
- Con: Vim cursor can land on headers but can't edit them. `dd` on a header does nothing. `j` from last task skips to next section. Partial-Vim feel.

**Option C — Ignorable headers.** Headers stay in the buffer. Edits on headers are allowed but never propagated (no block ID → nothing to propagate). On re-render, headers are rebuilt from scratch.

- Pro: Simplest implementation. Headers are ephemeral decorations that happen to be in the buffer.
- Con: User can type gibberish into a header. On next re-render it's replaced. Weird but harmless.

**Recommendation: Option A** (virtual decorations) is cleanest. If too expensive, **Option C** is pragmatic — a header edit that vanishes on re-render is acceptable UX.

#### 🟡 Cross-line Vim commands

A mutable buffer means ALL Vim commands work. Some are problematic:

| Command | What it does | Problem |
|---------|-------------|---------|
| `dd` | Delete line | Deletes task from view. Should it delete from source? |
| `o` / `O` | Open line below/above | New empty line — no source page, no block ID |
| `J` | Join lines | Merges two tasks into one — breaks both source lines |
| `p` / `P` | Paste | Pastes arbitrary content — no block ID, no source mapping |

**Option A — Restrict cross-line commands.** Filter in `translate_vim_action`: allow within-line mutations (i, a, c, r, s, R), block structural changes (dd, o, O, J, p, P, D, C that cross lines). Bloom-vim already has this filtering infrastructure for read-only buffers.

- Pro: Prevents all structural corruption.
- Con: Violates "full standard Vim grammar" (GOALS.md). Users expect `dd` to work. Some will hit it instinctively and be confused by silent failure.

**Option B — Give cross-line commands Agenda semantics.**

| Command | Agenda semantic |
|---------|----------------|
| `dd` | Remove task from Agenda (mark done? delete from source? configurable) |
| `o` | Quick capture — prompt for text, create new task in today's journal |
| `p` | Paste a task line — route to "current section's" source page |
| `J` | Blocked (silent no-op) — merging tasks is never correct |

- Pro: Every command does something meaningful. Vim users get muscle memory compatibility.
- Con: `dd` = mark done is a semantic leap. `o` = quick capture is magical. These need learning.

**Option C — Block cross-line, provide alternatives.**

| Vim command | Blocked | Alternative |
|-------------|---------|-------------|
| `dd` | Yes | `x` to toggle done, or `D` to archive |
| `o` | Yes | `SPC x a` for quick capture |
| `J` | Yes | — |
| `p` | Yes | — |

- Pro: Clear, honest. Commands that don't make sense don't pretend to work.
- Con: Still violates full Vim grammar.

**Recommendation: Option B** if we commit to editable Agenda. It's the most Vim-native. But it requires careful design of what each command means in Agenda context.

#### 🟡 @due change moves the task between sections

User changes `@due(2026-03-10)` to `@due(2026-03-20)` on a task in the "Overdue" section. After propagation and re-render, the task moves to "Upcoming." The cursor follows (by block ID). This is correct behavior but potentially surprising — the line "jumps" to a different part of the view.

Mitigation: Brief notification — "Task moved to Upcoming." This is a feature, not a bug.

#### 🟡 Stale Agenda when source changes in another pane

User edits source page in pane 1 (adds/removes tasks). Agenda in pane 2 doesn't know. If user then edits a task in the stale Agenda, propagation writes based on the old block ID → still correct (block ID identity, not line position). But the Agenda display is out of date.

Fix: Event bus subscription. Source `Edit` on a task block → `BlockChanged` → Agenda re-renders. This is exactly what the event bus was designed for. Currently not wired, but the infrastructure exists.

Without event bus: Agenda is stale until the user explicitly re-opens it (SPC a a). Acceptable for v1.

---

### What breaks

#### 🔴 Lines without block IDs

If a task line somehow lacks a `^xxxxx` (malformed, or from a file that hasn't had `EnsureBlockIds` run), there's no identity for propagation. An edit on this line would have nowhere to go.

Mitigation: Filter out tasks without block IDs from the Agenda. Or assign block IDs at query time (trigger `EnsureBlockIds` on source pages when the Agenda loads). The latter is cleaner — it's a pre-condition for the editable Agenda to open.

#### 🔴 Multi-user editing (future)

If Bloom ever supports multi-user (shared vaults), editable Agenda creates conflicts. Two users editing the same task in their respective Agendas. Last-write-wins via MirrorEdit is not sufficient — the second user's edit silently overwrites the first's.

Not a problem today (single-user app). But worth noting as a constraint on future architecture.

---

### Comparison: Structured edits vs Editable Agenda

| Dimension | Structured edits (current path) | Editable Agenda |
|-----------|-------------------------------|----------------|
| **Toggle task** | `x` key — works today | Same (or just edit the checkbox) |
| **Edit task text** | Enter → jump to source → edit → come back | Edit inline, Esc to propagate |
| **Change due date** | `d` key → mini-prompt (not built) | Edit `@due(...)` inline with Vim motions |
| **Add tag** | `t` key → mini-prompt (not built) | Type `#tag` inline with Vim motions |
| **Vim grammar** | Full — buffer is read-only, all navigation works | Partial — cross-line commands restricted or re-semanticized |
| **Undo** | N/A — structured edits are atomic | Works but diverges from source undo stack |
| **Section headers** | In buffer, read-only — not a problem | Need virtual decorations or protection |
| **Implementation cost** | ~50 lines per structured edit | ~200-300 lines (virtual headers, edit filter, propagation) |
| **Risk** | Low — proven by toggle | Medium — partial Vim, undo coherence, re-render jank |

### Where each wins

**Structured edits win when:** The operation is bounded and well-defined (toggle, set date, add tag). The user doesn't need to think about buffer mechanics. The view stays clean and predictable.

**Editable Agenda wins when:** The user wants to do something unanticipated — fix a typo, reword a task, reorganize inline. These are the "long tail" of edits that structured commands can't cover without a command for every possible transformation.

### A hybrid path

They're not mutually exclusive:

1. **Phase 1:** Expand structured edits on read-only Agenda — `x` toggle (done), `d` set date, `t` add tag, `s` snooze. Low risk, immediate value.

2. **Phase 2:** Make Agenda editable with verbatim rendering. Structured edit keys still work as shortcuts. Full Vim editing also works for the long tail. Requires virtual section headers and cross-line command handling.

Phase 1 is valuable on its own. Phase 2 is additive — it doesn't obsolete Phase 1, it builds on it.

---

### Verdict

The editable Agenda via block mirroring **holds under stress for within-line editing.** Block-ID identity solves the propagation problem cleanly. Propagation on Esc is a natural trigger. Cursor preservation by block ID works.

The hard problems are **at the edges:** section headers, cross-line commands, undo coherence. These are solvable but add complexity. None are showstoppers for the Agenda specifically (unlike the general "editable formatted view" case, which IS fundamentally broken due to projection mismatch).

**Recommended path:** Phase 1 structured edits → Phase 2 editable Agenda. The architecture supports both. Don't skip Phase 1 — it's lower risk and delivers value while we validate the Phase 2 design.

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL views as the read-only alternative for cross-context visibility
