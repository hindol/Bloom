# M0-06 - Deterministic simulation and synthetic replay

> Status: implemented for M0-06. This task adds deterministic test support only;
> it does not implement M1 persistence or product transitions.

## Decision

Keep the simulator in `bloom-kernel::simulation`, gated behind the `simulation`
feature. Do not add an eighth workspace crate: the simulator implements the
kernel-owned ports, while its optional feature keeps fake infrastructure out
of normal builds. It uses only memory and logical time; production filesystem,
watcher, clock, and entropy adapters remain outside the kernel.

`SimFs` stores bytes in an ordered in-memory map and supports explicit
one-operation read/write failures. `SimClock` is a monotonic `u64` logical tick
counter with checked advancement. `SimWatcher` is a FIFO event queue; repeated
or reordered notifications are represented literally in the event sequence,
and polling errors are distinct from an empty queue. `SeededRng` is a
deterministic, non-cryptographic source used only by simulation.

The replay format is a strict, versioned UTF-8 line format with the
`BLOOM-SIM-REPLAY 1` header. It records a seed, starting logical time, initial
synthetic files, and ordered simulation steps. Paths and byte contents are
lowercase hexadecimal fields, so tabs, newlines, and arbitrary UTF-8 file
contents cannot change record boundaries. Unknown versions, malformed records,
invalid paths, and duplicate initial paths are errors; records are never
silently skipped.

Each replay yields one typed observation per step, final in-memory files and
logical time, plus stable failure signatures `(format version, zero-based
step, error code)`. Signatures exclude paths, content, OS messages, and wall
time. `ReplayCase::encode` provides a portable counterexample artifact; checked
synthetic examples live under `rewrite/testdata/replays/`. A real-vault
diagnostic capture format is separate and is not introduced here.

M0 has no product model or kernel transition to drive. Its smoke replay
therefore exercises deterministic port behavior and replay fidelity only; it
does not claim product parity. The empty applicable-invariant set is explicit.
M0-07 supplies the `SimInput`-compatible adapter boundary in
[`M0_SIMINPUT_ADAPTER.md`](M0_SIMINPUT_ADAPTER.md); M1 connects applicable
invariant checkers to real `update` transitions.

## Limits and failure semantics

- Every port operation remains fallible where its trait is fallible. Missing
  files, injected failures, watcher emptiness, watcher errors, and logical-time
  overflow have distinct outcomes.
- Fixture order is preserved for event and failure injection. The filesystem
  uses sorted final-state output so replay comparisons are stable.
- Replay input is synthetic and local. The format is not a request to capture
  note contents, filenames, clipboard data, or raw keystrokes from production.
- The simulator does not spawn threads, sleep, access the host filesystem, or
  read ambient time. Bounded watchdog behavior is M0-08.

## Verification gate

1. Identical encoded fixture/seed/event log produces identical observations,
   failure signatures, final files, and logical time.
2. Read/write failures are surfaced, duplicate/reordered watcher events replay
   in the recorded order, and watcher errors differ from an empty poll.
3. Unsupported format versions and malformed records fail explicitly.
4. A checked-in synthetic smoke fixture runs through the empty-kernel boundary;
   no real vault or product persistence is involved.
5. No threaded scheduler, product transition, durable storage protocol, or
   production telemetry/capture is added.
