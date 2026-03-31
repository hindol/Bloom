# History Production Hardening — Implementation Plan

> Bounded coding-slice plan derived from the PM discovery and architect specs.
> Status: **Draft**

See also:

- `HISTORY_PRODUCTION_HARDENING.md`
- `HISTORY_PRODUCTION_HARDENING_ARCHITECTURE.md`
- `../HISTORY.md`
- `../TEMPORAL_NAVIGATION.md`

---

## Goal

Ship production-hardened history in small slices that each leave Bloom in a coherent state.

The target outcome is:

- one unified page-history surface for `SPC H h` and `SPC u u`
- richer stop metadata and inspector-driven explanation
- durable-history capture that is observable, coalesced, and failure-aware
- block history that can explain moves, splits, and merges over linear durable checkpoints

This plan intentionally separates:

- **data/model hardening**
- **worker/protocol hardening**
- **surface integration**
- **block-lineage enrichment**

so the team does not try to land all of it in one leap.

---

## Slice 0 — Baseline validation and documentation checkpoint

### Goal

Confirm current behavior and freeze the baseline before structural changes.

### Work

- run existing history-related tests and relevant editor tests
- note current behavior of:
  - `SPC H h`
  - `SPC H b`
  - `SPC u u`
  - restore behavior
  - history-thread completion handling
- capture known current limitation:
  - `CommitDone { oid: None }` is an ambiguous flush signal and appears only partially wired

### Output

- verified baseline
- updated implementation notes if any pre-existing failures are found

### Why first

Later slices change both behavior and state shape. We want a clear baseline before touching them.

---

## Slice 1 — Harden durable-capture protocol

### Goal

Replace the ambiguous current history-worker handshake with an explicit flush protocol.

### Work

- replace overloaded `CommitDone { oid: None }` signaling with explicit "flush requested" / "commit finished" semantics
- keep timing in the history worker
- keep snapshot collection in the editor
- preserve explicit shutdown and page-history/blob lookup flows

### Main files

- `crates/bloom-core/src/history/thread.rs`
- `crates/bloom-core/src/lib.rs`
- `crates/bloom-core/src/editor/files.rs`
- any history-related event loop plumbing

### Acceptance criteria

- auto-idle and max-interval both request an explicit flush
- editor can respond with a real snapshot commit
- failure and skip outcomes are distinguishable
- no behavior depends on the meaning of `oid: None`

### Validation

- targeted unit/integration tests for history request/completion flow
- existing `bloom-history` tests still pass

---

## Slice 2 — Track editor-owned durable state

### Goal

Add explicit editor-owned state for pending durable capture and visible health.

### Work

- introduce editor state for:
  - pending changed pages
  - commit in flight
  - last successful durable checkpoint
  - last durable error
- update save/autosave success path to mark pages as pending durable
- clear/update state on commit success, skip, and failure

### Main files

- `crates/bloom-core/src/lib.rs`
- `crates/bloom-core/src/editor/files.rs`
- render plumbing for modeline/status if needed

### Acceptance criteria

- Bloom can distinguish:
  - unsaved
  - saved but pending durable
  - durable current
  - durable failed
- state survives ordinary editor activity correctly
- mirror-related writes join the same pending durable changed set

### Validation

- focused unit tests around state transitions
- manual validation with normal save, autosave, and forced failure paths if available

---

## Slice 3 — Commit only the coalesced changed set

### Goal

Stop treating routine durable capture like a whole-vault scan.

### Work

- materialize snapshot commits from the pending changed-page set
- preserve shutdown correctness
- keep explicit checkpoint behavior coherent across multi-page edit epochs

### Main files

- `crates/bloom-core/src/lib.rs`
- `crates/bloom-core/src/history/thread.rs`
- any helper that currently gathers history snapshot content

### Acceptance criteria

- normal durable checkpoints send only the changed page subset
- mirror-linked files touched in one editing epoch commit together
- unchanged-tree commits still skip naturally
- shutdown remains correct even if implementation uses a broader fallback path initially

### Validation

- tests for multi-page changed-set commits
- tests that unchanged unrelated pages are preserved

---

## Slice 4 — Enrich stop metadata without changing the whole UI yet

### Goal

Introduce structured stop data before doing a large UI rewrite.

### Work

- enrich internal history items beyond `label + detail`
- add:
  - stop kind
  - time fields
  - scope summary
  - restore effect
  - branch context
  - checkpoint context
- keep existing strip rendering functional during the transition

### Main files

- `crates/bloom-core/src/lib.rs`
- `crates/bloom-core/src/render/frame.rs`
- `crates/bloom-core/src/editor/render.rs`
- `crates/bloom-core/src/editor/page_history.rs`

### Acceptance criteria

- structured stop metadata exists in the editor/model layer
- current UI can continue rendering during migration
- commit messages are no longer the sole source of meaning for durable stops

### Validation

- unit tests for stop construction
- spot checks of existing history surfaces for regressions

---

## Slice 5 — Unify `SPC H h` and `SPC u u`

### Goal

Make page history and undo-tree entry open the same underlying surface/state.

### Work

- route `SPC H h` and `SPC u u` into the same page-history surface
- distinguish only by initial selection/emphasis if still useful
- keep `SPC H b` as the block-history sibling variant

### Main files

- `crates/bloom-core/src/editor/keys.rs`
- `crates/bloom-core/src/editor/page_history.rs`
- `crates/bloom-core/src/lib.rs`

### Acceptance criteria

- both commands open one core page-history UX
- branch behavior is identical regardless of entry command
- restore/diff behavior is identical regardless of entry command

### Validation

- e2e coverage for:
  - opening from `SPC u u`
  - opening from `SPC H h`
  - equivalent navigation and restore behavior

---

## Slice 6 — Add the inspector-backed history surface

### Goal

Turn the current thin strip into the recommended rail + inspector history surface.

### Work

- expand render-frame support beyond current `TemporalStripFrame`
- add inspector region to explain selected stop
- keep rail for orientation and branch structure
- keep preview for diff/raw content
- expose lightweight durable health in the modeline

### Main files

- `crates/bloom-core/src/render/frame.rs`
- `crates/bloom-core/src/editor/render.rs`
- relevant frontend rendering code in `bloom-gui`

### Acceptance criteria

- selected stop shows structured explanation
- undo branch nodes explain fork context
- durable checkpoints explain checkpoint reason / scope
- modeline surfaces durable health state

### Validation

- GUI tests where feasible
- manual interaction pass across page history and undo-entry workflows

---

## Slice 7 — Add block-lineage synthetic stops

### Goal

Make `SPC H b` lineage-aware over linear durable history.

### Work

- define lineage metadata/events for:
  - move
  - split
  - merge
- project synthetic lineage stops into block history
- make inspector explain parent/child/survivor/retired semantics

### Main files

- block-history loading / projection code
- indexing or history helpers needed for lineage metadata
- render path for block-history stop kinds

### Acceptance criteria

- block moves appear as continuity events
- splits appear as parent/child lineage events
- merges appear as survivor/retired lineage events
- git history remains linear

### Validation

- targeted tests for block move / split / merge scenarios
- manual UX pass in `SPC H b`

---

## Slice 8 — Explicit checkpoint command

### Goal

Give the user a first-class "protect this version now" action.

### Work

- add command/keybinding surface
- define user-visible naming
- ensure flush ordering is correct:
  - write current dirty content
  - build snapshot from stable content
  - commit one explicit checkpoint
- carry optional user-authored label later if desired

### Acceptance criteria

- explicit checkpoint creates a meaningful durable stop
- failure is visible
- multi-page edit epochs stay coherent

### Validation

- e2e flow for explicit checkpoint then restore

---

## Slice 9 — Optional advanced block-ID gutter

### Goal

Add the optional live-observability chrome for advanced users without disturbing normal editing.

### Work

- add config surface for block-ID gutter
- render read-only faded IDs in a separate lane left of line numbers
- keep it out of cursor/motion/copy semantics

### Acceptance criteria

- default off
- visually secondary
- split/merge survivor behavior can be observed live

### Validation

- GUI rendering tests if feasible
- manual verification with split/merge flows

---

## Slice 10 — Docs steward + tester pass

### Goal

Finish the feature as a shipped system, not just code.

### Work

- update user-facing docs:
  - `docs/HISTORY.md`
  - `docs/TEMPORAL_NAVIGATION.md`
  - `docs/KEYBINDINGS.md`
  - relevant `docs/USE_CASES.md`
- add or extend e2e coverage for:
  - unified `SPC H h` / `SPC u u`
  - block-history lineage behavior
  - explicit checkpoint flow
  - restore semantics
- docs-steward review for drift

### Acceptance criteria

- docs and code agree
- tester-owned flows pass
- no stale docs still describe separate undo-tree/page-history surfaces

---

## Recommended order

Recommended coding order:

1. Slice 0 — baseline validation
2. Slice 1 — durable-capture protocol
3. Slice 2 — editor-owned durable state
4. Slice 3 — coalesced changed-set commits
5. Slice 4 — structured stop metadata
6. Slice 5 — unify `SPC H h` / `SPC u u`
7. Slice 6 — rail + inspector surface
8. Slice 7 — block-lineage synthetic stops
9. Slice 8 — explicit checkpoint command
10. Slice 9 — optional block-ID gutter
11. Slice 10 — docs/tester/docs-steward pass

This order prioritizes correctness and data-model stability before UI polish.

---

## Suggested first implementation milestone

If we want a sensible first shippable milestone, stop after **Slice 6**.

That would give Bloom:

- correct durable-capture state
- explicit worker/editor checkpoint protocol
- one unified page-history surface
- structured stop explanations
- better durable-state visibility

Then block-lineage richness, explicit checkpoints, and the advanced gutter can land as follow-up phases.

---

## Bottom line

The history work should be implemented as a sequence of layered upgrades, not a monolith.

First make durability explicit and correct.
Then make the unified surface meaningful.
Then make block history richer.
Then add optional expert affordances.

That sequencing is the safest path to a production-hardened history feature users can actually trust.
