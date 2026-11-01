# M0-11 - Performance harness and budget gates

> Status: implemented for synthetic-only M0 measurements. No product latency
> budget is claimed as measured or met.

## Decision

Use Criterion for a repeatable empty-kernel replay smoke benchmark over the
checked-in synthetic fixture. Configure sample count and warmup/measurement
duration explicitly in CI. The fixture and operation sequence stay fixed so a
measurement is comparable to another run of the same benchmark.

The regression policy is strict: a candidate more than 10% slower than its
accepted baseline fails; exactly 10% is within policy. Baseline and candidate
must use the same OS/runner class, Rust toolchain, benchmark source, fixture,
and Criterion configuration. Until a trustworthy per-environment baseline is
approved, M0 runs a real smoke measurement and tests the comparison gate with
known samples; it does not invent a zero or hardware-independent timing
baseline.

Criterion estimates are not a substitute for the redesign's p99 budgets.
Product budgets remain registered but pending until the relevant operations
and representative fixtures exist. In particular, keypress-to-frame latency
needs sampled end-to-end frame timings, not throughput; open/search/index
budgets need their own synthetic vault sizes and warm/cold state definitions.

## Registered measurements

| Operation | Fixture / method | Status |
|---|---|---|
| Empty-kernel replay smoke | Checked-in replay, Criterion repeated sample | Active measurement; no regression baseline yet |
| Keypress to frame p99 | 10k-line page, three panes, sampled event-to-frame latency | Pending product model/UI |
| Open page | 10k-line page, cold/warm state specified per run | Pending M1/M2 |
| Search | 10k-page synthetic vault, sampled query latency | Pending index milestone |
| Cold start to first frame | 10k-page vault with explicitly warm cache | Pending product startup |
| External-change index convergence | 100 synthetic page changes without blocking input | Pending vault/index model |

## Verification gate

1. CI performs an actual Criterion measurement on the deterministic smoke
   fixture with explicit warmup, sample count, and measurement duration.
2. The regression comparator accepts a 10% delta and rejects a delta greater
   than 10%; invalid zero baselines fail explicitly.
3. Product p99 budgets remain pending and are never represented by fabricated
   zero-duration measurements.

The Criterion benchmark is `rewrite/crates/bloom-kernel/benches/simulation_smoke.rs`;
the deterministic >10% gate checks are in
`rewrite/crates/bloom-kernel/tests/performance_gate.rs`.
