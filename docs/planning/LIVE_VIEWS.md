# Live Views 🔭

> A composable query language (BQL) with named views as dedicated surfaces.
> Status: **Draft** — exploratory, not committed.

---

## The Problem

Today Bloom has several hard-coded views: the agenda shows tasks, the timeline shows backlinks, the picker searches text. Each is a separate feature, separately implemented, with its own keybinding, its own renderer, its own data path.

But they're all the same thing: **a filtered, grouped, sorted projection of the vault.**

The agenda is `tasks where status = open, grouped by due date`. The timeline is `chunks where links to X, sorted by date`. Backlinks is `pages where links to X`. Tag browse is `pages where tag = Y`. These are all queries. We just hard-coded them.

**The cost of hard-coding:**
- Users can't ask questions we didn't anticipate ("show me all tasks tagged #work due this week that mention 'budget'")
- Every new view requires core code changes
- The agenda can't be customised — its grouping and filtering are frozen
- There's no way to save a useful query for reuse

**The opportunity:** A small query language that unifies all of these into one composable system. The agenda becomes a built-in query. The user can write their own.

---

## Design Principles

1. **Learnable in 5 minutes.** The entire language fits on one screen. No programming background required.

2. **Reads like English.** `tasks | where not done and due < today` not `SELECT * FROM tasks WHERE done = 0 AND due_date < date('now')`.

3. **Composable by piping.** Small operations chained with `|`. Each step transforms the result set.

4. **Zero boilerplate.** A query with no clauses returns everything. Every clause is optional. `tasks` alone is valid.

5. **Live feedback.** The interactive query prompt (`SPC v v`) parses and executes on every pause (150ms debounce), showing results or a clear error with position info.

6. **Views are surfaces, not documents.** A view is a dedicated full-screen overlay — not text embedded in a note. No cursor confusion, no mode-dependent rendering. Notes are for writing; views are for querying.

---

## The Language: Bloom Query Language (BQL)

### Grammar

```
QUERY  = SOURCE ("|" CLAUSE)*

SOURCE = "pages" | "tasks" | "journal" | "blocks" | "tags" | "links"

CLAUSE = WHERE | SORT | GROUP | LIMIT | COUNT

WHERE  = "where" EXPR
EXPR   = PRED (("and" | "or") PRED)*
PRED   = "not" ATOM | ATOM
ATOM   = "(" EXPR ")"
       | FIELD OP VALUE
       | FIELD "has" TAG
       | FIELD RANGE

OP     = "=" | "!=" | "<" | ">" | "<=" | ">="

RANGE  = "this week" | "last week" | "next week"
       | "this month" | "last month" | "next month"

TAG    = "#" identifier

FIELD  = identifier ("." identifier)*
VALUE  = STRING | DATE | NUMBER | BOOL | VAR | "none"
STRING = '"..."' | "'...'"
DATE   = YYYY-MM-DD | "today" | "yesterday" | "tomorrow"
NUMBER = digits
BOOL   = "true" | "false"
VAR    = "$page" | "$today"

SORT   = "sort" FIELD ["asc"|"desc"] ("," FIELD ["asc"|"desc"])*
GROUP  = "group" FIELD
LIMIT  = "limit" NUMBER
COUNT  = "count"
```

### Rules

- Strings **must** be quoted. `"hello"` or `'hello'`.
- `and` binds tighter than `or`: `a or b and c` parses as `a or (b and c)`.
- `not` can only precede an atom (no double negation).
- `count` must be the last clause. With `group`, it returns per-group counts. Without `group`, a single total.
- Sort only works on scalar fields (not lists like `tags`).
- `$page` resolves to the current page's ID. Returns empty results if no page is open.
- `none` represents a missing value: `where due = none` (no due date), `where due != none` (has due date).
- Range predicates (`this week`, etc.) only apply to date fields. Week start day is configured in `config.toml`.
- `has` only applies to list fields (`tags`). Uses Bloom's tag syntax: `tags has #rust`.
- Unknown fields or type mismatches produce compile-time errors with clear messages.

### Sources

| Source | Returns | Fields |
|--------|---------|--------|
| `pages` | All pages | `title`, `created`, `tags`, `path`, `backlinks.count` |
| `tasks` | All tasks (checkbox items) | `text`, `done`, `due`, `start`, `page`, `tags`, `line` |
| `journal` | Journal entries only | `date`, `title`, `tags` |
| `blocks` | All content blocks | `text`, `page`, `line`, `tags`, `modified` |
| `tags` | All unique tags | `name`, `count` |
| `links` | All links | `from`, `to`, `display` |

### Examples

```
tasks                                          -- all tasks
tasks | where not done                         -- open tasks
tasks | where not done and due < today         -- overdue
tasks | where not done and due this week       -- due this week
  and tags has #work
tasks | where tags has #work                   -- work tasks
  or tags has #urgent
tasks | where not done and due = none          -- tasks with no due date
tasks | where not done                         -- agenda
  | sort due | group due.category
tasks | where not done | count                 -- total open tasks
tasks | where not done                         -- per-page open task counts
  | group page | count

pages | sort created desc | limit 20           -- recently created
pages | where tags has #rust | sort title      -- all Rust pages
pages | where backlinks.count = 0              -- orphan pages

links | where to = $page                       -- what links here

tags | sort count desc                         -- all tags by frequency

blocks | where modified = today                -- everything touched today
```

---

## Named Views

### Why Not Embed Queries in Notes?

An earlier design had `{{query}}` blocks rendering live results inline in page content. This was abandoned because:

- **Cursor navigation breaks.** The cursor operates on buffer lines, but query results are synthetic — navigating through mixed buffer/synthetic content requires a second cursor context, mode-dependent rendering, and complex entry/exit semantics.
- **Notes become two things.** A page with embedded queries is both a document and a UI surface. These are different concerns — editing the query text vs. reading the results require different interactions.
- **Insert mode must show raw text.** The `{{...}}` source needs to be visible and editable in Insert mode, but hidden in Normal mode. Mode-dependent rendering is confusing.

**Views as dedicated surfaces** solve the same problem without these issues: a view owns the full screen, has its own navigation (`j`/`k`/`x`/`Enter`/`q`), and never conflicts with buffer editing.

### Keybindings

| Binding | Action |
|---------|--------|
| `SPC v v` | **Query prompt** — type BQL, see live results, `Ctrl-S` saves as a named view |
| `SPC v l` | **List views** — fuzzy picker of all saved views, `Enter` opens |
| `SPC v d` | **Delete view** — picker, select, confirm |
| `SPC v e` | **Edit view** — picker, select, opens query prompt pre-filled |
| `SPC a a` | **Agenda** — shortcut for the built-in Agenda view |

Same pattern as `SPC f f/r/D` (files), `SPC t a/r` (tags), `SPC j j/p/n/a/t` (journal).

### Configuration

```toml
[[views]]
name = "Agenda"
query = "tasks | where not done | sort due | group due.category"
key = "SPC a a"

[[views]]
name = "Work Tasks"
query = "tasks | where not done and tags has #work | sort due"
key = "SPC v w"

[[views]]
name = "Orphan Pages"
query = "pages | where backlinks.count = 0 | sort created desc"
# no key — accessible via SPC v l only
```

Bloom ships with the Agenda view pre-filled. Users add their own. Custom keybindings are optional — views without `key` are accessible through `SPC v l`. The which-key popup under `SPC v` shows both built-in sub-keys (`v`/`l`/`d`/`e`) and user-defined view keybindings.

### Query Prompt (`SPC v v`)

Full-screen overlay. Top: BQL input with syntax highlighting and error display. Bottom: live results that update on each pause (150ms debounce). Same rendering as the agenda — task rows with checkbox, page name, due date.

Navigation within results:

| Key | Action |
|-----|--------|
| `j` / `k` | Navigate result rows |
| `x` | Toggle task (writes to source file, undo-able) |
| `Enter` | Jump to source page at the result's line |
| `o` | Open source in split |
| `q` / `Esc` | Close view |
| `Ctrl-S` | Save current query as a named view (prompts for name) |

`$page` resolves to the page that was active *before* the view opened. `$today` resolves to today's date.

### View Rendering

A named view renders as a full-screen takeover — same as the current agenda. Task list with sections (if `group` clause), source preview below, footer with count and keybinding hints. All views share one rendering path.

### Saved Views Picker (`SPC v l`)

Standard fuzzy picker. Each row: view name, keybinding (if any), truncated query as marginalia. Preview pane shows the query results. `Enter` opens the view. `Tab` action menu: edit, delete, copy query.

---

## Implementation Architecture

### BQL Pipeline (implemented)

The BQL engine is built and tested (88 tests):

```
Input string
    → Tokeniser → Vec<Token>
    → Parser → Query AST
    → Validator → ValidatedQuery (field resolution, type checking)
    → Codegen → CompiledQuery (SQL + params)
    → Executor → QueryResult (typed rows with source metadata)
    → Cache → generation-based invalidation on IndexComplete
```

Modules: `query/parse.rs`, `query/schema.rs`, `query/validate.rs`, `query/compile.rs`, `query/execute.rs`, `query/cache.rs`.

### New Modules (for views)

| Module | Responsibility |
|--------|---------------|
| `views/mod.rs` | View storage, config deserialization, CRUD operations |
| `views/prompt.rs` | Interactive query prompt state (input, debounce, live results) |
| `editor/render.rs` | View overlay rendering (reuse agenda rendering patterns) |

### View Storage

Views are defined in `config.toml` under `[[views]]`. The editor loads them on startup. `Ctrl-S` in the query prompt appends a new `[[views]]` entry to the config file. Delete removes the entry.

No separate view files, no separate database table. Views are config — portable, editable, version-controllable.

---

## Migration: Agenda as a View

The current `AgendaFrame` and dedicated agenda overlay rendering are replaced by the general view renderer. The Agenda becomes a named view with a default query:

```toml
[[views]]
name = "Agenda"
query = "tasks | where not done | sort due | group due.category"
key = "SPC a a"
```

`SPC a a` opens this view. Users can edit the query, add sections, or delete the view entirely. The dedicated agenda rendering code is replaced by the general view renderer.

---

## Decisions

1. **Views are surfaces, not documents.** Full-screen takeover, not embedded in notes.
2. **Error rendering:** Error message in `critical` colour below the query input.
3. **Performance guardrails:** No implicit limit. Show result count. Warn if >1000.
4. **Week start:** `[calendar] week_starts = "monday"` in config. Default Monday (ISO 8601).
5. **`$page` resolution:** Resolves to the page active before the view opened.
6. **`and` binds tighter than `or`.** Standard precedence, parentheses for override.

## Open Questions

1. **`contains` operator.** Useful for `tasks | where text contains "?"`. Not in the grammar yet. Leaning towards: add it — the only substring predicate needed.

---

## Non-Goals

- **Turing completeness.** BQL is a query language, not a programming language. No variables, no loops, no conditionals, no user-defined functions.
- **Write operations.** Queries are read-only projections. You can *act on* results (toggle a task), but the query itself never mutates data.
- **Cross-vault queries.** Single vault only (consistent with Bloom's v1 scope).
- **Source OR-ing.** `pages or journal | ...` is not supported. Use `blocks` for cross-source queries.
- **Double negation.** `not not done` is a parse error. Keep the grammar simple.
- **Embedded queries.** No `{{...}}` syntax in notes. Views are dedicated surfaces, not document elements.

---

## References

- [Dataview (Obsidian)](https://blacksmithgu.github.io/obsidian-dataview/) — the closest existing thing. JavaScript-based, Obsidian-only, much heavier syntax. BQL aims to be simpler.
- [Org-mode agenda](https://orgmode.org/manual/Agenda-Views.html) — the inspiration for Bloom's agenda. Powerful but complex custom agenda commands.
- [jq](https://stedolan.github.io/jq/) — pipe-based composition for JSON. BQL borrows the `|` chaining philosophy.
- [Logseq Advanced Queries](https://docs.logseq.com/#/page/advanced%20queries) — Datalog-based. Powerful but steep learning curve. BQL explicitly avoids this complexity.
