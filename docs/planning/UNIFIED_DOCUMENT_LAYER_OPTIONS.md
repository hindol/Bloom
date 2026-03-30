# Unified Document Layer — Layering Options

> Architect option evaluation for the unified document layer investigation.
> Status: **Draft**

---

## Purpose

This document compares the main boundary options for Bloom's proposed unified document layer.

It answers:

- where the runtime owner of text + structure should live
- which option best preserves existing invariants
- which option best supports semantic edits and file-on-disk truth

It does **not** replace the upcoming risk review. It is the recommendation step before that review.

See also:

- `UNIFIED_DOCUMENT_LAYER.md`
- `UNIFIED_DOCUMENT_LAYER_ARCHITECTURE.md`
- the superseded block-ID-metadata branch
- `../ARCHITECTURE.md`
- `../UNIFIED_BUFFER.md`

## Evaluation criteria

Any acceptable design must preserve the following:

1. **Files on disk are the source of truth.**
2. **Block metadata remains serialized into file content.**
3. **The SQLite index remains rebuildable from files.**
4. **`bloom-buffer`'s cursor invariant is not casually weakened.**
5. **`bloom-vim` remains an input interpreter, not a mutation owner.**
6. **The design supports semantic edits without scattering them across post-edit hooks.**
7. **Future non-keyboard input should be able to use the same document semantics.**

---

## Option A — Add a document-model layer above `bloom-buffer`

### Summary

Keep `bloom-buffer` as the low-level rope/cursor/undo substrate.

Introduce or clarify a stronger document-model owner above it that owns:

- the buffer
- parse-tree lifecycle
- block boundaries / block metadata
- semantic edit application
- read-only structural queries used by higher layers

Conceptually:

```text
input interpreter (Vim, later mouse)
  -> raw edit or semantic intent
  -> document-model layer
  -> low-level buffer mutation + metadata/parse synchronization
```

### Pros

- Best matches the direction Bloom is already taking in `ManagedBuffer`.
- Preserves the clean low-level role of `bloom-buffer`.
- Preserves the existing "Vim produces, editor applies" separation.
- Gives semantic edits a natural home.
- Makes it easier to unify:
  - block ID behavior
  - block split/join behavior
  - parser invalidation
  - mirror-aware structural operations
- Fits future mouse input without coupling input semantics to Vim.

### Cons

- Adds another conceptual layer.
- Risks making `bloom-core` feel heavier if the boundary is not named clearly.
- Requires carefully deciding what stays in editor/index land versus what moves into the document owner.

### Best variant

The best near-term variant is:

- implement the document-model owner **inside `bloom-core` first**
- keep it as a distinct module / type boundary
- only consider extracting a new crate once the boundary has stabilized

This reduces churn while still moving the runtime ownership to the right place.

### Fit with source-of-truth model

Strong fit.

This option allows file-serialized block metadata to remain authoritative while still letting the in-memory document model coordinate text, parse state, and structural semantics.

---

## Option B — Move parser/block semantics into `bloom-buffer`

### Summary

Push the unified document owner all the way down into `bloom-buffer`, making that crate own:

- rope text
- parse state
- block metadata
- semantic document edits

### Pros

- Single low-level owner sounds attractive on paper.
- Could reduce runtime indirection if done perfectly.

### Cons

- Conflicts with the current architectural intent of `bloom-buffer` as a narrow text substrate.
- Pulls Bloom Markdown semantics into the lowest text layer.
- Makes the low-level buffer crate less reusable and less conceptually clean.
- Risks entangling cursor mechanics, undo behavior, and Markdown-specific structure too tightly.
- Weakens the current crate boundary where `bloom-md` is pure parsing and `bloom-vim` is pure interpretation.
- Makes it harder to distinguish generic text editing from Bloom-specific document semantics.

### Fit with source-of-truth model

Possible, but awkward.

It does not buy much for file-on-disk truth that Option A cannot already provide, while creating substantially more coupling.

### Verdict

Not recommended as the starting direction.

This is the option most likely to overcorrect and collapse healthy boundaries.

---

## Option C — Keep the current split and add only a thin intent layer

### Summary

Keep the current ownership mostly intact:

- `bloom-buffer` stays low-level
- `BufferWriter` keeps doing raw mutations
- editor code keeps most semantic follow-up logic

Add only a thin intent translation layer that sits between input and the current edit path.

### Pros

- Lowest immediate churn.
- Easier to prototype quickly.
- May solve a small subset of problems around mapping input into more explicit operations.

### Cons

- Does not really solve the ownership problem identified in the mapping pass.
- Leaves block metadata, parse state, and semantic behavior split across multiple owners.
- Risks becoming another coordination layer without a true document owner.
- Keeps too much logic in post-edit hooks.
- May make the architecture look cleaner from the outside while leaving the core tension unresolved.

### Fit with source-of-truth model

Neutral.

It can preserve disk truth, but it does not provide a strong new home for the structural state that must stay consistent with that truth.

### Verdict

Useful only as a stopgap or transition strategy, not as the preferred end state.

---

## Comparison table

| Option | Boundary clarity | Preserves current invariants | Supports semantic edits cleanly | Source-of-truth fit | Recommendation |
|---|---|---:|---:|---:|---|
| **A. Document-model above `bloom-buffer`** | High | High | High | High | **Recommended** |
| **B. Push semantics into `bloom-buffer`** | Medium on paper, low in practice | Low | Medium | Medium | Reject as first move |
| **C. Thin intent layer only** | Medium | High | Low to medium | Medium to high | Possible transition, not end state |

---

## Recommendation

### Recommended direction

Choose **Option A**:

> keep `bloom-buffer` low-level, and create or clarify a stronger document-model owner above it.

This is the best fit for Bloom because it:

- aligns with the current code trajectory
- preserves the healthiest existing boundaries
- gives semantic edits a real home
- supports future input evolution
- does not compromise the file-on-disk truth model

### Recommended implementation bias

For now, prefer:

> **Option A1:** introduce the document-model owner as a distinct `bloom-core` boundary first, not a new crate yet.

Reason:

- the runtime ownership question matters more than immediate crate extraction
- the boundary is still under investigation
- extracting too early would create churn before the model is proven

If the model stabilizes and becomes large enough, it can later be promoted into its own crate.

---

## What Option A should probably own

If Option A is pursued, the document-model owner should likely own:

- the mutable text buffer
- parse-tree lifecycle
- block metadata runtime state
- synchronization between parse boundaries and block metadata
- semantic edit application
- read-only structural queries needed by higher-level editor behavior

It should likely **not** own:

- raw key interpretation
- window/picker/dialog routing
- rendering orchestration
- SQLite indexing
- direct filesystem I/O

Those should remain in their existing higher or adjacent layers.

---

## Open questions left for the risk review

This recommendation still needs a dedicated architect risk pass on:

- undo/redo semantics for semantic edits
- interaction with mirror propagation and section mirroring
- thread safety and state ownership
- exact file serialization rules for block metadata
- parser invalidation strategy
- interactions with external file changes and MCP
- whether some semantic operations must still remain editor-owned because they require index access

## Bottom line

Option A is the best fit.

The strongest current recommendation is:

> do **not** push Bloom Markdown semantics down into `bloom-buffer`; instead, establish a proper document-model owner above it and let that become the home for parser-aware structural editing.
