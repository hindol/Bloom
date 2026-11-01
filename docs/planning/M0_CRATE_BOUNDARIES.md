# M0-02 - Crate boundaries and lint policy

> Status: approved implementation constraint for the `v2` rewrite branch.
> This establishes boundaries only; it does not implement M1 behavior.

## Decision

Keep the current root workspace and its test suites intact as the behavioral
oracle while the rewrite workspace is empty and independently buildable at
`rewrite/Cargo.toml`. The rewrite workspace contains only the seven crates in
the accepted M0 plan. This is a staging boundary, not a second product or a
change to the accepted in-place cutover: once the oracle inventory is ported and
the cutover is approved, the rewrite workspace becomes the repository's root
workspace and the old implementation is removed in that cutover change.

Do not make the legacy workspace depend on rewrite crates, or vice versa. Do
not copy legacy implementation code into the rewrite workspace. Behavioral
coverage moves by explicit M0-05/M0-07 disposition and porting, not by sharing
mutable implementation.

## Ownership and allowed dependencies

Arrows point from a consumer to a dependency. A crate may depend only on the
crates listed in its row; dependencies not yet needed remain absent.

| Crate | Owns | May depend on | Must not own |
|---|---|---|---|
| `bloom-text` | Rope-backed document, text properties, parser-facing model, token/projection rules, text-coordinate conversions | Third-party text/parser libraries | Vault paths, clocks, threads, SQLite, GUI state |
| `bloom-kernel` | Domain model, events, effects, transitions, focus, timers, invariant vocabulary | `bloom-text` | Filesystem, database, wall-clock reads, thread lifecycle, frontend details |
| `bloom-state` | Durable version records, transaction journal, session/settings state, retired-ID set, SQLite schema/migrations | `bloom-kernel`, `bloom-text` | Markdown files, GUI, user input routing |
| `bloom-vault` | Page loading, path/identity reconciliation, watcher integration, CAS writes, merge/recovery execution | `bloom-kernel`, `bloom-state`, `bloom-text` | UI focus/render state, independent page identity rules |
| `bloom-index` | Disposable derived projection, version fencing, search/index cache | `bloom-kernel`, `bloom-text` | Canonical content, durable versions, UI-owned editor state |
| `bloom-gui` | Iced event/effect adapter, input, font metrics, painting | `bloom-kernel`, `bloom-text`, `bloom-state`, `bloom-vault`, `bloom-index` | Domain transitions, persistence policy, duplicate geometry authority |
| `bloom-mcp` | JSON-RPC/IPC client adapter and request/response encoding | `bloom-kernel` | Direct vault/state writes or a second mutation path |

`bloom-kernel` is the only dependency root for domain transitions. It must not
depend on an adapter. Adapters may depend on kernel-owned protocol types and
feed results back as events. `bloom-state` is introduced before `bloom-vault`
in M1 as required by ADR-2; this dependency graph permits that ordering.
No shared catch-all crate is added in M0-02. Errors stay explicit at the
boundary that can interpret them; common error vocabulary can be introduced
only when a concrete cross-crate contract requires it.

## M0 implementation constraints

- Each crate begins as a minimal library boundary. No `update`/`settle`/`view`,
  persistence behavior, sync behavior, GUI behavior, or MCP mutation path is
  added in M0-02.
- `rewrite/Cargo.toml` is a standalone workspace so package names such as
  `bloom-gui` and `bloom-mcp` do not collide with the preserved oracle.
- Keep the legacy workspace's source files, tests, manifests, and test harness
  unchanged. Do not mark legacy e2e scenarios as passing parity merely because
  the old suite still runs.
- The staged workspace does not claim that current legacy Clippy/e2e failures
  are fixed. Its check/lint gate is separate and must run actual Cargo
  commands, not success-shaped placeholder steps.
- Add `clippy::disallowed_methods` denial at the kernel crate root and a
  workspace Clippy configuration that rejects representative direct I/O,
  wall-clock, and thread-entry points. This lint is a guardrail, not a proof
  that every side effect is impossible; the kernel's dependency direction and
  review remain required.
- Keep OS time acquisition and effect execution in adapters. The kernel may
  accept an injected time value as data; it may not obtain current time itself.
- Review salvaged code against L6 typed-coordinate rules and L7 side-effect
  rules before moving it. M0-02 moves no salvaged implementation.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| Two workspace roots drift or are mistaken for two supported products | Label the nested workspace as temporary rewrite staging; keep one CI check path; remove the staging boundary during the approved cutover |
| Legacy tests are lost or silently treated as v2 parity | Leave legacy files untouched; use the checked-in M0-05 inventory and M0-07 adapter as the only parity accounting |
| Kernel purity is overstated by an incomplete lint list | Deny the lint at crate scope, test a forbidden call, review the dependency graph, and treat the lint as a guardrail rather than a complete effect system |
| Persistence responsibilities become circular | Kernel owns protocol/domain types; State and Vault are adapters; neither is a kernel dependency |
| GUI or MCP bypasses the kernel | GUI/MCP depend on kernel protocols and may not depend on one another; MCP has no State/Vault dependency |
| M0 scaffolding accidentally becomes M1 | Empty library roots only; M0 acceptance checks compile/lint boundaries without product behavior |

## Verification gate for M0-02

1. The seven named packages exist only in the rewrite workspace and have no
   dependencies beyond the approved graph.
2. The root workspace, legacy source, and oracle test files are unchanged by
   crate setup.
3. `cargo check` and Clippy pass on the rewrite workspace.
4. A temporary direct-I/O call in `bloom-kernel` fails Clippy, and is removed.
5. The rewrite CI check executes both commands on pushes and pull requests
   targeting `v2`.
