# Copilot Instructions for Bloom

For non-trivial work in this repository:

1. Read `AGENTS.md`.
2. Read `docs/AGENT_WORKFLOW.md`.
3. Read the relevant design docs in `docs/` before proposing structural changes.

Apply the Bloom workflow consistently:

- Start new feature exploration from a checked-in idea brief based on `docs/IDEA_BRIEF_TEMPLATE.md`.
- Treat PM-owned discovery and user-facing docs as the source of truth for product intent.
- Treat architect-owned specs and risk notes as the source of truth for implementation constraints.
- Do not start non-trivial implementation until the architect has stress-tested the idea against edge cases such as thread safety, state ownership, persistence, and failure behavior.
- Use the existing `SimInput` / e2e infrastructure for high-level behavior validation.
- Before considering work complete, verify that user docs, technical docs, and code still agree.

If unsure, escalate to the human instead of guessing.

