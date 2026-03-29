# Bloom Lab 🧪

Exploratory design documents for ideas that aren't ready for implementation. These are half-baked, opinionated, and may never ship — but they're worth thinking about.

Each document captures a direction, not a commitment. Critique, contradiction, and abandonment are all valid outcomes.

| Document | Status | Idea |
|----------|--------|------|
| [BLOCK_ID_METADATA.md](BLOCK_ID_METADATA.md) | Superseded | Narrow block-ID design branch kept for reference, but no longer the active top-level direction after the unified document layer investigation |
| [UNIFIED_DOCUMENT_LAYER.md](UNIFIED_DOCUMENT_LAYER.md) | Draft | Idea brief to investigate a document layer that owns rope, parse state, block metadata, and semantic edit application |
| [UNIFIED_DOCUMENT_LAYER_ARCHITECTURE.md](UNIFIED_DOCUMENT_LAYER_ARCHITECTURE.md) | Draft | Architect mapping pass showing current ownership seams between input, buffer, parse state, and structural semantics |
| [UNIFIED_DOCUMENT_LAYER_OPTIONS.md](UNIFIED_DOCUMENT_LAYER_OPTIONS.md) | Draft | Architect option comparison recommending a document-model owner above `bloom-buffer` rather than pushing Markdown semantics downward |
| [UNIFIED_DOCUMENT_LAYER_RISKS.md](UNIFIED_DOCUMENT_LAYER_RISKS.md) | Draft | Architect risk review covering undo, parser invalidation, mirroring, file truth, external edits, and input-method parity |
| [EMERGENCE.md](EMERGENCE.md) | Draft | Local semantic embeddings for emergence detection, cognitive timelines, and semantic search |
| [LIVE_VIEWS.md](LIVE_VIEWS.md) | Implemented | Composable query language (BQL) with named views — agenda as a built-in view |
| [HISTORY.md](../HISTORY.md) | Implemented | Git-backed history via `gix` — auto-commit, file/block history, day activity, context strip, restore. **Promoted to docs/.** |
| [DAY_VIEW.md](DAY_VIEW.md) | Deleted | Merged into [HISTORY.md](../HISTORY.md) § Day Activity and [JOURNAL.md](../JOURNAL.md) |
| [AUTO_MERGE.md](AUTO_MERGE.md) | Draft | Three-way merge for concurrent edits — eliminate the "reload or keep?" prompt |
