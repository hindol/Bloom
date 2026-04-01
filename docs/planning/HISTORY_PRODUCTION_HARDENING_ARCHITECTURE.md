# History Production Hardening — Architecture Spec

> Architect technical spec for making Bloom history trustworthy, legible, and operationally sound.
> Status: **Draft**

---

## Purpose

This document turns the PM discovery in `HISTORY_PRODUCTION_HARDENING.md` into an implementation-ready technical shape.

It defines:

- where recent undo ends and durable history begins
- how fine-grained undo becomes coarse git-backed history
- who owns scheduling, persistence, and UI-visible history state
- how mirrored edits participate in history without creating noisy durable stop points
- what failure and restart behavior Bloom should guarantee before calling history production-hardened

See also:

- `HISTORY_PRODUCTION_HARDENING.md`
- `../HISTORY.md`
- `../ARCHITECTURE.md`
- `../TEMPORAL_NAVIGATION.md`
- `../UNIFIED_BUFFER.md`

---

## Short conclusion

Bloom should keep its two-layer history model.

- **Recent history** remains the branching undo tree, persisted via SQLite on a bounded recent horizon.
- **Durable history** remains git-backed and intentionally coarser.

The key architectural rule is this:

> Durable history is **not** built by promoting individual undo nodes.

Instead, durable history points are created from **coalesced, disk-stable checkpoints** after successful writes, explicit checkpoints, or shutdown flushes.

That design preserves Bloom's current strengths:

- the buffer still owns live text and cursors
- restore still becomes a normal edit
- git history stays sparse and readable
- mirrored edits remain correct without exploding page history into per-propagation noise

The current code already has most of the pieces. The main missing piece is a clear, explicit protocol between the editor and the history worker for "a durable checkpoint is due now." The current `CommitDone { oid: None }` signal is too ambiguous and appears only partially wired.

---

## Current state in code

Today the system already has a meaningful split:

### 1. Recent history

- `bloom-buffer` owns the undo tree.
- undo trees are persisted via SQLite/indexer-owned writes
- undo trees are pruned on a roughly 24-hour horizon on startup

This already supports the "recent, branching, survives restart" model Bloom wants for G9.

### 2. Durable history

- `bloom-core::editor::files` sends `HistoryRequest::FileDirty` on successful disk writes
- `bloom-core::history::thread` tracks idle and max-interval timers
- explicit `CommitNow { files, message }` already exists
- shutdown currently sends `CommitNow` and then `Shutdown`
- `bloom-history::HistoryRepo::commit_all()` can commit only the changed file subset; unspecified files are preserved from the parent tree

### 3. Surface integration

- page history and block history already merge undo items with git-backed items in the temporal strip
- restore already flows back into the current buffer as a normal undoable edit

### 4. Current gap

The current auto-commit path is conceptually clear but operationally muddy:

- the history worker marks the vault dirty on `FileDirty`
- when idle/max-interval elapses, it emits `HistoryComplete::CommitDone { oid: None }`
- comments say the editor should then respond with a real `CommitNow`

But the current editor-side handling only logs `CommitDone { oid: None }`; no follow-up `CommitNow` path was found outside session save / shutdown.

Architecturally, this means the protocol boundary is not explicit enough. That should be fixed before calling the feature hardened.

---

## Goals

### G1 — Preserve the two-layer model

Bloom should continue to have:

- dense, branching, recent undo
- sparse, durable, longer-lived git-backed history

Those layers solve different problems and should not be collapsed into one storage model.

### G2 — Keep durable history readable

Git-backed history should remain coarse enough that page history stays navigable over long periods.

The user should not be forced to scroll through a commit for every small edit group or mirrored propagation.

### G3 — Preserve ownership boundaries

- buffer owns text, cursors, undo
- disk writer owns file writes
- indexer owns SQLite writes
- history worker owns the git repo
- editor owns orchestration and user-visible history status

### G4 — Keep restore semantics unchanged

Restore must remain a normal edit in the current buffer. It should create forward history, not destructively rewind time.

### G5 — Make durability legible

The architecture should support a user-visible answer to:

> "Is my last good version durably captured yet?"

This requires explicit internal state, not just background best effort.

### G6 — Support meaningful per-stop explanation

The architecture should support history surfaces that explain **why** a stop matters, not just when it happened.

That means durable-history metadata should not be modeled as only a generic commit message plus timestamp. The system should be able to surface structured stop information such as:

- stop kind
- scope
- concise change summary
- explicit checkpoint label when present

### G7 — Expose one unified page-history surface

Bloom should not expose separate primary UIs for page undo history and page durable history.

At the command level, `SPC H h` and `SPC u u` should open the same underlying page-history surface. If they differ at all, they should differ only in initial focus or selection, not in the data model or view implementation.

---

## Non-goals

- replacing the undo tree with git commits
- making git history per-keystroke or per-edit-group
- mutating buffer content in the save path
- turning Bloom into cloud sync or archival backup infrastructure
- exposing raw git concepts as the primary user model

---

## Core architectural decisions

## 1. Durable history checkpoints are derived from stable saved state, not undo nodes

Fine-grained undo is for local editing flow. Durable git history is for coarser recovery points.

Therefore:

- undo nodes are **never** individually promoted to git commits
- successful writes create the pool of candidate changes for durable capture
- the history system coalesces those changes into a later durable checkpoint

This is the answer to "how does granular undo get promoted?"

It does **not** get promoted node-by-node.

It gets **collapsed through save/checkpoint boundaries** into a coarser durable snapshot.

## 2. The editor owns the changed set; the history worker owns timing and commits

The current `HistoryRequest::FileDirty` is too lossy for a hardened design.

The worker needs only enough information to decide **when** a checkpoint should happen. The editor needs enough information to decide **what** belongs in that checkpoint and how to represent its state in the UI.

So the ownership split should be:

- **Editor owns**
  - pending changed page IDs for the next durable checkpoint
  - user-visible history status
  - collection of current disk-stable content for a checkpoint
  - explicit checkpoint commands

- **History worker owns**
  - idle and max-interval timers
  - git repo access
  - commit execution
  - page-history and blob lookup

## 3. Durable checkpoints commit only the coalesced changed set

`HistoryRepo::commit_all()` already preserves unspecified files from the parent tree.

That means Bloom does **not** need to scan and re-send the whole vault for routine durable commits.

Instead, a durable checkpoint should include only:

- pages changed since the last successful durable checkpoint
- pages touched by the same logical editing epoch, including mirrored targets

This reduces work and keeps the protocol honest.

## 3a. One surface, two layers

The UI should continue to render undo and git-backed history as two layers inside one page-history model, not as separate feature stacks.

Implications:

- one page-history state object should be sufficient
- one rail / inspector surface should be sufficient
- command entrypoints may choose initial selection, but should not fork the implementation into separate "undo tree view" and "page history view" modes unless that becomes unavoidable

## 4. Mirror propagation is one durability epoch, not many

Mirror propagation may update several files, including files opened silently in the background.

Those writes must be treated as one conceptual editing moment for durable history.

So:

- mirror targets may receive their own undo nodes in their local recent history
- mirror-related writes join the same pending durable changed set
- the later git-backed checkpoint contains all affected pages in one commit
- mirror propagation must **not** create a separate durable stop point per target file

## 5. Block history should be lineage-aware over linear commits

Git-backed history can remain linear while block history becomes semantically richer.

The key rule:

> Block history is not just a rev-walk over exact block-ID matches. It is a lineage query over block identity events projected onto linear checkpoints.

### Split semantics

When one block splits into two:

- one resulting block keeps the original ID
- the other resulting block receives a new ID
- the system records a lineage edge from parent → child with event kind `Split`

This makes the original block history readable:

- the original block continues along the survivor ID
- block history can show a synthetic stop like "split; spawned ^newid"
- the new block's history can begin with "split from ^oldid"

### Merge semantics

When two blocks merge into one:

- one resulting block keeps a survivor ID
- the other merged-away ID is retired
- the system records a lineage edge from retired/source → survivor with event kind `MergedInto`

This makes the merged histories readable:

- the survivor block can show "merged from ^other"
- the retired block can end with "merged into ^survivor"

### Why this still fits linear git history

The git layer remains a sequence of page snapshots.

Split/merge meaning is produced by comparing adjacent snapshots plus lineage metadata. No git branching is required.

### Important consequence

`SPC H b` will eventually need more than "find the same block ID across commits."

It will need:

- exact-ID history
- lineage-edge traversal for split/merge events
- synthetic history stops derived from those events

### Optional live-observability surface

An optional block-tracking gutter is compatible with this model if it is treated as editor chrome, not document content.

Constraints:

- default off
- read-only
- rendered in a separate gutter lane, left of line numbers
- no effect on cursor semantics, motions, selections, or copy behavior
- styled as secondary/faded metadata in the steady state
- should prefer marker semantics over raw visible ID strings

Its job would be to expose **current live identity** for advanced users, not to replace lineage history.

In practice:

- after split, the gutter can briefly distinguish the preserved block from the new child block
- after merge, the gutter can briefly distinguish the survivor from the retired block

Implementation note:

- the render frame may still carry hidden block identity keys for frontend bookkeeping
- the normal user-facing gutter should render only tracked-state markers
- transient marker flashes can be derived at the GUI boundary from frame-to-frame visible identity changes, so the effect stays in frontend chrome instead of leaking new animation state into document ownership

But merge/split explanation over time still belongs to block-history lineage events, not to the gutter itself.

## 6. Durable history points must be based on disk-stable content

Git-backed checkpoints should represent content that Bloom has actually written successfully, not speculative in-memory state.

That means the normal auto-checkpoint path should read from disk-stable content after successful writes, not from dirty buffers directly.

Shutdown and explicit checkpoint flows may flush dirty buffers first, but the durable commit should still be based on the resulting stable content set.

---

## Proposed internal model

## Editor-owned state

The editor should maintain explicit history-capture state alongside normal dirty-buffer state.

Suggested shape:

```text
pending_history_pages: HashSet<PageId>
history_capture_state:
  - current_status
  - last_successful_commit_oid
  - last_successful_commit_at
  - last_error
  - commit_in_flight
```

The exact struct names are not important yet. The ownership is.

### Important distinction

Bloom should track two different truths:

1. **unsaved buffer state**
   - buffer has edits not yet written successfully

2. **undurable saved state**
   - disk has newer content than the last durable git-backed checkpoint

Those are not the same thing and should not be collapsed into one boolean.

## Unified History Surface state model

The current `TemporalStripState` / `TemporalItem` model is too thin for the planned UX.

Today it mostly carries:

- `label`
- `detail`
- `kind`
- branch count
- optional full content
- optional undo-node ID / git OID

That is enough for a compact strip. It is not enough for:

- rich inspector explanations
- unified `SPC H h` / `SPC u u` entry behavior
- block-lineage events
- durable-health state in the history surface
- structured restore semantics

The implementation should therefore evolve toward a state model closer to the following.

### 1. Surface-level state

```text
HistorySurfaceState {
  scope: HistoryScope,
  entry_context: HistoryEntryContext,
  rail: Vec<HistoryStop>,
  selected_stop: usize,
  preview_mode: PreviewMode,
  health: DurableHealth,
  current_page_id: PageId,
  current_content: String,
  current_block_id: Option<BlockId>,
  current_block_line: Option<usize>,
}
```

Where:

- `scope` = page history or block history
- `entry_context` = how the user opened the surface
- `rail` = the canonical list of stops for this surface
- `selected_stop` = one current selection for rail + inspector + preview
- `preview_mode` = diff vs raw historical content
- `health` = durable capture state shown in modeline/inspector

### 2. Scope

```text
enum HistoryScope {
  Page { page_id: PageId },
  Block { page_id: PageId, block_id: BlockId, current_line: usize },
}
```

This allows one family of UI with different stop-generation rules.

### 3. Entry context

```text
enum HistoryEntryContext {
  UndoEntry,
  PageHistoryEntry,
  BlockHistoryEntry,
}
```

This exists so `SPC u u` and `SPC H h` can share the same surface while still differing in initial selection/emphasis if desired.

### 4. Preview mode

```text
enum PreviewMode {
  Diff,
  Raw,
}
```

This should live on the surface state, not be inferred ad hoc from rendering.

### 5. Durable health

```text
enum DurableHealth {
  Unsaved,
  SavedPendingDurable,
  Committing { reason: CheckpointReason },
  DurableCurrent { at: i64, oid: Option<String> },
  DurableError { message: String },
}
```

This is the surface-friendly version of the broader capture state discussed earlier.

## History stop model

The rail and inspector should be driven by one structured stop type rather than by `label + detail`.

### Proposed shape

```text
HistoryStop {
  id: HistoryStopId,
  kind: HistoryStopKind,
  time: StopTime,
  scope_summary: ScopeSummary,
  summary: String,
  restore_effect: RestoreEffect,
  refs: StopRefs,
  branch: Option<BranchContext>,
  checkpoint: Option<CheckpointContext>,
  lineage: Option<LineageContext>,
  mirror: Option<MirrorContext>,
  skip: bool,
}
```

This structure separates:

- what the stop **is**
- what the stop **means**
- what underlying data source can materialize content/diff/restore

### Stop identity

```text
enum HistoryStopId {
  Undo(UndoNodeId),
  Git(String),              // oid
  Synthetic(String),        // stable synthetic lineage/event id within surface
}
```

### Stop kind

```text
enum HistoryStopKind {
  UndoNode,
  DurableCheckpoint,
  LineageEvent,
}
```

This maps directly to the UX language:

- `●` undo nodes
- `○` durable checkpoints
- `◇` lineage events

### Stop time

```text
struct StopTime {
  timestamp: Option<i64>,
  relative_label: String,
  absolute_label: Option<String>,
}
```

The rail can use the compact relative label. The inspector can use the full time.

### Scope summary

```text
enum ScopeSummary {
  CurrentPage,
  PageSet { count: usize, includes_mirrors: bool },
  CurrentBlock,
  BlockMove { from_page: String, to_page: String },
}
```

This allows the inspector to say something richer than "1 file changed."

### Restore effect

```text
enum RestoreEffect {
  ReplaceBufferCreatesUndoNode,
  ReplaceBlockLineCreatesUndoNode,
  CreateForwardNodeFromBranch,
}
```

This should be explicit so the inspector can truthfully tell the user what restore will do.

### Underlying refs

```text
struct StopRefs {
  undo_node_id: Option<UndoNodeId>,
  git_oid: Option<String>,
  blob_page_id: Option<PageId>,
}
```

Synthetic lineage events may point at adjacent real stops for materialized content.

## Branch context

Undo branching should be represented structurally, not only as `branch_count`.

```text
struct BranchContext {
  status: BranchStatus,
  fork_id: UndoNodeId,
  branch_count: usize,
  branch_index: usize,
  fork_summary: String,
}

enum BranchStatus {
  CurrentPath,
  AlternatePath,
  ForkNode,
}
```

The rail uses this for branch rendering. The inspector uses it for explanation.

## Checkpoint context

Durable checkpoints need structured metadata.

```text
struct CheckpointContext {
  reason: CheckpointReason,
  label: Option<String>,
  changed_pages: usize,
  changed_blocks: Option<usize>,
}

enum CheckpointReason {
  AutoIdle,
  AutoMaxInterval,
  ExplicitCheckpoint,
  ShutdownFlush,
  RestoreCommit,
  ExternalChangeCheckpoint,
}
```

This is how Bloom escapes generic commit-message-only history.

The git commit message can still exist as storage/logging detail, but the UI should be driven by these structured fields first.

## Lineage context

Block history needs explicit lineage events.

```text
struct LineageContext {
  event: LineageEventKind,
  primary_id: BlockId,
  related_ids: Vec<BlockId>,
  page_context: Option<LineagePageContext>,
}

enum LineageEventKind {
  Moved,
  SplitSpawnedChild,
  SplitFromParent,
  MergedInto,
  MergedFrom,
}

struct LineagePageContext {
  from_page: Option<String>,
  to_page: Option<String>,
}
```

This is what lets the block-history rail show meaningful `◇` stops while git remains linear.

## Mirror context

Mirror-heavy durable checkpoints should be able to explain cross-page scope.

```text
struct MirrorContext {
  mirror_count: usize,
  includes_background_opened_targets: bool,
}
```

This is optional, but it gives the inspector a principled place to explain mirror propagation when it matters.

## Selection model

Entry commands should share the same surface but set selection intentionally.

### `SPC u u`

- open `HistoryScope::Page`
- selected stop = current undo node if present
- preview mode = diff
- inspector emphasis = branch context

### `SPC H h`

- open `HistoryScope::Page`
- selected stop = current page stop, or most recent durable checkpoint if there is no recent undo history
- preview mode = diff
- inspector emphasis = broader page-recovery context

### `SPC H b`

- open `HistoryScope::Block`
- selected stop = current block on the current path
- preview mode = diff
- inspector emphasis = lineage context

This should be implemented through initialization policy, not through three different view types.

## Render-frame implications

The current `TemporalStripFrame` / `StripNode` API is too narrow for this UX.

It should evolve toward a history-specific frame that can carry:

- rail items with structured kind info
- selected-stop inspector fields
- preview mode and preview content
- durable health state
- block-history lineage details

One possible direction:

```text
HistorySurfaceFrame {
  scope: HistoryScopeFrame,
  rail_items: Vec<HistoryRailItemFrame>,
  selected: usize,
  preview: HistoryPreviewFrame,
  inspector: HistoryInspectorFrame,
  modeline: HistoryModelineFrame,
}
```

The exact names can change, but the key idea is that the frontend should not have to reconstruct meaning from a thin strip model.

## Synthetic stop generation

For page history:

- real undo nodes and real durable checkpoints are enough

For block history:

- exact-ID undo stops are real
- exact-ID git stops are real
- move/split/merge lineage stops are synthetic projections derived from:
  - adjacent snapshot comparison
  - lineage metadata
  - block-ID continuity / retirement facts

Synthetic stops should be stable for the active surface session so navigation does not jitter while blobs load.

## Minimal implementation strategy

To reduce risk, implementation can happen in stages:

1. enrich current stop data model without changing the whole UI at once
2. add checkpoint context and durable health fields
3. add unified `SPC H h` / `SPC u u` entry handling
4. add inspector rendering based on structured stop data
5. add block-lineage synthetic stops
6. optionally add tracked-block gutter later as separate chrome work

This lets Bloom harden the data model first, then the UI, instead of entangling both in one leap.

## History worker protocol

The current `CommitDone { oid: None }` overload should be replaced by an explicit flush request.

Suggested direction:

```text
HistoryRequest
  MarkDirty
  CommitSnapshot { files, reason, message, epoch }
  PageHistory { ... }
  BlobAt { ... }
  Shutdown

HistoryComplete
  FlushRequested { reason, epoch }
  CommitFinished { oid, reason, epoch }
  PageHistory { ... }
  BlobAt { ... }
  Error { ... }
  ShutDown
```

The exact enum names can vary, but the protocol should say explicitly:

- "a checkpoint is due"
- "here is the snapshot to commit"
- "the commit finished / skipped / failed"

That is clearer than using `CommitDone { oid: None }` as a signal for "please now start a commit."

## Checkpoint reasons

Durable checkpoints should carry a typed reason, at least:

- `AutoIdle`
- `AutoMaxInterval`
- `ExplicitCheckpoint`
- `ShutdownFlush`

This will help:

- logging
- user-visible status if needed
- commit-message generation
- tests

---

## The checkpoint pipeline

## 1. User edit → undo only

Local edits mutate buffers and create undo nodes according to Bloom's existing edit-group rules.

Nothing about this step creates durable history.

## 2. Save/autosave/mirror save → disk-stable candidate

When a file write completes successfully:

- the editor marks the buffer clean if appropriate
- the editor adds that page ID to `pending_history_pages`
- the editor signals the history worker that durable history is now stale relative to disk

This is the first moment at which a page becomes eligible for durable capture.

## 3. History worker coalesces writes

The history worker receives dirty signals and runs the existing timing model:

- idle timeout
- safety-net max interval

Repeated successful writes before the timer fires do **not** create multiple durable checkpoints. They just expand the current pending changed set.

This is the main mechanism that keeps git-backed history coarse.

## 4. Worker requests a checkpoint flush

When idle/max-interval says a checkpoint is due, the worker emits `FlushRequested`.

At this point the editor:

- snapshots only the pending changed pages from disk
- packages `(uuid, content)` pairs
- sends `CommitSnapshot`

Important:

- if the pending set is empty, no commit should be attempted
- if the materialized content matches the current history tree, the commit is skipped naturally by `commit_all()`

## 5. Worker commits

The history worker calls `HistoryRepo::commit_all()` with the coalesced changed set.

Properties we want:

- unchanged trees produce no extra commit
- unchanged unrelated files remain untouched
- changed pages land in one durable checkpoint

## 6. Completion handling

On successful commit:

- clear `pending_history_pages` for the committed epoch
- update last successful durable metadata
- clear visible history error state

On skipped commit (`oid = None` because tree unchanged):

- clear the pending epoch anyway
- treat durable history as current

On failure:

- keep the pending changed set
- mark history state degraded / failed
- never pretend the latest state is durably captured

---

## How granular undo becomes coarse git history

This is the intended reduction:

```text
many fine-grained edit groups
    ↓
undo tree nodes in one or more buffers
    ↓
successful file writes
    ↓
coalesced pending changed set
    ↓
idle / max-interval / explicit checkpoint trigger
    ↓
one durable git-backed checkpoint
```

So the reduction boundary is **not undo ancestry**.

It is:

- save/autosave completion
- checkpoint scheduling
- changed-set coalescing

That is what keeps recent history detailed while durable history stays readable.

---

## Mirror propagation policy

Mirror propagation is a special case because it crosses files without direct user focus changes.

Current behavior already allows Bloom to:

- open a mirror target silently if needed
- apply the propagated edit
- save the target

The hardened history design should preserve that behavior but constrain how it appears in history.

## Undo behavior

Mirror targets may receive recent undo nodes in their own buffers.

That is acceptable because recent undo is supposed to be fine-grained.

## Durable-history behavior

Mirror targets must **not** create separate git commits just because their writes complete separately.

Instead:

- the source page and all mirror targets touched by that propagation join the same pending changed set
- the next durable checkpoint captures them together

This preserves the user mental model:

> one conceptual edit, one durable checkpoint

not:

> one source edit, then several extra history noise points

## If a mirror target is not open

Opening a mirror target silently in the background is acceptable if:

- it does not steal focus
- it does not alter the user's pane/session model unexpectedly
- its history effects stay within the same durability epoch as the source edit

The background-opened target should therefore be treated as a correctness detail, not as a new durable-history event boundary.

---

## Explicit checkpoint semantics

Bloom should support an explicit "protect this version now" action.

Architecturally, that action should mean:

1. flush dirty buffers that belong to the current editing epoch
2. wait for successful write completion
3. build the pending changed set from stable content
4. create one durable checkpoint now

### Scope rule

The explicit checkpoint should apply to **all pages changed since the last successful durable checkpoint**, not just the active page.

Reason:

- page history is already per-page on top of shared durable commits
- mirrors and multi-note refactors should stay coherent
- checkpointing only the active page would create partial and confusing history boundaries

### Failure rule

If any required write fails, Bloom must not claim the checkpoint succeeded.

No silent partial durable guarantee.

---

## Shutdown semantics

Shutdown needs a stronger barrier than the current best-effort shape.

The hardened shutdown flow should be:

1. persist session state
2. persist undo trees
3. flush pending dirty writes needed for the current checkpoint epoch
4. build the pending changed set from stable content
5. issue a `ShutdownFlush` checkpoint if needed
6. shut down the history worker

### Why the barrier matters

If Bloom exits while the user thinks history was captured, but the durable checkpoint was never actually created, trust is broken.

The implementation may still allow "quit anyway" as a UX escape hatch, but the architecture should insist that:

- successful shutdown flush is a distinct state
- failed shutdown flush is visible
- Bloom must not blur those outcomes together

---

## Restart and crash behavior

## Restart

On normal restart:

- recent undo is restored from SQLite on its bounded recent horizon
- durable git-backed history is restored from the history repo
- the UI can then combine both in the temporal strip as it does today

## Crash before durable checkpoint

If Bloom crashes after some saves but before the next git-backed checkpoint:

- disk may contain newer content
- recent undo may still restore part of the user's recent state
- durable git-backed history may lag behind

This is acceptable **if and only if** Bloom never claims that the durable checkpoint already happened.

## Crash during commit

Git commit creation should remain atomic at the repo level: old-or-new, not half-visible.

After restart:

- if the commit exists, Bloom should treat it as the new durable point
- if it does not, Bloom should treat the previous durable point as current

## Crash during mirror propagation

Mirror correctness is already governed by the write path.

For durable history, the rule remains:

- no durable checkpoint is assumed until the coalesced checkpoint succeeds

---

## External file changes and branch switches

External file changes already interact with Bloom through the watcher/reload path.

The hardened history rules should be:

- external reload can extend the recent undo tree
- external reload does not itself immediately create a durable git-backed checkpoint
- the new content becomes eligible for durable history only after Bloom reaches a stable post-reload save/checkpoint boundary

This avoids recording transient external churn as if it were a user-approved durable checkpoint.

Branch switches should be handled the same way: they may change disk state and recent undo behavior, but durable history remains anchored to explicit stable checkpoints in Bloom's history repo.

---

## User-visible status model

The PM note calls for visible safety state. The architecture should support it directly.

Bloom should be able to represent at least:

- `Unsaved` — buffers have changes not yet written successfully
- `SavedPendingDurable` — disk is newer than the last durable checkpoint
- `Committing` — durable checkpoint in progress
- `DurableCurrent` — latest stable checkpoint captured
- `DurableError` — latest stable checkpoint attempt failed

This can be shown in many UI forms later. The architecturally important point is that the states are real and derivable from owned data.

The same principle applies to history-stop meaning: the UI should be able to render more than a raw commit message if needed, so the data model should preserve structured checkpoint metadata rather than flatten everything too early into one string.

---

## Implementation constraints

## 1. Do not let the history worker read editor internals directly

The worker should stay a background repo/timer owner, not become a second editor.

## 2. Do not let save mutate buffer content

History capture may depend on save completions, but save remains a serialization/write path, not a text-rewrite path.

## 3. Keep restore on the normal edit path

History restore must continue to produce ordinary document edits with ordinary undo semantics.

## 4. Keep git commits sparse

If a design would create many more durable commits for long writing sessions or mirror-heavy notes, that design is wrong.

## 5. Never silently downgrade safety

If Bloom fails to capture a durable checkpoint, it must retain enough state to surface that failure and retry later.

---

## Risks and edge cases the implementation must handle

### R1 — Ambiguous protocol events

Do not keep using `CommitDone { oid: None }` as an overloaded "flush requested" signal.

### R2 — Pending changed set drift

If writes succeed but pending page IDs are not tracked correctly, Bloom may omit pages from durable history or create misleading status.

### R3 — Mirror epoch fragmentation

If source and mirror targets checkpoint separately, page history becomes noisy and conceptually wrong.

### R4 — Partial explicit checkpoint

If a checkpoint commits only some pages from a multi-page edit epoch, the user gets a false sense of safety.

### R5 — Shutdown race

If shutdown reads files before required writes finish, Bloom may commit stale content or miss the last edit entirely.

### R6 — Duplicate commits

If repeated flushes with unchanged trees still create visible history events, page history density will become noisy. `commit_all()` already gives us the right skip behavior; the orchestration layer should preserve that.

---

## Validation required before implementation is done

### Unit / integration

- auto-checkpoint trigger requests an explicit flush event
- changed-set coalescing across multiple writes produces one durable commit
- unchanged-tree commit skips cleanly
- explicit checkpoint flushes and commits the full pending changed set
- shutdown flush waits for stable content
- mirror propagation across source + unopened target yields one durable commit, not several
- commit failure retains pending state and marks durable status degraded

### Higher-level behavior

- long edit session creates many undo nodes but few durable checkpoints
- page history remains sparse and readable
- block history still reflects meaningful versions
- restore from git history remains a normal undoable edit
- restart restores recent undo and older git history coherently
- `SPC H h` and `SPC u u` open the same page-history UX and remain behaviorally consistent

### Docs steward follow-up

If this architecture ships, `docs/HISTORY.md` must be updated to explain:

- recent undo vs durable checkpoints
- what "automatic protection" means
- what survives restart
- what explicit checkpointing does
- what error / pending durable states mean
- how history stops are explained in the UI beyond generic timestamps/messages
- that `SPC H h` and `SPC u u` are unified entrypoints into the same page-history surface

---

## Recommended implementation order

1. Replace the ambiguous auto-commit signal with an explicit flush-request protocol.
2. Add editor-owned pending changed-set tracking and history capture status.
3. Change routine durable commits to send only the changed set, not whole-vault scans.
4. Implement explicit checkpoint flush semantics.
5. Harden shutdown ordering.
6. Add mirror-coalescing tests and failure-state tests.
7. Update user docs and run tester/docs-steward passes.

---

## Bottom line

Bloom should not try to make git history behave like undo.

The right architecture is the opposite:

- let undo stay detailed
- let durable history stay sparse
- define a clean checkpoint pipeline between them

That pipeline should be driven by stable saves, coalesced over time, and explicit about failures.

If Bloom does that, history can become something the user actually trusts: not infinite granularity everywhere, but the right granularity in the right layer.
