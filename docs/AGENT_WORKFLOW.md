# Bloom 🌱 — Agent Workflow

> Doc-first solo-development workflow for GitHub Copilot CLI.

---

## Goal

Bloom uses GitHub Copilot CLI as the primary orchestration layer for day-to-day development work.

The aim is not to build a separate agent runtime inside Bloom. The aim is to make solo development more reliable by giving each kind of work a named owner and a checked-in artifact.

## Why this workflow exists

- Bloom already has strong design docs in `docs/`.
- Bloom already has meaningful e2e infrastructure through `SimInput` and `TestScreen`.
- Bloom has important architectural invariants around buffer ownership, threading, rendering, and persistence.
- The most expensive failures are usually not raw coding failures — they are product drift, architecture drift, test gaps, and documentation drift.

This workflow is designed to reduce those failures.

## Why not Microsoft Agent Framework first?

Microsoft Agent Framework may still be useful later for external automation, but it is not the first tool to reach for here.

For Bloom's current need, GitHub Copilot CLI already provides the right control surface:

- planning
- sub-agents
- fleet mode
- review
- task management
- MCP configuration

The first step is to codify a reliable operating model in the repo itself.

## Core roles

| Role | Owns | Main outputs |
|------|------|--------------|
| **CEO / human lead** | Product direction, trade-off decisions, final approval | Idea briefs, accept/reject decisions |
| **PM** | Discovery, user framing, UX rationale, user-facing docs | Discovery notes, recommendations, user-doc updates |
| **Architect** | Technical shape, invariants, risk analysis, edge cases | Technical spec, risk review, implementation constraints |
| **Coding agents** | Bounded implementation work | Code changes grounded in the approved docs |
| **Tester** | Executable user journeys and high-level behavior validation | `SimInput` / e2e coverage and validation results |
| **Docs steward** | Drift detection between docs and implementation | Review pass confirming code, user docs, and specs still agree |

## No separate standing user role

Bloom does not need a separate standing "user" role in this workflow.

- The **PM** acts as the user advocate and defines expected user journeys.
- The **tester** operationalizes those journeys into executable checks.

That split keeps the model small while still preserving the user point of view.

## Workflow stages

### 1. CEO idea intake

When you think a feature may be useful, create a checked-in idea brief using `docs/IDEA_BRIEF_TEMPLATE.md`.

The idea brief should stay lightweight. It exists to answer:

- what the feature idea is
- why it may matter
- who it helps
- what constraints are already known
- what you want the PM to investigate

The PM does not start from a vague chat summary. The checked-in idea brief is the starting artifact.

### 2. PM discovery

The PM investigates the idea and extends the brief or produces a linked discovery/spec doc.

The PM is responsible for:

- clarifying the user problem
- identifying UX risks and default behaviors
- driving lightweight UX exploration when surface design is central to the problem
- recommending whether to pursue, defer, or reject
- updating user-facing docs when a user-visible behavior changes

When the open questions are about interaction shape rather than pure feature scope, the PM should leave behind a concrete UX artifact.

Preferred order:

1. checked-in low-fidelity exploration in the repo
2. ASCII wireframes / interaction sketches in the PM doc
3. optional Figma exploration when the interaction is too spatial or stateful to reason about well in prose alone

The goal is not a heavyweight design handoff. The goal is to make the proposed UX concrete enough that the architect and coding agents are reacting to the same shape instead of freehanding it from prose.

### 3. Architect risk review

Before non-trivial coding begins, the architect stress-tests the idea from the standpoint of technical risk.

This review should explicitly consider:

- thread safety
- state ownership
- concurrency and channel boundaries
- persistence guarantees
- failure and recovery behavior
- performance hot paths
- confusing edge cases

The architect owns this step. If these risks are not written down, the work is not ready for implementation.

### 4. Coding fleet

Once the PM and architect artifacts are in place, coding agents can implement bounded tasks in parallel.

Coding agents must:

- read the relevant checked-in docs first
- stay within the approved scope
- escalate ambiguity instead of inventing policy

### 5. Tester validation

The tester validates high-level behavior using Bloom's existing e2e infrastructure.

The default path is the existing `SimInput` / `TestScreen` style in:

- `crates/bloom-core/tests/e2e.rs`
- `crates/bloom-test-harness/`

The tester should cover real user journeys such as:

- open a document
- edit it
- save it
- reopen it
- confirm the edit persisted

The tester uses PM-authored user journeys and architect-authored risk notes as input.

### 6. Docs steward review

Before work is treated as done, the docs steward checks for drift.

This review asks:

- does the shipped behavior still match the user-facing docs?
- does the implementation still match the technical spec?
- were architecture notes updated if invariants changed?

This is a focused review function, not a heavyweight extra manager.

### 7. Human sign-off

The human lead stays in charge of direction.

Escalate when there is:

- product scope ambiguity
- UX uncertainty
- architecture or threading risk
- destructive behavior
- conflict with existing docs

## Required artifacts

For non-trivial work, the workflow should leave behind durable artifacts:

1. an idea brief
2. PM discovery or product/spec notes
3. architect risk review and implementation constraints
4. tester validation or e2e updates
5. user-doc updates if behavior changed

## Suggested instruction surfaces

The workflow should be reinforced in the repo through:

- `AGENTS.md`
- `.github/copilot-instructions.md`
- checked-in docs under `docs/`

## Definition of done

Non-trivial work is done only when:

- the implementation matches the approved scope
- architect-owned edge cases were addressed
- tester-owned high-level validation passed
- user-facing docs were updated when needed
- docs-steward review found no drift
