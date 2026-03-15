# Auto-Merge 🔀

> Three-way merge for concurrent edits — eliminate the "reload or keep?" prompt.
> Status: **Draft** — exploratory, not committed.
> Depends on: [HISTORY.md](HISTORY.md) (git provides the base version).

---

## The Problem

You're editing a page in Bloom. Meanwhile, Syncthing syncs a newer version, or you `git checkout` in another terminal, or an external editor saves the same file. Bloom detects the change and asks:

> "File changed on disk. Reload (losing edits) or keep buffer version?"

This is a false dilemma. In most cases, the external change and your edits touch different parts of the file. A merge would preserve both — no data loss, no prompt, no interruption.

---

## The Design

### Three-Way Merge

Every merge needs three versions:

| Version | Source | Description |
|---------|--------|-------------|
| **Base** | Last git commit touching this file (via `History::file_at_commit`) | The common ancestor — what both sides started from |
| **Ours** | Current rope buffer content | Your in-progress edits |
| **Theirs** | `std::fs::read_to_string(&path)` | The external change on disk |

The merge algorithm compares ours and theirs against the base. Lines changed only in ours → keep ours. Lines changed only in theirs → keep theirs. Lines changed in both → conflict.

### Outcomes

| Situation | Action |
|-----------|--------|
| **Clean merge** (no conflicts) | Silently apply merged content to the buffer. Notification: "Merged external changes (3 lines)." No prompt. |
| **Conflicts** | Show a split diff view with conflict markers highlighted. User resolves in the editor. Same UX as git merge conflict resolution (G21 already specs this). |
| **Base unavailable** (no git history yet) | Fall back to the current "reload or keep?" prompt. |

### When It Triggers

Same as today — when `handle_file_event` detects an external change to an open dirty buffer. Instead of showing the dialog, it attempts a three-way merge first. The dialog is the fallback for conflicts or missing base.

```
File watcher: external change detected
    │
    ├── Buffer is clean → silent reload (existing behavior, unchanged)
    │
    └── Buffer is dirty →
            │
            ├── Base available (git history) →
            │       │
            │       ├── Merge succeeds → apply silently, notify
            │       │
            │       └── Merge conflicts → show conflict resolution view
            │
            └── Base unavailable → "Reload or keep?" prompt (existing)
```

---

## Implementation

### Merge Engine

A new module `merge.rs` in bloom-core:

```rust
pub enum MergeResult {
    /// Clean merge — all changes compatible.
    Clean(String),
    /// Conflicts — merged content with conflict markers.
    Conflict {
        content: String,
        conflict_count: usize,
    },
}

/// Three-way line-level merge.
pub fn merge_three_way(base: &str, ours: &str, theirs: &str) -> MergeResult;
```

Uses the `similar` crate (already available for diff in many Rust projects, or `diffy` for three-way). The algorithm:

1. Diff `base → ours` → set of our changes (hunks).
2. Diff `base → theirs` → set of their changes (hunks).
3. Apply non-overlapping hunks from both sides.
4. Overlapping hunks → conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`).

### Integration with `handle_file_event`

In `editor/files.rs`, replace the dirty-buffer branch:

```rust
// Current: show dialog
// New: attempt three-way merge
let base = self.history_base_for_page(&page_id);
match base {
    Some(base_content) => {
        let buf_content = self.buffer_mgr.get(&page_id).map(|b| b.text().to_string());
        match merge::merge_three_way(&base_content, &buf_content, &disk_content) {
            MergeResult::Clean(merged) => {
                self.buffer_mgr.reload(&page_id, &merged);
                self.push_notification("Merged external changes", Info);
            }
            MergeResult::Conflict { content, conflict_count } => {
                // Load with conflict markers, enter degraded mode (G21)
                self.buffer_mgr.reload(&page_id, &content);
                self.push_notification(
                    format!("{conflict_count} conflicts — resolve manually"),
                    Warning,
                );
            }
        }
    }
    None => {
        // No base available — fall back to existing prompt
        self.active_dialog = Some(ActiveDialog::FileChanged { ... });
    }
}
```

### Base Version Source

The base comes from git history — the most recent commit's version of this file. `History::file_at_commit(uuid, HEAD)` gives the last committed content. This is the common ancestor because:

- Auto-save writes to disk → auto-commit captures it in git.
- The buffer was loaded from disk, which matches the last commit.
- External changes and buffer edits both diverge from this commit.

If the file has never been committed (brand new, created this session), no base is available. Fall back to the prompt.

---

## Open Questions

1. **Merge granularity.** Line-level is standard (git's default). Character-level would preserve more but is more complex and can produce surprising results. Start with line-level.

2. **Conflict marker format.** Use git's standard `<<<<<<<`/`=======`/`>>>>>>>` markers? Bloom already detects these (G21) and enters degraded mode. Reusing the same markers means the existing conflict resolution UX applies unchanged.

3. **Auto-save after merge.** Should a clean merge trigger auto-save immediately? Leaning yes — the merged content should be persisted to avoid a second round of merge if another external change arrives.

4. **Undo integration.** A clean merge modifies the buffer — should it be an undo-able operation? Yes — `buf.reload()` should go through the undo tree so the user can undo the merge and get back to their pre-merge edits.

---

## Dependencies

| Crate | Purpose |
|-------|---------|
| `similar` or `diffy` | Three-way diff/merge algorithm |

---

## References

- Current conflict detection: [GOALS.md G21](../GOALS.md) (git merge conflict markers)
- Current external change handling: `editor/files.rs` `handle_file_event`
- Git base version: [HISTORY.md](HISTORY.md) (`History::file_at_commit`)
