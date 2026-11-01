# M0-03 - Typed coordinates and injected ports

> Status: architect risk review and implementation constraints. This task adds
> type boundaries and port contracts only; it does not implement M1 behavior.

## Coordinate ownership

`bloom-text` owns `CharIdx`, `ByteIdx`, `LineIdx`, and `GraphemeIdx`.
`bloom-kernel` owns `DisplayCol` and `Px`, because display columns and pixel
geometry belong to the kernel's layout authority (L8), not to text storage.
Each is a distinct public type with a private representation. No public raw
constructor, `From<usize>`, `Into<usize>`, or cross-unit arithmetic is allowed.
The value may be inspected only by code in its owning crate.

Construction is delegated to the model that can establish the invariant:
`Doc` converts between byte, character, line, and grapheme positions;
`Layout` converts text positions to display columns and pixel geometry using
injected font metrics. M0-03 only establishes opaque types. It does not invent
conversion behavior before `Doc` and `Layout` exist.

Indices are zero-based. `CharIdx` counts Unicode scalar values, not bytes or
grapheme clusters. `GraphemeIdx` counts extended grapheme clusters. A
`ByteIdx` is valid only at a UTF-8 boundary. `DisplayCol` is a layout result,
not a synonym for character count or `unicode-width`; `Px` is a finite,
non-negative logical-pixel measure. Future constructors must validate those
conditions rather than silently clamp or reinterpret values.

## Port contracts

The port traits live in `bloom-kernel`, while production adapters live outside
the kernel. The pure transition function does not call the filesystem, watcher,
clock, or RNG: the shell owns those resources, injects time/random data, and
turns asynchronous effect outcomes into explicit events.

| Port | Ownership and contract |
|---|---|
| `Fs` | A vault adapter owns it mutably. Reads and writes return `Result`; errors are never translated into empty content or success. This is a low-level boundary only; CAS, transaction, durability, and merge protocols remain M1/M4 work. |
| `Clock` | The shell owns the clock and passes its value into transitions. The trait exposes a comparable `Copy` instant; it never exposes `Instant::now()` or another ambient wall-clock read to the kernel. A simulator can use a deterministic logical instant. |
| `Watcher` | A watcher adapter owns it mutably. `Ok(None)` means no event is currently available; closure, overflow, and OS errors must be returned as errors, not treated as an empty queue. |
| `Rng` | The shell owns a mutable source and requests bytes explicitly. Entropy failure is returned to the caller; no time-based or deterministic production fallback is allowed. |

Stateful ports require exclusive `&mut self` access, making ownership and
thread-safety the adapter's responsibility. The kernel remains single-owner;
no port trait implies that it may be shared or called from the UI hot path.
This keeps blocking filesystem and watcher work in workers and preserves the
typing-latency constraint.

## Risks and mitigations

| Risk | Mitigation |
|---|---|
| A raw integer crosses a unit boundary unnoticed | Private representations, no public conversion shortcut, and compile-fail examples for mixed-unit use and construction |
| UTF-8 bytes are mistaken for character/grapheme positions | State the distinct counting rules; permit conversions only through `Doc` |
| Display-width calculation disagrees with painted geometry | `DisplayCol`/`Px` are produced by `Layout` with the same injected metrics used by the GUI |
| A fake or adapter hides an I/O failure | All fallible ports return `Result`; `None` is reserved for "no watcher event now"; no default-success implementation |
| Clock or RNG makes tests nondeterministic | Inject values/source explicitly; no ambient access in the kernel; fake ports can return fixed values or explicit errors |
| Blocking I/O leaks onto the event loop | Ports are adapter-owned and must be executed outside the kernel; asynchronous outcomes return as events |

## Verification gate for M0-03

1. Mixed units and public raw construction fail to compile in documentation
   compile-fail tests.
2. The rewrite workspace compiles with all six opaque coordinate types and the
   four port traits.
3. Test fakes demonstrate deterministic time and random bytes, and explicit
   filesystem/watcher/random failures without production I/O.
4. No M1 storage protocol, text conversion, timer wheel, or effect executor is
   introduced by this task.
