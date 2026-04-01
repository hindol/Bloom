# History Production Hardening

> PM discovery note for turning Bloom history into a feature users can trust during real writing.

---

## CEO intake

### Idea / hypothesis

Bloom history should feel production-hardened enough that a user can write a substantial block of text with confidence that this version will still be recoverable later.

### Why it may matter

History is not a side feature in Bloom. It is one of the emotional load-bearing parts of the editor. If the user does not trust history, they edit more timidly, save more nervously, and treat Bloom as fragile.

The current implementation is already strong enough to be promising. The risk is not that nothing exists. The risk is that Bloom already looks like it is making a trust promise without yet making that promise legible, observable, and clearly scoped.

### Target user or workflow

- a writer making a long edit pass on one page
- a researcher restructuring linked notes
- a journal-heavy user who wants quiet automatic protection without manual file rituals
- an experienced local-first user who expects recovery after crashes, branch switches, and regretted edits

### Constraints or non-negotiables

- local-first remains the default
- history must preserve Bloom's current ownership model: buffer owns live cursor state, history restore becomes a normal editor edit
- the save path must not secretly mutate content
- user trust matters more than implementation cleverness

### Questions for the PM

- What user promise should Bloom make around automatic backup and recovery?
- Where is the current history story already good enough, and where is it still too implicit?
- Which failure modes matter most before this can feel production-ready?
- What should be visible in the UI so the user can trust history without thinking about git internals?

---

## PM discovery

### Problem framing

The user does not actually want "history" in the abstract. They want confidence.

More concretely, they want to sit down, make a serious edit, and feel that Bloom is holding onto the last good version without asking them to manage checkpoint files, duplicate notes, or think like a source-control operator. The product need is not just rollback. It is calm.

Bloom already points in that direction. `docs/HISTORY.md` frames history in terms of recovery, comparison, and confidence. `docs/GOALS.md` says undo should branch and survive long enough to be useful, and that deeper recovery should come from git-backed history. The product intent is clear: fearless editing.

The gap is that the current product story is still more coherent to us than it is to the user.

### Current product shape

Today Bloom has a genuinely meaningful two-layer history model:

- recent history through a fine-grained branching undo tree
- older save-based history through the git-backed history thread
- restore into the current buffer as a normal undoable edit

This is not hypothetical. The current implementation already includes:

- persisted undo-tree support
- page history in the temporal strip
- block history in the temporal strip
- restore from undo history or git-backed history into the live buffer
- background history thread wiring with idle-based and max-interval commit timing

More specifically, the recent layer is intentionally detailed: the undo tree is branching, survives restart, is persisted through SQLite, and is pruned on roughly a 24-hour horizon. That makes it the right place for "what exactly happened a moment ago?" recovery.

The git-backed layer should stay intentionally coarser. Its job is not to record every microscopic edit stop. Its job is to provide durable, legible recovery points over longer time spans without overwhelming the user with noise.

Those pieces are strong. They are also still easier to trust if you wrote them than if you are simply using the editor.

### The trust gap

Bloom's current docs say history is not "a backup system that happens to be nearby." That framing made sense while the feature was still being introduced carefully.

But the user value proposition we now want is unmistakably backup-shaped:

- Bloom should automatically keep meaningful older versions
- the user should not have to remember to checkpoint manually
- after a regretted edit, crash, or long writing pass, the user should believe recovery is there

This does **not** mean Bloom should market itself as a cloud backup service or as archival infrastructure. It does mean history should feel dependable enough that "I can get that version back" becomes part of normal editing confidence.

That is the product shift this note is about.

### What is already strong

Several parts of the current design are exactly right for the intended value proposition:

- restore is a normal edit, not a destructive mode switch
- page history is keyed by page identity rather than filename, so renames do not sever the story
- block history leverages stable block identity, which makes the feature feel Bloom-native rather than bolted on
- history work is off the hot path in a background thread, which fits Bloom's architecture and keeps editing responsive
- autosave, file watching, and conflict handling already show that Bloom cares about recovery in the real world, not just in the happy path

These are not small wins. They mean the problem is not "invent history." The problem is "finish the trust model."

### What does not yet feel production-hardened

#### 1. Automatic backup is not legible

There are history settings and background commit behavior, but the user does not yet get a crisp answer to a simple question:

**"Is my last good version safely captured yet?"**

The current system can be working correctly while still feeling opaque. That is a product gap.

#### 2. The durability story is split across layers in a way the user cannot see

Undo tree and git-backed history are a good internal model. They are not yet a sufficiently clear user model.

The user needs to understand, without reading code:

- what recent edits are protected by the fine-grained branching undo tree
- that recent history survives restart through persisted undo state, but only on a bounded recent horizon
- what older edits are protected by coarser durable history
- what happens across restart
- what happens after long idle periods
- what happens if Bloom cannot write or commit history

Without that clarity, the user is forced to trust a black box.

#### 3. Failure modes are under-explained

The system already handles real-world conditions such as autosave, self-write detection, external file changes, and history-thread results. But the product layer does not yet make failure states feel managed.

The important PM question is not "can an error happen?" It is "what does the user experience when it does?"

Examples:

- history repository cannot open
- autosave/write fails
- background history commit falls behind or errors
- undo persistence ages out
- restore creates a new branch of editing and the user does not understand what happened

Production hardening means these become product behaviors, not just engineering cases.

#### 4. There is no explicit checkpoint gesture

If the value proposition includes "I can safely edit a good block of text," Bloom likely needs a deliberate checkpoint surface in addition to background safety.

That does not mean exposing raw git vocabulary. It means giving the user an intentional moment of certainty before a risky edit, before a branch switch, or after a satisfying draft milestone.

It also means being disciplined about checkpoint density. The git-backed layer should remain coarse enough that page history remains readable over time. A long file history full of tiny stop points is not reassuring; it is clutter.

#### 5. Docs do not yet set the right confidence contract

The current history docs explain the shape of the feature well. They do **not** yet fully answer the user's practical trust questions:

- how automatic is automatic?
- when should I expect older versions to exist?
- what survives restart?
- what should I do before a risky reorganization?
- what does restore actually do to my current buffer?

This is a place where PM-owned docs will matter as much as the implementation.

#### 6. The current surface is visually strong but semantically thin

The temporal strip is a good navigation backbone. It looks like a history tool.

But it does not yet do enough to answer the user's next question:

**"Why would I go to this stop instead of the one next to it?"**

Right now the available per-stop meaning is often too weak:

- a timestamp
- a generic commit message
- sometimes only very coarse file-count information

That makes the surface feel more like a stylish scrubber than a high-confidence recovery tool. The problem is not that the strip is bad. The problem is that the strip is currently carrying more meaning than its metadata can support.

### User-facing requirements for a hardened history story

If Bloom wants history to support fearless editing, the user should be able to rely on the following product-level behaviors:

#### A. Clear automatic protection

Bloom should make it obvious that changes are being protected in the background, without turning the editor into a sync dashboard.

The user should be able to tell:

- whether the current work is still only "live buffer / undo recent"
- whether a durable history point has been captured
- whether history capture is failing and needs attention

#### B. A visible, understandable recovery path

When something goes wrong, the user should know where to go next:

- undo tree for recent, fine-grained mistakes and branch exploration
- page history for older, coarser page-level recovery
- block history for targeted restore

That path should feel discoverable and calm, not like hidden rescue machinery.

The product should also teach the distinction clearly:

- the undo tree is the dense, recent, branching layer
- durable history is the calmer, sparser, longer-lived layer

But Bloom should not force that distinction into two separate primary UIs for the same page.

For page-level recovery, the product should converge on **one unified history surface** that contains:

- recent branching undo
- older durable git-backed checkpoints
- restore and diff actions

That means `SPC H h` and `SPC u u` should open the same core history experience, not two different tools that the user has to mentally stitch together.

#### C. Explicit checkpointing before risky work

Bloom should likely offer a first-class "make this recoverable now" action, phrased in Bloom terms rather than raw git language.

This is especially important for:

- long writing passes
- note refactors
- page splits / merges / structural edits
- external workflows such as branch switching or vault manipulation outside Bloom

#### D. Predictable restore semantics

Restore should continue to behave as a normal edit in the current buffer. That is the right interaction model.

But the UI and docs should also explain the consequence: restore does not erase time. It creates a new forward state while older branches remain part of history.

#### E. Failure states that preserve trust

The hardened feature should never quietly degrade from "you are protected" to "you are not" without the user noticing.

At minimum, the product should define what happens when:

- autosave fails
- history commit fails
- history repo cannot initialize
- external disk changes conflict with dirty buffers
- undo persistence is unavailable or pruned

#### F. Mirror propagation must preserve the right history granularity

Bloom's mirror behavior creates a special product constraint. When a mirrored path is not already open, Bloom can open it silently in the background, apply the propagated edit, and save it. That is acceptable for correctness.

But the history story must distinguish between two layers:

- at the undo level, it is acceptable for mirror propagation to create a node in the affected buffer's recent undo history
- at the durable git-backed layer, mirrored propagation should not create its own separate noisy stop point for each propagated edit

From the user's perspective, a mirrored edit belongs to the same editing moment as the source change. Durable history should preserve that coherence instead of turning one conceptual action into many history entries.

#### F2. Block history must follow lineage, not only raw commit order

`SPC H b` is trickier than page history because block history is not just "show me the page at older commits." It is "show me the life of this block."

That means linear git history is still fine, but the block-history surface needs to synthesize lineage events on top of those linear checkpoints.

The important product rule is:

- git commits remain linear
- block history may still show semantic events such as move, split, and merge

The user should experience:

- a block move as "same block, new page"
- a block split as "this block became two descendants"
- a block merge as "this block absorbed or was absorbed into another"

That does not require branching git history. It requires better identity semantics in the block-history layer.

#### F3. An optional tracked-block gutter could improve observability

One possible supporting surface is a configurable gutter to the **left of line numbers** that signals tracked block identity.

This is appealing because:

- it stays out of the text-editing flow
- it gives advanced users a way to see identity behavior directly
- it makes split / merge / move behavior more inspectable in real time

For example, a tech-savvy user could see:

- after a split, which block kept identity and which one became the new child
- after a merge, which block survived and which one disappeared into the survivor
- when a block moved or mirrored, that the identity remained stable

The important PM framing is that this should be an **advanced observability option**, not a default burden on ordinary writing.

Recommended shape:

- off by default
- read-only
- visually faded / secondary in the steady state
- placed in a dedicated gutter lane left of line numbers
- positioned so it never feels like part of the editable text
- marker-first rather than raw-ID-first
- able to briefly flash different marker states when split / merge changes happen

This would help observability, but it should not replace richer history UX. A gutter can show **current live identity**; it cannot by itself explain lineage over time.

#### G. History stops need stronger surface integration

Bloom should treat the current temporal strip as a **navigation rail**, not necessarily as the whole history experience.

For history to feel trustworthy, the selected stop should communicate more than date + generic label. The user should be able to understand:

- why this stop exists
- how it differs from nearby stops
- whether it is recent undo or durable history
- what scope it represents: one page, a block, a mirror-linked edit, or a broader checkpoint moment
- whether it was automatic, explicit, or caused by a special event such as restore or external change

The likely PM direction is:

- keep the strip for orientation and fast scrubbing
- add a richer selected-stop detail surface alongside or below it
- use structured summaries instead of raw generic commit messages as the main explanatory text

That richer surface could show combinations such as:

- checkpoint type: automatic / explicit / restore / external-change
- scope summary: `Rust Project + 2 mirrors`, `Block ^k7m2x moved`, `3 pages changed`
- meaningful diff summary rather than just generic file count
- user-authored checkpoint label when the stop came from an explicit "protect this version now" action

The product goal is not more metadata everywhere. It is better reasons to trust and choose a stop.

### Recommendation

**Pursue.**

Bloom already has too much good history machinery for this to remain merely "nice if polished later." It is close enough to a real trust feature that leaving it half-explicit would undersell the product and confuse the user.

The right next move is not to jump straight into implementation details. The right next move is to lock the product promise down:

- what exactly Bloom guarantees
- what it does not guarantee
- what the user should see
- what the user should do when recovery matters

That will give the architect a better target than "make history more robust."

### PM recommendation by priority

#### P0 — Make the safety state visible

Add a lightweight product surface for history state, such as:

- last durable history point
- history capture pending
- history capture failed

The goal is not status noise. The goal is confidence.

That surface should also reinforce the layering: recent undo is always denser, while durable history points are intentionally fewer and more meaningful.

#### P1 — Add an explicit checkpoint action

Give the user a Bloom-native way to say:

**"Protect this version now."**

This can map to the existing history machinery internally, but the product surface should be intentional and understandable.

It should also be framed as a way to create a meaningful durable point, not as a way to compensate for excessively chatty automatic snapshotting.

#### P1 — Write the user trust contract

Before calling history production-hardened, Bloom should have user-facing docs that clearly explain:

- what history protects automatically
- what survives restart
- how restore works
- how to recover from recent mistakes versus older ones
- what to do before especially risky operations

#### P1 — Evolve the history surface beyond a thin strip

Keep the temporal strip as the fast timeline rail, but stop asking it to carry the entire meaning of history on its own.

Bloom should likely move toward a **rail + inspector** model:

- the rail gives orientation and quick motion through time
- the selected-stop inspector explains why this stop matters

That inspector should favor structured Bloom-native summaries over generic commit-message text.

As part of PM discovery, this should include a lightweight UX exploration artifact.

Recommended format order:

- first: checked-in ASCII sketches in the planning doc
- second: a slightly more detailed checked-in wireframe if needed
- optional: Figma only when layout/state complexity is hard to reason about in text alone

For this feature, the UX exploration should compare at least:

- today's strip-only model
- rail + inspector
- whether history state (captured / pending / failed) belongs inside the history surface, in the modeline, or both
- how `SPC H h` and `SPC u u` enter the same unified page-history surface without redundant UX

#### P1 — Define visible failure behavior

The feature needs explicit product behavior for:

- save failure
- history failure
- repository initialization failure
- conflict on external disk change

The user should never have to infer safety from silence.

#### P2 — Clarify time horizons

The product should explain the difference between:

- immediate local recovery
- persisted recent branching recovery, surviving restart on the recent horizon
- durable older recovery through sparser git-backed checkpoints

That distinction is already in the system. It now needs a humane user explanation.

#### P2 — Define mirror-history policy explicitly

The product and architecture should explicitly agree on one rule:

**mirror propagation may be granular in undo, but should be coalesced in durable history.**

That keeps mirrored editing correct without making history unreadably noisy.

#### P2 — Define a stop-summary model

Bloom should define what makes a history stop meaningfully distinct in the UI.

Candidate fields:

- stop type
- relative/absolute time
- page / block / mirror scope
- concise change summary
- optional explicit checkpoint label

Without a clear stop-summary model, the UI will keep falling back to generic messages that do not help much during recovery.

For block history specifically, the stop-summary model should also support semantic lineage events such as:

- moved to page X
- split into `^a1b2c` + `^d3e4f`
- merged into `^k7m2x`
- merged from `^old12`

#### P2 — Consider an advanced tracked-block gutter

Bloom should consider an optional advanced gutter that surfaces current tracked block identity in the editor chrome.

This should be treated as:

- an observability/debugging aid for advanced users
- a way to make split/merge survivor behavior visible
- explicitly secondary to the main history surface
- better expressed through tracked markers than through raw ID strings

Default should remain off so hidden structure does not dominate the normal editing experience.

### UX exploration — history surface

This is a first low-fidelity exploration, not a final design.

The goal is to compare ways Bloom could make history feel more meaningful without losing the speed and elegance of the temporal strip.

#### Option A — Today: strip-first, preview-second

```text
┌─ Rust Project (diff vs current) ─────────────────────────────┐
│  diff preview                                                │
│                                                              │
├── ● 2m ── ● 5m ── ● 8m ── ○ 1h ── ○ yday ── ○ Mar 10 ──────┤
│              ▲                                               │
├──────────────────────────────────────────────────────────────┤
│ HIST  Rust Project                     d:diff  r:restore     │
└──────────────────────────────────────────────────────────────┘
```

Pros:

- compact
- elegant
- highly keyboardable
- good at showing that history is continuous across undo and git

Cons:

- the selected stop is weakly explained
- neighboring stops are hard to distinguish meaningfully
- generic commit messages do not carry enough product meaning
- no obvious place for history health state such as pending / captured / failed

Verdict:

- strong baseline rail
- weak as the full recovery surface

#### Option B — Rail + inspector drawer

```text
┌─ Rust Project ────────────────────────────────────────────────┐
│  diff preview                                                │
│                                                              │
├── ● 2m ── ● 5m ── ● 8m ── ○ 1h ── ○ yday ── ○ Mar 10 ──────┤
│              ▲                                               │
├──────────────────────────────────────────────────────────────┤
│ Stop details                                                 │
│ Type: Durable checkpoint   Time: Today, 3:12 PM             │
│ Why it exists: Auto checkpoint after idle                    │
│ Scope: Rust Project + 2 mirrors                              │
│ Summary: Heading rewrite + block moved + task edits          │
│ Safety: Captured                                             │
│ Actions: [d] diff   [r] restore   [c] checkpoint now         │
└──────────────────────────────────────────────────────────────┘
```

Pros:

- preserves the strip as orientation rail
- gives the selected stop a real explanation surface
- creates a natural home for checkpoint type, scope, and safety state
- can make git stops much more meaningful without cluttering the rail itself

Cons:

- taller surface
- requires Bloom to define structured stop metadata, not just string messages
- more design work around what belongs in the inspector vs the preview

Verdict:

- strongest current candidate
- best fit for "history you can trust"

#### Option C — Two-column history inspector

```text
┌─ History ────────────────────────────────────────────────────┐
│ Stops                              │ Selected stop           │
│                                    │                         │
│ ● 2m  typed "delta"                │ Type: undo             │
│ ● 5m  replaced heading             │ Scope: current page    │
│ ● 8m  restored old section         │ Summary: 4 lines       │
│ ○ 1h  checkpoint: before refactor  │ changed                │
│ ○ yday checkpoint: daily wrap-up   │ Safety: captured       │
│ ○ Mar 10 imported mirror changes   │ [d] [r] [c]            │
│                                    │                         │
├────────────────────────────────────┴─────────────────────────┤
│ preview / diff                                              │
└──────────────────────────────────────────────────────────────┘
```

Pros:

- highly legible
- much easier to scan than a pure timeline when many stops exist
- makes structured summaries first-class

Cons:

- loses some of the distinctive "time rail" feel
- starts to look more like a conventional version browser
- weakens the graceful seamlessness between undo nodes and git nodes

Verdict:

- useful comparison point
- probably too list-heavy for Bloom's current temporal-navigation identity

#### Option D — Rail with modeline-level health only

```text
┌─ Rust Project (diff vs current) ─────────────────────────────┐
│  diff preview                                                │
│                                                              │
├── ● 2m ── ● 5m ── ● 8m ── ○ 1h ── ○ yday ── ○ Mar 10 ──────┤
│              ▲                                               │
├──────────────────────────────────────────────────────────────┤
│ HIST  Rust Project   Captured 3:12 PM   d:diff  r:restore   │
└──────────────────────────────────────────────────────────────┘
```

Pros:

- minimal UI expansion
- easy place to expose durable state

Cons:

- helps with safety state but not with stop meaning
- still leaves most history stops semantically thin

Verdict:

- likely useful as a supplement
- not enough on its own

### UX recommendation

The best direction from this first pass is:

- keep the temporal strip as the orientation rail
- add a selected-stop inspector area
- keep history health visible in the modeline and/or inspector
- use one unified page-history surface for both `SPC H h` and `SPC u u`

In other words:

**Option B + a lightweight form of D** looks strongest.

That preserves what is already distinctive about Bloom while giving the user a better answer to:

- what this stop is
- why it matters
- whether it is safely captured
- what will happen if I restore it

#### Unified entrypoint policy

`SPC H h` and `SPC u u` should open the same page-scoped history surface.

The difference, if any, should be only the **initial emphasis**, not a different UI:

- `SPC u u` can land focused on the current undo node / recent branching portion of the rail
- `SPC H h` can land focused on the broader page-history view of the same surface

But once opened, the user should remain in one coherent history tool with:

- the same rail
- the same inspector
- the same diff / restore actions
- the same branch navigation model

This avoids a false split where Bloom internally has two history layers and then also exposes two separate page-history UIs on top of them.

### How undo branches should surface in the recommended UX

The recommended `rail + inspector` model should preserve Bloom's current branch-friendly temporal rail rather than flattening history into a simple list.

#### Rail behavior

- the **current undo path** stays on the main rail line
- when the selection lands on a fork-capable undo node, **alternate branches appear below the main line**
- `j` / `k` continue to switch between branch alternatives at that fork
- git-backed checkpoints remain linear and never show branch alternatives

So the rail still does the spatial job it already does well: showing that undo history can fork while durable history does not.

Example:

```text
┌─ Rust Project ────────────────────────────────────────────────┐
│  diff preview                                                │
│                                                              │
├── ● 2m ── ● 5m ──┬── ● 8m ── ● now ── ○ 1h ── ○ yday ──────┤
│                  └── ● 8m alt                               │
│                     ▲                                        │
├──────────────────────────────────────────────────────────────┤
│ Stop details                                                 │
│ Type: Undo branch node                                       │
│ Branch: alt 1 of 2 at fork after "replaced heading"         │
│ Summary: abandoned branch kept after undo → edit             │
│ Restore effect: creates a new forward node on current path   │
└──────────────────────────────────────────────────────────────┘
```

#### Inspector behavior

The inspector should make branch structure legible without asking the rail to explain everything by itself.

For selected undo nodes at or within a branch, show fields such as:

- branch status: current path / alternate branch
- branch count at this fork
- where the fork happened in user terms
- whether restore will:
  - move within the current path
  - switch to another branch view
  - create a new forward node from an abandoned branch

This keeps branch semantics understandable even when the rail remains compact.

#### Nested branches

Nested branches should stay visually subordinate:

- show only a small number of visible branch lines near the active fork
- collapse deeper nesting behind a concise indicator such as `+2 more`
- let the inspector explain the selected branch in more detail

That preserves Bloom's distinctive branching UI without letting it become visually chaotic.

#### Why this split works

The rail is good at answering:

- where am I in time?
- is this linear or forked?
- what nearby alternatives exist?

The inspector is better at answering:

- what does this branch represent?
- why did it fork?
- what will restore do from here?

That is the division of labor we want.

### UX questions for the next pass

- Should the inspector live below the rail, beside it, or replace the lower modeline area?
- Should the diff preview remain primary, with stop details secondary, or should details be primary and preview toggleable?
- Which fields are essential enough to always show for a selected stop?
- How much should undo stops and durable stops share one visual language versus intentionally differ?
- Should explicit checkpoints allow user-authored labels that become first-class stop titles?
- How much nested branch structure should appear directly in the rail before collapsing into the inspector?
- Should `SPC H h` and `SPC u u` differ only by initial focus, or should they become exact aliases?

### UX exploration — second pass for the recommended surface

This pass stops comparing broad options and instead sharpens the currently preferred direction:

- one unified page-history surface
- rail + inspector
- lightweight modeline health
- block history as a sibling variant of the same interaction model

#### Working name

For now, think of this as the **History Surface**, not "page history vs undo tree visualizer."

It has:

- one rail
- one preview area
- one inspector area
- one modeline

Different commands enter it with different initial focus, but they do not open different tools.

#### Unified page-history layout

```text
┌─ Rust Project — History ─────────────────────────────────────────────────────┐
│ diff / content preview                                                      │
│                                                                              │
│  ## Rope Data Structure                                                     │
│  - old text                                                                  │
│  + recovered text                                                            │
│                                                                              │
├── ● 2m ── ● 5m ──┬── ● 8m ── ● now ── ○ 1h ── ○ yday ── ○ Mar 10 ─────────┤
│                  └── ● 8m alt                                               │
│                     ▲                                                        │
├──────────────────────────────────────────────────────────────────────────────┤
│ Stop details                                                                 │
│ Kind: undo branch                                                            │
│ Time: 8 minutes ago                                                          │
│ Scope: current page                                                          │
│ Why it exists: undo → edit fork after heading rewrite                        │
│ Summary: replaced heading + 4 line edits                                     │
│ Branch: alternate 1 of 2                                                     │
│ Restore effect: create new forward node on current path                      │
├──────────────────────────────────────────────────────────────────────────────┤
│ HIST  Undo current • Durable captured 3:12 PM   d:diff  r:restore  c:savept │
└──────────────────────────────────────────────────────────────────────────────┘
```

This version gives each region a clear job:

- **preview** = what changed
- **rail** = where in time / branch structure you are
- **inspector** = why this stop matters
- **modeline** = current mode + durable safety state + actions

#### Entry behavior

The same surface should open from multiple commands:

- `SPC u u`
  - opens the unified history surface
  - initial selection = current undo node
  - initial inspector emphasis = branch context / undo semantics

- `SPC H h`
  - opens the same unified history surface
  - initial selection = most relevant page-history stop, likely current or most recent durable point
  - initial inspector emphasis = broader page recovery context

- `SPC H b`
  - opens the block-history variant of the same overall interaction model
  - initial selection = current block on the current path

The product difference is therefore **entry emphasis**, not **surface fragmentation**.

#### Block-history variant

Block history should feel like the same family of UI, but with lineage-specific detail.

```text
┌─ Block ^k7m2x — History ─────────────────────────────────────────────────────┐
│ inline diff / block preview                                                  │
│                                                                              │
│  - Review ropey + petgraph API @due(03-16)                                   │
│  + Review ropey API @due(03-16)                                              │
│                                                                              │
├── ● 2m ── ● 8m ── ○ 1h ── ◇ split ── ○ yday ── ◇ merged ── ○ Mar 10 ──────┤
│             ▲                                                                │
├──────────────────────────────────────────────────────────────────────────────┤
│ Stop details                                                                 │
│ Kind: lineage event                                                          │
│ Event: split                                                                 │
│ Parent: ^k7m2x                                                               │
│ Spawned: ^d3e4f                                                              │
│ Scope: Rust Project                                                          │
│ Meaning: current block kept original identity; new child started here        │
│ Restore effect: restore selected historical form into current block          │
├──────────────────────────────────────────────────────────────────────────────┤
│ HIST  Block line • Durable captured 3:12 PM   d:diff  r:restore             │
└──────────────────────────────────────────────────────────────────────────────┘
```

Notes:

- `◇` here represents a synthetic lineage stop, not a git branch
- block history is still traversing a linear durable sequence underneath
- lineage events become first-class stops because they are more meaningful than raw generic commits

#### Visual language for stop kinds

The surface needs a stronger stop taxonomy.

Suggested visual families:

- `●` undo nodes
- `○` durable git checkpoints
- `◇` synthetic lineage events for block history
- optional special styling for explicit checkpoints, e.g. labeled `○ checkpoint`

This keeps the rail scannable without making every stop depend on a text label.

#### Inspector fields: always visible vs conditional

Always-visible core fields:

- kind
- time
- scope
- summary
- restore effect

Conditional fields:

- branch status and branch count for undo nodes
- checkpoint reason for durable stops
- parent / child / survivor / retired IDs for block lineage stops
- mirror scope when the stop includes mirror propagation
- user-authored label for explicit checkpoints

That balance keeps the inspector informative without turning every stop into a dense debug panel.

#### Modeline health placement

The modeline should carry lightweight durable-state info continuously, even before the user opens history.

Inside the history surface, it can become slightly richer:

- `Undo current`
- `Durable pending`
- `Durable captured 3:12 PM`
- `Durable failed`

This keeps safety state visible without forcing the inspector to repeat it constantly.

#### Diff vs inspector priority

Current recommendation:

- preview remains visually primary
- inspector remains semantically primary

That means the preview should occupy the largest area, but the inspector should be the place where the user learns what the selected stop means.

This is important because Bloom's current problem is not lack of diff; it is lack of stop meaning.

#### Optional tracked-block gutter placement

The optional tracked-block gutter should **not** be part of the history rail itself.

Best placement:

- in the normal editor chrome
- left of line numbers
- faded and secondary
- visible while editing or while returning from history

Example:

```text
 o      42  Review ropey API
        43  Some supporting paragraph
 +      44  New split child block
```

The exact marker styling is frontend-owned, but the point is the same: a subtle steady-state marker can show trackedness, and a stronger transient marker can distinguish survivor/new-child behavior after split and merge. The history surface still owns the explanation of lineage over time.

#### Interaction summary

Recommended key behavior inside the unified surface:

- `h` / `l` move along the rail
- `j` / `k` switch undo branches at a fork
- `d` toggles diff / raw historical content
- `r` restores selected stop into current buffer
- `c` creates an explicit checkpoint
- `q` closes history and returns to editing

These work the same whether the user arrived via `SPC u u` or `SPC H h`.

#### PM recommendation from the second pass

The recommended product direction is now:

- one unified history surface for page-level recovery
- block history as a sibling lineage-focused variant of the same model
- branch structure remains in the rail
- stop meaning lives in the inspector
- durable safety state lives lightly in the modeline and optionally in the inspector
- optional tracked-block gutter remains a separate advanced observability aid

#### Questions for the architect from this pass

- What metadata schema is needed for stop kinds, checkpoint reasons, branch context, and block lineage events?
- How should synthetic lineage stops be produced from linear checkpoints?
- Should `SPC u u` and `SPC H h` differ only by initial selection, or literally map to the same command with a parameter?
- Can the modeline expose durable-state health globally outside history without introducing noisy redraw churn?

### Suggested next artifact

The next step should be an **architect technical spec** focused on production hardening for history.

That spec should answer:

- what "durable history point" means operationally
- what state transitions exist for history capture
- how commit timing, retries, and failures are surfaced
- how explicit checkpoints should work
- what is persisted, pruned, or recoverable across restart
- how the feature behaves under crash, disk failure, repo corruption, and branch switches
- how mirrored edit propagation participates in undo without splintering git-backed history

### Risks / open questions

- Should Bloom explicitly talk about "backup," or should it frame this as "durable history you can trust"?
- How visible should history state be without making the editor feel operationally busy?
- Should explicit checkpointing be a command, a keybinding, or both?
- What is the right user promise around restart, crash recovery, and older history retention?
- How much of the current internal split between undo and git history should remain visible to the user?
- Does the temporal strip remain the primary history surface, or does it become the rail inside a richer history inspector?
- What structured per-stop summary is informative enough to help recovery without turning history into a dashboard?

### Suggested implementation themes for the architect

These are not implementation instructions yet. They are the product pressures the architect should stress-test:

- observability of history state
- safe failure behavior
- explicit checkpointing
- recovery semantics after restore
- persistence and pruning guarantees
- structured metadata for history stops so the surface can explain why each stop matters
- conflict behavior when disk and buffer diverge

---

## Bottom line

Bloom already has the bones of a strong history feature.

What it lacks is not core ambition. It lacks the final layer of product trust: the feeling that the editor is quietly, reliably keeping hold of the version you may want back later.

That is the work now. Not just more history machinery, but a clearer promise around safety, recovery, and confidence.
