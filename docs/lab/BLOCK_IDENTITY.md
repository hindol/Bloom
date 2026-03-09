# Block Identity 🧬

> Universal, short, self-healing block IDs — stable identity for every piece of content.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

Today, blocks (paragraphs, list items, tasks) are identified by `(page_id, line_number)`. This is fragile:

- **Lines shift.** Insert a line above and every ID below is wrong.
- **Text matching is ambiguous.** Two tasks that say "Follow up with team" in different pages — or even the same page.
- **No cross-time identity.** The day view cache says "task on line 52" but by next week line 52 is something else entirely.
- **No cross-page identity.** Cut a block from page A, paste into page B — every reference to it breaks.

Every feature that says "act on this specific thing" needs stable identity: BQL result actions, day view task toggles, emergence detection linking chunks across time, MCP targeting specific blocks, link-to-block, refactoring operations.

The current design (GOALS.md G4) gives blocks IDs lazily — only when first linked to. This is insufficient. **Everything needs an ID, all the time.** And that ID must follow the block wherever it goes.

---

## The Design

### Vault-Scoped Unique IDs

Every block gets a **globally unique** ID — unique across the entire vault, across all time. The ID follows the block if it moves between pages.

```markdown
- [ ] Review the ropey API @due(2026-03-10) ^k7m2x
- [x] Compare with PieceTable ^p3a9f
Some paragraph about rope data structures. ^w1b5q
```

A block is `^k7m2x` everywhere, forever, regardless of which page it lives in. There is no composite key. The block ID alone is the identity.

### ID Format: 5-Character Base36

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
| Expected attempts per generation | ~1.09 (retry on collision, negligible) |

**Why not other formats:**

| Format | Chars | Space | Problem |
|--------|-------|-------|---------|
| 4-char base36 | 4 | 1.68M | Exceeds capacity at 1M live blocks |
| 4-char base62 | 4 | 14.8M | Mixed case — visual noise, case-sensitivity risk |
| 6-char hex | 6 | 16.8M | Longer for less space than base36 |
| **5-char base36** | **5** | **60.5M** | **Lowercase, compact, 12× headroom** |

**Valid as git tree entries.** Block history is tracked as virtual files in git (see [TIME_TRAVEL.md](TIME_TRAVEL.md)). Git tree entries only require: non-empty, no NUL bytes, no `/`. Base36 trivially satisfies this. The virtual files never touch the filesystem — git stores them by SHA internally.

### Why Vault-Scoped, Not Page-Scoped

Page-scoped IDs (the previous design) used short IDs like `^a3` unique within a page, with global identity as the composite `page_id^block_id`. This breaks on cross-page moves:

| Scenario | Page-scoped | Vault-scoped |
|----------|------------|--------------|
| Cut block from page A, paste into page B | ID orphaned in A, new ID in B. Links break. | **ID travels with the block. Links work.** |
| `SPC r b` (move block) | Must find and update all links | **No link updates needed** |
| `SPC r m` (merge pages) | Must rewrite block refs | **Block IDs carry through** |
| MCP targeting | Needs `page_id + block_id` or text match | **Block ID alone suffices** |
| Day view cache | Stores `(page_id, block_id)` composite | **Stores block ID alone** |
| BQL result actions | Resolves by composite key | **Resolves by block ID alone** |

The vault-scoped ID is 3 characters longer (`^k7m2x` vs `^a3`), but it eliminates an entire category of broken-reference bugs and simplifies every feature that acts on blocks.

### Links

Block links no longer require the page ID to resolve:

```markdown
[[^k7m2x|Review ropey API]]              ← block-only link, resolves from index
[[8f3a1b2c^k7m2x|Review ropey API]]      ← page hint + block ID (both supported)
```

When a page hint is present, Bloom checks that page first (fast path). If the block has moved, the index finds it in its new page. The stale page hint is updated in the background — same mechanism as display hint updates on page rename.

### Assignment Strategy

| Rule | Rationale |
|------|-----------|
| Random 5-char base36 | Uniform distribution, no information leakage, no ordering assumptions |
| Collision check on generation | `SELECT EXISTS` against the global ID set in SQLite — microseconds |
| Auto-assigned on first index | Every task, list item, paragraph, heading gets an ID when the indexer processes the page |
| Never reused | Retired IDs stay in a `retired_block_ids` table — avoids stale references in git history, day view caches, and external links |

### ID Placement

The ID is always appended to the **last line of the block** — end of the thought, never interrupting content mid-flow.

| Block type | Placement | Example |
|-----------|-----------|---------|
| Single-line (task, heading, simple list item) | End of that line | `- [ ] Review ropey API ^k7m2x` |
| Multi-line paragraph | End of last line before the blank line | `...structures for editing. ^w1b5q` |
| Multi-line blockquote | End of last `>` line | `> — Someone wise ^r4d8n` |
| List item with continuations | End of last continuation line | `  final detail here ^t2g6j` |

```markdown
> The best tool is one that
> disappears in your hand.
> — Someone wise ^r4d8n

Some paragraph that spans
multiple lines about rope
data structures for editing. ^w1b5q

- A list item that continues
  onto the next line with
  additional detail ^t2g6j
```

One rule: **end of the last line of the block.** No special cases.

### What Gets an ID

| Block type | Gets auto-ID? | Rationale |
|-----------|---------------|-----------|
| Task (`- [ ]` / `- [x]`) | ✅ Yes | Actionable from views, cacheable in day view, toggleable |
| List item (`- text`) | ✅ Yes | Referenceable, movable between pages (G18) |
| Paragraph | ✅ Yes | Emergence detection needs stable chunk identity over time |
| Heading | ✅ Yes | Already linkable via `^section-id`, just needs guaranteed assignment |
| Blockquote | ✅ Yes | Referenceable content |
| Code block | ❌ No | Not semantically meaningful as a "thought" — just formatting |
| Frontmatter | ❌ No | Metadata, not content |
| Blank lines | ❌ No | Not content |

### Visual Treatment

Block IDs render as `SyntaxNoise` — the lowest visibility tier in Bloom's semantic weight system:

- **Foreground:** `faded`
- **Decoration:** `dim`
- **Tier 3 (Noise):** Same treatment as `**` bold markers and `[[` link brackets

In practice, `^k7m2x` at the end of a line is barely perceptible. The content is what you read; the ID is what the system uses.

```
As rendered in Bloom (described):

  - [ ] Review the ropey API @due(2026-03-10) (^k7m2x)
                                               ^^^^^^^
                                               faded + dim, nearly invisible
```

---

## Self-Healing

Block IDs can be accidentally deleted — a user backspaces over `^k7m2x`, or an external tool strips it, or a git merge drops it. In other systems, this silently breaks every reference to that block. In Bloom, **git makes block IDs self-healing.**

### Detection

The indexer already parses every page on save. After re-parsing, it compares the new set of block IDs against what the index previously recorded for that page.

| Situation | Detection |
|-----------|-----------|
| Block ID present before, missing now | ID was deleted — candidate for repair |
| Block ID present before, still present | Normal — no action |
| New block ID not in index | Newly assigned — add to index |

### Repair Pipeline

When a missing ID is detected:

```text
Indexer: "^k7m2x was in page 8f3a1b2c, but it's gone now"
    │
    ▼
Git check (via gix):
    │ Diff current file vs last commit where ^k7m2x existed
    │ Find the line that had ^k7m2x — extract its text content
    │
    ▼
Content match:
    │ Search current page for a line with similar content
    │ (fuzzy match on the text portion, ignoring the ID itself)
    │
    │ Constraints:
    │   • Skip lines that already have a ^block-id
    │   • Use previous line number as proximity tiebreaker
    │   • If multiple missing IDs match the same line, closest-position wins
    │
    ├── Unique match found (line still exists, just lost its ID)
    │   └── Re-append ^k7m2x to the matched line
    │       └── Notify: "Restored block ID ^k7m2x"
    │
    ├── Ambiguous match (multiple candidate lines)
    │   └── Use line-number proximity to break tie
    │       └── Assign to closest line, restore
    │
    └── No match (content was deleted or rewritten beyond recognition)
        └── Mark ^k7m2x as orphaned in the index
            └── Any links to ^k7m2x become broken links (G20)
```

When multiple IDs are missing (e.g., an external tool stripped all IDs), all restorations are **batched into a single file write** — detect all missing IDs, compute all matches, write once. No write storm.

### Cross-Page Move Detection

Because IDs are vault-scoped, Bloom can **automatically detect** cross-page moves — no heuristics needed:

```text
Index cycle detects:
    • ^k7m2x disappeared from page A
    • ^k7m2x appeared in page B (same ID, pasted by user)
    │
    └── Update index: ^k7m2x now lives in page B
        └── All links to ^k7m2x resolve to page B automatically
        └── Background hint updater rewrites page hints in links
```

This works because the ID is globally unique. If the same 5-char string appears in a different page, it **is** the same block — not a coincidence. The user cut it from A and pasted it into B, carrying the `^k7m2x` marker with it.

If the user pastes without the ID (e.g., copied just the text, not the `^k7m2x` suffix), page B's content gets a new ID on next index. The old ID in page A goes through the self-healing pipeline (content match within page A). If the content is gone from A too, the ID is orphaned. This is correct — the user didn't preserve the identity.

### Why This Works

Git gives us the **complete history** of every line in every file. For any block ID that disappears, we can:

1. **Find when it was removed** — `gix` diff between current and previous commits
2. **Find what the line said** — the old content is in git, verbatim
3. **Find where the line is now** — match old content against current page

The content match isn't the fragile text matching we rejected earlier. It's a one-time repair operation on a *specific known line* from git history, not a vault-wide search. The old text is exact (from git), the search scope is one page, and we have the previous line number as a starting hint. This is reliable.

### Self-Healing Guarantees

| Scenario | Outcome |
|----------|---------|
| User backspaces over `^k7m2x` | Restored on next save — ID reappears |
| External tool strips all `^block-id` markers | All restored in a single batched write on next index |
| Git merge drops an ID due to conflict resolution | Restored when Bloom re-indexes after conflict resolution |
| User deletes the entire line | ID becomes orphaned — correct, the content is gone |
| User rewrites the line completely | Content match fails — ID becomes orphaned — correct |
| User intentionally removes an ID repeatedly | Restored each time — IDs are system-managed, not user-editable (future: IDs may be hidden entirely) |
| User merges two lines, one ID survives | Surviving ID stays; the other ID's content match skips the line (already has an ID) — orphaned correctly |
| User splits a line | Original ID stays on first fragment; new fragment gets a fresh ID on next index |
| **Cut-paste block to another page (with ID)** | **ID detected in new page, index updated, links resolve automatically** |
| Cut-paste block to another page (without ID) | New page gets fresh ID; old ID goes through self-healing on source page |

---

## Integration with Other Features

### Day View Cache (TIME_TRAVEL.md)

The day view cache stores block IDs for tasks that appeared in that day's git diff. Toggle from the day view resolves by block ID alone — one index lookup, no ambiguity:

```
Cache:  "task ^k7m2x was created on Mar 8"
Index:  "^k7m2x is currently in page 8f3a1b2c at line 54, status: [ ]"
Toggle: flip line 54 from [ ] to [x]
```

If `^k7m2x` is orphaned (content deleted), the day view shows the task as historical with no action available.

### BQL (LIVE_VIEWS.md)

Query results carry block identity. Acting on a result (toggle task, jump to source) resolves by block ID alone. This makes every BQL result reliably actionable — even if the block has moved pages since the query was written.

### Emergence Detection (EMERGENCE.md)

Embedding chunks get stable identity via block IDs. When Bloom says "these two chunks are semantically similar," it can reference them precisely across time — even if the text has been lightly edited or the block has moved between pages, the ID persists.

### MCP Server

`edit_note` and `toggle_task` currently use text matching to find targets. With vault-scoped block IDs, MCP tools can accept `block_id` as an optional parameter for precise targeting — no page context needed. Text matching remains as a fallback for LLMs that don't know the ID.

### Links

Deep links to blocks work by ID alone:

```markdown
[[^k7m2x|Review ropey API]]                  ← block-only link
[[8f3a1b2c^k7m2x|Review ropey API]]          ← with page hint (optional)
```

Every block is linkable from the moment it's indexed. Links survive cross-page moves without intervention.

### Git Virtual Files (TIME_TRAVEL.md)

Each block's content history is tracked as a virtual file in Bloom's internal git repo (`.index/.git/`). The block ID is the filename in the git tree:

```
.blocks/k7m2x    ← virtual file, never on disk, stores block content snapshots
```

This enables per-block `git log` / `git blame` without parsing entire page histories. The 5-char base36 ID is a valid git tree entry (no special characters, no restrictions).

---

## Codebase Impact

### New module

| Module | Responsibility |
|--------|---------------|
| `block_id.rs` | ID generation (random 5-char base36), collision check, block classification (which lines need IDs) |

### Modified modules

| Module | Change |
|--------|--------|
| `lib.rs` (save path) | Call `assign_block_ids` before writing buffer to disk |
| `buffer/rope.rs` | Add `insert_at_end_of_line(line_idx, text)` helper if needed |
| `index/writer.rs` | Global block ID registry; detect missing/moved IDs on re-index |
| `index/schema.rs` | `block_ids` table (block_id → page_id, line, status), `retired_block_ids` table |

### Future module (not in this phase)

| Module | Responsibility |
|--------|---------------|
| `healing/` | Self-healing pipeline: detect missing IDs, git lookup, content match, restore. Runs on history thread. |

### Dependencies

No new crate dependencies.

---

## Decisions

1. **Vault-scoped, not page-scoped.** Block IDs are globally unique across the entire vault and all time. This eliminates cross-page move breakage and simplifies every feature that acts on blocks. The cost is 3 extra characters per ID (`^k7m2x` vs `^a3`).

2. **5-char base36, fixed length.** 60.5M ID space for a design target of 5M lifetime blocks. Lowercase-only for visual consistency and no case-sensitivity issues. Fixed length for uniform appearance across the vault.

3. **No opt-out.** Block IDs are fundamental to Bloom. Every feature that says "act on this thing" depends on stable identity. The IDs are nearly invisible (faded + dim, Tier 3 noise). No configuration flag.

4. **Eager assignment on first run.** A vault with 10K pages gets all IDs assigned immediately during first indexing. The disk writer's fingerprint-based self-write detection suppresses file watcher re-triggers (stat match → event dropped), so 10K writes don't cause 10K re-index cycles. Needs verification at scale.

5. **`^` at end of line is sufficient disambiguation.** No prefix needed. The parser requires `^` preceded by space or at start of line — this is an unambiguous position that won't collide with prose.

6. **Self-healing runs on the history thread.** Detection happens in the indexer (compare old vs new block ID sets). Repair requests are sent via channel to the history thread, which does the git lookup and content match. The indexer never blocks on git.

7. **Cross-page moves are automatic.** Because IDs are vault-unique, the indexer detects when a block ID appears in a different page and updates the index. No user prompt needed — the block moved, the links follow.

8. **Never reused.** A deleted block's ID is retired permanently. Even if the block is gone, references to it in git history, day view caches, emergence tables, and external systems remain unambiguous.

---

## Profiling Results

Measured on macOS, Apple Silicon, **release build**.

| Scenario | Pages | Blocks | Time | Per-page |
|----------|-------|--------|------|----------|
| **Single large page** (250 blocks) | 1 | 250 | parse 0.58 ms + assign 0.16 ms = **0.74 ms** | — |
| **Bulk assignment** (no IDs → all assigned) | 1,000 | 7,000 | **17 ms** | 0.017 ms |
| **No-op** (all blocks have IDs) | 1,000 | 5,000 | **13 ms** | 0.013 ms |

**Extrapolated to reference vault (10K pages):**
- Bulk first-run: ~170 ms — imperceptible
- No-op per save cycle: ~130 ms — well within indexer budget

**Per-keystroke overhead:** The autosave path calls `ensure_block_ids` on the active buffer only (not all pages). For the common case (all blocks already have IDs), this is parse + empty-check = **< 0.02 ms**. No performance concern.

**Collision check overhead:** One `SELECT EXISTS` against the `block_ids` table per new ID. At 8% density, ~1.09 queries per assignment on average. SQLite handles this in microseconds with an index on the ID column.

**Performance gates in CI** (release build, tests fail if exceeded):
- Single large page (250 blocks): < 5 ms
- Bulk 1000 pages: < 200 ms
- Idempotent 1000 pages: < 100 ms

---

## Open Questions

1. **Self-healing profiling.** Git lookup + content match per missing ID — how much latency does this add? Needs benchmarking on the reference vault (10K pages, 18K commits). The common case (no IDs missing) is a set comparison — microseconds. The repair case should be rare.

2. **First-run write storm verification.** Assigning IDs to 10K pages means 10K file writes. The self-write detection (fingerprint match) should suppress watcher events, but this needs testing at scale. If the watcher floods the indexer despite fingerprints, we fall back to a background batch with progress indicator.

---

## References

- Current block ID design: [GOALS.md G4](../GOALS.md) (deep links, lazy ID generation)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history that enables self-healing and per-block virtual files
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL result actions that depend on stable block identity
- [EMERGENCE.md](EMERGENCE.md) — chunk identity for semantic embeddings
