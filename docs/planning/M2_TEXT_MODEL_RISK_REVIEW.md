# M2 Text Model Risk Review

> Architect review for the ADR-3 acceptance spike.
> Scope is the spike only; this does not approve full M2 implementation.

## Decision under test

ADR-3 keeps block-ID tokens in canonical rope text and derives a visible
coordinate projection from parsed token spans. Every visible edit is translated
through one `lower()` boundary. Registers retain typed text and ID provenance so
moving a block preserves its identity while copying it creates a fresh one.

The spike must test this design before ADR-3 is accepted. If hidden-token
handling leaks into motions or edits, use fallback (c) from the redesign and
record the failing cases. Do not silently relax the acceptance gate.

## Scope and ownership

- `bloom-text` owns canonical text, parsed ID-token spans, and projection
  snapshots. Mutable document state has one writer. Readers receive snapshots;
  no second mutable identity table is introduced as a source of truth.
- The projection is derived data, never persisted independently. Canonical
  serialization is the exact rope text, including IDs.
- The spike is in-memory and must not change the vault format, write files, or
  introduce shadow-dogfooding access. Production ID allocation must later use
  the durable State registry; the spike uses a deterministic test port and
  does not claim production uniqueness.
- Keep the legacy application and its `bloom-vim` crate unchanged as behavioral
  oracles. A test-only adapter may present projected text through the legacy
  `Buffer` API and lower its visible `EditOp` ranges into the prototype. That
  adapter must not become a second production mutation path.

## Risks and required controls

| Risk | Failure mode | Required control |
|---|---|---|
| Projection drift | Cursor, selection, or edit lands inside a hidden token or splits a grapheme; one visible range crosses several raw token gaps | Distinct visible/raw coordinate types; one checked lowering API that returns segmented raw edits plus explicit token effects; round-trip and boundary properties |
| Parser disagreement | A token is hidden by rendering but still editable, or a literal/code sample is hidden as structure | Token spans come from one parser with context; test code fences, inline code, malformed markers, links, trailing spaces, and end-of-file cases |
| Legacy grammar narrowing | Existing IDs stop parsing after the generator changes to seven characters | Preserve the current read grammar during the spike. The legacy parser accepts alphanumeric, `-`, and `_` IDs and recognizes `^=` markers; the new generator's seven-character base36 format must not narrow reads |
| Mirror transition | Existing `^=id` content is treated as a second live identity or rewritten before the migration | Parse and preserve legacy mirror markers as legacy tokens; do not create or propagate mirrors in the new model; defer their conversion to the approved M7 migration |
| Register provenance loss | `dd`+`p` duplicates an ID or `yy`+`p` steals the source ID | Typed register payload records source identity and operation provenance; paste consults an explicit liveness port rather than assuming a document-local answer |
| Cross-page liveness | A local document incorrectly concludes that a vault-wide ID is unused | Keep liveness/reservation behind a State-owned port; test with a deterministic multi-page registry |
| Partial structural edit | Allocation, parsing, or range validation fails after text has already changed | Produce a complete edit plan first; apply atomically or return an explicit error with the original document unchanged |
| Undo/redo identity drift | Undo removes an ID and redo invents a different one, or an undone ID is reused | Include IDs in the text history snapshot; reserve generated IDs monotonically; assert byte-identical undo/redo restoration |
| Legacy Vim integration gap | Projection tests pass but existing key handling still sends raw coordinates | Exercise the actual legacy `VimState` through a test-only projection adapter and run the existing ID-bearing scenarios; do not declare ADR-3 accepted if this boundary is untested |
| Performance and stale snapshots | Full-document remapping or stale spans enter the keypress path | Start with correctness-first recomputation in the spike; production M2 must use the single incremental parser and prove unaffected lines remain valid |

The read-grammar concern is evidenced by the current parser: `parse_block_id`
accepts alphanumeric, hyphen, and underscore characters and recognizes `^=`
markers (`crates/bloom-md/src/parser/extensions.rs:103`), while
`LineElements::split_block_id` only strips exactly five lowercase alphanumeric
characters (`crates/bloom-md/src/parser/extensions.rs:373`). The spike must
make the supported read grammar explicit rather than reproduce this mismatch.

## Identity-rule test matrix

Each rule in ADR-3's identity table needs a direct test, including:

- `dd` then `p` preserves the ID only when the register's source identity is
  no longer live; `yy` then `p` allocates a fresh ID while the source remains
  live.
- `J` keeps the first block ID and emits an explicit retirement effect for the
  second; allocation or retirement failures leave text unchanged.
- Splitting a block keeps the original ID on the left and allocates one fresh
  ID for the right, with undo/redo restoring the exact token.
- `x`, `X`, and `D` at both token boundaries affect visible text only.
- A visible range crossing one or more hidden tokens cannot become a single
  destructive raw range by accident. The lowering result must preserve token
  spans unless the operation's identity rule explicitly moves or retires them.
- Visual-block ranges, substitutions, and `.` repeat lower through the same
  projection and re-apply the identity rules. If the legacy Vim engine does not
  expose one of these operations, mark that rule uncovered rather than passing
  it by assumption.
- Unicode graphemes, CRLF line boundaries, empty lines, trailing newlines,
  multiple blocks, malformed IDs, code samples, and `^=` legacy markers do not
  create ambiguous positions or change canonical bytes on a no-op.

The registry used by tests must model page scope and retirement, not only a
set of IDs in the current document. New IDs are generated inside the same
planned edit as the text change; a reservation failure must not partially
mutate the rope.

## Acceptance gate

ADR-3 can be accepted only when:

1. The projection and all identity rules have executable tests, including
   property tests for visible-to-raw range lowering and canonical round trips.
2. The existing Vim unit suite and ID-bearing e2e scenarios pass without
   changing their expected behavior. At baseline, `cargo test -p bloom-vim`
   passes 42 tests, and the two hidden block-ID append scenarios pass. The
   current Vim API is coupled to `bloom_buffer::Buffer` and untyped
   `EditOp { range: Range<usize>, ... }`; the test adapter must explicitly
   cover that seam.
3. Every supported identity rule has a test or is explicitly recorded as
   uncovered; no rule is inferred from the current side-table implementation.
4. The verdict and any remaining gaps are recorded. If the gate fails, prototype
   fallback (c) before continuing with full M2.

No product or user-facing docs change is in scope for the spike. The legacy
product docs remain authoritative for current behavior until the migration
changes it.
