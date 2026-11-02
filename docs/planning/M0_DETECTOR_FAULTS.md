# M0-10 - Curated detector-fault corpus

> Status: implemented for the synthetic replay boundary. Product-state faults
> stay staged until their model and detector exist; the M1 plain-cursor I6
> detector is active.

## Active fault

| Fault | Named detector | Status | Scope |
|---|---|---|---|
| Corrupt one replay's final synthetic file bytes | Replay determinism comparison | Active and caught | Empty-kernel simulation |
| Leave a timer due after an update | `check_no_due_armed_timers` / I10 | Active and caught | M1 kernel transition |
| Place a cursor inside a grapheme | `check_cursor_grapheme_boundary` / I6 | Active and caught | M1 plain raw cursor |

The test runs the same checked-in replay twice, injects a test-only mutation into
one candidate result, and requires the named comparison to observe the mismatch.
The injector lives only in the integration test; it is not compiled into the
library or exposed by the simulation feature.

## Staged product faults

| Fault | Required detector | Status |
|---|---|---|
| Accept a stale index result | `check_index_matches_versions` / I8 | Pending M5 |
| Skip vault-wide block-ID uniqueness validation | `check_unique_live_block_ids` / I4 | Pending M2 |

These entries are registration of future sensitivity obligations, not passing
tests. They become active only when their owning model and executable detector
exist. A corpus runner must fail if an active fault survives; it must not
silently convert missing detectors into success.

The I6 fault check exercises the same grapheme-boundary checker registered
after every M1 kernel update. Its test-only probe injects interior offsets into
plain snapshots and checks valid boundary controls; it rejects snapshots with
tokens recognized by the prototype parser and out-of-bounds offsets. It does
not validate projected positions, selections, or motions; those remain in the
[M2 risk review](M2_TEXT_MODEL_RISK_REVIEW.md).

The broader mutation campaign remains a future nightly concern. PR CI runs the
small deterministic sensitivity checks for the active milestones.
