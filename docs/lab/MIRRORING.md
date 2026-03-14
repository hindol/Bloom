# Block Mirroring 🪞

> Same block, same ID, real content in multiple files — kept in sync via the BufferWriter.
> Status: **Architecture ready, partially active.** Toggle mirroring works end-to-end.

---

## The Idea

A block `^k7m2x` exists as real text in multiple files. Not a reference, not transclusion — the actual content is duplicated. Bloom keeps copies in sync: edit one, all other copies update. All copies are equal co-owners.

```markdown
pages/Text Editor Theory.md:
  - [ ] Review the ropey API @due(2026-03-10) ^=k7m2x

pages/Rust Programming.md:
  - [ ] Review the ropey API @due(2026-03-10) ^=k7m2x
```

Edit the task in either file → the other updates synchronously.

---

## Mirror Markers: `^` vs `^=`

Block IDs have two forms:

```
^k7m2x    → solo block (exists in one file only)
^=k7m2x   → mirrored block (peers exist in other files)
```

The `=` means "I have peers" — not "I am a copy." All `^=` instances are equal co-owners. There is no primary/secondary distinction.

**The marker lives in the file content.** The index derives mirror relationships from it. Delete the index, rebuild from files — all mirror relationships are preserved. Files are always the source of truth.

### Lifecycle

```
1. Block created in one file             → ^k7m2x  (solo)
2. User copies block to a second file    → Bloom promotes BOTH to ^=k7m2x
3. Three files mirror the same block     → all three have ^=k7m2x
4. User deletes from one file            → two ^=k7m2x remain
5. User deletes from a second file       → Bloom demotes survivor to ^k7m2x
```

### Promotion / demotion

Runs as part of `EnsureBlockIds` (post-save hook, already exists):

```
For each block_id on this page:
  mirror_count = index.count_pages_for_block(block_id)
  if mirror_count > 1 and marker is ^:
    rewrite to ^= in this page
    queue MirrorEdit to promote ^ → ^= in other pages
  if mirror_count == 1 and marker is ^=:
    demote to ^ in this page
```

Promotion is deferred during Insert mode (same as auto-save). The `^` → `^=` rewrite is a post-Insert-mode operation.

### Collision detection

When the indexer sees two `^` entries (not `^=`) for the same block ID:

- **Content matches** → auto-promote both to `^=` (user created a mirror)
- **Content differs** → **collision.** Notify user. Do not promote. Do not propagate.

Resolution: user renames one ID (Bloom provides a "Reassign block ID" command) or manually adds `=` to declare them mirrors despite different content.

A `^` entry alongside existing `^=` entries follows the same rule: compare content with any `^=` peer. Match → auto-promote. Mismatch → collision.

---

## Sync Mechanism

**Synchronous in-memory propagation via BufferWriter.** No file watchers, no last-write-wins races.

1. User edits `^=k7m2x` in page A.
2. `BufferWriter::apply(Edit)` mutates page A's buffer.
3. Writer queries the index: which other pages have `^=k7m2x`?
4. For each page B: load into buffer if needed, `apply(MirrorEdit)` — same rope operation, no events.
5. Auto-save writes all modified pages to disk.
6. Git commits capture the state.

All mutations happen in-memory through the single-threaded BufferWriter. No file watcher races, no dirty-buffer prompts.

### Edit vs MirrorEdit

```rust
Edit {
    page_id, range, replacement, cursor_after, cursor_idx
}
MirrorEdit {
    page_id, range, replacement
}
```

`Edit` is user-initiated. `MirrorEdit` is propagation. The distinction:
- `Edit` → emits `BlockChanged` events
- `MirrorEdit` → no events, no further propagation → prevents circular loops

One flag, checked in one place. This is the single mechanism that prevents infinite loops.

### Propagation trigger

Propagation fires on **Insert→Normal transition** (Esc). During Insert mode, the buffer is in a transient state — partial words, uncommitted edits. Propagation waits for the final content, same as auto-save.

```
User enters Insert mode → BeginEditGroup
User types keystrokes  → Edit messages (buffer mutates, no propagation)
User presses Esc        → EndEditGroup → propagation fires ONCE with final content
```

---

## What's Built

### Database schema

```sql
CREATE TABLE block_ids (
    block_id  TEXT NOT NULL,
    page_id   TEXT NOT NULL,
    line      INTEGER NOT NULL,
    is_mirror BOOLEAN NOT NULL DEFAULT FALSE,  -- TRUE for ^=
    PRIMARY KEY (block_id, page_id)
);
CREATE INDEX idx_block_ids_page  ON block_ids(page_id);
CREATE INDEX idx_block_ids_block ON block_ids(block_id);
```

`is_mirror` is derived from the `=` prefix in the file content. Fully rebuildable.

`retired_block_ids` ensures deleted IDs are never reused. `block_links` tracks `[[^block_id|hint]]` references separately from content mirrors.

### Mirror lookup

```rust
Index::find_all_pages_by_block_id(&BlockId) -> Vec<(PageMeta, line)>
```

One query, returns every page containing the block and its line number.

### BufferWriter

```rust
pub struct BufferWriter {
    buffer_mgr: BufferManager,
    block_watchers: HashMap<String, Vec<Box<dyn Fn() + Send>>>,
}
```

The event bus exists on BufferWriter. When `Edit` (not `MirrorEdit`) touches a watched block, callbacks fire.

### Toggle mirroring (active)

Full mirror propagation for checkbox toggles:

1. Load source page into buffer
2. Flip `- [ ]` ↔ `- [x]`
3. Extract block ID from the toggled line
4. Query `find_all_pages_by_block_id()`
5. For each mirror page: load, replace line, save

This is the proof-of-concept for the full pipeline.

### General text mirroring (not yet wired)

Same pipeline as toggle, generalized to any within-line edit:

1. Detect which block was edited (parse `^=xxxxx` from the edited line)
2. Propagate the full line to mirror pages via `MirrorEdit`
3. Queue saves for mirror targets

~30 lines in `BufferWriter::apply()`. Architecture is ready; this is wiring.

---

## Stress Test: `^=` Design

### ✅ Scenario 1: Create mirror

```
Page A: "- [ ] Review ropey ^k7m2x"         (solo)
User copies line to page B → saves
Indexer: two ^ entries, content matches → promote both to ^=
Result: both pages have ^=k7m2x, mirror active
```

### ✅ Scenario 2: Edit propagation

```
A and B both have ^=k7m2x
User edits A → Esc → propagation fires
MirrorEdit B with new content → both in sync
No content comparison needed — ^= is trusted
```

### ✅ Scenario 3: Delete mirror (demotion)

```
A and B both have ^=k7m2x
User deletes the line from B → saves
Indexer: only (k7m2x, A) remains → demote A to ^k7m2x
```

### ✅ Scenario 4: Collision (different content, same ID)

```
Page A: "- [ ] Review ropey ^k7m2x"
External editor adds "- Buy groceries ^k7m2x" to page C
Indexer: two ^ entries, content differs → COLLISION
User notified. No promotion. No propagation.
```

### ✅ Scenario 5: Collision alongside existing mirror

```
A and B have ^=k7m2x (active mirror)
External editor adds "- Buy groceries ^k7m2x" to page C
Propagation from A: finds ^= entries only → B. C has ^, not ^= → untouched.
Indexer: C has ^ with different content → collision flagged.
Existing mirror continues working. Collision isolated.
```

### ✅ Scenario 6: Indexer race during propagation (THE critical race)

```
A and B both have ^=k7m2x
User edits A → auto-save writes A to disk
Indexer triggers → reads A (new content), reads B (old content)
Content of A ≠ content of B — temporary divergence!

BUT: both have ^= → they are declared mirrors → no collision check.
Indexer trusts the marker.

MirrorEdit updates B moments later → content matches again.
```

**This is the scenario that broke content-comparison approaches.** The `^=` marker survives temporary divergence. The indexer never misinterprets a mid-propagation state as a collision.

### 🟡 Scenario 7: Three-way promotion race

```
Page A: ^k7m2x (solo)
User copies to B → saves (two ^ entries)
External editor simultaneously copies to C

Indexer processes B first → A and B promoted to ^=
Indexer processes C → C has ^, A and B have ^=
Compare C content with ^= peers → matches → auto-promote C to ^=
```

Works if content matches. If content differs, C is flagged as collision.

### ✅ Scenario 8: Manual `^=` without peers

```
User types "- [ ] New task ^=k7m2x" (no other pages have this ID)
Indexer: mirror_count == 1 but marker is ^=
EnsureBlockIds: demote to ^k7m2x
```

Self-correcting.

### 🟡 Scenario 9: User manually removes `=` from a mirror

```
A and B both have ^=k7m2x
User edits A: changes ^=k7m2x to ^k7m2x
Indexer: A has ^, B has ^= — mixed state
Content matches → re-promote A to ^= (treat as accidental edit)
Content differs → flag for user resolution
```

### 🟡 Scenario 10: External editor changes mirrored content

```
A, B, C all have ^=k7m2x with matching content
External editor changes C's content (keeps ^=)
User edits A → propagation fires
MirrorEdit B → ✅ (content was in sync)
MirrorEdit C → overwrites external editor's changes
```

**Defensible:** `^=` means "keep me synced." The external editor left the marker intact. If they didn't want sync, they should have removed the `=`. Bloom trusts inline markers.

### ✅ Scenario 11: Promotion during active editing

```
User is editing page A in Insert mode
Promotion wants to rewrite ^k7m2x → ^=k7m2x
```

Deferred. Promotion runs in `EnsureBlockIds`, a post-save hook. Auto-save is deferred during Insert mode. Promotion happens after Normal mode transition.

### ✅ Scenario 12: Rapid successive edits

```
A and B both have ^=k7m2x
User types 20 keystrokes in Insert mode → no propagation
User presses Esc → ONE propagation with final content
B gets one MirrorEdit, one save
```

---

## Design Decisions

| Decision | Rationale |
|----------|-----------|
| `^=` marker in file content | Files are source of truth. Index is derivable. Survives rebuild. |
| `^=` is symmetric (no owner/copy) | All copies are equal co-owners. `=` means "peers exist." |
| Content comparison only at promotion | `^` + `^` detected → compare once. After `^=`, trust the marker. |
| In-memory sync, not file patching | BufferWriter owns all buffers. No file watcher races. |
| `MirrorEdit` as separate message | One-flag circular prevention. No events, no re-trigger. |
| Single-threaded writer | All mutations serialized. No concurrent-edit races. |
| Propagation on Insert→Normal | Buffer is in final state. Same trigger as auto-save. |
| No CRDT, no merge logic | Local-first, single-user. Git is the safety net. |

---

## Pre-conditions

### Block IDs required

Every block participating in mirroring must have a block ID. `EnsureBlockIds` runs as a post-save hook and assigns IDs to blocks that lack them.

### Single-user assumption

Bloom is a single-user, local-first app. Two users editing the same mirrored block simultaneously is not supported. If shared vaults are ever added, mirroring would need conflict resolution beyond last-write-wins.

---

## Prior Art

### Notion synced blocks

Notion duplicates a block across pages with a centralized server and CRDT-like conflict resolution. The synced block has a single canonical ID; all instances are rendered inline.

**Difference:** Notion is cloud-first with a sync server. Bloom is local-first with no server. MirrorEdit is synchronous in-memory propagation — simpler, no conflicts in single-user.

### Roam Research / Logseq block references

Both use `((block-id))` syntax to embed a reference to a block. The reference renders content inline but is **read-only** — edits don't propagate. This is transclusion, not mirroring.

**Bloom's approach:** Block links (`[[^block_id|hint]]`) serve the read-only reference role. Block mirroring (`^=`) goes further — all copies are editable, edits propagate via MirrorEdit.

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net
