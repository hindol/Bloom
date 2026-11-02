# M0-04 - Invariant checker framework

> Status: architect risk review and implementation constraints. This task adds
> a checker framework and coverage map only; product models and invariant
> implementations activate in their owning milestones.

## Decision

The kernel owns the invariant identifiers, violation vocabulary, checker
registry, and coverage metadata. A checker is registered for one invariant and
is a read-only closure over the model state it needs. It returns an explanatory
message on failure; the registry attaches the registered invariant ID, so a
checker cannot report a different invariant accidentally.

The caller supplies the set of invariants applicable to the current model
surface. The registry rejects duplicate registrations, duplicate applicability
IDs, and any applicable invariant without a registered checker. It runs the
applicable checkers in stable ID order and returns every violation rather than
stopping at the first one. This makes an omitted checker visible instead of
turning an unimplemented property into `Ok(())`.

The same registry entry point is used after updates in debug builds, by tests,
and after simulator steps. The empty M0 kernel has no product model to wire into
an `update` hook; M1 must call this entry point at the update boundary, while
tests and later simulation use it directly. No checker performs I/O, reads a
clock, mutates model state, or runs asynchronously.

## Coverage activation

The coverage map names the owning module, intended checker, primary test, and
first activation milestone for every invariant in §7 of the redesign proposal.
An entry remains staged until its model surface exists. These names are
implementation targets, not claims that a checker or product behavior already
exists.

| ID | Invariant | Owner | Checker | Test | Activation |
|---|---|---|---|---|---|
| I1 | Round-trip valid unedited text | `bloom-text::TextProps` | `check_round_trip` | `text_props_round_trip` | M2 |
| I2 | Bounded crash loss and untorn files | `bloom-state::TxnJournal` | `check_bounded_loss` | `crash_recovery_simulation` | M1 |
| I3 | Page ID and path key are bijective | `bloom-vault::VaultStore` | `check_page_path_bijection` | `page_path_bijection` | M1 |
| I4 | Live block IDs are vault-wide unique | `bloom-text::BlockId` | `check_unique_live_block_ids` | `unique_live_block_ids` | M2 |
| I5 | Clean page matches observed disk base | `bloom-vault::PageSync` | `check_clean_page_matches_base` | `page_sync_external_write` | M1 |
| I6 | Positions are valid grapheme boundaries | `bloom-text::Doc` | `check_cursor_grapheme_boundary` | `active_grapheme_detector_catches_injected_cursor_faults` | M1 raw cursor; M2 projected positions |
| I7 | Exactly one component owns input focus | `bloom-kernel::Ui` | `check_exactly_one_focus_owner` | `focus_stack_ownership` | M1 |
| I8 | Quiescent index equals latest projection | `bloom-index::Projection` | `check_index_matches_versions` | `index_full_rebuild_differential` | M5 |
| I9 | Writes preserve unmerged observed disk data | `bloom-vault::CasWrite` | `check_write_respects_observed_base` | `external_writer_cas` | M4 |
| I10 | No armed timer is due after update | `bloom-kernel::TimerWheel` | `check_no_due_armed_timers` | `due_timer_is_consumed` | M1 |
| I11 | View is pure for the same model | `bloom-kernel::View` | `check_view_purity` | `repeated_view_is_identical` | M1 |
| I12 | Undo then redo restores identical text and IDs | `bloom-text::Undo` | `check_undo_redo_identity` | `undo_redo_identity` | M2 |

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| A missing checker is mistaken for a passing invariant | Callers declare applicable IDs; the registry returns an explicit missing-checker error |
| A checker reports the wrong invariant | The registry, not the closure, supplies the invariant ID on each violation |
| Debug checks change editor state or block the hot path | Checker closures are immutable and synchronous; expensive randomized campaigns remain in test/simulation |
| The human coverage table drifts from executable metadata | A kernel test compares all 12 IDs and every map field with the checked-in table |
| Empty M0 model appears to prove product invariants | All product checks are staged with an activation milestone; M0 tests only framework behavior and synthetic violations |
| Future checker activation silently skips existing update paths | M1's `update` boundary must pass its applicable ID set to the shared registry hook; CI tests missing-checker failure |

## Verification gate

1. The coverage metadata contains each I1-I12 exactly once and has non-empty
   owner, checker, test, and activation fields.
2. The docs-steward test fails if an invariant row is missing or differs from
   the executable coverage metadata.
3. A synthetic checker violation is returned with its registered invariant ID.
4. An applicable invariant without a checker and duplicate registrations are
   explicit errors.
5. No product model, state transition, persistence behavior, or M1 `update`
   implementation is introduced here.
