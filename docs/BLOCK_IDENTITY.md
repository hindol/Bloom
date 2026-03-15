# Block Identity & Mirroring 🧬🪞

> Universal, short, self-healing block IDs — stable identity for every piece of content.
> Same ID in multiple files = mirrored block, kept in sync via `^=` markers and MirrorEdit.
> Status: **Implemented.** IDs assigned, `^=` parser/index/highlighter, mirror promotion/demotion, general text propagation, mirror UX (gutter, status hint, SPC m s/m), retired IDs, stale row cleanup.

---

## The Problem

Today, blocks (paragraphs, list items, tasks) are identified by `(page_id, line_number)`. This is fragile:

- **Lines shift.** Insert a line above and every ID below is wrong.
- **Text matching is ambiguous.** Two tasks that say "Follow up with team" in different pages — or even the same page.
- **No cross-time identity.** The day view cache says "task on line 52" but by next week line 52 is something else entirely.
- **No cross-page identity.** Cut a block from page A, paste into page B — every reference to it breaks.

Every feature that says "act on this specific thing" needs stable identity: BQL result actions, day view task toggles, emergence detection linking chunks across time, MCP targeting specific blocks, link-to-block, mirroring, refactoring operations.

**Everything needs an ID, all the time.** And that ID must follow the block wherever it goes.

---

## ID Format: 5-Character Base36

```markdown
- [ ] Review the ropey API @due(2026-03-10) ^k7m2x
- [x] Compare with PieceTable ^p3a9f
Some paragraph about rope data structures. ^w1b5q
```

A block is `^k7m2x` everywhere, forever, regardless of which page it lives in. There is no composite key. The block ID alone is the identity.

| Property | Value |
|----------|-------|
| Alphabet | `a-z0-9` (36 characters) |
| Length | 5 characters, fixed |
| Space | 36⁵ = **60,466,176** unique IDs |
| Generation | Random, with collision check against index |
| Reuse | **Never.** Retired IDs are reserved forever. |

**Why 5-char base36:**

| Design parameter | Value |
|-----------------|-------|
| Peak live blocks | 10,000 pages × 100 blocks = 1,000,000 |
| Lifetime blocks (10 years, with churn) | ~5,000,000 |
| ID space | 60,500,000 |
| Density at lifetime peak | **8.3%** — virtually no collisions during random generation |

**Valid as git tree entries.** Block history is tracked as virtual files in git (see [HISTORY.md](lab/HISTORY.md)). Base36 trivially satisfies git requirements (non-empty, no NUL, no `/`).

### Vault-Scoped, Not Page-Scoped

| Scenario | Page-scoped | Vault-scoped |
|----------|------------|--------------|
| Cut block from page A, paste into page B | ID orphaned in A, new ID in B. Links break. | **ID travels with the block. Links work.** |
| `SPC r b` (move block) | Must find and update all links | **No link updates needed** |
| MCP targeting | Needs `page_id + block_id` | **Block ID alone suffices** |
| BQL result actions | Resolves by composite key | **Resolves by block ID alone** |

### Placement

The ID is appended to the **last line of the block** — end of the thought, never interrupting content.

| Block type | Example |
|-----------|---------|
| Task / list item | `- [ ] Review ropey API ^k7m2x` |
| Multi-line paragraph | `...structures for editing. ^w1b5q` |
| Blockquote | `> — Someone wise ^r4d8n` |
| List with continuations | `  final detail here ^t2g6j` |

### What Gets an ID

| Block type | Auto-ID? | Rationale |
|-----------|----------|-----------|
| Task (`- [ ]` / `- [x]`) | ✅ | Actionable from views, toggleable |
| List item (`- text`) | ✅ | Referenceable, movable |
| Paragraph | ✅ | Emergence detection needs stable identity |
| Heading | ✅ | Already linkable, needs guaranteed assignment |
| Blockquote | ✅ | Referenceable content |
| Code block | ❌ | Not a semantic "thought" |
| Frontmatter / blank lines | ❌ | Not content |

### Visual Treatment

Block IDs render as `SyntaxNoise` — faded + dim, Tier 3 (same as `**` bold markers and `[[` link brackets). Nearly invisible in practice.

### Assignment Strategy

| Rule | Rationale |
|------|-----------|
| Random 5-char base36 | Uniform distribution, no information leakage |
| Collision check on generation | `SELECT EXISTS` against index — microseconds |
| Auto-assigned on first index | Every block gets an ID when the indexer processes the page |
| Never reused | Retired IDs in `retired_block_ids` — avoids stale references |

### Retired IDs and Never-Reuse

A retired ID is one that *used to exist but was deleted*. Reusing it would cause two problems:
1. **Broken links point to wrong block.** `[[^k7m2x|old hint]]` now resolves to a completely different block.
2. **Wrong git history.** The new block inherits the old block's commit history in `.blocks/k7m2x`.

The `retired_block_ids` table caches known retired IDs for collision avoidance during generation. But the table itself lives in the index DB — which is a deletable cache.

**Recovery on index rebuild — three sources, in priority order:**

| Data available | Source of retired IDs | Cost |
|---------------|----------------------|------|
| Index DB intact | `retired_block_ids` table | Instant |
| Index DB deleted, `.git/` intact | Scan git history: union of all `^xxxxx` ever seen − current live IDs | ~400ms for 10K pages / 18K commits |
| Index DB deleted, `.git/` deleted | Broken link scan: `{ id \| [[^id\|...]] in any file } − { id \| ^id in any file }` | During normal file parse (free) |

**Why this is watertight:**

- If `.git/` survives, we recover *all* retired IDs from history. Full protection.
- If `.git/` is also deleted, history is gone — so problem #2 (wrong git history) is impossible. Only problem #1 (broken links) remains. The broken link scan catches exactly those IDs: a link references a block that doesn't exist. Any ID found this way goes into `retired_block_ids`.
- If neither `.git/` nor any broken links survive, the retired ID is truly forgotten. No references to it exist anywhere. Collision is harmless.

**Each level of data loss has a proportional recovery mechanism. The worst case still protects against the only harmful collision scenario.**

### Stale Row Cleanup

When block `^k7m2x` moves from page A to page B (cut-paste), both `(k7m2x, A)` and `(k7m2x, B)` may exist in `block_ids` until page A is re-indexed. Stale rows cause `find_all_pages_by_block_id` to return pages where the block no longer exists.

**Fix:** After inserting a page's block_ids during re-index, clean up stale rows:

```sql
-- After inserting block_ids for page B:
-- For each block_id that B now owns, verify other pages still have it.
-- (Run during incremental_update, per-page)
DELETE FROM block_ids
WHERE block_id = ?1 AND page_id != ?2
  AND page_id NOT IN (
    SELECT page_id FROM block_ids WHERE block_id = ?1 AND page_id = ?2
  )
```

Simpler approach: during full rebuild, the entire table is wiped and re-inserted — stale rows are impossible. During incremental updates, the per-page `DELETE FROM block_ids WHERE page_id = ?` already cleans up that page's stale entries. The only window is between "block moves to B" and "A is re-indexed." This is acceptable — `MirrorEdit` to a stale target is a no-op (line not found), not data loss.

---

## Mirror Markers: `^` vs `^=`

Block IDs have two forms:

```
^k7m2x    → solo block (exists in one file only)
^=k7m2x   → mirrored block (peers exist in other files)
```

The `=` means "I have peers" — not "I am a copy." All `^=` instances are equal co-owners. There is no primary/secondary distinction.

**The marker lives in the file content.** The index derives mirror relationships from it. Delete the index, rebuild from files — all mirror relationships are preserved. Files are always the source of truth.

### Mirror Lifecycle

```
1. Block created in one file             → ^k7m2x  (solo)
2. User copies block to a second file    → Bloom promotes BOTH to ^=k7m2x
3. Three files mirror the same block     → all three have ^=k7m2x
4. User deletes from one file            → two ^=k7m2x remain
5. User deletes from a second file       → Bloom demotes survivor to ^k7m2x
```

### Promotion / Demotion

Runs as part of `EnsureBlockIds` (post-save hook):

```
For each block_id on this page:
  mirror_count = index.count_pages_for_block(block_id)
  if mirror_count > 1 and marker is ^:
    rewrite to ^= in this page
    queue MirrorEdit to promote ^ → ^= in other pages
  if mirror_count == 1 and marker is ^=:
    demote to ^ in this page
```

Deferred during Insert mode (same as auto-save).

### Collision Detection

When the indexer sees two `^` entries (not `^=`) for the same block ID:

- **Content matches** → auto-promote both to `^=` (user created a mirror)
- **Content differs** → **collision.** Notify user. Do not promote. Do not propagate.

Resolution: user renames one ID ("Reassign block ID" command) or manually adds `=` to declare them mirrors.

A new `^` entry alongside existing `^=` entries: compare content with any `^=` peer. Match → auto-promote. Mismatch → collision.

---

## Mirror Sync Mechanism

**Synchronous in-memory propagation via BufferWriter.** No file watchers, no last-write-wins races.

1. User edits `^=k7m2x` in page A.
2. `BufferWriter::apply(Edit)` mutates page A's buffer.
3. Writer queries the index: which other pages have `^=k7m2x`?
4. For each page B: load into buffer if needed, `apply(MirrorEdit)` — same rope operation, no events.
5. Auto-save writes all modified pages to disk.

### Edit vs MirrorEdit

```rust
Edit { page_id, range, replacement, cursor_after, cursor_idx }
MirrorEdit { page_id, range, replacement }
```

`Edit` emits `BlockChanged` events. `MirrorEdit` does not — no events, no further propagation. This single distinction prevents circular loops.

### Propagation Trigger

Fires on **Insert→Normal transition** (Esc). During Insert mode, the buffer is in a transient state. Propagation waits for the final content, same as auto-save.

---

## Links

Block links resolve by ID alone:

```markdown
[[^k7m2x|Review ropey API]]              ← block-only link
[[8f3a1b2c^k7m2x|Review ropey API]]      ← page hint + block ID (optional)
```

When a page hint is present, Bloom checks that page first (fast path). If the block has moved, the index finds it in its new page. Stale page hints are updated in the background.

---

## Self-Healing

Block IDs can be accidentally deleted — user backspaces, external tool strips them, git merge drops them. In Bloom, **git makes block IDs self-healing.**

### Detection

The indexer compares new block ID sets against what the index previously recorded. Missing IDs are candidates for repair.

### Repair Pipeline

```text
Indexer: "^k7m2x was in page A, but it's gone now"
  → Git: diff vs last commit where ^k7m2x existed → extract old line content
  → Content match: find a line in current page with similar content
    → Match found → re-append ^k7m2x → notify "Restored block ID"
    → No match → ID orphaned → links become broken links
```

Multiple missing IDs (e.g., external tool stripped all) are batched into a single file write.

### Cross-Page Move Detection

Because IDs are vault-scoped, moves are automatic:

```text
^k7m2x disappeared from page A, appeared in page B
  → Index updated: ^k7m2x now lives in page B
  → All links to ^k7m2x resolve to page B automatically
```

If the user pastes without the ID (just the text), page B gets a new ID. The old ID goes through self-healing on page A.

### Self-Healing Guarantees

| Scenario | Outcome |
|----------|---------|
| User backspaces over `^k7m2x` | Restored on next save |
| External tool strips all IDs | All restored in batched write |
| Git merge drops an ID | Restored on next re-index |
| User deletes entire line | ID orphaned — correct |
| Cut-paste with ID | ID detected in new page, index updated |
| Cut-paste without ID | New ID assigned; old ID goes through self-healing |

---

## Stress Test: `^=` Design

### ✅ Scenario 1: Create mirror

```
Page A: "- [ ] Review ropey ^k7m2x"         (solo)
User copies line to page B → saves
Indexer: two ^ entries, content matches → promote both to ^=
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
A and B both have ^=k7m2x → user deletes from B
Indexer: only A remains → demote A to ^k7m2x
```

### ✅ Scenario 4: Collision (different content, same ID)

```
Page A: "Review ropey ^k7m2x"
External editor adds "Buy groceries ^k7m2x" to page C
Two ^ entries, content differs → COLLISION. No propagation.
```

### ✅ Scenario 5: Collision alongside existing mirror

```
A and B have ^=k7m2x (active mirror)
External editor adds "Buy groceries ^k7m2x" to page C
Propagation from A → B only (C has ^, not ^=). Collision flagged on C.
```

### ✅ Scenario 6: Indexer race during propagation (THE critical race)

```
A and B both have ^=k7m2x
User edits A → auto-save writes A → indexer triggers
Indexer reads A (new), B (old) — content diverges temporarily

BUT: both have ^= → declared mirrors → no collision check.
MirrorEdit updates B moments later → content matches again.
```

**`^=` survives temporary divergence.** This broke content-comparison approaches.

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

Self-correcting. A `^=` with no peers is demoted to `^`.

### 🟡 Scenario 9: User removes `=` from a mirror

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

## What's Built

### Database schema

```sql
CREATE TABLE block_ids (
    block_id  TEXT NOT NULL,
    page_id   TEXT NOT NULL,
    line      INTEGER NOT NULL,
    is_mirror INTEGER NOT NULL DEFAULT 0,  -- 1 for ^=
    PRIMARY KEY (block_id, page_id)
);
```

`is_mirror` derived from `=` prefix in file content. Fully rebuildable. `retired_block_ids` for never-reuse. `block_links` for `[[^id|hint]]` references.

### Implementation status

| Component | Status |
|-----------|--------|
| Parser (`parse_block_id`) | ✅ Recognizes `^=xxxxx` → `is_mirror: true` |
| Centralized line parsing (`parse_line`) | ✅ `LineElements` struct, `extract_link_at_col` |
| Index (`block_ids` table) | ✅ `is_mirror` column, ALTER TABLE migration |
| Highlighter | ✅ Styles `^=` same as `^` (SyntaxNoise) |
| Align engine (`split_block_id`) | ✅ Delegates to `LineElements::split_block_id` |
| Toggle handler | ✅ Uses `parse_block_id()` from parser |
| ID generation | ✅ 5-char base36, collision check |
| Toggle mirroring | ✅ Full propagation pipeline |
| Retired ID detection | ✅ Old vs new comparison on re-index |
| Retired ID recovery (broken links) | ✅ On rebuild, scan block_links for orphaned targets |
| Stale row cleanup | ✅ Solo blocks cleaned on re-index |
| `^` → `^=` promotion | ✅ Automatic on index, rewrites files |
| `^=` → `^` demotion | ✅ Automatic on index, rewrites files |
| General text mirroring | ✅ Propagates edited `^=` line to all peers on Esc |
| Self-healing | ❌ Deferred (git-based repair pipeline) |

---

## Integration with Other Features

| Feature | How block IDs help |
|---------|-------------------|
| **Day view cache** | Stores block ID alone. Toggle resolves by one index lookup. |
| **BQL views** | Results carry block identity. Actions resolve by ID alone. |
| **Emergence detection** | Stable chunk identity over time, across edits and page moves. |
| **MCP server** | `block_id` parameter for precise targeting, no page context needed. |
| **Links** | `[[^k7m2x|hint]]` resolves by ID. Survives cross-page moves. |
| **Git virtual files** | `.blocks/k7m2x` — per-block history without parsing page histories. |

---

## Design Decisions

1. **Vault-scoped, not page-scoped.** Eliminates cross-page move breakage. Cost: 3 extra chars per ID.
2. **5-char base36, fixed length.** 60.5M space for 5M lifetime target. Lowercase-only.
3. **No opt-out.** Block IDs are fundamental. Nearly invisible (Tier 3 noise).
4. **Eager assignment on first run.** 10K pages → ~170ms. Fingerprint-based self-write detection suppresses re-index.
5. **`^=` marker in file content.** Files are source of truth. Index is derivable. Survives rebuild.
6. **`^=` is symmetric.** All copies are equal co-owners. No primary/secondary.
7. **Content comparison only at promotion.** `^` + `^` → compare once. After `^=`, trust the marker.
8. **`MirrorEdit` as separate message.** One-flag circular prevention.
9. **Propagation on Insert→Normal.** Buffer is in final state. Same trigger as auto-save.
10. **Self-healing via git.** Detection in indexer, repair on history thread.
11. **Never reused.** Retired IDs reserved permanently. Recovered from git history or broken links on index rebuild.
12. **No CRDT, no merge logic.** Single-user, local-first. Git is the safety net.
13. **Stale rows are transient.** Cleaned on re-index of the source page. MirrorEdit to stale target is a no-op.

---

## Profiling Results

Measured on macOS, Apple Silicon, **release build**.

| Scenario | Pages | Blocks | Time |
|----------|-------|--------|------|
| Single large page (250 blocks) | 1 | 250 | **0.74 ms** |
| Bulk assignment (all new) | 1,000 | 7,000 | **17 ms** |
| No-op (all have IDs) | 1,000 | 5,000 | **13 ms** |

Extrapolated to 10K pages: bulk ~170ms, no-op ~130ms. Per-keystroke overhead: < 0.02ms.

---

## Prior Art

### Notion synced blocks

Cloud-first, CRDT-like conflict resolution. **Bloom:** local-first, synchronous in-memory propagation, no server.

### Roam Research / Logseq block references

`((block-id))` renders content inline but is **read-only** — edits don't propagate. This is transclusion, not mirroring. **Bloom:** `[[^id|hint]]` for read-only references, `^=` for bidirectional mirroring.

---

## Mirror UX

Mirroring should be discoverable but never intrusive. The user copies a block, Bloom handles the rest. Three signals: a gutter indicator, a status bar hint, and keybindings that activate only on mirrored lines.

### Gutter indicator

The mirror indicator is the line number color — but only on the **cursor line**. When the cursor is on a `^=` line, its line number renders in `salient` instead of the normal cursor-line style. When the cursor moves away, the line number returns to normal `faded`. No colored numbers scattered across the viewport.

```
 42  - [ ] Solo task ^k7m2x                           ← faded line number
 43  - [ ] Mirrored task @due(2026-03-15) ^=abc01     ← CURSOR HERE: salient line number
 44  - Some notes                                      ← faded line number
 45  - [ ] Another mirror ^=def02                      ← faded (cursor not here)
 46  ## Heading                                        ← faded line number
```

This builds association: the user lands on a line, sees the colored number AND the status bar hint (`🪞 3 pages · SPC m: mirror`) simultaneously. After a few encounters, the colored number alone triggers recognition. No "mystery colors" on non-cursor lines that the user has to puzzle over.

**Implementation:** `RenderedLine` gains an `is_mirror: bool` flag, set from the line text during `render()` (parse `^=` — no index query). The TUI gutter renderer branches: `if is_cursor_line && is_mirror { salient } else if is_cursor_line { base_style } else { faded }`.

**Theming:** Uses existing `salient` palette slot. No new slots needed.

### Status bar hint

When the cursor sits on a `^=` line, the status bar right side shows mirror context:

```
NORMAL  page-title.md  42:15                  🪞 3 pages · SPC m: mirror
```

- `🪞 3 pages` — mirror count from the index (`find_all_pages_by_block_id`)
- `SPC m: mirror` — hint that the mirror menu is available
- Disappears instantly when cursor moves to a non-`^=` line

When the cursor is on a solo `^` line or a line without a block ID, the right hints show nothing (or journal hints if in JRNL mode — existing behavior).

### Keybindings: `SPC m` (mirror menu)

The `SPC m` prefix activates only when the cursor is on a `^=` line. On non-mirror lines, `SPC m` shows a notification: "Not on a mirrored block."

| Key | Action | Description |
|-----|--------|-------------|
| `SPC m s` | Sever mirror | Replace `^=xxxxx` with a new `^yyyyy`. This block becomes independent. Other mirrors keep their `^=xxxxx`. If only two mirrors existed, the remaining one is demoted to `^xxxxx` on next index. |
| `SPC m m` | Go to mirror | Inline menu showing all pages that share this block ID. j/k to select, Enter jumps. Esc dismisses. |

#### Sever mechanics

```
Before (page A and B mirror ^=abc01):
  Page A: - [ ] Shared task ^=abc01
  Page B: - [ ] Shared task ^=abc01

User on page A, cursor on the task line, presses SPC m s:
  → Generate new ID: ^xyz99
  → Replace in page A: - [ ] Shared task ^xyz99
  → Save page A
  → On next index: B has ^=abc01 alone → demote to ^abc01
  → A has ^xyz99 (solo, new block)

After:
  Page A: - [ ] Shared task ^xyz99    (independent)
  Page B: - [ ] Shared task ^abc01    (independent, demoted from ^=)
```

The content is now identical but the blocks are independent. Future edits to A won't propagate to B.

#### Go to mirror picker

```
┌─ Mirrors of ^abc01 ────────────────────┐
│ > Text Editor Theory.md       line 42  │
│   Rust Programming.md         line 15  │
│   Weekly Review.md            line 88  │
└────────────────────────────────────────┘
```

Standard fuzzy picker. Enter opens the selected page and jumps to the line. The current page is shown but not highlighted as the default — the user wants to go *elsewhere*.

### Notification on mirror creation

When the indexer promotes `^` → `^=` (duplicate block detected), show a transient notification:

```
🪞 Block ^abc01 mirrored in 2 pages
```

This makes the mirroring event visible. The user didn't ask for mirroring explicitly — they just copy-pasted. The notification confirms Bloom detected it and will keep copies in sync.

### Notification on mirror propagation

When `propagate_mirror_edit` fires (Insert→Normal on a `^=` line), show:

```
🪞 Updated 2 mirrors
```

Brief confirmation that the edit propagated. Same transient notification style as "✓ Saved filename.md".

---

## Open Questions

1. **Self-healing profiling.** Git lookup + content match per missing ID — needs benchmarking. Deferred until a feature requires it.
2. **First-run write storm.** 10K file writes — fingerprint detection should suppress re-index, needs scale testing.
3. **Git history scan performance.** Retired ID recovery from 18K commits estimated at ~400ms — needs validation on real vault.

---

## References

- [UNIFIED_BUFFER.md](UNIFIED_BUFFER.md) — BufferWriter architecture, MirrorEdit design, event bus
- [HISTORY.md](lab/HISTORY.md) — git-backed history for self-healing and per-block virtual files
- [LIVE_VIEWS.md](lab/LIVE_VIEWS.md) — BQL result actions that depend on stable block identity
- [EMERGENCE.md](lab/EMERGENCE.md) — chunk identity for semantic embeddings
