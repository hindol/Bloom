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

**Global identity** is the composite `page_id#block_id`:

```
8f3a1b2c#a3    ← globally unique across the entire vault
```

This is already Bloom's deep-link syntax: `[[8f3a1b2c#a3|Review ropey API]]`. The linking architecture already expects this format — we're just ensuring every block *has* an ID, not only blocks that happen to be linked.

### Assignment Strategy

| Rule | Rationale |
|------|-----------|
| Shortest available ID first | `^a`, `^b`, ..., `^z`, `^a0`, `^a1`, ... — minimal visual noise |
| Auto-assigned on first index | Every task, list item, paragraph, heading gets an ID when the indexer processes the page |
| Never reused within a page | Deleted block's ID is retired — avoids stale references in git history, day view caches, and external links |
| Manually-set IDs are respected | If a user writes `^my-note`, Bloom keeps it — auto-assignment only fills gaps |
| Deterministic | Same content in same position always gets the same ID on first assignment — no randomness, reproducible across rebuilds |

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
            └── Any links to 8f3a1b2c#a3 become broken links (G20)
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

Query results carry block identity. Acting on a result (toggle task, jump to source) resolves by `page_id#block_id`. This makes every BQL result reliably actionable.

### Emergence Detection (EMERGENCE.md)

Embedding chunks get stable identity via block IDs. When Bloom says "these two chunks are semantically similar," it can reference them precisely across time — even if the text has been lightly edited, the ID persists.

### MCP Server

`edit_note` and `toggle_task` currently use text matching to find targets. With universal block IDs, MCP tools can accept `block_id` as an optional parameter for precise targeting. Text matching remains as a fallback for LLMs that don't know the ID.

### Links

`[[8f3a1b2c#a3|Review ropey API]]` — deep links to blocks become reliable by default, not just for blocks that were manually or lazily assigned an ID. Every block is linkable from the moment it's indexed.

---

## Codebase Impact

### Modified modules

| Module | Change |
|--------|--------|
| `index/writer.rs` | Track block IDs per page; detect missing IDs on re-index |
| `parser/` | Auto-assign IDs to blocks without them during parse→serialize |
| `store/` | Write-back modified pages (with new/restored IDs) via disk writer |

### New module

| Module | Responsibility |
|--------|---------------|
| `healing/` | Self-healing pipeline: detect missing IDs, git lookup, content match, restore |

### Dependencies

No new crate dependencies. Self-healing uses `gix` (already required for TIME_TRAVEL) and the existing indexer infrastructure.

---

## Open Questions

1. **Opt-out for power users?** Some users may not want Bloom modifying their files. A `config.toml` flag like `auto_block_ids = false` could disable auto-assignment (but then day view actions, BQL result actions, and deep linking degrade). Leaning towards: always on — the IDs are nearly invisible and the benefits are too fundamental.

2. **Bulk assignment on first run.** A vault with 10K pages and no block IDs — the first index assigns IDs to every block in every page. That's thousands of file writes. Should this be: (a) immediate (noisy but done), (b) lazy (assign when a page is opened/saved), (c) background batch with progress indicator? Leaning towards (b) — assign on save, so only pages you touch get IDs immediately.

3. **ID format.** Base-36 (`a-z0-9`) is compact but IDs like `^a3` could collide with content. Should we prefix? `^.a3`? Or is the `^` caret at end-of-line sufficient disambiguation? The current parser already handles `^block-id` syntax — need to ensure short IDs don't accidentally match inline content.

4. **Performance of self-healing.** Git lookup + content match on every save adds latency to the indexer pipeline. For the common case (no IDs missing), this is just a set comparison — microseconds. For the repair case, one `gix` diff per missing ID. Should be rare and fast, but needs profiling.

5. **Deterministic assignment.** "Same content, same position → same ID" means the assignment algorithm must be deterministic. Proposal: hash of (page_id, block_index) → base36, truncated. This means IDs survive `:rebuild-index` without changing.

6. **Cross-page move detection.** When the indexer sees orphaned content in page A and identical new content in page B in the same cycle, should it automatically suggest updating links? Or is this too risky as a heuristic? Leaning towards: prompt the user, don't auto-update.

---

## References

- Current block ID design: [GOALS.md G4](../GOALS.md) (deep links, lazy ID generation)
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git-backed history that enables self-healing
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL result actions that depend on stable block identity
- [EMERGENCE.md](EMERGENCE.md) — chunk identity for semantic embeddings
