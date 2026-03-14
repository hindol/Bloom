# Parse Tree Architecture

> Persistent, incrementally-updated parse trees for each buffer.
> Enables instant semantic queries; rendering is already fast enough without caching.

---

## Current State

**No persistent parse tree.** Parsing happens in three disconnected paths:

| Path | When | What | Result lifetime |
|------|------|------|-----------------|
| `highlight_line()` | Every render frame, per visible line | Tokenize → styled spans | Discarded after frame |
| `parse()` | On save, on index | Full document → `Document` (sections, links, tags, tasks) | Consumed by indexer, discarded |
| `parse_frontmatter()` | On demand (open, navigate, picker) | YAML frontmatter → `Frontmatter` | Used once, discarded |

### What's fast enough (benchmarked)

| Operation | Time | Per frame (60fps) |
|-----------|------|--------------------|
| `highlight_line()` — 1 line | **0.4µs** | — |
| 50-line viewport highlight | **16µs** | 0.1% of 16ms budget |
| Full document parse (1000 lines) | **741µs** | 4.6% of budget |

*Measured on Apple Silicon, release build.*

At 50 lines × 60fps, total highlighting cost is **1.2ms/second** — negligible. The uncached rendering path is fast enough. Span caching would save ~16µs per frame, which is not worth the complexity.

### What's NOT fast enough

1. **Context scanning is O(N).** `highlight_line()` takes a `LineContext { in_code_block, in_frontmatter }`. The renderer computes this by scanning from line 0 (or the top of the visible range). Editing line 500 in a code-fence-heavy file requires scanning all preceding lines. Today this is cheap because code fences are rare in notes, but it's architecturally fragile.

2. **No live semantic data.** "Jump to heading," "validate link targets," "find all tags on this page" require a full re-parse (~741µs). The indexer has this data in SQLite but it's stale until the next save+index cycle. For interactive features (autocomplete, go-to-definition), 741µs per query adds up.

3. **Parse/highlight mismatch.** `parse()` produces a `Document` (structural), `highlight_line()` produces `Vec<StyledSpan>` (visual). These are separate code paths with no shared state. A bug in one doesn't show in the other.

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

The primary value is **instant semantic queries**, not rendering performance.

| Feature | Before | After |
|---------|--------|-------|
| "Jump to heading" | Re-parse entire buffer (741µs) | Scan cached elements — O(n) over pre-built data |
| "All links on page" | Re-parse entire buffer | Collect from cached `LineElements` |
| "Am I in a code block?" | Scan from line 0 — O(N) | Read `context_out` from previous line — O(1) |
| Link validation | Wait for indexer (save cycle) | Immediate from cached elements |
| Tag completion | Query SQLite (stale until save) | Collect from cached elements (always fresh) |
| Syntax highlighting | 16µs/frame (fast enough) | ~0µs if cached (marginal improvement) |

---

## Implementation Plan

Priority order based on benchmarks:

### P1 — Structural element cache (when features need it)
Build a persistent `ParseTree` that caches structural elements (headings, links, tags, tasks, block IDs) per line. Incrementally update on edit. This is the gate for: jump-to-heading, inline link validation, tag completion, and live structural queries.

### P2 — Line-end context cache (nice to have)
Store line-end context (`in_code_block`, `in_frontmatter`) for all lines. Eliminates O(N) context scan. Low urgency — code fences are rare in notes and the scan is fast — but it's architecturally clean and a prerequisite for proper incremental invalidation.

### P3 — Viewport span cache (not needed)
Cache rendered `Vec<StyledSpan>` for visible lines. Would save ~16µs per frame. **Not worth the complexity.** Only reconsider if profiling shows highlight cost growing (e.g., complex syntax extensions, very wide lines).

### Steps (for P1 + P2)

1. **Define `ParseTree` and `LineEndContext` structs** in bloom-md (or bloom-core).
2. **Build initial ParseTree** on buffer open (full parse, populate all line states).
3. **Wire incremental invalidation** in `BufferWriter::apply(Edit)` — mark dirty range.
4. **Context propagation** — if line N's context_out changes, cascade until stable.
5. **Migrate structural queries** — link following, tag completion, heading jump use ParseTree elements.
6. **Remove redundant parse calls** — `parse_frontmatter()` on demand → read from ParseTree.

Step 5 is the payoff. Steps 1–4 are infrastructure. Don't build 1–4 until a feature in step 5 is needed.

---

## Memory Model — Learn From Other Editors

### How other editors do it

| Editor | Strategy | Per-line cost | 1000-line file |
|--------|----------|---------------|----------------|
| **Tree-sitter** (Neovim, Zed, Helix) | Concrete syntax tree (CST), ~40 bytes/node | Varies (~3 nodes/line) | ~120KB |
| **VS Code / Monaco** | Line-end tokenizer state (tiny) + on-demand token cache for visible lines | 4 bytes state + ~100 bytes cached spans (visible only) | ~9KB |
| **Bloom (proposed)** | Line-end context + viewport span cache | Same as VS Code | ~9KB |

### The key insight

**Don't cache spans for the entire file.** Cache two things:

1. **Line-end context** — the state flowing from line N to line N+1 (`in_code_block`, `in_frontmatter`). Tiny: ~4 bytes per line, stored for ALL lines. This is what makes incremental invalidation work — if an edit doesn't change the line-end context, no cascade.

2. **Viewport span cache** — the rendered `Vec<StyledSpan>` for currently visible lines (~50). Re-computed on scroll. At ~10µs per `highlight_line()` call, re-highlighting 50 lines on scroll costs 500µs — imperceptible.

### Memory budget

```
Line-end contexts:  1000 lines × 4 bytes  =    4 KB
Viewport spans:       50 lines × 100 bytes =    5 KB
Structural elements:  on-demand (from full parse on save) = 0 KB in steady state
─────────────────────────────────────────────────────
Total per buffer:                              ~9 KB
```

Compare: a 1000-line Markdown file is ~25KB of raw text. The parse cache is ~36% of the text size. Acceptable.

### What this changes in the design

The `LineState` struct from the proposal above should be split:

```rust
/// Stored for ALL lines — enables incremental context propagation.
struct LineEndContext {
    in_code_block: bool,
    in_frontmatter: bool,
    code_fence_lang: Option<SmallString>,  // ~12 bytes with SSO
}

/// Stored for VISIBLE lines only — evicted on scroll.
struct CachedSpans {
    line_idx: usize,
    spans: Vec<StyledSpan>,
}
```

The `LineElements` (links, tags, tasks) are NOT cached per-line. They come from the full `Document` parse on save (already stored in the SQLite index). Live queries use the index. The ParseTree doesn't duplicate structural data.

---

## Open Questions

1. **Where does ParseTree live?** Alongside BufferSlot in BufferWriter (parallel HashMap) or bundled into a `ManagedBuffer` struct? Bundled is cleaner but requires changing BufferManager's storage type.

2. **Frozen buffers.** Read-only view buffers don't need incremental re-parse (content never changes). Build the ParseTree once on freeze and never invalidate.

3. **Thread safety.** ParseTree is accessed by the render path (read) and the edit path (write). Both are on the UI thread currently, so no issue. If we ever move rendering to a separate thread, the ParseTree would need to be behind an `Arc<RwLock>` or double-buffered.

4. **When to build.** Don't build the parse tree infrastructure until a concrete feature needs it (jump-to-heading, inline link validation, tag completion). The rendering path is already fast enough without it.
