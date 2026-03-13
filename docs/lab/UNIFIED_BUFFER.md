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

## Migration Path

This is a large refactor. A gradual migration:

1. **Phase 1** (current): Direct mutation on UI thread. `ReadOnly<Buffer>` for views. `x` toggle sends a message that the UI thread processes synchronously (same thread, just a function call).

2. **Phase 2**: Extract mutation logic into a `BufferWriter` struct (still on UI thread). All mutations go through `writer.apply(message)`. No threading change — just consolidation.

3. **Phase 3**: Move `BufferWriter` to its own thread. UI thread sends messages via channel. `ReadOnly<Buffer>` references updated on StateChanged signal.

Phase 1 gives us the UX (toggle works). Phase 2 gives us the architecture (single mutation path). Phase 3 gives us the threading (non-blocking UI).

---

## Open Questions

1. **Cursor ownership.** Today cursors live on the Buffer. With a writer thread, cursor movement would be a message round-trip (too slow for 60fps typing). Current solution: cursor stays on ReadOnly via `set_cursor`. Better solution: cursor lives in pane state, not on the buffer.

2. **Undo across views.** If the Agenda toggles a task in `tasks.md`, the undo entry is on `tasks.md`'s buffer. Can the user undo from the Agenda? Probably not — undo should be per-buffer, and the Agenda is a derived view.

3. **Buffer eviction.** MCP and view toggles may load buffers for files not visible in any pane. These should be evicted after idle timeout (already spec'd at 60s in GOALS.md G17).

4. **Snapshot consistency.** If the writer is on a separate thread, the UI needs a consistent snapshot for rendering. Options: double-buffer (writer publishes a new snapshot, UI swaps), or reader-writer lock on the buffer collection.

---

## References

- [ARCHITECTURE.md](../ARCHITECTURE.md) — current threading model
- [MIRRORING.md](MIRRORING.md) — block mirroring (parked, enabled by this architecture)
- [GOALS.md G17](../GOALS.md) — MCP background buffers and eviction
- Elm Architecture: https://guide.elm-lang.org/architecture/
