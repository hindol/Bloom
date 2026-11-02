# M0-10 - Curated detector-fault corpus

> Status: implemented for the synthetic replay boundary. Product-state faults
> stay staged until their model and detector exist.

## Active fault

| Fault | Named detector | Status | Scope |
|---|---|---|---|
| Corrupt one replay's final synthetic file bytes | Replay determinism comparison | Active and caught | Empty-kernel simulation |
| Leave a timer due after an update | `check_no_due_armed_timers` / I10 | Active and caught | M1 kernel transition |

The test runs the same checked-in replay twice, injects a test-only mutation into
one candidate result, and requires the named comparison to observe the mismatch.
The injector lives only in the integration test; it is not compiled into the
library or exposed by the simulation feature.

## Staged product faults

| Fault | Required detector | Status |
|---|---|---|
| Accept a stale index result | `check_index_matches_versions` / I8 | Pending M5 |
| Skip vault-wide block-ID uniqueness validation | `check_unique_live_block_ids` / I4 | Pending M2 |
| Place a cursor inside a grapheme | `check_positions_are_valid` / I6 | Pending M2 |

These entries are registration of future sensitivity obligations, not passing
tests. They become active only when their owning model and executable detector
exist. A corpus runner must fail if an active fault survives; it must not
silently convert missing detectors into success.

An experimental, test-only I6 probe detects injected interior cursor offsets
on plain snapshots and checks valid boundary controls. It explicitly rejects
snapshots with tokens recognized by the prototype parser and out-of-bounds
offsets; it cannot detect tokens the parser misses. This restricted probe does
not activate the product detector; see the
[M2 risk review](M2_TEXT_MODEL_RISK_REVIEW.md).

The broader mutation campaign remains a future nightly concern. PR CI runs the
small deterministic sensitivity checks for the active milestones.
