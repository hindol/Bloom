# Unified Document Layer — Current State Map

> Architect mapping pass for the unified document layer investigation.
> Status: **Draft**

---

## Purpose

This document maps the current layering in code and docs before evaluating boundary options.

It does **not** choose the final design yet. It establishes:

- where ownership lives today
- which responsibilities are already bundled
- which responsibilities are still split awkwardly
- which seams matter for the next architect review

See also:

- `UNIFIED_DOCUMENT_LAYER.md`
- the superseded block-ID-metadata branch
- `../PARSE_TREE.md`
- `../UNIFIED_BUFFER.md`
- `../BLOCK_IDENTITY.md`

## Short conclusion

Bloom already partially implements the shape of a unified document owner, but not completely.

Today:

- `bloom-buffer` is still a narrow rope/cursor/undo substrate
- `bloom-core` already bundles parse state with buffers
- input routing and semantic follow-up behavior live mostly in editor code
- block metadata and structural semantics are not owned in one coherent place

This means the current system is **closer to "document layer above buffer"** than to "put everything inside `bloom-buffer`."

That should be the starting assumption for the next architect step.

---

## Current layering in practice

### 1. Low-level buffer layer: `bloom-buffer`

`bloom-buffer` owns the raw text substrate and editing mechanics:

- `Rope`
- cursor tracking
- undo/redo
- dirty/version state
- raw insert/delete/replace

Key property:

- the buffer owns cursors and adjusts them automatically on mutation

Important observation:

- this layer is intentionally **Markdown-agnostic**
- it does not own parse state
- it does not own block metadata as a first-class runtime structure

It does expose `EditOp`, which is currently the raw edit descriptor used by Vim.

### 2. Input interpretation layer: `bloom-vim`

`bloom-vim` is already a clean interpreter layer.

It:

- processes key input
- reads the buffer in a read-only way
- produces `VimAction`
- returns `EditOp` descriptors for content changes
- never mutates buffers directly

This part already matches the desired architecture well:

- Vim is an input interpreter, not the document owner

### 3. Input routing / orchestration layer: `bloom-core::editor`

`BloomEditor::handle_key()` already implements a broad input-routing pipeline:

- wizard
- dialog
- picker
- date picker
- quick capture
- leader sequences
- inline completion
- Vim
- command/search handling

After Vim returns an action, `translate_vim_action()` maps it into editor behavior.

For content edits, it currently does the simplest possible thing:

- `VimAction::Edit(EditOp)` becomes `BufferMessage::Edit`
- the raw edit is applied immediately
- follow-up structural/system work happens later

This is the clearest sign that Bloom does **not** yet have a first-class intent layer.

### 4. Partial document-state layer: `BufferWriter` + `ManagedBuffer`

`bloom-core` already bundles more than just raw text.

Each open buffer is represented as:

```text
ManagedBuffer {
  slot: BufferSlot,
  info: BufferInfo,
  parse_tree: ParseTree,
}
```

This is important:

- text state and parse state already share one lifecycle
- they open together
- they reload together
- they close together
- parse trees are marked dirty on edit and refreshed later

So Bloom already has the beginnings of a document model — just not a fully unified one.

### 5. Truth / persistence layer

Bloom's existing docs strongly constrain the design:

- files on disk are the source of truth
- block IDs and mirror markers live in file content
- SQLite is rebuildable
- the index is derived, not authoritative

This means any future unified document layer must still serialize structural metadata into disk files.

That requirement rules out any design that keeps essential block structure only in memory or only in SQLite.

---

## What is already unified

### Buffer + parse-tree lifecycle

This is already unified in `ManagedBuffer`.

The code today already assumes:

- parse state belongs with the buffer, not as an unrelated cache elsewhere
- edits mark parse state dirty
- render refreshes dirty parse trees lazily

This is a strong signal that Bloom is already moving toward a document-level owner above the raw rope.

### Input interpretation vs mutation

This is also partly unified in a good way:

- `bloom-vim` interprets
- `bloom-core` mutates

That separation is healthy and should likely be preserved for mouse input later.

---

## What is still split awkwardly

### 1. Structural semantics are mostly post-edit side effects

The current raw path is:

```text
key input
  -> VimAction::Edit(EditOp)
  -> BufferMessage::Edit
  -> raw rope mutation
  -> later structural/system follow-up
```

That means structural meaning is often applied after the raw text edit, not as part of a first-class semantic edit pipeline.

Examples:

- `ensure_block_ids()` runs on mode transitions
- mirror propagation happens after edits
- section structure propagation happens after edits
- alignment runs after insert-mode exit

This is workable, but it spreads document semantics across timing hooks instead of making them explicit.

### 2. Block metadata is not bundled where parse state already is

The superseded block-ID-metadata branch imagines block metadata living alongside edits, but the current code path does not show a single runtime owner that holds:

- rope text
- parse state
- block boundaries
- block metadata

all together.

That is the central mismatch your idea is reacting to.

### 3. Semantic operations are distributed across different owners

`BufferMessage` already contains higher-level operations such as:

- `MirrorEdit`
- `ToggleTask`
- `AlignPage`
- `AlignBlock`
- `EnsureBlockIds`

But the ownership is inconsistent:

- `Edit` and `MirrorEdit` are applied in `BufferWriter`
- `AlignPage` / `AlignBlock` are applied in `BufferWriter`
- `ToggleTask` is declared in `BufferMessage` but handled at editor level
- `EnsureBlockIds` is declared in `BufferMessage` but also handled at editor level

This tells us the message vocabulary is already trying to become more semantic than the current owner can comfortably handle.

### 4. `EditOp` lives in the low-level buffer crate

`bloom-vim` re-exports `bloom_buffer::EditOp`.

That means the current raw edit shape originates in the lowest text layer, not in a document or intent layer.

This is not necessarily wrong, but it is a sign that the current abstraction stack is still centered on text replacement rather than document semantics.

### 5. Parse-tree ownership is ahead of the rest of the architecture

The code already gives parse state a durable lifecycle in `bloom-core`, but many structural behaviors still rely on editor-level orchestration and post-edit hooks.

So the architecture is asymmetrical:

- parse state already moved up into a document-like owner
- block/semantic behavior has not fully followed it

---

## Current responsibility map

### `bloom-buffer`

Owns:

- rope text
- cursor invariants
- undo/redo
- raw text mutation

Does not own:

- Markdown structure
- parse-tree lifecycle
- block identity as a runtime document model

### `bloom-vim`

Owns:

- key interpretation
- modal grammar
- motions/operators/text objects
- raw edit intent at the level of `EditOp`

Does not own:

- actual mutation
- parser-backed structural semantics

### `bloom-core::BufferWriter`

Owns:

- buffer lifecycle
- parse-tree lifecycle
- centralized mutation entrypoint
- some higher-level buffer actions

Partially owns:

- document-like state, but not the full semantic model

### `bloom-core::editor`

Owns:

- routing input through modes and overlays
- translating `VimAction` into mutation messages
- some semantic follow-up behavior after edits
- operations that still need index/editor context

This layer currently absorbs work that a future document/intent layer might own instead.

---

## Architecturally important inconsistencies

### Inconsistency A: one lifecycle, multiple semantic owners

Buffers and parse trees are bundled together, but semantic block behavior is still scattered.

### Inconsistency B: message vocabulary is more advanced than ownership

The system already names semantic messages, but not all of them can be handled by the central mutation owner.

### Inconsistency C: source-of-truth requirement is stronger than the current metadata note

The superseded block-ID-metadata branch emphasizes metadata tracking logic, but Bloom's larger architecture emphasizes file truth and rebuildability.

That means the final design must not merely compute correct metadata in memory. It must ensure the right metadata is recoverable from file content.

### Inconsistency D: input layering is cleaner than document layering

The input stack is already fairly legible:

- interpret input
- produce an action
- apply mutation

The document stack is less clean:

- mutate text now
- reconcile structure in a few different places
- rebuild or propagate as needed

That is likely the heart of the redesign opportunity.

---

## Implications for the next architect step

The next step should **not** ask "should everything move into one crate?"

It should ask:

1. what is the correct runtime owner of:
   - text
   - parse state
   - block metadata
   - semantic edit application
2. which responsibilities must stay below that line as low-level text machinery?
3. which responsibilities must stay above that line as editor/index/UI concerns?

Current bias from this mapping:

- keep `bloom-buffer` low-level
- preserve `bloom-vim` as an interpreter
- evaluate a stronger document-model owner above the raw buffer

That owner would absorb the responsibilities that are currently split between:

- `BufferWriter`
- editor post-edit hooks
- block-metadata reconciliation logic

---

## Current implementation checkpoint after the first slice

Bloom now has a real `document.rs` owner in `bloom-core`, but it is still only
the **first slice** of the intended shape.

Today `Document` / `DocumentMut` already own:

- local edit application
- parse-tree dirtying / rebuild
- undo / redo / restore through the active cursor index
- reload and align coordination
- the current save-time `ensure_block_ids()` bridge

That means the next step should **evolve this owner**, not replace it with a new
parallel abstraction.

The main missing piece is that block identity is still represented as visible
text in the rope and consumed by callers through `parse_block_id(...)` or
`ParseTree` block-ID queries. That is exactly the seam the next slice should
close.

## Recommended concrete runtime shape for the next slice

Keep the current layering:

- `bloom-buffer` remains the low-level rope / cursor / undo substrate
- `Document` / `DocumentMut` remain the runtime owner exposed by `BufferManager`
- `ParseTree` remains the authority on structure for the **clean editing text**

Refine the in-memory shape like this:

```text
ManagedBuffer {
  slot: BufferSlot,        // clean editing text only
  info: BufferInfo,
  document: DocumentState,
}

DocumentState {
  parse_tree: ParseTree,           // built from clean text
  block_ids: Vec<BlockIdEntry>,    // block identity overlay
}
```

Recommended details:

- `slot` continues to hold the only mutable rope and the undo tree
- `parse_tree` is built from the **clean** rope, not canonical disk text
- `block_ids` becomes the authoritative open-document identity overlay
- no persistent char/byte projection map is needed initially; line-based spans
  are enough for the current block-ID and mirror workflows

If a future feature truly needs stable intra-line canonical/display mapping,
that should be added as a narrower serialization helper rather than as a
per-edit projection structure carried everywhere.

### Recommended concrete types

```rust
pub struct BlockIdEntry {
    pub id: BlockId,
    pub first_line: usize,
    pub last_line: usize,
    pub is_mirror: bool,
}

pub struct DocumentSection {
    pub level: u8,
    pub title: String,
    pub line_range: Range<usize>,
    pub block_id: Option<BlockId>,
    pub is_mirror: bool,
}
```

Notes:

- keep `is_mirror: bool` initially for lower churn with existing parser/index
  concepts
- `DocumentSection` should be a **document-layer overlay** built from
  `ParseTree::sections()` plus `BlockIdEntry` lookup on the heading line
- do **not** make `ParseTree` itself the long-term owner of block identity for
  open buffers once the rope is clean

### Recommended `Document` API shape

The existing `Document` / `DocumentMut` wrappers are the right place to grow the
API. The next slice should add queries like:

```rust
impl Document<'_> {
    pub fn buffer(&self) -> &Buffer;
    pub fn parse_tree(&self) -> &ParseTree; // clean-text parse

    pub fn block_ids(&self) -> &[BlockIdEntry];
    pub fn block_id_at_line(&self, line: usize) -> Option<&BlockIdEntry>;
    pub fn block_id(&self, id: &BlockId) -> Option<&BlockIdEntry>;

    pub fn sections(&self) -> Vec<DocumentSection>;
    pub fn mirror_sections(&self) -> Vec<DocumentSection>;
    pub fn section_by_block_id(&self, id: &BlockId) -> Option<DocumentSection>;

    pub fn canonical_text(&self) -> String;
}

impl DocumentMut<'_> {
    pub fn apply_edit(&mut self, request: EditRequest<'_>) -> bool;
    pub fn reload_from_disk(&mut self, canonical_text: &str);
    pub fn assign_missing_block_ids(&mut self, known_ids: Option<&mut HashSet<String>>) -> bool;
}
```

Internal helpers should own the projection boundary:

```rust
fn deserialize_canonical(text: &str, parser: &impl DocumentParser)
    -> (String, Vec<BlockIdEntry>);

fn serialize_canonical(clean_text: &str, block_ids: &[BlockIdEntry]) -> String;

fn refresh_structure_after_edit(&mut self);
```

Important recommendation:

- `canonical_text()` is the save/reload boundary
- `assign_missing_block_ids()` replaces the *concept* of visible-rope
  `ensure_block_ids()`
- callers should query `Document`, not re-parse suffixes from `buf.line(...)`

## Recommended migration boundary

The key migration is not "move everything at once." It is:

1. keep the rope clean in open documents
2. move block-ID-aware reads to `Document`
3. keep file truth at the serialize / deserialize boundary

### Phase 1: open / reload / save through the document projection

- `BufferManager::open(...)` and reload paths should deserialize canonical disk
  text into:
  - clean rope text in `BufferSlot`
  - `DocumentState.block_ids`
  - `DocumentState.parse_tree` built from clean text
- `save_page()` should call `Document::canonical_text()` and write that result
- once this lands, `ensure_block_ids()` should stop meaning "insert visible
  suffixes into the rope"

### Phase 2: replace direct block-ID parsing in editor call sites

These callers should stop reading inline suffixes from buffer text and instead
query `Document`:

- `editor/commands.rs`
  - `mirror_sever`
  - `mirror_goto`
- `editor/render.rs`
  - mirror context-strip lookup
- `editor/page_history.rs`
  - current-block history lookup
- `editor/keys.rs`
  - mirror propagation
  - task toggle mirror propagation

The concrete replacement is mostly:

```rust
let bid = doc.block_id_at_line(cursor_line);
```

instead of:

```rust
parse_block_id(&buf.line(cursor_line).to_string(), cursor_line)
```

### Phase 3: move block-ID-aware section queries out of `ParseTree`

Open-buffer section and mirror logic should no longer depend on
`ParseTree::block_ids()`, `ParseTree::mirror_sections()`, or
`ParseTree::section_by_block_id()` once the rope is clean.

Those queries should migrate to the document layer because they combine:

- clean-text structure from `ParseTree`
- block identity from `BlockIdEntry`

This especially affects:

- `editor/section_mirror.rs`
- any open-buffer mirror-heading lookup
- tests that currently assert via `parse_tree.block_ids()`

`ParseTree` can still retain canonical-text block-ID parsing helpers for
disk-oriented or parser-unit-test scenarios, but it should not remain the
primary runtime API for open-document identity queries.

### Phase 4: alignment ignores block IDs

`align/engine.rs` currently splits visible ` ^id` suffixes directly from rope
lines. That will not hold once the open rope is clean.

Recommendation:

- alignment should operate on clean text in memory
- block-ID suffix placement should happen only during canonical serialization
- block IDs should **not** be treated as alignment participants
- auto-alignment should stay focused on user-facing fields such as
  `@due(...)`, `@start(...)`, and similar visible content
- do not add document/runtime complexity just to preserve pretty ID columns in
  canonical file text

## Most important concrete recommendation

Do **not** make the next slice "block IDs in `ParseTree` but not in the rope."

That would keep the same coupling in a different place.

The cleaner contract is:

- `ParseTree` owns structure from clean text
- `Document` owns identity overlay and canonical serialization
- editor consumers ask `Document` for identity-aware queries

That gives Bloom one coherent runtime owner for open-document structure without
weakening the file-on-disk source-of-truth model.

---

## Questions to resolve next

1. Should the document owner live inside `bloom-core` or as a new crate?
2. What should the semantic edit API look like beyond raw `EditOp`?
3. Which existing post-edit hooks become first-class document operations?
4. Which operations still need index/editor context and therefore remain outside?
5. How should file-serialized block metadata be modeled so disk truth stays intact?

## Recommended next document

The next architect artifact should evaluate boundary options:

- **Option A:** keep `bloom-buffer` low-level and add a document-model layer above it
- **Option B:** move parser/block semantics into `bloom-buffer`
- **Option C:** keep the current split and add only a thin intent layer

That comparison should include risks, rejected alternatives, and a recommendation.
