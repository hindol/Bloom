# Block Identity 🧬

> Universal, short, self-healing block IDs — stable identity for every piece of content.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

Today, blocks (paragraphs, list items, tasks) are identified by `(page_id, line_number)`. This is fragile:

- **Lines shift.** Insert a line above and every ID below is wrong.
- **Text matching is ambiguous.** Two tasks that say "Follow up with team" in different pages — or even the same page.
- **No cross-time identity.** The day view cache says "task on line 52" but by next week line 52 is something else entirely.

Every feature that says "act on this specific thing" needs stable identity: BQL result actions, day view task toggles, emergence detection linking chunks across time, MCP targeting specific blocks, link-to-block, refactoring operations.

The current design (GOALS.md G4) gives blocks IDs lazily — only when first linked to. This is insufficient. **Everything needs an ID, all the time.**

---

## The Design

### Page-Scoped Short IDs

Block IDs are unique **within their page**, not globally. This means they can be extremely short.

```markdown
- [ ] Review the ropey API @due(2026-03-10) ^a3
- [x] Compare with PieceTable ^b1
Some paragraph about rope data structures. ^c
```

A page with 100 blocks needs 100 unique IDs. Two base-36 characters give 1,296 combinations — more than enough for any realistic page.

**Global identity** is the composite `page_id^block_id`:

```
8f3a1b2c^a3    ← globally unique across the entire vault
```

This is Bloom's deep-link syntax: `[[8f3a1b2c^a3|Review ropey API]]`. The `^` mirrors the block ID marker in the file, so the user sees a consistent symbol. We're just ensuring every block *has* an ID, not only blocks that happen to be linked.

### Assignment Strategy

| Rule | Rationale |
|------|-----------|
| Shortest available ID first | `^a`, `^b`, ..., `^z`, `^a0`, `^a1`, ... — minimal visual noise |
| Auto-assigned on first index | Every task, list item, paragraph, heading gets an ID when the indexer processes the page |
| Never reused within a page | Deleted block's ID is retired — avoids stale references in git history, day view caches, and external links |
| Manually-set IDs are respected | If a user writes `^my-note`, Bloom keeps it — auto-assignment only fills gaps |

### ID Placement

The ID is always appended to the **last line of the block** — end of the thought, never interrupting content mid-flow.

| Block type | Placement | Example |
|-----------|-----------|---------|
| Single-line (task, heading, simple list item) | End of that line | `- [ ] Review ropey API ^a3` |
| Multi-line paragraph | End of last line before the blank line | `...structures for editing. ^b` |
| Multi-line blockquote | End of last `>` line | `> — Someone wise ^c` |
| List item with continuations | End of last continuation line | `  final detail here ^d` |

```markdown
> The best tool is one that
> disappears in your hand.
> — Someone wise ^c

Some paragraph that spans
multiple lines about rope
data structures for editing. ^b

- A list item that continues
  onto the next line with
  additional detail ^d
```

One rule: **end of the last line of the block.** No special cases.

### What Gets an ID

| Block type | Gets auto-ID? | Rationale |
|-----------|---------------|-----------|
| Task (`- [ ]` / `- [x]`) | ✅ Yes | Actionable from views, cacheable in day view, toggleable |
| List item (`- text`) | ✅ Yes | Referenceable, movable between pages (G18) |
| Paragraph | ✅ Yes | Emergence detection needs stable chunk identity over time |
| Heading | ✅ Yes | Already linkable via `#section-id`, just needs guaranteed assignment |
| Blockquote | ✅ Yes | Referenceable content |
| Code block | ❌ No | Not semantically meaningful as a "thought" — just formatting |
| Frontmatter | ❌ No | Metadata, not content |
| Blank lines | ❌ No | Not content |

### Visual Treatment

Block IDs render as `SyntaxNoise` — the lowest visibility tier in Bloom's semantic weight system:

- **Foreground:** `faded`
- **Decoration:** `dim`
- **Tier 3 (Noise):** Same treatment as `**` bold markers and `[[` link brackets

In practice, `^a3` at the end of a line is barely perceptible. The content is what you read; the ID is what the system uses.

```
As rendered in Bloom (described):

  - [ ] Review the ropey API @due(2026-03-10) (^a3)
                                               ^^^^
                                               faded + dim, nearly invisible
```

---

## Self-Healing

Block IDs can be accidentally deleted — a user backspaces over `^a3`, or an external tool strips it, or a git merge drops it. In other systems, this silently breaks every reference to that block. In Bloom, **git makes block IDs self-healing.**

### Detection

The indexer already parses every page on save. After re-parsing, it compares the new set of block IDs against what the index previously recorded for that page.

| Situation | Detection |
|-----------|-----------|
| Block ID present before, missing now | ID was deleted — candidate for repair |
| Block ID present before, still present | Normal — no action |
| New block ID not in index | Newly assigned or user-created — add to index |

### Repair Pipeline

When a missing ID is detected:

```text
Indexer: "Page 8f3a1b2c previously had ^a3, but it's gone now"
    │
    ▼
Git check (via gix):
    │ Diff current file vs last commit where ^a3 existed
    │ Find the line that had ^a3 — extract its text content
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
    │   └── Re-append ^a3 to the matched line
    │       └── Notify: "Restored block ID ^a3"
    │
    ├── Ambiguous match (multiple candidate lines)
    │   └── Use line-number proximity to break tie
    │       └── Assign to closest line, restore
    │
    └── No match (content was deleted or rewritten beyond recognition)
        └── Mark ^a3 as orphaned in the index
            └── Any links to 8f3a1b2c^a3 become broken links (G20)
```

When multiple IDs are missing (e.g., an external tool stripped all IDs), all restorations are **batched into a single file write** — detect all missing IDs, compute all matches, write once. No write storm.

### Why This Works

Git gives us the **complete history** of every line in every file. For any block ID that disappears, we can:

1. **Find when it was removed** — `gix` diff between current and previous commits
2. **Find what the line said** — the old content is in git, verbatim
3. **Find where the line is now** — match old content against current page

The content match isn't the fragile text matching we rejected earlier. It's a one-time repair operation on a *specific known line* from git history, not a vault-wide search. The old text is exact (from git), the search scope is one page, and we have the previous line number as a starting hint. This is reliable.

### Self-Healing Guarantees

| Scenario | Outcome |
|----------|---------|
| User backspaces over `^a3` | Restored on next save — ID reappears |
| External tool strips all `^block-id` markers | All restored in a single batched write on next index |
| Git merge drops an ID due to conflict resolution | Restored when Bloom re-indexes after conflict resolution |
| User deletes the entire line | ID becomes orphaned — correct, the content is gone |
| User rewrites the line completely | Content match fails — ID becomes orphaned — correct |
| User intentionally removes an ID repeatedly | Suppressed after 2 consecutive restore-delete cycles (see below) |
| User merges two lines, one ID survives | Surviving ID stays; the other ID's content match skips the line (already has an ID) — orphaned correctly |
| User splits a line | Original ID stays on first fragment; new fragment gets a fresh ID on next index |
| Cut-paste block to another page | Old page's ID orphaned; new page gets a fresh ID (see Limitations) |
| No git history available | Self-healing degrades gracefully — missing IDs become orphaned, not restored |

### Intentional Deletion Suppression

If a user deliberately deletes a block ID, Bloom should not fight them indefinitely.

**Mechanism:** the index tracks a `heal_attempts` counter per `(page_id, block_id)`. Each time Bloom restores an ID and the user deletes it again within the same session (or within 2 consecutive index cycles), the counter increments. After **2 consecutive restore-delete cycles**, Bloom marks the ID as **user-suppressed** and stops restoring it.

```
Save 1: user deletes ^a3 → Bloom restores ^a3 (heal_attempts: 1)
Save 2: user deletes ^a3 again → Bloom restores ^a3 (heal_attempts: 2)
Save 3: user deletes ^a3 again → Bloom accepts deletion, marks suppressed
```

Suppressed IDs remain in the index as orphaned. The suppression flag can be cleared by the user via `:restore-block-ids` if they change their mind.

### Limitations

**Cut-paste across pages.** If a user manually cuts a block from page A and pastes it into page B, Bloom sees a deletion in A and an addition in B. The block ID in A is orphaned. The pasted content in B gets a *new* page-scoped ID. Any links to `pageA#a3` break.

This is inherent to page-scoped IDs under raw cut-paste — Bloom can't distinguish "moved" from "deleted in A, coincidentally similar text written in B." The refactoring commands (`SPC r b` — move block) handle this correctly by updating links. Raw cut-paste cannot.

**Mitigation:** when the indexer detects orphaned content in page A and newly-indexed identical content in page B within the same index cycle, it could *suggest* the move: "Block ^a3 appears to have moved to page B. Update links?" This is a heuristic, not a guarantee — offered as a prompt, not an automatic action.

**No git history.** Self-healing requires git history (`[history] enabled = true`). Without it, missing IDs become orphaned immediately — no recovery attempt. Bloom shows a notification: "Block IDs cannot be self-healed without history enabled." Auto-assignment of *new* IDs still works normally.

---

## Integration with Other Features

### Day View Cache (TIME_TRAVEL.md)

The day view cache stores `(page_id, block_id)` pairs for tasks that appeared in that day's git diff. Toggle from the day view resolves by block ID — one index lookup, no ambiguity:

```
Cache:  "task ^a3 in page 8f3a1b2c was created on Mar 8"
Index:  "^a3 in 8f3a1b2c is currently at line 54, status: [ ]"
Toggle: flip line 54 from [ ] to [x]
```

If `^a3` is orphaned (content deleted), the day view shows the task as historical with no action available.

### BQL (LIVE_VIEWS.md)

Query results carry block identity. Acting on a result (toggle task, jump to source) resolves by `page_id^block_id`. This makes every BQL result reliably actionable.

### Emergence Detection (EMERGENCE.md)

Embedding chunks get stable identity via block IDs. When Bloom says "these two chunks are semantically similar," it can reference them precisely across time — even if the text has been lightly edited, the ID persists.

### MCP Server

`edit_note` and `toggle_task` currently use text matching to find targets. With universal block IDs, MCP tools can accept `block_id` as an optional parameter for precise targeting. Text matching remains as a fallback for LLMs that don't know the ID.

### Links

`[[8f3a1b2c^a3|Review ropey API]]` — deep links to blocks become reliable by default, not just for blocks that were manually or lazily assigned an ID. Every block is linkable from the moment it's indexed.

---

## Codebase Impact

### New module

| Module | Responsibility |
|--------|---------------|
| `block_id_gen.rs` | ID generation (shortest available base36), block classification (which lines need IDs) |

### Modified modules

| Module | Change |
|--------|--------|
| `lib.rs` (save path) | Call `assign_block_ids` before writing buffer to disk |
| `buffer/rope.rs` | Add `insert_at_end_of_line(line_idx, text)` helper if needed |
| `index/writer.rs` | Detect missing IDs on re-index (for future self-healing handoff) |
| `index/schema.rs` | Add `retired_block_ids` table (reserve for future self-healing) |

### Future module (not in this phase)

| Module | Responsibility |
|--------|---------------|
| `healing/` | Self-healing pipeline: detect missing IDs, git lookup, content match, restore. Runs on history thread. |

### Dependencies

No new crate dependencies.

---

## Decisions

1. **No opt-out.** Block IDs are fundamental to Bloom. Every feature that says "act on this thing" depends on stable identity. The IDs are nearly invisible (faded + dim, Tier 3 noise). No configuration flag.

2. **Eager assignment on first run.** A vault with 10K pages gets all IDs assigned immediately during first indexing. The disk writer's fingerprint-based self-write detection suppresses file watcher re-triggers (stat match → event dropped), so 10K writes don't cause 10K re-index cycles. Needs verification at scale.

3. **`^` at end of line is sufficient disambiguation.** No prefix needed. The parser requires `^` preceded by space or at start of line — this is an unambiguous position that won't collide with prose.

4. **Self-healing runs on the history thread.** Detection happens in the indexer (compare old vs new block ID sets). Repair requests are sent via channel to the history thread, which does the git lookup and content match. The indexer never blocks on git. Needs profiling.

5. **Non-deterministic assignment.** IDs are stored in file content, not out-of-band. A rebuild re-reads files which already contain their IDs. No need for deterministic regeneration. Simple shortest-available incrementing is fine.

6. **Cross-page move detection.** When the indexer sees orphaned content in page A and identical new content in page B in the same cycle, prompt the user via a persistent notification (visible in `:messages`). Don't auto-update links.

---

## Profiling Results

Measured on macOS, Apple Silicon, debug build. Production (release) will be faster.

| Scenario | Pages | Blocks | Time | Per-page |
|----------|-------|--------|------|----------|
| **No-op** (all blocks have IDs) | 1,000 | 5,000 | 46 ms | 0.05 ms |
| **Bulk assignment** (no IDs → all assigned) | 1,000 | 7,000 | 71 ms | 0.07 ms |
| **Single large page** (250 blocks) | 1 | 250 | 14 ms | — |

**Extrapolated to reference vault (10K pages):**
- No-op per keystroke: ~0.5 ms — imperceptible
- Bulk first-run: ~710 ms — acceptable, one-time cost

**Per-keystroke overhead:** The autosave path calls `ensure_block_ids` on every `handle_key`. For the common case (all blocks already have IDs), this is parse + empty-check = **0.05 ms**. No performance concern.

**Performance gates in CI** (tests fail if exceeded):
- Single large page (250 blocks): < 50 ms
- Bulk 1000 pages: < 2,000 ms
- Idempotent 1000 pages: < 1,000 ms

---

## Open Questions

1. **Self-healing profiling.** Git lookup + content match per missing ID — how much latency does this add? Needs benchmarking on the reference vault (10K pages, 18K commits). The common case (no IDs missing) is a set comparison — microseconds. The repair case should be rare.

2. **First-run write storm verification.** Assigning IDs to 10K pages means 10K file writes. The self-write detection (fingerprint match) should suppress watcher events, but this needs testing at scale. If the watcher floods the indexer despite fingerprints, we fall back to a background batch with progress indicator.

---

## References

- Current block ID design: [GOALS.md G4](../GOALS.md) (deep links, lazy ID generation)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history that enables self-healing
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL result actions that depend on stable block identity
- [EMERGENCE.md](EMERGENCE.md) — chunk identity for semantic embeddings
