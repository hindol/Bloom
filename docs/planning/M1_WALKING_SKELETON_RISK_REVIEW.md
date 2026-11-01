# M1 Walking Skeleton - Architect Risk Review

> Status: pre-implementation risk review complete; M1 end-to-end product
> behavior is not yet implemented. This review constrains the accepted M1 row in
> [FIRST_PRINCIPLES_REDESIGN.md](FIRST_PRINCIPLES_REDESIGN.md); it does not
> change product scope or the accepted ADRs.

## Goal and boundaries

Retire integration risk with one synthetic-vault journey: open a page, edit it,
save it, terminate the process at controlled persistence boundaries, recover,
and replay the recorded journey. The real vault remains out of scope until M7.

M1 includes the pure `update`/`settle`/`view` kernel, a consuming `TimerWheel`,
an editor-only focus stack, plain rope text, the SQLite State store, a minimal
single-page Vault Store, the GUI event/effect adapter, and a bounded local
metadata recorder. It does not include Markdown parsing, block IDs, Vim
parity, indexing, three-way merge, multi-page transactions, MCP, migration,
or product parity claims.

## Ownership constraints

| Boundary | Owns | Must not do |
|---|---|---|
| `bloom-kernel` | Model, events, effects, transitions, focus, timer state, invariant checks | Read wall-clock time, access files or SQLite, own threads, or render through GUI APIs |
| `bloom-text` | Plain rope and text operations required by the skeleton | Own vault paths, persistence, or editor orchestration |
| `bloom-state` | `state.db`, schema versioning, version records, group commit, recovery metadata | Write Markdown files or route input |
| `bloom-vault` | Synthetic page load, observed base hash, single-page CAS save and typed conflict result | Bypass State, overwrite a changed file, or silently resolve a conflict |
| `bloom-gui` | Translate OS input/time and execute kernel effects; paint supplied frames | Add a second geometry authority or persistence policy |
| Test harness | Synthetic fixtures, process supervision, crash injection, replay assertions | Treat an unavailable screen/buffer or pending oracle case as a pass |

The kernel receives time as data. `update` drains all timers due at or before
that time before handling the waking event, and removes a fired timer before
dispatching its handler. Each completed update is checked for a due timer;
the external watchdog remains outside the process it observes.

## Persistence and failure invariants

1. **State precedes Vault.** Persist version/recovery data in a SQLite
   transaction before applying the corresponding file write. A saved marker is
   recorded only after the Vault Store confirms the write.
2. **Distinguish acceptance from durability.** The UI may accept an edit without
   blocking on `fsync`, but it must not describe it as durably committed until
   the State store reports commit. Preserve the accepted idle group-commit
   behavior and the maximum two-second loss bound; explicit save forces a
   synchronous commit before the CAS write.
3. **Never overwrite an observed external change.** A save compares disk with
   the last observed base. A mismatch returns a typed conflict and leaves disk
   unchanged; merge and conflict UI are deferred to M4.
4. **No torn vault file.** Create a unique same-directory temporary file with
   an ignored `.bloom-tmp` suffix, write and sync its contents, replace the
   target atomically, and read back to verify. On Unix, sync the parent
   directory after replacement. Windows has no documented directory-flush
   equivalent: use `MoveFileExW` with `MOVEFILE_REPLACE_EXISTING |
   MOVEFILE_WRITE_THROUGH` for the same-directory replacement instead. If
   replacement or its platform durability operation fails, surface the error;
   never fall back to deleting the target before renaming. Windows sharing or
   filesystem restrictions that prevent replacement are explicit save
   failures, not permission to weaken atomicity.
5. **Recover at every boundary.** Inject termination before and after the
   State commit, temporary-file flush, replace, readback, and saved-marker
   commit. On restart, recover a committed version or report a conflict; never
   infer success from process exit or a partially written temp file.
6. **Keep diagnostics private and bounded.** Record event kinds, operation
   identifiers, versions, timings, and effect outcomes only. Do not record
   note text, filenames, clipboard data, or raw keystrokes. Enforce explicit
   event/byte ceilings and local-only storage.

## State-store implementation constraints

- One worker thread owns the SQLite connection for its entire lifetime. The
  kernel and callers submit immutable version snapshots through a bounded,
  non-blocking queue; queue saturation returns the unaccepted snapshot with an
  explicit error, never a hidden wait or discarded edit on the input path.
  The worker also caps each pending batch and commits at the cap, bounding
  queued version count even during sustained input.
- SQLite uses WAL with `synchronous=FULL`, foreign keys, and an explicit
  `user_version`. Migrations run transactionally. A newer schema or an
  unversioned database containing unknown tables is rejected; startup must
  never "recover" by dropping or recreating user state.
- A version receipt means accepted for persistence, not durable. Its separate
  commit acknowledgement is delivered only after the transaction commits.
  Normal edits group-commit after 300 ms idle, with a 2 s maximum from the
  first accepted pending edit. That loss bound assumes a scheduled process
  and storage that completes the transaction within the interval; OS or
  device stalls can delay durability. Any commit error fails affected receipts
  and must be surfaced as degraded durability. A forced barrier commits pending
  versions before an explicit save can proceed.
- M1 stores each submitted snapshot as a full keyframe and supports the
  `Saved` version tag needed for restart recovery. Recovery reports the latest
  version newer than the saved tag; it never writes a vault file or marks a
  version saved on its own. Delta encoding and other tag kinds remain later
  work.
- Process-isolated crash tests terminate the writer immediately before and
  after SQLite commit. Restart must expose only committed versions, and
  `integrity_check` must remain clean in both cases. Commit failures are
  reported to every affected receipt; the worker does not acknowledge a
  success-shaped fallback.

## Bounded implementation sequence

Dependencies are real prerequisites; tasks may proceed in parallel only where
the table shows no dependency.

| ID | Deliverable | Depends on | Status | Exit check |
|---|---|---|---|---|
| M1-01 | Kernel model with `update`/`settle`/`view`, editor-only focus, and consuming `TimerWheel`; plain rope boundary | M0 | Rewrite hosted gate passed; legacy root Clippy and two baseline e2e failures remain | Deterministic transition tests cover due timers on every event path, timer replacement/cancellation, exactly-one focus owner, and pure repeatable views |
| M1-02 | Minimal `state.db` schema (`WAL`, `user_version`), version records, group-commit worker and startup recovery | M1-01 | Rewrite hosted gate passed; legacy root Clippy and two baseline e2e failures remain | Fresh and migrated schema tests; commit/ack ordering; crash injection demonstrates I2 and no corrupted database |
| M1-03 | Minimal Vault Store load and single-page CAS save, with `PageSync` and typed conflict result | M1-02 | Implemented locally; hosted verification pending | Simulated stale-writer and write-failure tests preserve disk bytes; real filesystem tests verify replace/readback behavior |
| M1-04 | Synthetic open/edit/save/crash/recover/replay journey with the external watchdog | M1-01, M1-02, M1-03 | Pending | Process-isolated replay checks I2, I5, I9, I10, I11 at every transition and reports blocked/failure distinctly |
| M1-05 | GUI adapter paints kernel-provided layout using injected `FontMetrics`; bounded metadata recorder | M1-01, M1-04 | Pending | Headless snapshot and geometry agreement checks; recorder tests prove content exclusion and size bounds |
| M1-06 | CI, platform filesystem checks, and docs-steward closure | M1-01 through M1-05 | Pending | Rewrite gates pass on the supported OS matrix; M1 acceptance and pending parity/performance scope agree with code |

## Risks to retire before M1 exit

| Risk | Required mitigation |
|---|---|
| Timer ordering regresses into a busy loop or skips a due timer | Deterministic clock tests, I10 checker after every update, the staged due-timer detector fault, and an external watchdog |
| State commit and Vault write disagree after a crash | Single SQLite durability point, explicit saved marker, fault injection at each boundary, and restart/replay assertions |
| `PageSync` or CAS accepts stale state | Derive dirtiness from versions; compare against last observed disk hash; test external writes and stale acknowledgements |
| OS replace/flush semantics differ | Use Unix parent-directory sync and Windows write-through replacement; run real-filesystem tests on Windows, macOS, and Linux. If a platform cannot complete its documented operation, fail the save and gate rather than silently weakening the guarantee |
| GUI and kernel geometry diverge | Inject the same shaping implementation used for painting; compare kernel positions with rendered glyph positions |
| Recorder becomes a content leak or unbounded queue | Content-free typed records, explicit byte/event ceilings, and no raw text or keystrokes |
| Walking skeleton expands into premature parity work | Keep all M2-M6 features and real-vault access outside the milestone; pending oracle scenarios remain pending |

## M1 exit gate

M1 is complete only when the synthetic journey can open, edit, save, terminate,
recover, and replay; I2, I5, I9, I10, and I11 checkers run at the relevant
boundaries; the external progress watchdog detects a stalled journey; the
key-press budget is measured against its accepted fixture; and platform
filesystem checks pass. No result from the empty kernel alone counts as product
parity or as a product performance measurement.

### M1-03 filesystem contract

- The Vault Store accepts only a relative path with normal components. It
  canonicalizes the existing page and rejects a resolved target outside the
  canonical synthetic-vault root. Load and save require exclusive mutable
  access to one Vault Store instance; `PageSync` state cannot be moved to a
  second instance. Save re-resolves the page and rejects path redirection
  before reading or writing. Page creation, deletion, and rename reconciliation
  are outside M1-03.
- Each loaded page retains its observed content hash as the compare-and-swap
  base. The State saved marker is checked against the loaded bytes; if they
  differ, load records an external-change conflict rather than treating the
  page as clean. A missing page or changed hash is a typed conflict; the store
  does not create a missing page or overwrite an observed external edit.
- Save first asks State for the exact version record, with a commit barrier,
  and writes that record's bytes rather than caller-supplied text. State's
  `Saved` marker advances only after replacement, the platform durability
  operation, and readback all succeed. If the marker commit then fails, the
  file remains written but the API reports that metadata failure explicitly.
- Temporary files are uniquely created beside the target and are removed on
  pre-replacement failure when cleanup succeeds. After replacement, failures
  are reported as post-replacement failures; the caller must not treat them as
  a successful save or blindly retry against the old base.
- The Unix guarantee is file sync, atomic same-directory replacement, parent
  directory sync, and readback. The Windows guarantee is file sync, native
  same-directory replacement with `MOVEFILE_WRITE_THROUGH`, and readback.
  Windows does not claim a POSIX directory `fsync`; filesystems that reject
  the documented replacement operation fail closed. No result claims to
  eliminate the documented compare/read-to-replace race.
