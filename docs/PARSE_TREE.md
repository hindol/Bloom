# Parse Tree Architecture

> Persistent, incrementally-updated parse trees for each buffer.
> Eliminates redundant parsing, enables fast semantic queries.

---

## Current State

**No persistent parse tree.** Parsing happens in three disconnected paths:

| Path | When | What | Result lifetime |
|------|------|------|-----------------|
| `highlight_line()` | Every render frame, per visible line | Tokenize → styled spans | Discarded after frame |
| `parse()` | On save, on index | Full document → `Document` (sections, links, tags, tasks) | Consumed by indexer, discarded |
| `parse_frontmatter()` | On demand (open, navigate, picker) | YAML frontmatter → `Frontmatter` | Used once, discarded |

### Problems

1. **Redundant highlighting.** `highlight_line()` is called ~50× per frame at ~60fps = 3000 calls/second. Each call re-tokenizes the same line from scratch. If the buffer hasn't changed, all of this is wasted work.

2. **Context scanning.** `highlight_line()` takes a `LineContext { in_code_block, in_frontmatter }`. The renderer computes this by scanning from the top of the visible range. Editing line 500 requires scanning 500 lines to determine if you're inside a code fence.

3. **No live semantic data.** Features like "jump to heading," "validate link targets," or "find all tags on this page" require re-parsing the entire buffer. The indexer has this data (in SQLite) but it's stale until the next save+index cycle.

4. **Parse/highlight mismatch.** `parse()` produces a `Document` (structural), `highlight_line()` produces `Vec<StyledSpan>` (visual). These are separate parsers with no shared state. A bug in one doesn't show in the other.

### Parser Capabilities

The current `BloomMarkdownParser` is **stateless and line-oriented**:

- `highlight_line(line, context)` — tokenizes one line, needs external context
- `parse(text)` — full-document parse, line-by-line scan, tracks code fence state
- `parse_frontmatter(text)` — YAML extraction

**No incremental support.** The parser has no concept of "this line changed, re-parse only affected lines." Every call re-parses from scratch.

---

## Proposed Architecture

### ParseTree — per-buffer persistent parse state

```
BufferSlot {
    Mutable(Buffer),         // rope + cursors + undo
    Frozen(ReadOnly<Buffer>),
}
    +
ParseTree {
    line_states: Vec<LineState>,  // per-line parse result
    dirty_range: Option<Range>,   // lines needing re-parse
}
```

Each buffer gets a `ParseTree` that persists across frames. The tree is invalidated incrementally when the buffer is edited.

### LineState — cached parse result per line

```rust
struct LineState {
    /// Syntax spans for rendering (cached highlight_line output).
    spans: Vec<StyledSpan>,
    /// Line-level context flowing INTO the next line.
    context_out: LineContext,
    /// Structural elements found on this line.
    elements: LineElements,
}

struct LineElements {
    links: Vec<ParsedLink>,
    tags: Vec<ParsedTag>,
    task: Option<ParsedTask>,
    block_id: Option<ParsedBlockId>,
    timestamps: Vec<ParsedTimestamp>,
    heading: Option<(u8, String)>,
}
```

### Incremental Update

When the buffer is edited (insert/delete/replace):

1. **Mark dirty range.** The edit touches lines `start..end`. Mark those lines dirty in the ParseTree.

2. **Context propagation.** If line N's `context_out` changes (e.g., a code fence was opened/closed), mark lines N+1.. dirty until context stabilizes. This cascades only when code fence or frontmatter delimiters change — rare.

3. **Lazy re-parse.** On next `render()`, re-parse only dirty lines. Update their `LineState`. Clear dirty range.

```
Edit at line 42:
  → dirty_range = Some(42..43)
  → render() re-parses line 42
  → check: did context_out change? (code fence, frontmatter)
    → no: done (1 line re-parsed)
    → yes: cascade until context_out matches the old value
```

### Context Propagation Rules

Context changes are rare — they only happen when these markers appear/disappear:

| Marker | Context change | Cascade |
|--------|---------------|---------|
| ` ``` ` (code fence) | `in_code_block` flips | Until matching close fence |
| `---` (frontmatter) | `in_frontmatter` flips | Until closing `---` |
| Everything else | No context change | No cascade |

In practice, most edits (typing prose, editing tasks, adding links) cascade **zero lines**. The O(1) common case is the whole point.

### Render Path (after)

```
render():
  for each visible line:
    if parse_tree.is_dirty(line_idx):
      parse_tree.reparse(line_idx, buf.line(line_idx))
    spans = parse_tree.spans(line_idx)  // cached, no re-parse
```

### Ownership

```
BufferWriter {
    buffer_mgr: BufferManager,     // owns Buffer/ReadOnly<Buffer>
    parse_trees: HashMap<PageId, ParseTree>,  // parallel to buffers
}
```

The `ParseTree` lives alongside the buffer in the `BufferWriter`. When `apply(Edit)` is called, the writer also marks the parse tree dirty. When `render()` reads spans, it goes through the parse tree.

Alternatively, bundle them:

```
struct ManagedBuffer {
    slot: BufferSlot,
    parse_tree: ParseTree,
}
```

This keeps the buffer and its parse tree in sync — they're created, closed, and evicted together.

---

## What This Enables

| Feature | Before | After |
|---------|--------|-------|
| Syntax highlighting | 3000 parse calls/sec | 0 (cached spans, re-parse on edit only) |
| "Jump to heading" | Re-parse entire buffer | Scan `parse_tree.elements` — O(n) over cached data |
| "All links on page" | Re-parse entire buffer | Collect from cached `LineElements` |
| "Am I in a code block?" | Scan from line 0 | Read `context_out` from previous line — O(1) |
| Link validation | Wait for indexer | Immediate from cached elements |
| Tag completion | Query SQLite | Collect from cached elements (for current buffer) |

---

## Implementation Plan

1. **Define `ParseTree` and `LineState` structs** in bloom-md (or bloom-core).
2. **Build initial ParseTree** on buffer open (full parse, populate all LineStates).
3. **Wire incremental invalidation** in `BufferWriter::apply(Edit)` — mark dirty range.
4. **Lazy re-parse in render** — re-parse dirty lines, update LineState, clear dirty.
5. **Migrate highlight path** — render reads `parse_tree.spans()` instead of calling `highlight_line()`.
6. **Migrate structural queries** — link following, tag completion, heading jump use ParseTree elements.
7. **Remove redundant parse calls** — `parse_frontmatter()` on demand → read from ParseTree.

---

## Open Questions

1. **Where does ParseTree live?** Alongside BufferSlot in BufferWriter (parallel HashMap) or bundled into a `ManagedBuffer` struct? Bundled is cleaner but requires changing BufferManager's storage type.

2. **Frozen buffers.** Read-only view buffers don't need incremental re-parse (content never changes). Build the ParseTree once on freeze and never invalidate. Simple.

3. **Thread safety.** ParseTree is accessed by the render path (read) and the edit path (write). Both are on the UI thread currently, so no issue. If we ever move rendering to a separate thread, the ParseTree would need to be behind an Arc<RwLock> or double-buffered.

4. **Memory cost.** Each LineState holds spans + elements. For a 1000-line file with ~5 spans per line, that's ~5000 small structs. Estimate: ~100KB per buffer. Acceptable.
