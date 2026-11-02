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
| M1-03 | Minimal Vault Store load and single-page CAS save, with `PageSync` and typed conflict result | M1-02 | Rewrite hosted gate passed; legacy root Clippy and two baseline e2e failures remain | Simulated stale-writer and write-failure tests preserve disk bytes; real filesystem tests verify replace/readback behavior |
| M1-04 | Synthetic open/edit/save/crash/recover/replay journey with the external watchdog | M1-01, M1-02, M1-03 | Rewrite hosted gate passed; legacy root Clippy and two root e2e tests failed | Crash matrix validates I2; each page observation checks I5, each save checks I9, and every update checks I10/I11; watchdog outcomes remain distinct |
| M1-05 | GUI adapter paints kernel-provided layout using injected `FontMetrics`; bounded metadata recorder | M1-01, M1-04 | Implemented; local workspace tests and strict Clippy passed; first hosted Rewrite foundation failed because Linux Iced had no X11/Wayland backend enabled; corrected in M1-06 | Headless snapshots and geometry agreement checks; recorder tests prove content exclusion and size bounds |
| M1-06 | CI, platform filesystem checks, and docs-steward closure | M1-01 through M1-05 | Hosted run [41353605457](https://github.com/hindol/Bloom/actions/runs/41353605457) passed all Rewrite foundation gates on Linux, macOS, and Windows after widening the Windows timing-test tolerance. The legacy root Test/Clippy jobs still fail on the known baseline. | Rewrite gates pass on the supported OS matrix; M1 acceptance and pending parity/performance scope agree with code |

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

### Group-commit deadline test stability

The Windows rewrite job in
[run 41378159319](https://github.com/hindol/Bloom/actions/runs/41378159319)
failed when the continuous-submission test unconditionally unwrapped a
`QueueFull` response. Queue saturation is an explicit, nonblocking outcome, and
the documented durability bound assumes that the worker is scheduled and
storage completes; the log does not establish why the queue filled.

Exercise the actual worker's deadline decision with a test-supplied monotonic
clock, an in-memory SQLite connection, and queued submissions spaced less than
the idle delay. The first receipt must be acknowledged at the maximum deadline
while later submissions remain, before shutdown or the batch-size cap can force
a commit. Keep production on `Instant::now`; do not increase the maximum
deadline, queue capacity, or timing tolerance to make the test pass. Separately
check that saturation returns the exact unaccepted snapshot and that it can be
retried. Existing real-filesystem commit, acknowledgement, and crash tests
remain in place; a deterministic scheduling test is not a storage-latency
guarantee.

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

### M1-04 crash-recovery journey contract

- The test-only journey uses `SimInput` to open the synthetic page, insert
  literal text, advance logical time, flush background work, and explicitly
  save. Its recorded `SimInputEvent` sequence is replayed against a fresh
  synthetic vault and State database. This does not extend the M0 port replay
  format or claim GUI/oracle parity.
- Crash injection is scoped to child processes and explicit test hooks. Cover
  both sides of the State version transaction, temporary-file sync, durable
  replacement, readback, and saved-marker transaction. No production
  diagnostics or user content capture is added.
- Recovery expectations are boundary-specific: before the version commit,
  only the previous saved page is promised; after that commit and before
  replacement, the committed draft must remain a recovery candidate while disk
  stays unchanged; after replacement but before the saved-marker commit, the
  exact draft remains recoverable and disk is treated as an observed mismatch,
  not assumed saved; after the saved-marker commit, disk and State must agree
  and no recovery candidate remains. Orphan `.bloom-tmp` files are ignored.
- Every completed input/update checks I10 and I11. Every page observation
  checks I5. Each save checks I9 by preserving disk bytes whenever the
  observed base no longer matches. Restart checks I2, including SQLite
  integrity and untorn page bytes. Conflicts and failures remain explicit;
  neither is converted into success.
- The existing external watchdog remains outside each watched child and owns
  its time, step, queue, retained-document, and due-timer bounds. A deliberate
  child exit at a crash point is distinguished from timeout, malformed
  protocol, failure, and blocked outcomes.
- The test backend is infrastructure only. It must not add persistence,
  scheduling, filesystem access, or recovery policy to `bloom-kernel`, and it
  must not add real-vault access, UI behavior, or M2-M7 scope.

### M1-05 geometry and recorder contract

- `bloom-kernel` owns wrapping, positioned text runs, line height, and cursor
  geometry. Its layout input is the document snapshot, cursor, viewport width,
  and injected `FontMetrics`; the GUI receives the resulting layout and must
  not calculate a second wrap or cursor position.
- Font metrics are finite, non-negative logical-pixel values with a positive
  line height. The deterministic monospace implementation is used by headless
  tests. The GUI implementation and its paint style share the same font family,
  size, and shaping engine; layout tests compare the emitted text and cursor
  positions against the kernel frame rather than a GUI-side estimate.
- The paint adapter emits only primitives derived from `Frame`. A renderer
  target may paint those primitives, but it may not change their geometry.
  The Iced Canvas program consumes those primitives directly. Headless
  snapshots cover soft wrapping, hard newlines, empty lines, and cursor
  positions at wrap boundaries.
- Production metadata records are a closed typed schema: event kind,
  operation ID, version, elapsed time, and effect outcome only. Text, paths,
  clipboard data, and raw keystrokes are not accepted as fields or free-form
  error strings.
- The M1 recorder is process-local and in-memory; it performs no file or
  network I/O. It retains at most 512 fixed-size records and 16 KiB, evicts
  oldest records first, and exposes the eviction count so truncation is
  visible. It is not an exact replay log and must never be described as one.
- Initial model construction still shapes every physical line. Subsequent
  edits reuse line-relative wrap geometry, reshape only the affected line
  range, and materialize only viewport rows. The deterministic 10k-line test
  verifies one-line invalidation. The Windows local release harness reports
  single-editor p50 of 83.7 us and p99 of 119.7 us; hosted Linux run
  [41353605457](https://github.com/hindol/Bloom/actions/runs/41353605457)
  reports p50 of 89.688 us and p99 of 105.988 us, against the necessary 8 ms
  ceiling. Rewrite foundation gates passed on all three supported operating
  systems. These are single-editor kernel measurements, not a three-pane
  product-budget claim.

## M1-06 performance and platform closure

- The layout cache remains kernel-owned and single-writer. Each physical line
  stores reusable grapheme measurements and wrap geometry keyed by its content
  and viewport width. Text edits report the affected physical-line range so
  ordinary insert/backspace transitions invalidate only those entries; a
  split or merge may reindex line metadata, but must not reshape unchanged
  lines. A width change invalidates geometry because wrapping can change.
- Cached glyph positions are line-relative. Absolute character offsets and
  viewport coordinates are derived only for the visible frame, avoiding a
  whole-document copy or per-grapheme position rewrite after an edit. The
  kernel owns scroll positioning and keeps the cursor visible; the GUI still
  paints only the frame it receives.
- Add a deterministic work-count test proving a one-line edit in a 10k-line
  document reshapes only the affected line, plus a release-mode sampled
  event-to-frame measurement using the accepted fixture size. Report the
  single-editor result separately from the original three-pane product budget:
  panes do not exist in M1, so this milestone cannot claim the full product
  budget is met. Do not invent a cross-runner regression baseline.
- Run the rewrite workspace checks and the real `bloom-vault` filesystem tests
  on Windows, macOS, and Linux. The existing vault tests exercise the platform
  replacement path; unsupported or failed durability operations remain
  explicit failures. CI must not turn a platform-specific failure into a
  skipped success.
- The docs-steward closeout must distinguish the measured single-editor
  skeleton from deferred three-pane geometry, parser/index/search, and full
  oracle parity. It must keep M1 acceptance and the remaining M2-M7 scope in
  agreement with the code and the redesign proposal.

### Physical-line edit invalidation

The M1 rope currently uses Ropey's default physical-line boundaries, including
standalone CR and Unicode line separators in addition to LF and CRLF. Deleting
the character between a CR and LF can merge two old lines into one, so the
new cursor's line can be earlier than the deleted character's old line.
Inserting after a CR can likewise change the preceding line. An edit delta
must account for that left context in the old rope before mutation, then
measure the replacement range against the new rope. The unchanged physical
lines before and after the range must retain their exact bytes. Ordinary
single-line edits must still reshape only one line.

Full and cached layout must use the document's same physical-line boundaries.
This fix preserves the current M1 `line_text` behavior and exact document bytes;
it does not decide the eventual product treatment of standalone CR or other
Unicode separators, mixed line endings, or grapheme-safe cursor landing.
In particular, M1 currently strips LF and CRLF from line text but retains the
other recognized separator characters there. Those limitations and the
separate M2 I6 cursor blocker remain visible rather than being presented as
an accepted text-format policy.

Exercise insertions and backspaces around CR, LF, CRLF, Unicode separators,
empty lines, and EOF against an independent physical-line reference. Compare
incremental frames with full layout, include the kernel-level CRLF regression,
and retain the 10k-line single-invalidation check.
