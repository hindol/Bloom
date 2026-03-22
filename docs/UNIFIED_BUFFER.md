# Unified Buffer Architecture 🏗️

> Elm-inspired state machine for buffer management — in-memory and on-disk as one abstraction.
> Status: **Implemented.** BufferWriter, BufferMessage, MirrorEdit, event bus, per-pane cursors all in place.

---

## The Problem

Today Bloom has two code paths for buffer mutations:

1. **In-memory buffers** (open pages): mutations go through `Buffer::insert/delete/replace`, cursor adjusts, undo tree records, autosave debounces to disk.

2. **On-disk files** (not open): mutations require read→modify→write. The MCP server, background hint updater, and view toggle all need to mutate files that may or may not be in a buffer.

These two paths are maintained separately, with edge cases at the boundary: what if a file is loaded into a buffer while a disk write is in flight? What if MCP edits a file that the user just opened?

---

## The Vision: Unified Buffer State Machine

Inspired by the Elm Architecture (TEA / MVU):

```
Input (key, MCP, file watcher, timer)
    │
    ▼
Message (EditRequest, CursorMove, ToggleTask, FileChanged, ...)
    │
    ▼
BufferWriter (single owner of all buffer state)
    │
    ├── Page in memory? → mutate Buffer directly
    │
    └── Page on disk only? → read → mutate → queue debounced write
    │
    ▼
State Changed signal → UI re-renders
```

### Key Properties

1. **Single writer, many readers.** The BufferWriter thread owns all `Buffer` instances. The UI thread holds `ReadOnly<Buffer>` references for rendering. Cursor movement is the one exception (viewport concern, allowed on read-only).

2. **Messages, not direct mutation.** Every mutation is a message: `EditRequest { page_id, range, replacement }`, `ToggleTask { block_id }`, `CursorMove { page_id, position }`. The writer processes them in order. No concurrent mutation possible.

3. **In-memory and on-disk are the same.** An `EditRequest` for a page that's not in memory transparently: reads the file, creates a Buffer, applies the edit, queues a disk write, and evicts the buffer after idle timeout (same as MCP background buffers today).

4. **View toggle is just a message.** Pressing `x` in the Agenda sends `ToggleTask { block_id: "k7m2x" }`. The writer finds the page containing that block (via index), opens or reuses the buffer, flips `[ ] ↔ [x]`, and queues a save. The view regenerates from the next BQL query.

### State Machine Per Page

```
             ┌─────────┐
             │ On Disk  │ (not in memory)
             └────┬─────┘
                  │ EditRequest / Open
                  ▼
             ┌─────────┐
             │ Loading  │ (reading from disk)
             └────┬─────┘
                  │ Content loaded
                  ▼
             ┌─────────┐
        ┌───▶│  Clean   │◀──── SaveComplete
        │    └────┬─────┘
        │         │ EditRequest
        │         ▼
        │    ┌─────────┐
        │    │  Dirty   │──── autosave debounce timer
        │    └────┬─────┘
        │         │ Timer fires
        │         ▼
        │    ┌─────────┐
        └────│ Saving   │──── atomic write in progress
             └──────────┘
                  │ Idle timeout (no edits, not visible)
                  ▼
             ┌─────────┐
             │ Evicted  │ (back to On Disk)
             └──────────┘
```

### Execution Model

The BufferWriter is **a struct on the UI thread**, not a separate thread. Synchronous mutation is fast enough (rope ops are µs — see Industry Practice below). The struct centralizes all mutation logic behind a single `apply(Message)` method.

```
UI Thread:
    poll input → produce Message
    buffer_writer.apply(message) → mutate Buffer, emit events
    render(buffers) → RenderFrame → GUI draws

    BufferWriter (struct, not thread):
        apply(EditRequest)  → mutate rope, mark dirty, queue WriteRequest
        apply(ToggleTask)   → resolve block_id → find page → edit line
        apply(FileChanged)  → read disk, compare, merge or prompt
        emit(BlockChanged)  → notify subscribed views

DiskWriter Thread (existing, separate):
    recv(WriteRequest) → debounce → atomic write → send WriteComplete
```

No channels between UI and BufferWriter — it's a direct method call. The DiskWriter stays on its own thread (I/O-bound). This is the Elm pattern for code organization without Xi Editor's threading complexity.

---

## How This Solves View Toggle

The Agenda view shows tasks from multiple pages. Today, toggling requires:
1. Find the source page
2. Check if it's in a buffer
3. If yes, mutate the buffer
4. If no, read file, modify, write back
5. Regenerate the view

With the unified model:
1. Send `ToggleTask { block_id: "k7m2x" }` to the writer
2. Writer resolves block_id → page via index
3. Writer loads/reuses the buffer, flips the checkbox
4. Writer queues save, sends StateChanged
5. UI regenerates the view on next render

One code path. No in-memory vs on-disk distinction.

---

## How This Relates to Mirroring

Full block mirroring (BLOCK_IDENTITY.md § Mirror Markers) was originally parked because of 4 problems. The unified buffer model solved all of them:

| Original problem | Solution |
|---|---|
| **Dirty-buffer prompt on mirror sync** | Writer updates both buffers synchronously before save. Self-write detection suppresses file watcher events. No prompt. |
| **Cascading undos** | Mirror is one event → one propagation. Each buffer has an independent undo tree. Undo in source → mirror propagates reverted content. Undo in target → target diverges until next source save. |
| **Silent file modification** | Mirroring is opt-in per block (user pastes a block with its ID into another file). Transient notification on mirror sync. |
| **Last-write-wins race** | Single-threaded writer — all mutations serialized. No race condition. |

**One remaining edge case:** User edits the same mirrored line in two panes simultaneously. Resolution: use the existing "file changed" dialog — "Block `^k7m2x` changed in `tasks.md`. Reload or keep your version?" Same UX the user already knows from external file changes. No CRDT — simple, predictable, local-first.

**Mirror mechanics in the writer:**
```
writer.apply(Edit { page: tasks.md, block: k7m2x, ... })
  → mutate tasks.md buffer
  → emit BlockChanged("k7m2x") → views refresh
  → index lookup: which other pages contain ^k7m2x?
  → for each target page:
      → skip if target cursor is on this block
      → writer.apply(MirrorEdit { ... })  ← does NOT emit events or re-trigger mirrors
      → mark dirty → queue WriteRequest
```

**Circular prevention:** `MirrorEdit` is a separate message variant from `Edit`. It applies the same rope mutation but does NOT emit `BlockChanged` and does NOT trigger further mirror propagation. This is the single mechanism that prevents infinite loops. One flag, checked in one place.

**Mirror lifecycle:**
- **Created:** User pastes a block preserving its `^id` into another file. Indexer detects duplicate block ID.
- **Active:** Edits propagate via writer. Both files are equal co-owners.
- **Broken:** Block deleted from one file — other copies become independent. No cascading delete.

Mirroring is now implemented: `^=` markers, toggle propagation, general text propagation on Insert→Normal, and mirror UX (gutter, status hint, SPC m s/g).

---

## Threading: Keep DiskWriter Separate

The existing DiskWriter thread handles debounced atomic writes (write → fsync → rename). It's I/O-bound and blocking. The BufferWriter would be CPU-bound and fast (rope operations are µs). **Keep them separate:**

```
BufferWriter (CPU, fast):
    mutate rope → mark dirty → send WriteRequest to DiskWriter
    never touches the filesystem

DiskWriter (I/O, blocking, existing):
    recv WriteRequest → debounce → atomic write → send WriteComplete
    never touches rope/buffer state
```

Two threads, clear separation of concerns. The BufferWriter never blocks on I/O. The DiskWriter never holds buffer locks. Communication is the existing channel pair.

### Write Pipeline: Indexer Reads, DiskWriter Writes, UI Decides

**Principle:** The indexer never writes to disk. All file mutations flow through the DiskWriter. The UI thread is the brain — it filters, guards, and routes. Every write carries a monotonic `write_id` so completions can't be confused.

```
Indexer (read-only):
  read files → compute mutations (pure functions) → optimistic CAS (mtime+size, max 3 retries)
  → IndexComplete { pending_writes: HashMap<PathBuf, CasWriteRequest> }

UI thread (the brain):
  write_counter: u64 (monotonic)
  receive IndexComplete → filter pending_writes:
    open + dirty buffer → drop (user's Esc handles it)
    open + clean buffer → eager reload, assign write_id, set buf.pending_write_id
    not open → assign write_id
  → send WriteBatch(HashMap) to DiskWriter

  save_page:
    write_counter += 1
    buf.pending_write_id = Some(write_counter)
    → send Single { path, content, write_id, buffer_version } to DiskWriter

DiskWriter:
  Single { path, content, write_id, buffer_version }
    → fstat skip-if-identical, atomic_write
    → WriteComplete { path, write_id, buffer_version } or WriteFailed { path, write_id, error }
  WriteBatch(HashMap<PathBuf, CasWriteRequest>)
    → par_values() CAS (stat mtime+size vs expected) + atomic_write
    → WriteComplete / WriteFailed per file

UI thread:
  WriteComplete { write_id, buffer_version }
    → if buf.pending_write_id == write_id AND buf.version() == buffer_version → mark_clean
    → clear pending_write_id
  WriteFailed { write_id, error }
    → clear pending_write_id, show error notification
  FileEvent → read disk, compare to buffer content
    match → no action
    mismatch + clean → auto-reload
    mismatch + dirty → dialog
```

**Key properties:**

- **One mutation function, two delivery mechanisms.** `assign_block_ids` and `promote_mirrors` are pure functions used by both the indexer and the UI thread. The indexer packages results; the DiskWriter delivers them.
- **HashMap guarantees dedup.** The indexer accumulates writes in `HashMap<PathBuf, CasWriteRequest>`. Same file mutated twice (IDs + promotion) → HashMap keeps only the final result. `par_values()` for parallel writes — unique paths by construction.
- **Optimistic CAS.** The indexer re-stats after computing. Mtime+size mismatch → retry (max 3). Gives up gracefully — next cycle catches it. DiskWriter also CAS-checks before writing batches.
- **Monotonic write IDs.** Each write gets a unique `u64`. WriteComplete is only applied if the buffer's `pending_write_id` matches AND `buffer_version` hasn't changed since the save. Prevents stale completions from clearing dirty flags.
- **WriteComplete vs WriteFailed.** Success marks clean. Failure clears pending and notifies. No boolean flags.
- **Content comparison replaces fingerprints.** FileEvent handling reads the file and compares to buffer content. No mtime/size fingerprint tracking, no pending-write counters, no races. ~50µs per event.

---

## Event Bus: Block-Level Notifications

Views need to know when their visible blocks change. Instead of polling (re-running BQL on every render) or global invalidation (regenerate everything on IndexComplete), use a **targeted notification layer**:

```rust
pub enum BufferEvent {
    /// A specific block was modified (toggle, edit, etc.)
    BlockChanged { block_id: BlockId, page_id: PageId },
    /// A page was saved to disk
    PageSaved { page_id: PageId },
    /// Index was rebuilt (tags, links, search results may have changed)
    IndexComplete,
}
```

### Subscription Model

Views register interest in specific block IDs when they render:

```rust
// When the Agenda renders, it registers the blocks it's showing
view.watch(vec!["a1b2c", "d3e4f", "g5h6i"]);

// BufferWriter emits events after mutations
writer.emit(BufferEvent::BlockChanged { block_id: "a1b2c", page_id: ... });

// Views with matching subscriptions regenerate affected rows
// Only the Agenda refreshes — other views showing different blocks are untouched
```

### Why This Matters

| Approach | Cost per mutation | Scales with |
|----------|------------------|-------------|
| Polling (re-run BQL every frame) | O(query) per frame | Query complexity |
| Global invalidation (on IndexComplete) | O(all views) per save | Number of views |
| **Block-level subscription** | O(1) lookup per mutation | Number of watchers on that block |

For a vault with 10K pages and 5 open views, a toggle in the Agenda touches 1 block. Block-level notification refreshes 1 view row. Global invalidation would re-run 5 BQL queries.

### Implementation

The event bus is a `HashMap<BlockId, Vec<Sender<BufferEvent>>>` on the BufferWriter. When a mutation touches block `^k7m2x`:

1. BufferWriter applies the edit
2. Looks up watchers for `k7m2x` → finds the Agenda view's sender
3. Sends `BlockChanged` to that sender
4. Agenda receives the event, **re-runs its BQL query** (not just a row patch — the task may now be filtered out)

For Phase 1 (synchronous, no separate thread): the event bus is just a callback list. The BufferWriter calls each registered callback after a mutation. No channels needed yet.

### Event Bus Lifecycle

- **Register:** When a view renders, it registers watchers for all block IDs in its result set.
- **Notify:** BufferWriter emits `BlockChanged` after any mutation that touches a watched block.
- **Unregister:** When a view closes, watchers are cleaned up. With channels, the dead `Receiver` causes `send()` to fail — BufferWriter prunes it lazily.
- **Circular events:** Impossible — view buffers are frozen (rebuilt, not mutated), and the event bus only emits from BufferWriter mutations.

---

## Stress Test Results

Systematic analysis of every interaction pattern:

### BufferWriter + DiskWriter Separation

| Scenario | Result | Notes |
|----------|--------|-------|
| Rapid typing (60 keys/s) | ✅ | BufferWriter never waits for disk. Autosave fires on idle. |
| DiskWriter falls behind (10 queued writes) | ✅ | Buffers stay dirty but functional. Eviction checks dirty flag. |
| Crash during disk write | ✅ | Atomic write guarantees old-or-new. Undo tree persisted to SQLite. |
| WriteComplete for evicted buffer | ✅ | Ignored — fingerprint map handles stale events. |

### Event Bus

| Scenario | Result | Notes |
|----------|--------|-------|
| Toggle task in Agenda | ✅ | BlockChanged → re-run BQL → done tasks filtered out. <1ms. |
| Two views watching same block | ✅ | Both receive event. Both re-query. No double mutation. |
| Block moved to another page | ✅ | IndexComplete triggers re-query. Stale index window handled by dual event (BlockChanged + IndexComplete). |
| Watcher registration churn (scroll) | ✅ | HashMap insert/remove is O(1). 50 watchers × 60fps = 3K ops/s — trivial. |
| 10K blocks in large view | ✅ | 10K HashMap inserts on view open ~1ms. Lookup per mutation O(1). |
| Block deleted | ✅ | Re-query returns empty for that block. Stale watcher cleaned on next refresh. |
| View closes without unregister | ✅ | Dead Receiver → send fails → lazy prune. No memory leak. |
| Circular events (toggle → refresh → toggle?) | ✅ | Impossible — frozen buffer rebuild doesn't emit events. |

### In-Memory / On-Disk Uniformity

| Scenario | Result | Notes |
|----------|--------|-------|
| MCP edits file not in memory | ✅ | Read → Buffer → edit → WriteRequest → evict after 60s. |
| User opens page MCP is editing | ✅ | Buffer already in memory, user sees latest. HashMap `or_insert_with` deduplicates. |
| Toggle on page not in memory | ✅ | Read from disk (~1ms), create Buffer, edit, save. Phase 1 blocks UI briefly — acceptable. |
| Two edits for same page (MCP + user) | ✅ | Serialized by single-threaded message processing (Phase 1) or message queue (Phase 3). |
| File watcher vs BufferWriter race | ✅ | Self-write detection (fingerprint match) suppresses watcher re-trigger. |

### Cursor

| Scenario | Result | Notes |
|----------|--------|-------|
| Scroll in view while source mutates | ✅ | Different buffers — no conflict. |
| MCP insert above cursor | ⚠️ | Cursor shifts. Existing behavior. Acceptable — MCP edits are rare during active typing. |
| Cursor in pane state | ✅ | Each pane has own cursor_idx. Buffer owns multiple cursors. |

---

## Migration Path

Phase 2 is complete. All mutations go through `BufferWriter::apply(message)`.

1. **Phase 1** (done): Direct mutation. `ReadOnly<Buffer>` for views. Toggle as a direct function call.

2. **Phase 2** (done): `BufferWriter` struct on UI thread. `BufferMessage` enum with 18 variants. Event bus on the struct. Per-pane cursors. MirrorEdit for circular prevention.

No Phase 3 — synchronous is correct for single-user editors (see Industry Practice below).

---

## Industry Practice: Synchronous Mutation is Fine

Surveyed how production editors handle buffer mutation threading:

| Editor | Model | Threading |
|--------|-------|-----------|
| **VS Code / Monaco** | Synchronous | Main thread. Fast enough — rope ops are µs. |
| **Neovim** | Synchronous | Single-threaded for all buffer mutations. Async for RPC/plugins only. |
| **Zed** | CRDT (for collab) | Local edits synchronous on main thread. CRDT for remote sync. |
| **Xi Editor** | Async rope + message passing | Core ↔ frontend communicated via messages. **Abandoned** — complexity outweighed benefits. |
| **Helix** | Synchronous | Main thread. No separate writer. |

**Consensus:** Single-user editors don't need a separate writer thread. Rope mutations are µs — the rendering pass (ms) is always the bottleneck. The Elm *pattern* (messages → update → view) is valuable for code organization, but the *threading* aspect adds complexity without measurable latency improvement.

**Conclusion:** Phase 2 (extract `BufferWriter` as a struct, synchronous on UI thread) is the sweet spot. It gives the Elm architecture's benefits (single mutation path, clear message types, testable) without Xi Editor's fate. Phase 3 is reserved for if/when we have empirical evidence of mutation latency issues (unlikely).

---

## Decisions

1. **Buffer owns cursors.** This eliminates a class of UI bugs where cursor position drifts out of sync with content. Every `insert`/`delete`/`replace` auto-adjusts all tracked cursors. This is an architectural invariant (see ARCHITECTURE.md). On frozen buffers, `ReadOnly::set_cursor()` is the one allowed mutation — cursor is navigation state, not content. No edits means no auto-adjustment needed, but movement still works.

2. **DiskWriter stays separate.** I/O-bound (blocking disk writes) and CPU-bound (rope mutations) don't mix. Two threads, existing channel pair.

3. **Event bus uses HashMap<BlockId, Vec<Sender>>** for O(1) lookup on mutation.

4. **View refresh re-runs BQL query** (not row patch) because a mutation may change what the query returns (e.g., toggled task filtered out by `where not done`).

5. **One-level undo for toggle.** After `x` toggles a task, the BQL refresh may filter it out of the view (`where not done` removes completed tasks). The user can't press `x` again on a row that's gone. Fix: the view keeps the **last toggle** as `Option<(BlockId, PageId)>`. Pressing `u` reverses it — untoggling in the source, then refreshing. Only one level deep — simple, no stack, covers the common case. After any other action (navigation, new toggle), the undo slot is overwritten.

6. **Toggle is debounced.** Rapid `x` presses (multiple tasks or accidental double-tap) are collected for ~150ms, then applied in batch, then the view refreshes once. Same latency model as autosave. The user may see stale checkboxes for one frame — acceptable.

6. **Synchronous mutation on UI thread (Phase 2).** The Elm pattern is for code organization, not threading. Rope ops are µs. A separate writer thread adds complexity without measurable benefit. Xi Editor's async approach was abandoned for this reason.

7. **Direct calls for tight couplings, event bus for loose.** `mark_clean`, `set_cursor`, `begin/end_edit_group` — direct calls (one producer, one consumer). Block changes → views — event bus (one producer, N unknown consumers).

8. **Event bus is block-level only.** Pickers use snapshots — they're ephemeral (open, pick, close) so stale titles are fine. The user expects this. No page-level subscriptions needed. The event bus serves only long-lived views that watch specific blocks. One event type (`BlockChanged`), one subscriber type (views).

9. **Indexer never writes to disk.** The indexer is read-only. File mutations (block ID assignment, mirror promotion) are computed as pure functions and packaged in `IndexComplete.pending_writes`. The UI filters (skip dirty buffers, eager-reload clean ones) and routes to DiskWriter. One write path for all mutations.

10. **Content comparison replaces fingerprints.** FileEvent handling reads the file and compares to buffer content. Match → no action. No mtime/size fingerprint tracking, no pending-write counters, no race conditions. ~50µs per event.

11. **DiskWriter CAS for indexer batches.** Batch writes check mtime+size before writing. Stale batches (file changed since indexer read) are silently skipped. Self-correcting on next cycle.

12. **HashMap for write dedup.** Indexer accumulates writes in `HashMap<PathBuf, CasWriteRequest>`. Same file mutated by IDs + promotion → HashMap keeps only final state. `par_values()` for parallel writes — unique paths by construction.

---

## Open Questions

1. **Buffer eviction.** MCP and view toggles may load buffers for files not visible in any pane. These should be evicted after idle timeout (already spec'd at 60s in GOALS.md G17).

---

## Final Stress Test

Systematic analysis of the settled architecture (BufferWriter struct on UI thread, block-level event bus, DiskWriter on separate thread):

### Extreme Typing Speed

| Speed | Keys/sec | Per-key mutation | Per-frame (60fps) | CPU usage |
|-------|----------|------------------|--------------------|-----------|
| Normal (80 WPM) | ~7 | 3-9µs | ~1 key + render 3ms | ~20% |
| Fast (200 WPM) | ~17 | 3-9µs | ~1 key + render 3ms | ~20% |
| Extreme (1000 WPM) | ~83 | 3-9µs | ~1.3 keys + render 3ms | ~20% |
| Paste (10K chars) | burst | 5-10µs (O(log n) rope) | 1 insert + render 3ms | spike then idle |

Rope O(log n) insert means 10K-char paste costs the same as one keystroke. The bottleneck is always rendering (terminal I/O ~5ms, syntax highlighting ~500µs), never mutation. BufferWriter.apply() adds zero overhead — it's a direct method call.

### View Toggle (x in Agenda)

| Step | Latency |
|------|---------|
| Index lookup for block_id | µs (SQLite) |
| Buffer mutation (in-memory) | µs |
| Buffer mutation (disk read) | ~1ms |
| Event bus → Agenda callback | µs |
| BQL re-query | <1ms (SQLite FTS) |
| Frozen buffer rebuild | µs |
| **Total** | **~2ms (in-memory) / ~3ms (disk read)** |

### Concurrency

| Scenario | Result |
|----------|--------|
| MCP + user edit same page | Serialized — single-threaded. No race. |
| File watcher during toggle | Self-write fingerprint → dropped. No double-toggle. |
| Syncthing + our save race | Existing auto-merge handles it. Same as today. |
| Stale index after toggle | <300ms staleness window. IndexComplete triggers refresh. |

### Scale

| Scenario | Cost |
|----------|------|
| 100 views open | Toggle → 1 HashMap lookup → ~3 matching views → 3 BQL queries ~3ms |
| 10K blocks in event bus | 10K HashMap entries ~1ms setup. O(1) lookup per mutation. |
| View toggle undo | One-level: `Option<(BlockId, PageId)>`. `u` reverses last toggle even if task was filtered out by BQL refresh. |
| Toggle debounce | Batch 150ms, refresh once. Stale checkbox for one frame — acceptable. |

### Migration (Phase 1 → Phase 2)

| Change | Scope |
|--------|-------|
| `get_mut()` + mutation → `writer.apply()` | ~15 call sites |
| Read path (`get()`, `text()`, `cursor()`) | Unchanged |
| `set_cursor()` | Unchanged (stays on buffer) |
| Save path | Moves into writer (dirty flag → WriteRequest) |
| **Breaking changes** | **Mechanical refactor — no logic changes** |

**Verdict: No issues found.** Architecture is simple, fast, correct, and migratable.

---

## References

- [ARCHITECTURE.md](ARCHITECTURE.md) — current threading model
- [BLOCK_IDENTITY.md](BLOCK_IDENTITY.md) — block identity and mirroring (enabled by this architecture)
- [GOALS.md G17](GOALS.md) — MCP background buffers and eviction
- Elm Architecture: https://guide.elm-lang.org/architecture/
