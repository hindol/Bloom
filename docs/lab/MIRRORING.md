# Block Mirroring 🪞

> Same block, same ID, real content in multiple files — kept in sync by last-write-wins.
> Status: **Parked** — mechanically feasible, UX tradeoffs not justified yet.

---

## The Idea

A block `^k7m2x` exists as real text in multiple files. Not a reference, not transclusion — the actual content is duplicated. Bloom keeps copies in sync: edit one, save, all other copies update. Both files are equal co-owners.

```markdown
pages/Text Editor Theory.md:
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x

pages/Rust Programming.md:
  - [ ] Review the ropey API @due(2026-03-10) ^k7m2x
```

Edit the task in one file, save, the other file is patched automatically.

---

## Sync Mechanism

**Last-write-wins, git as safety net.**

1. User edits `^k7m2x` in file A. Auto-save writes file A to disk.
2. Save path queries the index: which other files contain `^k7m2x`?
3. For each file B: read B, find the line with `^k7m2x`, replace line content with A's version, write B.
4. File watcher picks up B's change. If B is open and clean → silent reload. If dirty → prompt.
5. Git commits capture every intermediate state. Any version is recoverable.

No merge logic, no CRDT, no version vectors. The most recently saved version is authoritative.

---

## What Works

| Scenario | Behaviour |
|----------|-----------|
| Edit in one file, other not open | Patch on save. Other file updated on disk. Clean. |
| Edit in one pane, other pane open (clean) | Auto-save → patch → file watcher → silent reload in other pane within ~600ms. |
| Block moved within a file | Content unchanged, no sync triggered. |
| Circular sync | Self-write detection (fingerprint match) prevents re-trigger loops. |
| Block in N files | Last save patches N-1 files. Batched disk writes, debounced watcher events. |
| Recovery from any state | Git has every version. `SPC H h` shows full history. |

---

## What's Awkward

| Scenario | Problem |
|----------|---------|
| Both buffers dirty on same block | Dirty-buffer prompt fires unpredictably. Last save wins — the other buffer's edits are overwritten on reload. Not a data loss (git has both), but a surprising UX. |
| Undo after sync | Undo reverts the local buffer. Next auto-save re-syncs the reverted content to other files. Cascading, but correct. |
| Delete block from one file | Other files keep their copy. Mirror count decreases. No cascading delete. The remaining copies become independent blocks. |
| User doesn't expect silent file modification | Editing file A silently modifies file B. This is surprising in a local-first app where files are the source of truth. |
| Block in 10+ files | Mechanically works but suggests the wrong abstraction — tags or views would serve this better. |

---

## Why It's Parked

The core use case — "I want this task visible from two contexts" — is already served by:

- **Links:** `[[page^k7m2x|Review ropey API]]` — the task lives in one place, referenceable from anywhere.
- **BQL views:** `tasks | where tags has #rust` — see all relevant tasks without duplication.
- **Backlinks:** The task's page shows all pages that reference it.

These are read-many-write-one patterns with zero sync complexity. Mirroring is write-many, which requires sync semantics (even simple last-write-wins) and introduces surprising file-modification behaviour.

The implementation cost (~100 lines in the save path + index queries) is modest. The UX cost (explaining silent cross-file modifications, handling dirty-buffer prompts, teaching users when mirroring is appropriate vs. when links suffice) is higher than the feature warrants.

**Revisit when:** a clear use case emerges that links + views can't solve, or if users request "live copies" as a feature.

---

## References

- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — vault-scoped block IDs that make mirroring possible
- [TIME_TRAVEL.md](TIME_TRAVEL.md) — git history as the safety net for last-write-wins
- [LIVE_VIEWS.md](LIVE_VIEWS.md) — BQL views as the alternative for cross-context visibility
