# Unified Buffer Architecture 🏗️

> Elm-inspired state machine for buffer management — in-memory and on-disk as one abstraction.
> Status: **Draft** — architectural exploration, not committed.

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

### Threading Model

```
UI Thread (read-only):
    poll input → produce Message → send to Writer channel
    render(ReadOnly<Buffer>) → RenderFrame → TUI draws
    cursor movement: direct set_cursor on ReadOnly (viewport only)

BufferWriter Thread (single writer):
    recv(Message) → match {
        EditRequest → mutate Buffer → mark dirty → start debounce
        CursorMove → set_cursor (forwarded from UI for frozen views)
        ToggleTask → resolve block_id → find page → edit line
        FileChanged → read disk → compare → merge or prompt
        Save → atomic write → send SaveComplete
        Evict → drop Buffer (if clean + invisible + idle)
    }
    send(StateChanged) → UI re-renders

Disk Writer Thread (existing):
    recv(WriteRequest) → atomic write → send WriteComplete
```

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

Full block mirroring (MIRRORING.md) was parked because bidirectional sync across files is complex. The unified buffer model makes it simpler:

- Mirroring becomes: "when block `^k7m2x` is edited in any buffer, the writer finds all other buffers containing that block ID and applies the same edit."
- The writer is the single mutation authority — no race conditions.
- The file watcher is no longer needed for sync — the writer already knows which files changed.

This doesn't mean we should implement mirroring, but the architecture makes it feasible with minimal complexity.

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
| Cursor in pane state (future) | 🔮 | Cleaner separation. Each pane has own cursor. Requires bloom-buffer refactor. |

---

## Migration Path

This is a large refactor. A gradual migration:

1. **Phase 1** (current): Direct mutation on UI thread. `ReadOnly<Buffer>` for views. `x` toggle sends a message that the UI thread processes synchronously (same thread, just a function call). Event bus is a simple callback list.

2. **Phase 2**: Extract mutation logic into a `BufferWriter` struct (still on UI thread). All mutations go through `writer.apply(message)`. Event bus notifies views after each mutation. No threading change — just consolidation.

3. **Phase 3**: Move `BufferWriter` to its own thread. UI thread sends messages via channel. Event bus becomes channel-based. `ReadOnly<Buffer>` references updated on StateChanged signal.

Phase 1 gives us the UX (toggle works). Phase 2 gives us the architecture (single mutation path + event bus). Phase 3 gives us the threading (non-blocking UI).

---

## Open Questions

1. **Cursor ownership.** Today cursors live on the Buffer. With a writer thread, cursor movement would be a message round-trip (too slow for 60fps typing). Current solution: cursor stays on ReadOnly via `set_cursor`. Better solution: cursor lives in pane state, not on the buffer.

2. **Undo across views.** If the Agenda toggles a task in `tasks.md`, the undo entry is on `tasks.md`'s buffer. Can the user undo from the Agenda? Probably not — undo should be per-buffer, and the Agenda is a derived view.

3. **Buffer eviction.** MCP and view toggles may load buffers for files not visible in any pane. These should be evicted after idle timeout (already spec'd at 60s in GOALS.md G17).

4. **Snapshot consistency.** If the writer is on a separate thread, the UI needs a consistent snapshot for rendering. Options: double-buffer (writer publishes a new snapshot, UI swaps), or reader-writer lock on the buffer collection.

5. **Event bus granularity.** Block-level is the sweet spot for views. But what about the picker (needs to know when page titles change) or the status bar (needs to know when dirty flag changes)? These could be separate event types on the same bus, or separate subscription channels.

---

## References

- [ARCHITECTURE.md](../ARCHITECTURE.md) — current threading model
- [MIRRORING.md](MIRRORING.md) — block mirroring (parked, enabled by this architecture)
- [GOALS.md G17](../GOALS.md) — MCP background buffers and eviction
- Elm Architecture: https://guide.elm-lang.org/architecture/
