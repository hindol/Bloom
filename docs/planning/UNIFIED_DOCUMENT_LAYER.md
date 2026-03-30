# Unified Document Layer

> Idea brief for PM + architect investigation.
> Status: **Draft**

---

## Current outcome of this investigation

Current recommendation:

- keep this work in `docs/planning/` for now
- treat the older block-ID-metadata branch as superseded as the primary direction
- do not promote this topic into `docs/` until there is a tighter implementation-facing spec for ownership, undo, parser invalidation, and cross-document boundaries

Why:

- the direction looks promising after PM framing, architecture mapping, option evaluation, and risk review
- but it is still an architectural investigation, not yet a settled implementation contract
- promoting too early would make the docs look more final than the current design really is

---

## CEO intake

### Idea / hypothesis

Bloom may need a stronger **document layer** that owns:

- rope-backed text storage
- parser / parse-tree lifecycle
- block-level metadata such as boundaries and UUIDs
- semantic edit application via `apply_edit`

In this model:

- raw input belongs to an input layer
- Vim is one interpreter inside the input layer
- future mouse input should be able to use the same downstream pipeline
- low-level edit requests flow into an intent-aware middle layer
- the document layer applies edits with parser-backed structural awareness

Example:

- inserting a blank line inside a block is not just "text inserted"
- it may express the semantic intent "split this block in two"
- the document layer needs parser help to apply that correctly

### Why it may matter

The current direction in the older block-ID-metadata branch feels too metadata-centric.

It appears to separate:

- the rope
- parse-derived structure
- and block metadata

more than Bloom's current architecture wants.

That may create long-term friction in areas like:

- block split / join behavior
- keeping block IDs aligned with true parser boundaries
- structural edits that should not be treated as blind text replacement
- consistency between editing semantics and parser semantics
- future non-keyboard input

### Target user or workflow

Primary target:

- Bloom development itself
- future users who rely on reliable structural editing

Likely impacted workflows:

- editing paragraphs, tasks, headings, and list items
- splitting and joining blocks
- block ID assignment and preservation
- mirrored / structurally linked content
- any future input path beyond keyboard/Vim

### Constraints or non-negotiables

- Keep Bloom's architectural clarity around ownership and invariants.
- Do not regress buffer-owned cursor behavior lightly.
- Do not blur thread-safety or state-ownership boundaries.
- Keep the parser authoritative for structure.
- Block ID metadata must still be serialized into files written to disk.
- Files on disk remain the absolute source of truth.
- The SQLite index must remain rebuildable from file content.
- Preserve a clean path for undo/redo and persistence.
- Do not assume "one ownership layer" automatically means "one crate."
- If writable external-editor interoperability creates disproportionate complexity, prefer exposing on-disk files as effectively read-only to external tools rather than weakening Bloom's source-of-truth model.

### Questions for the PM

1. What concrete user-visible pain is this trying to solve?
2. Which user journeys would become meaningfully better if Bloom had a parser-aware document layer?
3. Which behaviors should feel more reliable or predictable to the user?
4. Is the value here large enough to justify architecture churn?
5. Which user-facing docs or mental models would need to change if this architecture becomes real?

### Questions for the architect

1. Should the unified owner live above `bloom-buffer`, inside `bloom-core`, or in a new document-model crate?
2. Should `bloom-buffer` remain a low-level rope/cursor/undo substrate?
3. How should `apply_edit` interact with semantic intents, undo groups, and parser invalidation?
4. Which edits stay raw text edits, and which become semantic operations?
5. How should block metadata, parse state, and document text stay synchronized?
6. How does this interact with mirroring, block IDs, section structure, MCP edits, and external file reloads?
7. What risks does this introduce around thread safety, state ownership, and future input sources?

---

## PM discovery

### Problem framing

This is both an implementation-facing and user-facing problem.

At the implementation level, the current direction risks treating structure as something reconstructed around text edits after the fact. That makes Bloom's most valuable behaviors feel bolted on instead of native:

- block IDs
- block splits and joins
- mirrored or structurally linked content
- parser-backed editing semantics

At the user level, the failure mode is not "the architecture is ugly." The failure mode is that Bloom behaves like a plain text editor in moments when users expect a structure-aware knowledge tool.

The clearest motivating examples are:

- inserting a blank line inside a block and expecting a clean split with stable identities
- editing tasks, headings, lists, and paragraphs without structural metadata drifting
- saving, reopening, or rebuilding the index and expecting exactly the same truth to come back from disk
- eventually supporting non-keyboard input without inventing a second editing model

The strongest product constraint is that the vault files on disk are the source of truth. If Bloom cannot serialize the necessary structural metadata into the files themselves, the design is suspect.

### UX / behavior notes

The desired UX is:

- structural edits feel intentional, not accidental
- splitting or joining a block produces predictable structure and predictable identity behavior
- parser-derived truth and editing behavior do not drift apart
- save/reopen/index rebuild preserves the same structural meaning because the important metadata is in the files
- the same document semantics remain stable across input sources, even if Vim remains the first and best-supported one

Specific expectations:

- when a block splits, the user should not feel like Bloom "guessed" after the fact
- when block IDs matter, they should survive persistence and index rebuilds
- when future mouse input arrives, it should not create a second-class editing model with different structural outcomes

External-editor note:

- if full writable interoperability with outside editors materially weakens the model or adds significant conflict complexity, Bloom can reasonably prefer read-only exposure of vault files to those tools rather than compromising the source-of-truth principle

### Recommendation

**Pursue, but as an architecture investigation first — not as an implementation commitment yet.**

Reason:

This idea appears product-relevant, not merely aesthetically appealing.

It could improve:

- reliability of structural edits
- coherence between editing and parsing
- long-term extensibility for new input methods
- trust in Bloom's "files are the source of truth" promise

However, the scope is large enough that it should only advance after architect review of layering, risks, and alternatives.

### Risks / open questions

Initial open questions:

- Is the real need a new document layer, or just better coordination between existing layers?
- Is the older block-ID-metadata branch wrong in principle, or just too narrow?
- Should this be framed as a block-ID redesign, or as a larger document-model redesign?
- How much writable external-editor support is truly worth preserving if it complicates the source-of-truth model?
- Can file-serialized metadata stay understandable enough for humans while still supporting strong structural semantics?
- Does the value come mainly from better block-ID handling, or from making semantic edits first-class across the whole editor?

### Suggested next artifact

If this idea moves forward, the next artifact should likely be:

- an architect-owned technical investigation comparing boundary options
- a risk review focused on thread safety, state ownership, parser invalidation, and undo semantics
- explicit evaluation of how the design preserves file-on-disk truth and rebuildable indexing

---

## Initial architectural context

This idea should be evaluated against:

- the older block-ID-metadata branch
- `docs/BLOCK_IDENTITY.md`
- `docs/PARSE_TREE.md`
- `docs/UNIFIED_BUFFER.md`
- `crates/bloom-buffer/src/rope.rs`
- `crates/bloom-core/src/parse_tree.rs`

Current observation:

- the repo already has a stronger "document owner" direction than the older block-ID-metadata branch alone suggests
- the main unresolved question is boundary placement, not whether parser-backed structure matters
