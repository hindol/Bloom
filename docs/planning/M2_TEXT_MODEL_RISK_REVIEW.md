# M2 Text Model Risk Review

> Architect review for the ADR-3 acceptance spike.
> Scope is the spike only; this does not approve full M2 implementation.
> Status: projection lowering and fixture-backed append, join, split, and boundary-edit prototypes pass; ADR-3 remains unaccepted pending the identity-rule matrix.

## Decision under test

ADR-3 keeps block-ID tokens in canonical rope text and derives a visible
coordinate projection from parsed token spans. Every visible edit is translated
through one `lower()` boundary. Registers retain typed text and ID provenance so
moving a block preserves its identity while copying it creates a fresh one.

The spike must test this design before ADR-3 is accepted. If hidden-token
handling leaks into motions or edits, use fallback (c) from the redesign and
record the failing cases. Do not silently relax the acceptance gate.

## Workspace-isolated legacy test bridge

The M0 crate-boundary decision forbids dependencies in either direction
between the legacy workspace and `rewrite/`. Preserve that boundary: the
adapter communicates through the versioned text fixture
`testdata/adr3_vim_oracle.v1`, not a Cargo dependency. Legacy `VimState` tests
run the real key sequence, apply its `EditOp`s to a visible `Buffer`, and
assert that both the resulting visible text and the serialized edit stream
match the fixture. `bloom-text` consumes the same fixture and lowers those
visible edits through a fresh projection, asserting canonical text and token
identity afterward.

The append fixture covers ordinary and legacy mirror IDs. A second versioned
fixture, `testdata/adr3_vim_identity_oracle.v1`, now records the real `dd`+`p`,
`yy`+`p`, `J`, insert-newline split, and `x`/`X`/`D` edit streams against
ID-bearing input. The legacy Vim tests
execute those key sequences; rewrite tests consume the `x`/`X`/`D` cases and
lower their actual ranges while checking that IDs survive. The identity-policy
tests separately exercise registry decisions for move, copy, join, and split.

The fixture exposed an existing legacy behavior mismatch: `p` pastes the
linewise register at a character position (for example, `yy`+`p` yields
`oone\nne\nnext\n`), rather than inserting a line. Treat this as observed
legacy behavior, not as evidence that the ADR-3 move/copy rules are
implemented. Kernel prototype tests now consume the fixture's actual `J` and
split edit ranges, build canonical text plans, and atomically commit those
plans alongside retirement/reservation changes. These tests cover the
prototype's join/split canonical-text transaction, not a production
`VimState`-to-document integration. The structural `dd`/`p` and `yy`/`p`
streams cannot serve as transaction oracles for the intended identity
semantics; the bridge does not authorize ADR-3.

The unchanged legacy CI baseline also has two reproducible e2e failures:
`multiple_undo_redo` observes `onetwo three ` where its setup expects
`one two three`, and `theme_persists_to_config_without_corrupting_views`
expects the literal name of a default view that is omitted from the migrated
default configuration. Root Clippy also reports three warnings in the legacy
undo implementation. These failures are visible in
[run 41370494835](https://github.com/hindol/Bloom/actions/runs/41370494835)
and reproduce locally. They are separate from rewrite acceptance. The oracle
preservation constraints below do not authorize changing legacy tests, source,
or CI gates to make this baseline green; resolve that conflict with the human
before altering the oracle.

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
| Detached register discarded | An ID remains detached forever when its final move-register alias is replaced or cleared | Retire the matching detached ID only when its final owner is cleared; verify an already-moved ID remains live and unrelated pages cannot be retired |
| Stale delete-register alias | An old register claims or retires a later detachment of the same ID from the same page | Associate each detach with a unique generation, and require paste and discard to match it; page identity alone is insufficient |
| Cross-page liveness | A local document incorrectly concludes that a vault-wide ID is unused | Keep liveness/reservation behind a State-owned port; test with a deterministic multi-page registry |
| Partial structural edit | Allocation, parsing, or range validation fails after text has already changed | Produce a complete edit plan first; apply atomically or return an explicit error with the original document unchanged |
| Stale edit plan | A prepared text result overwrites a different or newer document, or its registry assumptions change before commit | Bind in-memory plans to the exact document and registry until commit; validate all expected identity states in the atomic registry batch. Production transactions must additionally validate their expected page versions across effect boundaries |
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

### Detachment generations and register aliases: spike constraints

The in-memory registry prototype must keep a monotonic detachment generation
with each identity even while it is live. Reserving a never-used ID starts at
generation zero; detaching increments it with checked arithmetic in the same
atomic batch as the live-state precondition. A delete register records the
resulting generation. Move and retirement must compare the exact ID, source
page, and generation, and join/split/delete plans must revalidate their live
generation so that a detach-and-return to the same page cannot satisfy a stale
plan. Overflow is an explicit error. The production State store must persist
these generations and make them part of its transaction/recovery rules; the
spike's fake registry does not provide that durability.
Undo and recovery must also define how historical IDs are restored without
rewinding an issued generation or weakening retired-ID guarantees; the spike
does not yet implement those transitions.

The spike's register owner must track aliases rather than retire an identity
whenever any alias is cleared. Registers carrying move provenance are not
freely cloneable outside that owner. Copying an occupied slot into a vacant
slot creates another tracked alias; clearing a nonfinal alias only removes that
slot, and clearing the final alias retires its still-matching detachment.
Failure of retirement leaves the slot unchanged. An older generation is never
the owner of a newer detachment, even if both use the same ID and source page.
If a delete register's identity unexpectedly becomes available rather than
live, detached, or retired, paste and final clear report the missing registry
entry instead of silently abandoning the register. An uninstalled register can
also be explicitly discarded; if cleanup fails, the caller gets the register
back for retry.
The prototype does not define Vim register names or production replacement
semantics. Integration with the real register lifecycle, shutdown, undo, and
crash recovery remain required before this control is production ready.
Dropping an uninstalled register or its owner cannot perform fallible registry
cleanup automatically.

For the in-memory spike, replacing a slot must first determine whether the
old detachment still has another tracked owner, including the incoming
register if it carries the same ID, source page, and generation. Retire only
an unowned detachment whose registry state still matches that exact owner.
If registry lookup or retirement fails, retain the old slot and return the
incoming register to the caller for retry; neither register may be silently
discarded. Replacing a slot with an alias of another slot must leave the source
intact, handle a same-slot request as a no-op, and report a missing source.
These are synchronous ownership rules only: they do not make a prior delete,
register replacement, or recovery after a crash one durable transaction.

The prototype's linewise paste currently rejects register text containing CR
or LF. Deletion must apply the same representability check before detaching an
ID or changing canonical text. Exclude a CR from captured text only when it is
part of an actual raw CRLF line separator; hiding an ID token can make an
unrelated content CR appear adjacent to LF in the visible projection. Reject
unsupported content explicitly rather than normalize or discard it. General
multiline and standalone-CR register behavior is not yet a production design.

### Identity-preserving visible edits: spike constraints

The spike may plan an ordinary visible replacement without changing registry
state only when neither the selected text nor the replacement contains CR or
LF. Multiline and CR-containing edits require a separate structural policy;
this restriction is not an approved product UX. Create the projection while
holding an exclusive borrow of the canonical text, accept only a range tied
to that projection, and lower it through the existing grapheme-checked API.
Reject foreign ranges even if their offsets still fit the document. Keep every
hidden-token byte outside the replacement, then reparse the complete candidate
and require the expected visible text, the same ordered token kinds and IDs,
and the same original token spans shifted only by the edit. Reject
parser-context changes, newly recognized tokens, and ambiguous leading-token
insertions before committing any text.

The plan must retain an exclusive borrow of the exact canonical text until it
is committed or dropped. These are in-memory, correctness-first checks, not a
production editor path: they do not supply cursor policy, undo, registry/page
version fencing, durable transactions, or incremental performance.

### Fenced code context: spike constraints

The user-facing format promises that IDs inside fenced code remain literal.
The prototype must track the opening fence character and run length, and close
only on the same character with at least that many markers and no trailing
non-whitespace content. Shorter or different fences and fence-like lines with
trailing content remain literal until a valid close or end of file. Preserve
LF/CRLF bytes and existing opening-fence indentation recognition in this
bounded spike. This does not claim complete CommonMark parsing or approve a
production format change.

The frozen legacy parser currently toggles its fence state on every line
starting with three backticks or tildes, regardless of type or length. The
prototype's safer treatment therefore differs for those fence-like cases,
including valid Markdown containing shorter or different markers as code.
Record that divergence rather than changing the legacy oracle or claiming
parser parity; the production parser and migration need an explicit
compatibility review.

### Multi-block structural plans: spike constraints

Join and split must work in documents containing unrelated ordinary and legacy
mirror tokens without reconstructing the whole document from visible text.
Resolve each participating ordinary ID uniquely, reject duplicate participating
IDs and a split ID that collides with any parsed token, and require the checked
visible edit to address the participating lines. An unrelated mirror may
reference the same ID; it is not another live identity and must not be changed.

Rewrite only the participating canonical line content; preserve all other raw
bytes, including token spelling, whitespace, and original line endings. Join
retires only the second block's ID; split retains the original on the left and
adds the fresh ID on the right. Reparse the complete candidate and require both
the expected visible result and the exact ordered sequence of preserved,
retired, and added token kinds and IDs. This full check must reject changes in
Markdown context that would hide, expose, or reinterpret unrelated tokens.
Keep grapheme, CRLF, inline-code, and failure-atomicity checks. The prototype
still reparses the whole document; it does not claim an incremental production
path, durable state, undo support, or complete Vim integration.

### Current prototype evidence (not acceptance)

`rewrite/crates/bloom-kernel/src/identity.rs` now contains a policy prototype
with typed page/block IDs, register provenance, available/live/detached/retired
registry states, and explicit reserve/detach/move/retire plans. Its fake
registry covers cross-page copy behavior, move-vs-copy selection, stale-yank
protection, join retirement, split reservation, collision rejection, and
all-or-nothing registry batches. The canonical edit facades retain exclusive
borrows of the exact text and registry until the plan is consumed or dropped;
they assign the planned text only after the registry accepts the whole change
set. Registry changes revalidate expected page/liveness and fresh-ID
preconditions atomically, including if another registry handle changes the
underlying state after planning. Injected failures leave the original text
unchanged. This is a synchronous in-memory boundary, not a substitute for
page/version preconditions or atomic persistence across effect boundaries.
Projection tests also exercise `x`/`X`/`D`-shaped ranges at a trailing token
boundary and verify that the hidden token is preserved. A separate borrow-bound
visible-edit plan now consumes those recorded ranges and checks the resulting
canonical token spans as well as the visible text. A 336-case line-local
reference model covers Unicode graphemes, LF/CRLF, trailing whitespace,
mirrors, and missing final newlines. Injected token reattachment and marker
changes are rejected even when visible text is unchanged. Ranges from another
or an older projection are rejected, and an exclusive-borrow compile check
prevents intervening text mutation. This does not interpret Vim keys or
implement multiline editing, cursor policy, or undo.

The projection also tracks fence marker type and opening length. Shorter or
different markers and fence-like lines with trailing content no longer end a
fence, including in an unclosed block. LF/CRLF and missing-final-newline cases
keep code IDs visible and recognize IDs after a matching close; a 360-case
boundary model exercises both marker types, run lengths, and closing suffixes.
A visible replacement can edit such literal code without changing an outside
ID; a fence edit that would reclassify IDs is rejected.

Fixture-driven canonical-text planners replay the observed `J` and split
ranges, then commit the resulting text with the join retirement or split
reservation. The split planner preserves the current line's LF/CRLF style in
this prototype. The planners reject edits on unrelated lines, rewrite only the
participating canonical lines, and reparse the complete result before returning
a plan.
Additional ordinary IDs and legacy mirror tokens are preserved, including
mirrors referencing a participating ID; duplicated participating ordinary IDs
and fresh-ID collisions are rejected. The checks compare both the visible text
and the full ordered token kinds and IDs. Reference cases cover multi-block
LF/CRLF documents, Unicode graphemes, trailing whitespace, and missing final
newlines; injected missing, changed, and reordered tokens are detected even
when the visible text is unchanged. The recorded legacy `J` and split ranges
also run with surrounding ordinary and mirror tokens. Parser-context changes
that would reinterpret tokens are rejected, and an insertion at an ID-only
leading token still requires a structural decision.
The structural `plan_join_edit` and `plan_split_edit` facades bundle canonical
text with the corresponding registry changes, so a failed text plan returns no
commit-ready edit. Their tests exercise this path, including a rejected
inline-code split, a candidate collision, a parser-context change, and injected
registry failures that leave the source text and registry unchanged.

The kernel prototype now also plans canonical deletion of one uniquely
identified block line and linewise paste of a one-line register. Delete and
paste use the same borrow-bound plan boundary; a deleted register is returned
only after its text and registry changes succeed. Policy tests cover moving a
detached ID to another page and copying a live ID with a newly reserved
identity; target text and the registry remain unchanged when the paste commit
fails. A CRLF delete/paste test verifies that the line separator does not enter
the register text. Delete also rejects an unpasteable content CR before
detachment, including when a hidden token makes that CR appear to be part of a
visible CRLF separator; text and registry stay unchanged. These are explicit
policy/text-plan tests, not replays of the legacy `dd`/`p` or `yy`/`p` streams.
Unlike join/split's preservation of unrelated mirrors, block deletion rejects
any same-ID mirror in the source document, and paste rejects one in the
destination. Whether those mirror cases should be supported requires an
explicit legacy-mirror and migration decision; the spike does not infer one.
The two-phase move can leave a detached identity in its register after a failed
paste or process interruption. The prototype now assigns per-ID detachment
generations and rechecks them in delete, move, discard, join, and split batches. Its
register bank tracks aliases and retires a matching detachment only when its
last owner is cleared or replaced. Replacement keeps the old slot and returns
the incoming register if registry reads or retirement fail; alias replacement
retains its source. Tests cover ownership across matching and stale
generations, same-slot replacement, and an intervening registry change. A
bounded independent model checks all 100,000 five-action schedules of delete,
move, alias, clear, and alias replacement across two slots; inapplicable
actions are skipped. The same model comparisons reject injected
premature-retirement and stale-reuse outcomes. These are in-memory prototype
controls only. The actual editor register lifecycle does not invoke this bank
on replacement or clear; durable generation storage and transaction recovery
also remain unimplemented.

This remains prototype evidence, not acceptance. It does not execute the
identity matrix through a production Vim-to-document path; fresh ID candidates
are supplied by the caller, durable registry behavior and register lifecycle
wiring are not implemented. A legacy `UndoTree` regression test now verifies
that persisted undo/redo restores canonical text containing IDs byte-for-byte;
this is an oracle for the existing snapshot mechanism, not a test of rewrite's
projected-edit or identity transaction path, which has no undo integration yet.
`dd`/`p` and `yy`/`p` are recorded as real key streams but are not replayed
through canonical-text transactions. ADR-3 remains unaccepted.

An ordinary split inside an inline-code span can make the generated ID tokens
unrecognizable to the current line-oriented token parser. The planner now
rejects that result rather than silently losing identity, but the edit is
still unsupported. Resolving this parser/projection interaction (or testing
fallback (c)) is an acceptance blocker, not an approved change to editor UX.

### Fallback (c) assessment: typed spans

The spike may use a test-only typed-span micro-prototype to compare a generated
identity with literal code that has identical canonical bytes. It must check
both in-memory projections and reconstruction from the serialized Markdown,
including Unicode and CRLF. This experiment must not add a persisted sidecar,
change the format, or be presented as a complete fallback implementation.

The trailing-token parser rejects candidates inside inline code
(`rewrite/crates/bloom-text/src/projection.rs`, `parse_trailing_id_token`), so
`^id` there is literal content and not an identity. A parallel typed-span map
could identify a generated token during an editing session, but cannot
reconstruct that distinction from canonical Markdown after reload: the same
bytes can be either literal code or an identity token. Keeping spans
authoritative would therefore require a durable discriminator, such as a
format change, authoritative State metadata, or another sidecar. That would
change the current text-authority or format constraints and needs an explicit
durability and migration design, including what happens when Markdown is
imported or State is missing.

A test-only typed-span micro-prototype now constructs literal and generated
snapshots with identical Markdown bytes but different visible text, for both LF
and CRLF with Unicode content. Reparsing the bytes produces the literal view
for both snapshots, so it cannot restore the generated identity without more
information. This is a counterexample, not a complete editing, persistence, or
recovery implementation and not proof that fallback (c) is impossible. Until
an alternative is approved, the prototype continues to reject splits whose IDs
would be hidden by code-context rules rather than silently dropping identity.

## Acceptance gate

ADR-3 can be accepted only when:

1. The projection and all identity rules have executable tests, including
   property tests for visible-to-raw range lowering and canonical round trips.
2. The existing Vim unit suite and ID-bearing e2e scenarios pass without
   changing their expected behavior. Before the new oracle checks,
   `cargo test -p bloom-vim` passed 42 tests; it now passes 45, including both
   append cases and the identity-stream case. The current Vim API is coupled to
   `bloom_buffer::Buffer` and untyped `EditOp { range: Range<usize>, ... }`;
   the serialized test adapter must explicitly cover that seam without
   introducing a cross-workspace dependency. The serialized bridge covers
   append, structural command streams, and visible-only boundary deletions.
   Fixture-driven kernel tests replay the `J` and split edit ranges as
   canonical-text transactions, including with surrounding token-bearing
   lines; single-line `x`/`X`/`D` ranges use the identity-preserving text plan;
   and policy/text-plan tests cover move/copy identity preservation. None is a
   production Vim-to-document replay.
3. Every supported identity rule has a test or is explicitly recorded as
   uncovered; no rule is inferred from the current side-table implementation.
4. The verdict and any remaining gaps are recorded. If the gate fails, prototype
   fallback (c) before continuing with full M2.

No product or user-facing docs change is in scope for the spike. The legacy
product docs remain authoritative for current behavior until the migration
changes it.
