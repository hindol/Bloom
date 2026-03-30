# Emergence 🌱🔬

> Local semantic embeddings for emergence detection, cognitive timelines, and semantic search.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

You have 500 journal entries. Scattered across them are fragments — a half-formed analogy here, a contradictory observation there, a question you asked in March that you answered in June without realising it. These connections die in the noise. No human can hold 500 documents in working memory.

Bloom can.

Today Bloom finds connections through two mechanisms: explicit `[[links]]` (high signal, requires manual effort) and unlinked mentions (keyword matching on page titles — shallow, misses semantic connections). Both require you to already know what you're looking for.

The missing piece: **connections you haven't made yet.**

---

## The Vision

### Emergence Detection

Bloom continuously analyses the semantic structure of your notes and surfaces *pre-conscious* insights — connections that exist in your writing but that you haven't explicitly made.

Not "these pages mention the same word." This:

> "You described your team's communication problem on Feb 3 using the same structural pattern as the distributed consensus problem you studied on Jan 15. You may be reasoning about the same underlying problem."

It's not AI generating ideas for you. It's pointing at *your own ideas* that you haven't connected yet. The insight is already in your notes — you just can't see it because you're bounded by working memory. Bloom extends your working memory across months and thousands of pages.

**The pitch: "Bloom remembers what you're forgetting you already know."**

### Cognitive Timeline

For any concept — not just a page, but an *idea* — Bloom reconstructs how your thinking evolved over time. Not a list of files sorted by date. A semantic trajectory: here's what you knew in January, here's where your understanding shifted, here's the question you raised but never answered.

### Semantic Search

"What do I think about performance?" finds notes about benchmarking, latency, profiling, "fast enough" — even if they never use the word "performance." The query language composes with existing filters (tags, dates, pages) but the *ranking* is semantic, not keyword-frequency.

---

## The Unlock: Local Embeddings

Embedding models have gotten small enough to ship. A model like `all-MiniLM-L6-v2` is ~22 MB, runs on CPU in pure Rust via the `ort` crate (ONNX Runtime), and embeds a paragraph in ~10 ms. No Python, no GPU, no network calls.

This means we can compute a semantic vector for every meaningful chunk of text in the vault, store them locally, and do vector similarity search — all without leaving the user's machine.

---

## Architecture

### Chunk & Embed (extend the indexer)

The indexer already reads every Markdown file, parses it, and writes to SQLite. We add one step: split each page into **semantic chunks** and compute an embedding vector for each.

**Chunking strategy:**

A chunk is the smallest unit of self-contained meaning:
- A paragraph (text between blank lines)
- A list item (including nested sub-items)
- A heading + its immediate content (before the next heading or blank line)
- A blockquote
- A task (checkbox line + any continuation)

Frontmatter, code blocks, and blank lines are not chunked. Links within a chunk are resolved to display text before embedding (so the model sees "Text Editor Theory", not "8f3a1b2c").

**Storage:**

```sql
CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY,
    page_id     TEXT NOT NULL,       -- FK to pages
    block_range TEXT NOT NULL,        -- "L42-L48" line range in source
    text        TEXT NOT NULL,        -- raw chunk text (for display)
    embedding   BLOB NOT NULL,        -- f32 × 384 = 1,536 bytes per chunk
    created_at  TEXT NOT NULL,        -- ISO timestamp from page context
    FOREIGN KEY (page_id) REFERENCES pages(id)
);

CREATE TABLE discoveries (
    id           INTEGER PRIMARY KEY,
    chunk_a      INTEGER NOT NULL,
    chunk_b      INTEGER NOT NULL,
    similarity   REAL NOT NULL,       -- cosine similarity score
    status       TEXT DEFAULT 'new',  -- new | seen | dismissed | promoted
    surfaced_at  TEXT NOT NULL,
    FOREIGN KEY (chunk_a) REFERENCES chunks(id),
    FOREIGN KEY (chunk_b) REFERENCES chunks(id)
);
```

**Scale:**

- 10K pages × ~10 chunks/page = 100K chunks
- 100K × 384 dims × 4 bytes = **~150 MB** (vectors alone)
- Fits comfortably in memory on any modern machine
- Brute-force cosine similarity scan over 100K vectors: **< 50 ms**
- No approximate nearest neighbour index needed at this scale
- If we grow to 100K pages / 1M chunks, we add HNSW (the `instant-distance` crate)

### Indexing Pipeline (extended)

```text
File on disk
    │
    ▼
  Read + Parse (existing)
    │
    ├──▶ FTS5 index (existing — keyword search)
    │
    └──▶ Chunk + Embed (new)
            │
            ├──▶ chunks table (vector storage)
            │
            └──▶ Emergence scan (background, periodic)
                    │
                    └──▶ discoveries table
```

The embedding step runs on the indexer thread, after FTS5 writes. For incremental indexing (the common case — a few files changed), this adds < 100 ms. For a full vault re-embed (10K pages), estimate 60–90 seconds. The UI stays responsive throughout — same graceful degradation as FTS5 indexing today.

### Emergence Detection (background sweep)

After embedding, a background pass finds chunk pairs where:

1. **High semantic similarity** — cosine similarity above a tuned threshold (e.g., > 0.75)
2. **No explicit link** — their pages are not connected by `[[links]]`
3. **Temporal separation** — created more than N days apart (same-session connections are usually obvious)
4. **Novelty** — not already in the discoveries table (no re-surfacing dismissed connections)

The sweep runs:
- After each incremental index (compare new/changed chunks against the corpus)
- As a full sweep on `:rebuild-index` or periodically (e.g., daily)

**Ranking:** Discoveries are ranked by a composite score:
- Similarity strength (higher = more interesting)
- Temporal distance (further apart in time = more surprising)
- Connection density (chunks from pages with fewer existing links = more novel)

### Semantic Search (extend the picker)

`SPC s s` today does FTS5 keyword search. We add a **semantic mode**:

1. User's query text is embedded using the same model
2. Cosine similarity against all chunk embeddings
3. Results ranked by semantic similarity, not keyword frequency
4. Existing composable filters (tags, dates, task status) still apply — they filter the candidate set, semantic similarity ranks within it

**Activation:** Could be automatic (fall back to semantic when FTS5 returns few results) or explicit (a toggle in the picker, or a different keybinding like `SPC s S`).

**Preview:** Each result shows the chunk text with source page and date, same as today's line-level search results. But the matches will be *conceptually* related, not just lexically matching.

### Cognitive Timeline (new view)

For any concept — a page, a search query, or a specific chunk — find all semantically related chunks and sort by creation time.

```text
┌─ Cognitive Timeline: "distributed systems" ───────────────────────┐
│                                                                    │
│  Jan 15 · Consensus Algorithms                                    │
│  "Paxos requires a majority quorum, which maps surprisingly       │
│   well to how our team makes decisions..."                        │
│                                                                    │
│  Feb 3 · Team Retro Notes                                         │
│  "Communication breaks down when more than 3 people need to       │
│   agree on something. Same as the consensus problem?"             │
│                                                                    │
│  Feb 20 · Journal                                                 │
│  "Realised that our deploy process is basically two-phase          │
│   commit. No wonder it's fragile."                                │
│                                                                    │
│  ▸ Mar 5 · Journal                     ← UNANSWERED QUESTION     │
│  "Is there an equivalent of eventual consistency for team          │
│   communication? Need to think about this more."                  │
│                                                                    │
│  3 related chunks · 4 pages · Jan–Mar 2026                        │
└────────────────────────────────────────────────────────────────────┘
```

**Drift detection:** Compare the centroid of related chunks in one time window vs another. Visualise how the cluster has moved — your understanding is evolving, made visible. (This is a later refinement, not a launch requirement.)

---

## What We Ship vs. What Users Bring

| Component | Ships with Bloom | User-provided |
|-----------|-----------------|---------------|
| Embedding model | Small bundled ONNX model (~22 MB) | Optional: local ollama / llama.cpp endpoint for higher quality |
| Chunking + vector storage | Built-in | — |
| Emergence detection | Built-in | — |
| Semantic search | Built-in | — |
| Cognitive timeline | Built-in | — |
| Synthesis / summarisation | — | Requires local LLM via MCP |
| Contradiction detection | — | Requires local LLM via MCP |

The base experience (emergence, semantic search, timelines) works **out of the box** with the bundled model. Advanced features (natural language synthesis, "summarise my thinking on X") layer on top via the existing MCP server — the user's choice to run a local LLM.

---

## Codebase Impact

### New modules in bloom-core

| Module | Responsibility |
|--------|---------------|
| `embedding/` | Model loading (ONNX via `ort`), chunking, vector computation |
| `embedding/chunker.rs` | Split parsed pages into semantic chunks |
| `embedding/model.rs` | Load + run the ONNX embedding model |
| `embedding/vector.rs` | Cosine similarity, centroid computation, vector I/O |
| `discovery/` | Emergence detection algorithm, ranking, deduplication |

### Extended modules

| Module | Change |
|--------|--------|
| `index/` | New tables (`chunks`, `discoveries`), extended writer pipeline |
| `picker/` | Semantic search mode alongside FTS5 |
| `render/frame.rs` | New `CognitiveTimelineFrame` for the timeline view |

### New dependency

| Crate | Purpose | Size |
|-------|---------|------|
| `ort` | ONNX Runtime bindings | ~15 MB (dynamic lib, platform-specific) |

The ONNX Runtime dynamic library is the largest addition. It's a single `.dylib` / `.dll` shipped alongside the binary. The embedding model file (~22 MB) ships as a bundled asset.

### Threading

Embedding runs on the existing indexer thread (which already does file I/O + SQLite writes). The emergence sweep can share this thread or get its own — it's CPU-bound but infrequent. No new thread types needed; the existing channel architecture handles it.

---

## Open Questions

1. **Chunking granularity.** Too fine (sentences) = noisy similarities. Too coarse (whole pages) = loses precision. Paragraph-level is the starting hypothesis. Needs experimentation.

2. **Similarity threshold.** 0.75 cosine is a guess. Too low = noise. Too high = misses interesting connections. Likely needs to be tunable, or adaptive based on vault characteristics.

3. **Bundled model choice.** `all-MiniLM-L6-v2` is the obvious starting point (small, fast, well-tested). But there are newer options (nomic-embed-text, snowflake-arctic-embed-s). Need to benchmark quality vs. speed on note-like text.

4. **UX for discoveries.** How do we surface emergences without being annoying? Notification? Sidebar? Agenda-like section? A dedicated `SPC s d` (search discoveries) picker? Probably start passive (a picker) and see if users want push notifications.

5. **Privacy of the model.** The bundled model runs locally — no data leaves the machine. But we should be explicit about this in documentation. Users who configure external ollama endpoints are making their own privacy decisions.

6. **Incremental embedding updates.** When a chunk is edited, do we re-embed just that chunk? Re-embed the whole page's chunks? Need to handle chunk identity across edits (content-addressed, or positional?).

7. **Cold start.** A new vault with 5 pages won't have enough data for emergence detection to be interesting. We need graceful degradation and probably a minimum threshold before enabling discovery notifications.

---

## Non-Goals (for this feature)

- **Cloud sync of embeddings.** Vectors stay local. If you sync your vault via git, the embedding index rebuilds on the other machine (same as FTS5).
- **Training / fine-tuning.** We use a pre-trained model as-is. No user-specific training.
- **Real-time embedding.** We embed on save/index, not on every keystroke. Latency budget is seconds, not milliseconds.
- **Replacing explicit links.** Emergence complements `[[links]]`, it doesn't replace them. Explicit links are high-signal, human-curated connections. Emergence finds the ones you missed.

---

## References

- [all-MiniLM-L6-v2](https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2) — 22 MB, 384-dim, Apache 2.0
- [`ort` crate](https://github.com/pykeio/ort) — ONNX Runtime Rust bindings
- [`instant-distance`](https://github.com/instant-labs/instant-distance) — HNSW in pure Rust (for when we outgrow brute-force)
- Rougier, ["On the Design of Text Editors"](https://arxiv.org/abs/2008.06030) — the same philosophy of "compute should serve cognition"
