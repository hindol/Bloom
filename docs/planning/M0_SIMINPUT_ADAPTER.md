# M0-07 - SimInput-compatible adapter

> Status: implemented for M0-07. This does not mark any legacy oracle scenario
> as implemented or change product behavior.

## Decision

Keep the adapter inside the existing, feature-gated
`bloom-kernel::simulation` module. Do not add a crate or make the legacy
workspace depend on rewrite code. `SimInput` is a small, generic driver boundary
with explicit fallible operations for keys, literal text, logical time,
background settling, screen snapshots, and buffer text. Future product-backed
drivers can implement it without sharing the legacy editor or its mutable
state.

The M0 empty-kernel driver records input operations in order but does not
interpret keys, edit a document, access a vault, or fabricate a render frame.
Its `TestScreen` explicitly reports that product rendering is unavailable.
Screen and buffer assertions against that driver return an error, never an
empty/default value that could look like a passing assertion.

Parity status is separate from execution outcome. The adapter exposes a
fail-closed gate: an oracle scenario marked pending cannot be reported as
passed. M0's smoke scenario is labelled infrastructure-only and is not an
oracle parity result. Failures from a driver remain errors and propagate to the
test caller.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| An empty shell is mistaken for a functioning editor | Empty-kernel snapshots and buffer reads are explicitly unavailable |
| Legacy tests silently pass without product behavior | Pending oracle cases reject a passing outcome; the smoke is marked non-parity |
| Test helper errors are swallowed | All driver operations and assertions return typed `Result`s |
| Adapter introduces a second product or side-effect boundary | Generic driver trait only; the M0 implementation records events in memory |
| Existing oracle changes during migration | Leave legacy harness/tests untouched and use M0-05 inventory IDs |

## Verification gate

1. A smoke journey dispatches key, text, logical-time, and settle actions through
   the adapter in exact order.
2. Empty-kernel screen/buffer assertions return an explicit unavailable error.
3. Driver errors propagate unchanged to the caller.
4. A pending oracle case cannot be recorded as passed; the infrastructure-only
   smoke cannot be counted as parity.
5. No product transitions, real-vault access, wall-clock time, threads, or
   legacy workspace changes are introduced.

The adapter implementation and empty-kernel smoke live in
`rewrite/crates/bloom-kernel/src/simulation/harness.rs`.
