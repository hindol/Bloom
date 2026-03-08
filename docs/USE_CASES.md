# Bloom 🌱 — Use Cases

> Every user-facing scenario in Bloom, written as testable acceptance criteria.
> See [GOALS.md](GOALS.md) for goals, [KEYBINDINGS.md](KEYBINDINGS.md) for keybinding reference.
>
> **Development approach:** Code is built to satisfy these use cases. Each UC maps to one or more automated tests via `SimInput` + `insta` snapshots. The `Verifies:` line traces each UC back to a goal or architectural property.

---

## Daily Workflow

### UC-01: Open today's journal

Verifies: G14 (Daily Journal)

1. User presses `SPC j j` in Normal mode.
2. Today's journal page opens in the current window.
3. If the journal file doesn't exist on disk, a buffer is created in memory with frontmatter (`id`, `title: "YYYY-MM-DD"`, `created`). The file is NOT written to disk yet.
4. If the file already exists, it opens normally.
5. Cursor is positioned at the end of the file, ready for appending.

### UC-02: Quick-capture a thought

Verifies: G14 (Daily Journal)

1. User is editing any page in Normal mode.
2. User presses `SPC j a`.
3. A single-line input appears at the bottom of the screen: `📓 Append to journal > _`
4. The current buffer is undisturbed — still fully visible.
5. User types "Rope crate looks promising for buffer model".
6. User presses Enter.
7. The line `- Rope crate looks promising for buffer model` is appended to today's journal.
8. If today's journal didn't exist, the file is created with frontmatter first.
9. Status bar shows "✓ Added to Mar 2 journal" for 2 seconds.
10. User is back in their original buffer, cursor unchanged.

### UC-03: Quick-capture a task

Verifies: G14 (Daily Journal)

1. User presses `SPC j t` in Normal mode.
2. A single-line input appears: `☐ Append task to journal > _`
3. User types "Review the ropey crate API".
4. User presses Enter.
5. The line `- [ ] Review the ropey crate API` is appended to today's journal.
6. Confirmation in status bar. User returns to original buffer.

### UC-04: Navigate between journal days

Verifies: G14 (Daily Journal)

1. User is viewing the journal for 2026-03-01.
2. User presses `SPC j n` — the journal for 2026-03-02 opens.
3. User presses `SPC j p` — the journal for 2026-03-01 opens again.
4. If the target date has no journal file, Bloom creates a buffer in memory (lazy file creation).

### UC-05: Jump to a specific journal date

Verifies: G14 (Daily Journal)

1. User presses `SPC j d`.
2. A date picker appears.
3. User selects 2026-02-14.
4. The journal for that date opens.

### UC-06: Use a link inside quick capture

Verifies: G14, G4 (Linking)

1. User presses `SPC j a` — quick capture input appears.
2. User types "Read about [[" — the `[[` triggers the inline link picker.
3. User selects "Rope Data Structures" from the picker.
4. `[[uuid|Rope Data Structures]]` is inserted into the capture input.
5. User finishes typing and presses Enter.
6. The full line (with link) is appended to today's journal.

---

## Page Management

### UC-07: Create a new page from template

Verifies: G19 (Templates)

1. User presses `SPC n` in Normal mode.
2. A picker shows available templates: "Blank page", "Daily journal", "Meeting notes", "Book review", "Project page", plus any user templates from `templates/`.
3. User selects "Meeting notes".
4. Picker prompts: "Page title: _". User types "Q1 Review".
5. A new buffer is created with the template content. Magic variables are auto-filled: `${AUTO}` → UUID, `${DATE}` → today, `${TITLE}` → "Q1 Review". Tags from the template are set.
6. Cursor lands on `${1:Attendees}` — the first numbered placeholder.
7. User types "Alice, Bob" — replaces the placeholder.
8. User presses Tab — cursor jumps to `${2:Topics}`.
9. User fills remaining placeholders with Tab progression.
10. After the last numbered stop, Tab moves cursor to `$0` (or end of file if no `$0`). Template mode ends.
11. The file is NOT written to disk until auto-save triggers after the first edit beyond placeholders.

### UC-08: Find and open an existing page

Verifies: G16 (Fuzzy Picker)

1. User presses `SPC f f`.
2. The Find Page picker opens, showing all pages sorted by recency.
3. User types "edt thry" — results narrow via orderless fuzzy matching.
4. "Text Editor Theory" appears as the top result with marginalia: tags (`#rust #editors`), last modified date.
5. A preview of the page content appears in the preview pane.
6. User presses Enter — the page opens in the current window.

### UC-09: Rename a page

Verifies: G3 (UUID-Based Linking)

1. User is viewing "Text Editor Theory".
2. User presses `SPC f r`.
3. An input appears pre-filled with the current title.
4. User changes it to "Text Editor Architecture".
5. User presses Enter.
6. The frontmatter `title` is updated to "Text Editor Architecture".
7. The filename on disk is renamed to `Text Editor Architecture.md`.
8. A background thread scans all files and updates display hints in links: `[[8f3a1b2c|Text Editor Theory]]` → `[[8f3a1b2c|Text Editor Architecture]]`.
9. For open buffers, the update is applied directly to the rope (cursor-safe, undo-able).
10. For files not open, the background thread writes to disk.

### UC-10: Delete a page

Verifies: G16 (Fuzzy Picker)

1. User presses `SPC f D` while viewing "Scratch Notes".
2. Bloom shows a confirmation prompt: "Delete 'Scratch Notes'? This cannot be undone."
3. User confirms.
4. The file is deleted from disk.
5. The buffer is closed.
6. Any links to this page in other files become orphaned (broken link indicators appear — G20).

### UC-11: Switch between open buffers

Verifies: G16 (Fuzzy Picker)

1. User has three pages open: today's journal, "Text Editor Theory", "Rust Programming".
2. User presses `SPC b b`.
3. The buffer picker shows all open buffers, most recently focused first.
4. Buffers with unsaved changes show `[+]`. The active buffer shows "active".
5. User selects "Rust Programming" — it opens in the current window.

### UC-12: Close a buffer

Verifies: —

1. User presses `SPC b d` while viewing "Scratch Notes".
2. If the buffer has unsaved changes, Bloom prompts: "Save changes to 'Scratch Notes'?"
3a. User chooses "Save" — file is saved, buffer is closed.
3b. User chooses "Discard" — buffer is closed without saving.
3c. User chooses "Cancel" — returns to the buffer.

### UC-13: Create a new blank page

Verifies: G19 (Templates)

1. User presses `SPC n`.
2. In the template picker, user selects "Blank page".
3. An input appears: "Page title: _"
4. User types "CRDT Notes".
5. A new buffer is created with frontmatter (`id`, `title: "CRDT Notes"`, `created`).
6. Cursor is positioned after the frontmatter, in the body.

---

## Writing & Editing

### UC-14: Basic Vim editing — insert, navigate, delete

Verifies: G7 (Vim-Like Modal Editing)

1. User opens a page. Mode is Normal. Cursor is a block.
2. User presses `i` — mode changes to Insert. Cursor becomes a bar. Status bar shows `INSERT`.
3. User types "Hello world" — text appears in the buffer.
4. User presses Escape — mode returns to Normal. Cursor becomes a block on the last character.
5. User presses `w` — cursor moves to the next word.
6. User presses `dw` — the word under the cursor is deleted.
7. Status bar shows pending keys: pressing `d` shows `d_` until the motion completes.

### UC-15: Vim operators with counts and motions

Verifies: G7 (Vim-Like Modal Editing)

1. User's buffer contains: `The quick brown fox jumps over the lazy dog`.
2. Cursor is on `T`.
3. User presses `3w` — cursor moves forward 3 words to `fox`.
4. User presses `d$` — deletes from cursor to end of line: `fox jumps over the lazy dog` is deleted.
5. User presses `u` — undo restores the deleted text.
6. User presses `.` — repeats the last command (`d$`) at the current cursor position.

### UC-16: Bloom-specific text objects

Verifies: G7, G4

1. User's buffer contains: `See [[8f3a1b2c|Text Editor Theory]] for details.`
2. Cursor is on any character within the link.
3. User presses `dil` — deletes inside the link brackets: `8f3a1b2c|Text Editor Theory`.
4. Result: `See [[]] for details.`
5. User presses `u` to undo.
6. User presses `dal` — deletes around the link including brackets.
7. Result: `See  for details.`

Repeat for other Bloom text objects:
- `i#` / `a#` on a `#tag`
- `i@` / `a@` on a `@due(2026-03-05)`
- `ih` / `ah` on a heading section

### UC-17: Visual mode selection

Verifies: G7

1. User presses `v` — enters Visual mode. Status bar shows `VISUAL`.
2. User presses `3w` — selection extends forward 3 words. Selected text is highlighted with `mild` background.
3. User presses `d` — deletes the selection. Returns to Normal mode.

### UC-18: Undo and redo

Verifies: G9 (Undo Tree)

1. User types "alpha". Then types "beta". Then types "gamma".
2. User presses `u` — "gamma" is undone. Buffer shows "alpha beta".
3. User presses `u` — "beta" is undone. Buffer shows "alpha".
4. User presses `Ctrl+R` — redo. Buffer shows "alpha beta".
5. User types "delta" instead — a new branch is created in the undo tree.
6. Buffer shows "alpha delta".
7. User presses `SPC u u` — the undo tree visualizer opens, showing the branch point.
8. User navigates to the "gamma" node and presses Enter — buffer is restored to "alpha beta gamma".

### UC-19: Undo tree visualization

Verifies: G9 (Undo Tree)

1. User has made several edits with branches (see UC-18).
2. User presses `SPC u u`.
3. A panel opens showing the tree structure with nodes and branches.
4. `j`/`k` navigates between nodes. `h`/`l` switches between branches.
5. `p` previews a state without restoring it.
6. `Enter` restores the selected state.
7. `q` closes the visualizer.

### UC-20: Command mode

Verifies: G7

1. User presses `:` in Normal mode — command input appears at the bottom.
2. User types `rebuild-index` and presses Enter.
3. The index rebuild runs. Status bar shows progress.
4. On completion, status bar shows "Index rebuilt: 147 pages, 34 tags".

### UC-21: Registers and system clipboard

Verifies: G7

1. User selects text in Visual mode and presses `"ay` — yanks to register `a`.
2. User moves to another location and presses `"ap` — pastes from register `a`.
3. User selects text and presses `"+y` — yanks to system clipboard.
4. User switches to another application and pastes — the text is there.

### UC-22: Macro recording and playback

Verifies: G7

1. User presses `qa` — starts recording macro into register `a`. Status bar shows `Recording @a`.
2. User performs a sequence of edits (e.g., `0dwjdd`).
3. User presses `q` — stops recording.
4. User presses `@a` — replays the macro.
5. User presses `5@a` — replays the macro 5 more times.

### UC-23: Dot repeat

Verifies: G7

1. User presses `ciw` and types "replacement" then Escape.
2. User moves to another word and presses `.` — that word is also replaced with "replacement".

---

## Linking

### UC-24: Create a link while writing

Verifies: G4 (Linking), G16 (Fuzzy Picker)

1. User is typing in Insert mode: `Today I learned about `.
2. User types `[[`.
3. An inline fuzzy picker appears anchored below the cursor.
4. User types "rope" — results narrow to pages matching "rope".
5. User selects "Text Editor Theory" and presses Enter.
6. `[[8f3a1b2c|Text Editor Theory]]` is inserted at the cursor.
7. The picker closes. Cursor is positioned after `]]`.
8. The link renders as "Text Editor Theory" in `strong` + underline. The `[[`, UUID, `|`, `]]` are dimmed (Tier 3 syntax noise).

### UC-25: Create a link to a non-existent page

Verifies: G4, G16

1. User types `[[` — inline picker opens.
2. User types "New Topic" — no matching page exists.
3. User presses `Alt+Enter` (or `Ctrl+Enter`).
4. A new page "New Topic" is created with auto-generated UUID and frontmatter.
5. A link to the new page is inserted at the cursor.

### UC-26: Follow a link

Verifies: G4

1. User's cursor is on a `[[link]]` in Normal mode.
2. User presses `Enter` (or `gd`).
3. The linked page opens in the current window.
4. If the link targets a specific section (`#section-id`), the cursor jumps to that heading.
5. If the link targets a block (`#block-id`), the cursor jumps to that block.

### UC-27: View backlinks to current page

Verifies: G5 (Unlinked Mentions), G16

1. User is viewing "Text Editor Theory".
2. User presses `SPC s l`.
3. The backlinks picker opens, showing all pages that link TO "Text Editor Theory".
4. Each result shows the source page title and a truncated context snippet around the link.
5. Preview pane shows the source page with the linking line highlighted.
6. User selects a result — the source page opens at the linking line.

### UC-28: Discover and promote unlinked mentions

Verifies: G5

1. User is viewing "Text Editor Theory".
2. User presses `SPC s u`.
3. The unlinked mentions picker shows pages containing the text "Text Editor Theory" that are NOT explicit `[[links]]`.
4. Each result shows context around the text match.
5. User highlights a mention and presses Tab — the mention is marked for batch promotion.
6. User marks 3 mentions total, then presses Enter.
7. All 3 text occurrences are replaced with `[[uuid|Text Editor Theory]]` links in their respective files.
8. Status bar: "Promoted 3 unlinked mentions."

### UC-29: Insert link via leader key

Verifies: G4

1. User presses `SPC l l` in Normal mode.
2. The full-screen link picker opens (same as `SPC f f` but inserts a link instead of opening).
3. User searches and selects a page.
4. `[[uuid|title]]` is inserted at the cursor position.

### UC-30: Yank link to current page

Verifies: G4

1. User is viewing "Text Editor Theory".
2. User presses `SPC l y`.
3. `[[8f3a1b2c|Text Editor Theory]]` is copied to the system clipboard.
4. Status bar: "Copied link to clipboard."

### UC-31: Yank link to current block

Verifies: G4

1. User's cursor is on a line with `^rope-perf` block ID.
2. User presses `SPC l Y`.
3. `[[8f3a1b2c^rope-perf|Ropes are O(log n)...]]` is copied to the clipboard.
4. If the current line has no block ID, Bloom generates one, appends it to the line, then copies the link.

---

## Tags

### UC-32: Add a tag to a page

Verifies: G4

1. User presses `SPC t a` while viewing a page.
2. A picker shows all existing tags (for consistency) plus a text input for new tags.
3. User types "architecture" and presses Enter.
4. `architecture` is added to the frontmatter `tags: [...]` array.
5. If the page had no tags array, one is created.

### UC-33: Remove a tag from a page

Verifies: G4

1. User presses `SPC t r`.
2. A picker shows only the tags on the current page.
3. User selects `#editors` and presses Enter.
4. The tag is removed from the frontmatter array.

### UC-34: Browse and filter by tag

Verifies: G16 (Fuzzy Picker)

1. User presses `SPC s t`.
2. The tag picker shows all tags with note counts: `#rust (23 notes)`, `#editors (8 notes)`, etc.
3. User selects `#rust`.
4. The picker transitions to the full-text search picker with `[tag:rust]` filter pre-applied.
5. User can further narrow by typing, adding more filters, or just browse all `#rust` pages.

### UC-35: Inline tag in body text

Verifies: G4

1. User types `#rust` in the body of a page (in Insert mode).
2. On save (or auto-save), the indexer picks up the inline tag.
3. The tag appears in tag searches and can be used as a filter.
4. The tag renders in `faded` style (Tier 1 — `#` is part of the tag identity).

### UC-36: Tag rename across all files

Verifies: G16

1. User opens the tag picker (`SPC s t`).
2. User highlights `#editors` and presses Tab to open the action menu.
3. User selects "Rename tag."
4. Input appears: "Rename #editors to: _"
5. User types "text-editors" and presses Enter.
6. Bloom scans all files and replaces `#editors` with `#text-editors` in frontmatter and body text.
7. Status bar: "Renamed #editors → #text-editors in 8 files."

---

## Search

### UC-37: Full-text search across all notes

Verifies: G16 (Fuzzy Picker), G12 (Structured Filters)

1. User presses `SPC s s`.
2. The search picker opens — each result is a matching line (not a page).
3. User types "rope data structure".
4. Results show matching lines with the source page name as marginalia.
5. Preview shows ±5 lines of context around the match, with the matching line highlighted.
6. User presses Enter — the source page opens with cursor at the matching line.

### UC-38: Search with stacked filters

Verifies: G12 (Structured Filters)

1. User opens search (`SPC s s`).
2. User types "buffer" — results show all lines matching "buffer".
3. User presses `Ctrl+T` — a tag filter input appears. User types "rust" and presses Enter.
4. A `[tag:rust]` filter pill appears below the search input.
5. Results narrow to lines matching "buffer" in pages tagged `#rust`.
6. User presses `Ctrl+D` — a date range filter appears. User selects "This week".
7. A `[date:this-week]` pill appears. Results narrow further.
8. User presses `Ctrl+←` to navigate to the `[tag:rust]` pill, then Backspace to remove it.
9. Results update: lines matching "buffer" from this week, any tag.

### UC-39: Search journal entries

Verifies: G16

1. User presses `SPC s j`.
2. The journal picker shows all journal entries sorted by date (newest first).
3. Each entry shows the date, item count, and tags used that day.
4. User types "feb" — filters to February entries.
5. User selects "2026-02-28" — the journal opens.

### UC-40: Search tasks by status

Verifies: G12

1. User opens search (`SPC s s`).
2. User presses `Ctrl+S` — task status filter appears.
3. User selects "Open tasks".
4. Results show all lines with `- [ ]` across all pages.
5. User can further filter by tag or date range.

---

## Tasks & Agenda

### UC-41: Create a task in a page

Verifies: G4

1. User is in Insert mode.
2. User types `- [ ] Review the ropey crate API @due(2026-03-05)`.
3. The `-` renders as `ListMarker` (`foreground`), `[ ]` renders in `accent_yellow` (Tier 1 — structural).
4. `@due` renders in `faded` (`TimestampKeyword`), the parentheses in `faded` + dim (`TimestampParens`), the date in `foreground` (`TimestampDate`). Overdue dates render in `accent_red` (`TimestampOverdue`).
5. The task appears in the agenda (UC-43).

### UC-42: Toggle a task

Verifies: G15

1. User's cursor is on a `- [ ] Review the ropey crate API` line in Normal mode.
2. User triggers `Action::ToggleTask` (via leader key, ex-command, or agenda view `x`).
3. The line changes to `- [x] Review the ropey crate API`.
4. The `[x]` renders in `accent_green` + strikethrough (`CheckboxChecked`). The task text renders in `faded` + strikethrough (`CheckedTaskText`).
5. The index is updated — the task moves from "open" to "done" in the agenda.

### UC-43: Open the agenda

Verifies: G15 (Agenda)

1. User presses `SPC a a`.
2. The agenda view opens showing:
   - **Overdue**: Tasks with `@due` in the past.
   - **Today**: Tasks with `@due` or `@start` today, plus undated tasks from today's journal.
   - **Upcoming**: Tasks with future `@due` or `@start`.
3. Each task shows its source page and due date.
4. Footer shows total: "5 open tasks across 4 pages."

### UC-44: Act on a task from the agenda

Verifies: G15

1. Agenda is open (UC-43).
2. User navigates with `j`/`k` to "Review PR".
3. User presses `x` — the task is toggled to done in the source file.
4. The task disappears from the agenda (or moves to a "Done" section if shown).
5. User navigates to another task and presses `Enter` — jumps to the source page at that line.
6. User navigates to another task and presses `o` — opens the source in a split window.

### UC-45: Reschedule a task from the agenda

Verifies: G15

1. Agenda is open. User navigates to an overdue task.
2. User presses `s` — a date picker appears.
3. User selects 2026-03-10.
4. The task's `@due(2026-02-25)` is updated to `@due(2026-03-10)` in the source file.
5. The task moves from "Overdue" to "Upcoming" in the agenda.

### UC-46: Filter agenda by tag

Verifies: G15

1. Agenda is open.
2. User presses `t` — tag filter input appears.
3. User types "work" and presses Enter.
4. Agenda shows only tasks from pages tagged `#work`.

### UC-47: Insert a timestamp via leader key

Verifies: G4

1. User presses `SPC i d` (insert due date).
2. A date picker appears.
3. User selects 2026-03-10.
4. `@due(2026-03-10)` is inserted at the cursor.

Repeat for `SPC i s` (`@start`) and `SPC i a` (`@at`).

---

## Timeline

### UC-48: Open the timeline for a page

Verifies: G6 (Timeline View)

1. User is viewing "Text Editor Theory".
2. User presses `SPC l t`.
3. A timeline panel opens showing a chronological list of all notes that link to "Text Editor Theory".
4. Each entry shows: date, source page title, and an excerpt (context around the link).
5. Entries are sorted newest first.

### UC-49: Navigate and act on timeline entries

Verifies: G6

1. Timeline is open (UC-48).
2. User presses `j`/`k` — moves between entries.
3. User presses `Enter` on an entry — jumps to the source note at the linking line.
4. User presses `o` on an entry — opens the source in a split window.
5. User presses `e` — toggles the entry between excerpt and full content view.
6. User presses `q` — closes the timeline.

### UC-50: Pin timeline alongside editor

Verifies: G6, G11 (Window Management)

1. User opens the timeline (`SPC l t`).
2. The timeline opens in a split window (vertical by default).
3. User can continue editing in the left pane while the timeline is visible on the right.
4. Navigating to a different page updates the timeline to show that page's backlinks.

### UC-51: Timeline with no backlinks

Verifies: G6

1. User opens "Brand New Page" that has no backlinks.
2. User presses `SPC l t`.
3. The timeline opens but shows: "No notes link to this page yet."
4. The panel remains open — it will update as links are created.

---

## Windows & Navigation

### UC-52: Vertical and horizontal splits

Verifies: G11 (Window Management)

1. User presses `SPC w v` — the window splits vertically. Both panes show the same buffer.
2. User presses `SPC b b` in the right pane and opens a different page.
3. User presses `SPC w s` — the right pane splits horizontally.
4. Three panes are now visible, each independently navigable.

### UC-53: Navigate between windows

Verifies: G11

1. User has multiple panes open.
2. User presses `SPC w h` — focus moves to the left pane.
3. User presses `SPC w l` — focus moves to the right pane.
4. `SPC w j` — down. `SPC w k` — up.

### UC-54: Resize and balance windows

Verifies: G11

1. User presses `SPC w >` — current window widens.
2. User presses `SPC w <` — current window narrows.
3. User presses `SPC w =` — all windows balance to equal sizes.

### UC-55: Maximize and restore

Verifies: G11

1. User presses `SPC w m` — current window maximizes to full screen. Other panes are hidden.
2. User presses `SPC w m` again — layout is restored to the previous split configuration.

### UC-56: Close a window

Verifies: G11

1. User presses `SPC w d` — the current window closes. The buffer remains open (just not visible).
2. If this is the last window, the buffer stays open and the window cannot be closed.

### UC-57: Move buffer to another window

Verifies: G11

1. User presses `SPC w H` — moves the current buffer to the window on the left.
2. `SPC w L` — right. `SPC w J` — down. `SPC w K` — up.

---

## Templates

### UC-58: Use a built-in template

Verifies: G19 (Templates)

1. User presses `SPC n` → template picker shows available templates with names and descriptions.
2. User selects "Meeting notes".
3. Picker prompts: "Page title: _". User types "Sprint Retrospective".
4. New buffer is created. `${AUTO}` → UUID, `${DATE}` → today, `${TITLE}` → "Sprint Retrospective".
5. Cursor lands on `${1:Attendees}`.
6. User types "Alice, Bob, Carol".
7. Tab → cursor jumps to `${2:Topics}`.
8. User types "Q1 Review, Roadmap".
9. Tab → cursor jumps to `${3:First action item}`.
10. User types "Follow up on budget".
11. Tab → cursor moves to `$0` (final cursor position, in the Notes section). Template mode ends.
12. Next Tab press inserts a normal tab character.

### UC-58a: Template mirroring

Verifies: G19 (Templates)

1. A template contains `${1:Component}` in two places: the frontmatter title and a heading.
2. User types "Authentication" for `${1:Component}`.
3. User presses Tab to advance to `${2:...}`.
4. Both occurrences of `${1:Component}` are replaced with "Authentication" (search-and-replace).

### UC-58b: Escape mid-template

Verifies: G19 (Templates)

1. User is filling `${1:Attendees}` in Insert mode.
2. User presses Escape → returns to Normal mode. Template mode persists.
3. User navigates with Vim motions, then presses `i` to re-enter Insert mode.
4. User presses Tab → cursor advances to `${2:Topics}`. Template mode still active.

### UC-58c: Skip a placeholder

Verifies: G19 (Templates)

1. Cursor is on `${1:Attendees}`.
2. User presses Tab without typing.
3. The text "Attendees" remains as literal content.
4. Cursor advances to `${2:Topics}`.

### UC-59: Create a custom template

Verifies: G19

1. User creates a file `templates/bug-report.md`:
   ```markdown
   <!-- template: Bug Report | Track and document software bugs with reproduction steps -->
   ---
   id: ${AUTO}
   title: "${TITLE}"
   created: ${DATE}
   tags: [bug]
   ---

   ## Description
   ${1:What happened?}

   ## Steps to Reproduce
   ${2:Steps}

   ## Expected Behavior
   ${3:What should happen?}

   $0
   ```
2. Next time user presses `SPC n`, "Bug Report" appears in the picker with description "Track and document software bugs with reproduction steps".
3. If the `<!-- template: ... -->` comment is omitted, the name is derived from the filename: `bug-report.md` → "Bug report".

### UC-60: Template with auto-filled values

Verifies: G19

1. User creates a page from any template. Picker prompts for a title.
2. `${AUTO}` in the frontmatter `id` field is replaced with a generated 8-char hex UUID.
3. `${DATE}` is replaced with today's ISO date.
4. `${TITLE}` is replaced with the title the user entered in the picker prompt.
4. These are filled silently — the cursor does not stop on them.

### UC-61: Template available via MCP

Verifies: G17 (MCP), G19

1. An LLM client calls `create_note` with `template: "meeting-notes"` and `template_values: { "1": "Sprint Retro", "2": "Team" }`.
2. Bloom creates a page using the meeting notes template with placeholders filled.
3. Unfilled placeholders remain as plain text.

---

## Refactoring

### UC-62: Split a page — extract section

Verifies: G18 (Note Refactoring)

1. User is viewing "Text Editor Theory" which has sections: "Rope Data Structure", "Piece Table", "Gap Buffer".
2. User presses `SPC r s`.
3. A picker shows all headings in the current page.
4. User selects "Rope Data Structure".
5. Bloom prompts: "New page title: _" (pre-filled with "Rope Data Structure").
6. User confirms.
7. A new page "Rope Data Structure" is created with the extracted content.
8. In "Text Editor Theory", the section is replaced with `[[new-uuid|Rope Data Structure]]`.
9. All existing links to blocks within that section are updated to point to the new page.

### UC-63: Merge two pages

Verifies: G18

1. User presses `SPC r m`.
2. A picker appears: "Merge which page into current?" User selects "CRDT Notes".
3. Bloom prompts: "Merge CRDT Notes into Text Editor Theory?"
4. User confirms.
5. The content of "CRDT Notes" is appended under a `## CRDT Notes` heading in "Text Editor Theory".
6. All links to "CRDT Notes" in other files are updated to point to "Text Editor Theory".
7. The "CRDT Notes" file is deleted.

### UC-64: Move a block to another page

Verifies: G18

1. User positions cursor on a block (paragraph or list item).
2. User presses `SPC r b`.
3. A picker appears: "Move block to which page?" User selects "Rust Programming".
4. Bloom moves the block to the end of "Rust Programming".
5. If the block had a `^block-id`, the ID follows it — all links remain valid.
6. If the block had no block ID, no links need updating.

---

## MCP Server

### UC-65: LLM searches notes

Verifies: G17 (MCP Server)

1. MCP server is enabled in `config.toml`.
2. An LLM client calls `search_notes` with `query: "rope data structure"`.
3. Bloom returns matching lines with page titles and context.
4. Results respect `exclude_paths` — excluded files are not returned.

### UC-66: LLM reads a note

Verifies: G17

1. LLM calls `read_note` with `title: "Text Editor Theory"`.
2. Bloom fuzzy-matches the title, finds the page, returns its raw Markdown.
3. The Markdown shows display hints in links (`[[uuid|Text Editor Theory]]`), not bare UUIDs.
4. If the title is ambiguous (multiple close matches), Bloom returns an error with top-N candidates.

### UC-67: LLM creates a note

Verifies: G17

1. LLM calls `create_note` with `title: "CRDT Overview"`, `content: "..."`, `tags: ["crdt", "distributed"]`.
2. Bloom creates a new page with auto-generated UUID, frontmatter, and the provided content.
3. If a page titled "CRDT Overview" already exists, the content is appended to the end.

### UC-68: LLM edits a note

Verifies: G17

1. LLM calls `edit_note` with `title: "Text Editor Theory"`, `old_text: "binary trees to represent text."`, `new_text: "binary trees to represent text.\n\nGood for large files. Used by Xi Editor and Zed."`.
2. Bloom fuzzy-matches the title, finds the page, searches for `old_text` in the buffer.
3. The text is found — Bloom replaces it with `new_text`.
4. The edit goes through the rope buffer (same one the UI uses).
5. If the user has the page open, the edit appears in real-time.
6. The edit is a node in the undo tree — the user can undo it.
7. Status bar shows: `MCP: edited Text Editor Theory`.
8. If `old_text` is not found: error "Text not found in page. Re-read and try again."
9. If `old_text` matches multiple locations: error "Ambiguous match (N occurrences). Include more context."

### UC-69: LLM adds to journal

Verifies: G17

1. LLM calls `add_to_journal` with `content: "Discussed CRDT approach with team"`.
2. The line is appended to today's journal (creating it if needed).
3. LLM calls `add_to_journal` with `content: "..."`, `date: "2026-02-28"` — appends to that specific journal.

### UC-70: LLM toggles a task

Verifies: G17

1. LLM calls `toggle_task` with `title: "Text Editor Theory"`, `task_text: "Review ropey"`.
2. Bloom fuzzy-matches the task text, finds `- [ ] Review the ropey crate API`, toggles it to `- [x]`.
3. The change appears in the UI in real-time.

### UC-71: MCP in read-only mode

Verifies: G17

1. `config.toml` has `mode = "read-only"` under `[mcp]`.
2. LLM calls `search_notes` — works normally.
3. LLM calls `edit_note` — Bloom returns a permission error: "MCP server is in read-only mode."

### UC-72: MCP path exclusion

Verifies: G17

1. `config.toml` has `exclude_paths = ["journal/therapy/*"]`.
2. LLM calls `search_notes` with a query matching content in `journal/therapy/2026-03-01.md`.
3. That file is NOT included in results.
4. LLM calls `read_note` targeting a therapy journal — Bloom returns an error as if the page doesn't exist.

---

## System & Setup

### UC-73: First launch — setup wizard

Verifies: G21 (Setup Wizard)

1. User launches Bloom for the first time. No vault exists.
2. Setup wizard appears: "Choose vault location" (default: `~/bloom/`).
3. User accepts the default.
4. Bloom creates the vault directory structure: `pages/`, `journal/`, `templates/`, `images/`, `.index/`, `.gitignore`, `config.toml`.
5. Wizard asks: "Import from Logseq?" with options "Yes" / "No".
6. User selects "No" — wizard completes.
7. Today's journal opens.

### UC-74: Import from Logseq

Verifies: G13 (Logseq Import)

1. User triggers import via setup wizard or `:import-logseq` command.
2. Bloom prompts for the Logseq directory path.
3. Bloom reads the Logseq directory (never modifies it).
4. Files are copied and transformed:
   - `journals/` → `journal/` (filenames: `2026_02_28.md` → `2026-02-28.md`)
   - `pages/` → `pages/`
   - `assets/` → `images/`
   - `[[page link]]` → `[[uuid|page link]]`
   - `((block-ref))` → `[[page-uuid^block-id|text]]`
   - `{{embed ...}}` → `[[uuid|page]]` (converted to links)
   - Properties → YAML frontmatter
5. An import report is displayed: pages imported, links resolved, warnings, errors.
6. User reviews the report. Failed files can be re-imported individually.

### UC-75: Logseq import — idempotent re-run

Verifies: G13

1. User runs import again on the same Logseq directory.
2. Already-imported pages (matched by title) are skipped.
3. Only new/changed files are imported.
4. No duplicates are created.

### UC-76: Rebuild the index

Verifies: G22 (Index Rebuild)

1. User runs `:rebuild-index` (or `SPC h r`).
2. Bloom re-scans all `.md` files in the vault.
3. The SQLite index is rebuilt from scratch: pages, links, backlinks, tags, FTS5 entries.
4. Status bar shows progress: "Rebuilding index... 47/147 pages".
5. On completion: "Index rebuilt: 147 pages, 523 links, 34 tags."

### UC-77: Session restore

Verifies: G23 (Session Restore)

1. User has 3 buffers open, split into 2 panes, with specific cursor positions and scroll positions.
2. User quits Bloom.
3. User relaunches Bloom.
4. The session is restored: same 3 buffers, same 2-pane layout, same cursor and scroll positions.
5. Undo trees are NOT restored (G9 — RAM-only).

### UC-78: Startup mode — journal

Verifies: G23

1. User has `startup.mode = "journal"` in `config.toml`.
2. User launches Bloom.
3. Today's journal opens directly (no session restore).

---

## Edge Cases & Error Handling

### UC-79: Broken link detection

Verifies: G20 (Orphaned Link Indicators)

1. User opens a page containing `[[deadbeef|Deleted Page]]`.
2. The UUID `deadbeef` does not match any existing page.
3. The link renders in `critical` colour + strikethrough.
4. A tooltip (or status bar hint when cursor is on the link) shows: "Page not found: deadbeef."
5. User presses `]l` — jumps to the next broken link. `[l` — previous broken link.

### UC-80: External file change — clean buffer

Verifies: ARCHITECTURE (Data Safety)

1. User has "Rust Programming" open with NO unsaved changes.
2. An external tool modifies `pages/Rust Programming.md` on disk.
3. The file watcher detects the change.
4. Bloom silently reloads the buffer from disk.
5. The undo tree is preserved up to the reload point.

### UC-81: External file change — dirty buffer

Verifies: ARCHITECTURE (Data Safety)

1. User has "Rust Programming" open WITH unsaved edits ([+] in status bar).
2. An external tool modifies the same file on disk.
3. Bloom shows a prompt: "File changed on disk. Reload (losing edits) or keep buffer?"
4a. "Reload": buffer is replaced with disk contents. Undo tree is reset.
4b. "Keep": buffer retains in-memory edits. Next save overwrites disk version.

### UC-82: Git merge conflict detection

Verifies: G21 (Vault Structure)

1. User pulls from git and a merge conflict occurs in `pages/Shared Notes.md`.
2. The file now contains `<<<<<<<`, `=======`, `>>>>>>>` markers.
3. Bloom detects the conflict markers on file load.
4. The file opens in raw/degraded mode: no link resolution, no indexing.
5. A warning appears: "Merge conflict detected. Resolve manually, then save to re-index."
6. User resolves the conflict, removes markers, saves.
7. Bloom re-parses and re-indexes the file.

### UC-83: File adoption — unrecognized .md file

Verifies: G21

1. User manually places `research.md` (no frontmatter) into the `pages/` directory.
2. The file watcher detects the new file.
3. Bloom auto-adds frontmatter: generates UUID, derives title from filename ("Research").
4. A notification appears: "Adopted 'Research' — frontmatter added."
5. The file is now a full Bloom page, searchable and linkable.

### UC-84: UUID collision

Verifies: G3

1. During page creation, Bloom generates UUID `8f3a1b2c`.
2. The index already contains a page with that UUID.
3. Bloom regenerates: tries a new random 8-char hex.
4. Repeats until a unique UUID is found.
5. The collision is transparent to the user.

### UC-85: Case-insensitive filename collision

Verifies: G3

1. User creates a page titled "Rust".
2. A file `rust.md` already exists (from a page titled "rust").
3. On a case-insensitive filesystem (Windows, macOS), this would collide.
4. Bloom detects the collision and creates the file as `Rust-8f3a1b2c.md` (UUID suffix).
5. The frontmatter title remains "Rust" — the filename is just a filesystem accommodation.

### UC-86: Auto-save and crash recovery

Verifies: ARCHITECTURE (Data Safety)

1. User is typing in a page. Auto-save triggers 300ms after the last keystroke.
2. The disk writer uses atomic write: write to temp file → fsync → rename over target.
3. If Bloom crashes mid-write, the atomic rename ensures either the old or new file exists — never a half-written file.
4. On next launch, the file is intact (either the pre-save or post-save version).

---

## Discoverability

### UC-87: Which-key popup

Verifies: G8 (Which-Key)

1. User presses `SPC` in Normal mode.
2. After ~300ms timeout, a popup appears showing available next keys:
   ```
   f → files    s → search    l → links    j → journal
   t → tags     a → agenda    n → new page w → windows
   ```
3. User presses `f` — the popup updates to show file sub-commands:
   ```
   f → find page    r → rename    D → delete
   ```
4. User presses `f` — `SPC f f` executes (find page picker opens).

### UC-88: Which-key during Vim grammar

Verifies: G8

1. User presses `d` in Normal mode (start of a delete command).
2. After ~300ms, a popup shows available motions/text objects:
   ```
   w → word    $ → end of line    d → line    iw → inner word
   ap → around paragraph    il → inner link    ...
   ```
3. User presses `w` — `dw` executes.

### UC-89: All commands — M-x equivalent

Verifies: G16

1. User presses `SPC SPC`.
2. The command picker opens, listing all Bloom commands with keybindings and categories.
3. User types "split" — results narrow to "Window: Vertical Split (SPC w v)", "Window: Horizontal Split (SPC w s)", "Refactor: Split Page (SPC r s)".
4. User selects a command — it executes immediately.

### UC-90: Platform shortcuts override Vim

Verifies: G10 (Cross-Platform)

1. User is in Normal mode.
2. User presses `Cmd+S` (macOS) or `Ctrl+S` (Windows).
3. The file saves, regardless of Vim state. Platform shortcuts are checked first.
4. If user is mid-command (e.g., pressed `d` and is waiting for a motion), the Vim pending state is canceled and the save executes.

---

## Related Documents

| Document | Contents |
|----------|----------|
| [GOALS.md](GOALS.md) | Goals and non-goals that these use cases verify |
| [KEYBINDINGS.md](KEYBINDINGS.md) | Keybinding reference |
| [PICKER_SURFACES.md](PICKER_SURFACES.md) | Picker wireframes |
| [CRATE_STRUCTURE.md](CRATE_STRUCTURE.md) | Testing strategy (SimInput, insta snapshots) |
