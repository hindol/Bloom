# M0-09 - Independent reference models

> Status: architect constraints for synthetic differential checks. Product
> parser, index, UI/MCP, and vault-convergence parity remain pending.

## Decision

Add a test-only reference filesystem model that independently interprets the
synthetic replay's filesystem operations. It uses a simple linear list and
linear path lookup, not `SimFs`, its `BTreeMap`, its failure queue, or replay
execution helpers. The fixture is shared as input, but expected state is
computed from scratch by the reference for each ordered filesystem operation.
The comparison includes reads, injected failures, writes, and final file bytes.

The comparison is deliberately narrow: it validates that one deterministic
simulation result agrees with a separate, transparent model on synthetic file
state. It is not a product-parity result and does not implement M2 parsing, M4
vault/index behavior, or UI/MCP equivalence.

## Registered comparisons

| Comparison | Candidate | Independent reference | Status |
|---|---|---|---|
| Synthetic replay filesystem state | `run_replay` / `SimFs` | Linear vector model that interprets the fixture operations directly | Active |
| Full parse vs incremental parse | M2 parser | Full-document parse from canonical text, with no incremental tree reuse | Pending M2 |
| Index projection vs full rebuild | M4/M5 index projector | Rebuild derived records from the complete synthetic vault | Pending M5 |
| UI vs MCP semantic edits | M1/M7 adapters | Compare kernel state after equivalent domain events | Pending M7 |
| Duplicate-notification convergence | M4 watcher reconciliation | Apply notifications to a fresh state from the same authoritative files | Pending M4 |

Future references must not reuse the candidate's mutable cache, revision
bookkeeping, incremental parser state, or result-producing helper. Shared input
types and explicit comparison value types are allowed; candidate expected output
is not an oracle.

## Verification gate

1. The reference computes the checked-in replay fixture's filesystem result
   without using `SimFs` or `run_replay`.
2. Candidate and reference outputs compare equal for all modeled file
   operations, including injected write failure.
3. Perturbing candidate output causes the comparison to fail.
4. Product-level comparisons are visibly pending until their owning milestone;
   none are represented as passing empty-kernel checks.
