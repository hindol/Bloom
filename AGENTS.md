# Bloom Agent Workflow

This repository uses a doc-first, role-based workflow for GitHub Copilot CLI.

For non-trivial work, read:

1. `docs/AGENT_WORKFLOW.md`
2. `docs/ARCHITECTURE.md`
3. `docs/GOALS.md`

## Core roles

- **CEO / human lead** — drives direction, approves risky or ambiguous decisions, and opens idea briefs for PM research.
- **PM** — owns user intent, discovery, UX framing, and user-facing docs.
- **Architect** — owns technical specs, invariants, edge-case analysis, and pre-implementation risk review.
- **Coding agents** — implement bounded tasks from approved specs.
- **Tester** — validates high-level user journeys with the existing `SimInput` / e2e infrastructure.
- **Docs steward** — checks for drift between code, user docs, and technical specs before work is considered done.

## Required workflow

1. New feature ideas start from a checked-in idea brief based on `docs/IDEA_BRIEF_TEMPLATE.md`.
2. The PM investigates first. Do not jump straight to implementation for non-trivial ideas.
3. Before coding starts, the architect must document risks and edge cases for changes that affect structure, state ownership, concurrency, persistence, or user-visible behavior.
4. Coding agents must read the relevant checked-in spec docs before making changes.
5. User-visible changes require PM-owned docs updates.
6. Non-trivial user flows require tester involvement and `SimInput`/e2e validation.
7. Work is not done until the docs-steward pass confirms there is no spec/docs drift.

## Escalation

Escalate to the human lead when there is:

- product scope ambiguity
- UX trade-off uncertainty
- architecture or threading risk
- destructive behavior
- conflict with existing docs or invariants

