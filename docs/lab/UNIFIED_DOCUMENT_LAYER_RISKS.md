# Unified Document Layer — Risk Review

> Architect risk review for the unified document layer investigation.
> Status: **Draft**

---

## Purpose

This document stress-tests the recommended direction:

> keep `bloom-buffer` low-level, and introduce a stronger document-model owner above it.

The question here is not whether the idea is elegant.

The question is whether the idea can preserve Bloom's invariants around:

- source-of-truth files
- parser authority
- undo behavior
- mirror behavior
- thread safety
- future input expansion

## Overall assessment

The direction still looks viable.

But it is only viable if the document-model layer is treated as the **single semantic mutation owner** for document structure, not merely as another helper around the current edit path.

The largest risks are not performance risks. They are:

- semantic undo boundaries
- duplicated ownership between editor and document layers
- file-truth drift
- index-coupled operations leaking back into the document core

## Risk 1 — Semantic undo/redo becomes confusing

### Why it matters

Today, raw text edits and follow-up system operations can happen in separate stages:

- raw edit
- end edit group
- ensure block IDs
- mirror propagation
- section propagation
- optional alignment

The current code even notes a TODO around merging system ops into a single undo story.

If a new document layer adds semantic edits but still lets follow-up actions escape into separate post-edit hooks, undo behavior may become more confusing, not less.

### Failure mode

The user performs one conceptual action, but undo replays multiple internal steps in surprising order.

### Required mitigation

The document layer must define what counts as:

- one raw text edit
- one semantic edit
- one undo unit

The safest rule is:

- one user-visible semantic action should produce one coherent undo unit whenever practical

### Recommendation

Treat undo semantics as a first-class design input, not cleanup work.

---

## Risk 2 — The document layer becomes ambiguous with editor ownership

### Why it matters

The current editor still owns important semantic follow-up behavior:

- `ensure_block_ids`
- mirror propagation
- section structure propagation
- some index-coupled actions

If the new document layer is introduced without moving clear responsibilities into it, Bloom will end up with:

- an input layer
- an intent layer
- a document layer
- editor post-edit hooks

all partially responsible for structure.

### Failure mode

The redesign adds abstraction without removing ambiguity.

### Required mitigation

Each semantic behavior must be classified as one of:

1. document-owned
2. editor-owned
3. index-owned

with explicit reasons.

### Recommendation

Do not ship a document-model layer unless it removes meaningful ownership confusion from the current system.

---

## Risk 3 — File-on-disk truth gets weakened

### Why it matters

Bloom's architecture is explicit:

- files are the source of truth
- block markers live in file content
- SQLite is rebuildable

If the new document layer becomes the "real truth" and the file becomes merely a lossy projection, the design violates a core Bloom promise.

### Failure mode

- essential metadata exists only in memory
- files become insufficient to reconstruct structure
- index rebuilds stop being authoritative

### Required mitigation

Any structural metadata that matters for long-term identity must remain serializable into file content.

For this idea, that means:

- block identity cannot become purely in-memory metadata
- parser-aware document structure must still serialize back to the Markdown representation Bloom considers canonical

### Recommendation

Use this as a hard gate:

> if the document-layer design cannot round-trip through file content without loss of meaning, reject it

---

## Risk 4 — Parser invalidation becomes too eager or too magical

### Why it matters

A parser-aware document layer sounds clean, but if every semantic edit requires too much full-document recomputation, the layer may become fragile or slow.

Bloom already has incremental dirty/refresh mechanics in the parse tree.

The new layer must avoid replacing that with "just rebuild everything" as soon as edits become semantic.

### Failure mode

- simple edits trigger disproportionate reparsing
- parser refresh logic duplicates across layers
- semantic operations rely on stale parse state

### Required mitigation

The document layer must define one consistent rule for:

- when parse state is considered authoritative
- when it is stale
- when semantic edits may consult it
- when it must be refreshed first

### Recommendation

Prefer:

- parser as structural authority
- incremental invalidation as the normal path
- targeted rebuilds only when truly necessary

---

## Risk 5 — Mirror behavior and section structure become unstable

### Why it matters

Bloom already has real structural behaviors built on top of edits:

- mirror propagation
- section structure propagation
- block identity repair

These are precisely the areas most likely to benefit from a document layer — and also the areas most likely to break if the design is underspecified.

### Failure mode

- semantic block splits produce wrong mirror targets
- block identity is preserved in the wrong place
- section child lists drift from true block structure
- mirrored edits and semantic edits trigger each other in loops

### Required mitigation

The new layer must define:

- how structural operations resolve target blocks
- when mirror propagation is triggered
- what data is stable enough to propagate
- how semantic edits interact with block identity reassignment or preservation

### Recommendation

Treat mirroring and section structure as primary design constraints, not edge integrations.

If the new model cannot express them cleanly, the model is incomplete.

---

## Risk 6 — Some "document" operations actually require index/editor context

### Why it matters

Some semantic operations are local to a document.

Others are not.

For example:

- block split is mostly local
- toggle task by block ID may require index lookup
- mirror propagation may require cross-page lookup
- external file reloads involve editor/store coordination

If the document layer tries to own all of these directly, it may become polluted with index and storage concerns.

### Failure mode

The document layer becomes a hidden editor-core replacement.

### Required mitigation

Split operations into:

- **local document semantics**
- **cross-document / index-assisted semantics**

The document layer should own the former and offer a clean interface that higher layers can orchestrate for the latter.

### Recommendation

Do not force all semantic operations into one owner if some fundamentally require cross-document context.

Keep the document layer strong, but local-first.

---

## Risk 7 — External writable editors complicate the model

### Why it matters

The stronger the document semantics become, the more external freeform editing can bypass them.

That tension already exists today, but it becomes sharper if Bloom's semantics rely on coordinated parser-aware document operations.

### Failure mode

- external edits bypass semantic invariants
- Bloom has to repair too much after the fact
- complexity rises just to preserve writable interoperability

### Required mitigation

Be explicit about the product stance:

- if preserving writable external-editor workflows weakens the design substantially, Bloom may prefer effectively read-only interoperability with external tools

### Recommendation

Do not let external writable interoperability drive the core document model if it conflicts with file-truth and semantic consistency.

---

## Risk 8 — Mouse input could accidentally create a second semantic path

### Why it matters

One reason to like the proposed layering is future mouse support.

But that only works if mouse actions and Vim actions converge on the same downstream semantic pipeline.

### Failure mode

- Vim goes through semantic document edits
- mouse goes through ad hoc editor operations
- behavior diverges by input method

### Required mitigation

The future document API must be input-agnostic.

Vim and mouse should both end up producing:

- raw edit requests where appropriate
- semantic intents where appropriate

against the same document owner.

### Recommendation

Use input-method parity as a design test for the API shape.

---

## Thread-safety and ownership assessment

### Current situation

Bloom already prefers:

- a single semantic writer on the UI thread
- background threads for disk/index/watcher/MCP I/O
- channels instead of shared mutable state

### Implication

The proposed document layer fits that model well **if** it stays under the same single-writer discipline as `BufferWriter`.

### Main risk

If read-only parser access becomes broadly exposed while mutations are happening, stale reads or unclear lifetimes may leak into higher layers.

### Recommendation

Keep the rule simple:

- one mutable document owner
- read-only structural snapshots or queries outside it

Do not introduce multiple semantic writers.

---

## Recommendation after risk review

Proceed with the direction, but only under these constraints:

1. the document layer becomes the primary local semantic mutation owner
2. file-on-disk truth remains absolute
3. block metadata remains serializable into files
4. undo semantics are designed explicitly
5. local document semantics are separated from cross-document/index-assisted orchestration
6. input methods converge onto one semantic pipeline

## Conditions that would invalidate the direction

Reject or significantly narrow the idea if:

- it cannot preserve file-truth round-tripping
- it cannot provide a cleaner undo story
- it leaves semantic ownership as scattered as today
- it forces too much index/storage logic into the document owner
- it requires broad compromises to preserve writable external-editor interoperability

## Bottom line

The proposed document-model direction survives the risk review.

But it should advance only as:

- a **disciplined local semantic owner**
- above `bloom-buffer`
- under the existing single-writer architecture
- with explicit safeguards for file truth, undo grouping, and cross-document boundaries

