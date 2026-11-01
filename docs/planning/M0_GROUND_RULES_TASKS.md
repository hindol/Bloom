# M0 Ground Rules - Execution Backlog

> Source of truth: [FIRST_PRINCIPLES_REDESIGN.md](FIRST_PRINCIPLES_REDESIGN.md),
> especially sections 7, 8, 11, and 14.
> Status: M0-01 through M0-05 implemented; remaining M0 tasks pending.

## Scope and branch contract

This is the actionable backlog for the accepted M0 milestone, not another design
proposal. It preserves the accepted laws, CEO decisions, milestone ordering, and
exit criteria. Follow [AGENT_WORKFLOW.md](../AGENT_WORKFLOW.md) for role ownership.

This file is the actionable M0 plan and status record. Each task below is a
bounded implementation slice; M1-M7 work remains out of scope until the M0 exit
criteria are met.

The published long-lived branch is `v2`. The app-managed local work branch may
have a prefix (this slice uses `hindol-v2`); publish the isolated groundwork
commit to `v2`, not to the F1/F13 review branch. The first published groundwork
commit must already contain `.github/workflows/ci.yml` with explicit `main` and
`v2` filters for both pushes and pull-request **base** branches.

Keep all existing Rustfmt, Clippy, coverage, and release-build jobs and their
failure thresholds. Do not add success-shaped placeholder jobs for infrastructure
that does not exist yet. Release and documentation deployment remain tag-driven;
a push to `v2` must not release or deploy the app.

`main` remains the daily driver and fix-only until cutover. Tactical F1/F13 fixes
are tracked separately; this slice neither modifies nor claims to deliver them.
Never merge `main` into `v2`. Carry regression scenarios across, not fix code.
No rewrite milestone may touch the real vault before the M7 cutover gate.

## Task order and ownership

Each row is a bounded handoff. "Depends on" lists actual prerequisites, not an
instruction to serialize independent tasks. Owners are workflow roles, not
additional standing agents.

| ID | Task / owner | Depends on | Status |
| --- | --- | --- | --- |
| M0-01 | Publish rewrite-branch CI groundwork / coding agent | None | Implemented in this slice; verify first push below |
| M0-02 | Establish crate boundaries and lint policy / architect + coding agent | M0-01 | Implemented; see [M0_CRATE_BOUNDARIES.md](M0_CRATE_BOUNDARIES.md) |
| M0-03 | Add typed coordinates and injected ports / architect + coding agent | M0-02 | Implemented; see [M0_TYPED_PORTS.md](M0_TYPED_PORTS.md) |
| M0-04 | Add invariant-checker framework and coverage map / coding agent + docs steward | M0-03 | Implemented; see [M0_INVARIANT_FRAMEWORK.md](M0_INVARIANT_FRAMEWORK.md) |
| M0-05 | Inventory and triage the behavioral oracle / PM + tester | None | Implemented; see [M0_ORACLE_INVENTORY.md](M0_ORACLE_INVENTORY.md) |
| M0-06 | Add minimal deterministic simulation and replay / tester + coding agent | M0-03, M0-04 | Implemented; see [M0_SIMULATION_REPLAY.md](M0_SIMULATION_REPLAY.md) |
| M0-07 | Add SimInput-compatible e2e adapter / tester + coding agent | M0-05, M0-06 | Pending |
| M0-08 | Add external progress watchdog / architect + tester | M0-06 | Pending |
| M0-09 | Add independent reference-model harness / architect + tester | M0-06 | Pending |
| M0-10 | Add curated detector-fault corpus / tester + coding agent | M0-04, M0-08, M0-09 | Pending |
| M0-11 | Add performance harness and budget gates / tester + coding agent | M0-06, M0-08 | Pending |
| M0-12 | Wire executable M0 CI gates and close milestone / tester + docs steward | M0-07, M0-10, M0-11 | Pending |

## Bounded handoffs and acceptance checks

### M0-01 - CI groundwork (this slice)

Deliver this backlog, its link from the accepted delivery plan, and explicit
`[main, v2]` push and PR-base filters in the existing CI workflow.

Acceptance: parse the workflow as YAML; check both exact filter lists; confirm
the four existing jobs, commands, and thresholds are unchanged. Check that a
first push to `v2` and a PR targeting `v2` are included, `main` remains included,
and an unrelated branch/base is excluded. Inspect the final diff for scope.
After publication, inspect the push-triggered CI run at the exact commit SHA;
local workflow validation is not proof that the hosted Rust jobs passed.

### M0-02 - Crate boundaries and lint policy

Deliver the section 11 layout: `bloom-kernel`, `bloom-state`, `bloom-vault`,
`bloom-text`, `bloom-index`, `bloom-gui`, and `bloom-mcp`. Record module ownership,
dependency direction, and salvage constraints before editing the workspace.
Preserve the oracle inventory; do not delete tests as part of crate setup.
Enforce the L7/I11 kernel ban on direct filesystem, wall-clock, and thread access
with `disallowed_methods` and crate-level denial. Review salvaged leaves against
L6 as well as the side-effect policy.

The approved ownership, risk review, and temporary `rewrite/` workspace boundary
are recorded in [M0_CRATE_BOUNDARIES.md](M0_CRATE_BOUNDARIES.md). The root
workspace and behavioral oracle remain intact during staging; the rewrite
workspace becomes the root workspace only at the approved cutover. CI runs
real check and Clippy commands against the staged workspace on `v2`.

Acceptance: the empty workspace builds and lints; a deliberately forbidden
kernel call is rejected by the configured lint, then removed. No real
editor/store behavior, GUI integration, or M1 `update`/`settle`/`view` is added.

### M0-03 - Typed coordinates and injected ports

Deliver L6 coordinate newtypes (`CharIdx`, `ByteIdx`, `LineIdx`, `GraphemeIdx`,
`DisplayCol`, `Px`) and the `Fs`, `Clock`, `Watcher`, and `Rng` ports.
Define conversion ownership and prohibit public `From<usize>` shortcuts.
Document effect completion, explicit failure, ownership, and time/randomness
injection contracts. Keep store protocols and text semantics for later milestones.

Acceptance: compile-fail checks reject mixed units and unrestricted construction;
port fakes can return explicit errors and supply deterministic time and randomness
without accessing production I/O.

The coordinate ownership and port error/ownership contracts are recorded in
[M0_TYPED_PORTS.md](M0_TYPED_PORTS.md). Opaque units live in `bloom-text` or
`bloom-kernel` according to conversion authority; the port traits are in the
kernel boundary and are exercised only by deterministic test fakes in this
slice.

### M0-04 - Invariant framework

Deliver a `Violation` result vocabulary, checker registration, and an invariant
coverage map for every I1-I12 entry in section 7. Identify each invariant's owning
module, checker, test, and activation milestone. Missing subsystem checks must
be visibly pending, not unconditional `Ok(())` implementations.

Acceptance: debug/test/simulation hooks run all applicable checkers; a known
synthetic violation fails the check. The docs-steward coverage check catches a
missing invariant mapping. Track staged activation explicitly; full I1-I12
execution remains mandatory as the relevant model surfaces become available.

### M0-05 - Behavioral oracle inventory

Deliver a checked-in inventory of the actual e2e and Vim scenarios at the chosen
baseline. Start from `crates/bloom-core/tests/e2e.rs`, the Vim tests, and
`crates/bloom-test-harness`; include subsequent tactical regression scenarios.
The proposal's 209/42 counts are historical context, not an inventory substitute.
Classify each scenario as **port**, **rewrite**, or **drop**. Rewrites cite an
accepted decision (such as D2); drops explain the implementation-only assertion.
PM owns behavioral changes, and unresolved policy goes to the human lead.

Acceptance: every discovered scenario has a unique entry, disposition, reason,
and target journey. Nothing is silently omitted. This triage list is the parity
checklist used through M6 and cutover.

The checked-in case-by-case roster is [M0_ORACLE_INVENTORY.md](M0_ORACLE_INVENTORY.md).
The chosen baseline has 209 e2e cases, 42 Vim cases, and 3 harness animation
scenarios. The inventory also carries the F1 and F13 tactical regressions,
records ignored cases without counting them as passing, and marks the UC95
open-and-save regression explicitly.

### M0-06 - Deterministic simulation and synthetic replay

Deliver `SimFs`, `SimClock`, and `SimWatcher`, seeded randomness, and a versioned
synthetic fixture/seed/event-log replay format. Start with one minimal journey
against the empty-kernel boundary and a path to the dozen hand-written race/crash
scenarios in section 8. Do not implement M1 persistence to make the harness pass.
Define stable failure signatures and reproducible counterexample storage.

Acceptance: identical fixture plus seed/log replays identical outcomes, including
injected filesystem errors and duplicate/reordered notifications. Replay uses no
real vault. Synthetic exact replay and content-free production diagnostics are
clearly distinct; sensitive capture is local and opt-in only.

The constraints and failure semantics are recorded in
[M0_SIMULATION_REPLAY.md](M0_SIMULATION_REPLAY.md). The feature-gated
`bloom-kernel::simulation` module provides the in-memory ports, strict versioned
codec, stable failure signatures, and
[`empty-kernel-smoke.sim`](../../rewrite/testdata/replays/empty-kernel-smoke.sim).
The replay and port tests verify deterministic outcomes, explicit failures,
watcher ordering/duplicates, and malformed or unsupported input. No product
transition or real-vault persistence is exercised. Rewrite CI checks nested
workspace formatting and builds the opt-in simulation feature.

### M0-07 - E2e compatibility adapter

Deliver the `SimInput`/`TestScreen`-compatible adapter boundary and one smoke
scenario on the empty-kernel surface. Link adapter coverage to M0-05's inventory.
Keep product scenarios visibly pending until their implementation milestone;
do not weaken their expectations or assert product parity at M0.

Acceptance: the smoke journey runs through the adapter, failures propagate to
the test runner, and unimplemented parity entries cannot count as passing.
Geometry-inclusive user journeys remain required under L8.

### M0-08 - Progress watchdog

Deliver an observer outside the component being watched, with bounded operation
steps/time, queue/retained-document limits, and explicit success/failure/blocked
outcomes. Specify repeated open/edit/close resource checks and the due-timer
progress contract I10. Bounds must not depend solely on the watched component's
own clock or progress report.

Acceptance: a deliberately stalled synthetic operation fails within the bound;
an explicitly blocked operation is distinguishable from completion and a hang.

### M0-09 - Independent reference models

Deliver a simple reference-model harness and at least one independently computed
synthetic comparison. Document which production bookkeeping it must not reuse.
Register future full-parse, full-index-rebuild, UI/MCP semantic equivalence, and
duplicate-notification convergence comparisons without implementing M2/M4/M5.

Acceptance: deliberately perturbing the candidate output causes a mismatch;
the reference does not share the candidate's incremental caches or bookkeeping.

### M0-10 - Detector sensitivity corpus

Deliver test-only faults mapped to named detectors, beginning with an applicable
empty-kernel fault. Track stale-index acceptance, skipped ID uniqueness, and an
unconsumed timer as staged faults for their owning milestones, not fake passes.
Keep fault hooks unavailable to production builds.

Acceptance: every active curated fault is caught by its named detector; a
surviving fault fails the corpus runner. Plan small PR and broader nightly
campaigns as section 8 requires, with minimized deterministic regressions.

### M0-11 - Performance harness

Deliver the Criterion harness, synthetic fixture definitions, and regression
comparison policy (>10% regression fails). Register all section 8 budgets:
p99 keypress-to-frame <8 ms (10k lines, three panes), open <30 ms (10k lines),
search <50 ms (10k pages), cold-start first frame <300 ms (10k pages, warm cache),
and index convergence <2 s after 100 external page changes without blocking input.

Acceptance: run an empty-kernel smoke measurement and verify the gate detects a
deliberately regressed sample. Criterion throughput alone does not prove a p99
budget: define latency sampling, warmup, and baseline comparability. Product
budgets stay pending until their operations exist; no fabricated zero timings.

### M0-12 - Executable gates and milestone closure

Wire the implemented lints, checker coverage, one deterministic simulation
journey, adapter smoke, small detector-fault corpus, and benchmark smoke/gates
into `v2` push and PR CI. Add the broader nightly checks once their runners exist.
The tester checks failing paths as well as passing commands. The docs steward
checks this backlog, technical contracts, and product docs against actual code.
No user-visible behavior changes are claimed by the groundwork slice.

Acceptance: the full M0 exit criteria in section 11 are executable and green,
triage is complete, and no missing detector or budget is reported as passing.
Only then mark M0 complete and proceed to M1's walking skeleton. Keep the State
store before the Vault Store and the ADR-3 acceptance spike at the start of M2;
this backlog does not reorder those decisions.
