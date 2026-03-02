use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::index::{IndexError, SqliteIndex};
use crate::store::{NoteStore, StoreError};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum AgendaError {
    #[error("index error: {0}")]
    Index(#[from] IndexError),
    #[error("store error: {0}")]
    Store(#[from] StoreError),
    #[error("line {line} out of range in {path}")]
    LineOutOfRange { path: PathBuf, line: usize },
    #[error("no timestamp found on line {line} in {path}")]
    NoTimestamp { path: PathBuf, line: usize },
    #[error("no checkbox found on line {line} in {path}")]
    NoCheckbox { path: PathBuf, line: usize },
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single task/event item extracted from the vault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaItem {
    /// Source page ID.
    pub page_id: String,
    /// Source file path.
    pub path: PathBuf,
    /// Source page title.
    pub page_title: String,
    /// Line number in the indexed content (0-based).
    pub line: usize,
    /// The full text of the task/event line.
    pub text: String,
    /// The timestamp kind and date.
    pub timestamp: AgendaTimestamp,
    /// True if this is a completed task `[x]`.
    pub completed: bool,
    /// Tags on this line or page.
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaTimestamp {
    pub kind: TimestampKind,
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampKind {
    /// `@due(YYYY-MM-DD)`
    Due,
    /// `@start(YYYY-MM-DD)`
    Start,
    /// `@at(YYYY-MM-DD)`
    At,
}

/// Grouped agenda view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaView {
    pub overdue: Vec<AgendaItem>,
    pub today: Vec<AgendaItem>,
    pub upcoming: Vec<AgendaItem>,
}

// ---------------------------------------------------------------------------
// Timestamp parsing helper
// ---------------------------------------------------------------------------

/// Extract all `@due(…)`, `@start(…)`, `@at(…)` timestamps from a line.
fn parse_timestamps(line: &str) -> Vec<AgendaTimestamp> {
    let mut timestamps = Vec::new();
    let patterns: &[(&str, TimestampKind)] = &[
        ("@due(", TimestampKind::Due),
        ("@start(", TimestampKind::Start),
        ("@at(", TimestampKind::At),
    ];

    for &(prefix, kind) in patterns {
        let mut search_from = 0;
        while let Some(start) = line[search_from..].find(prefix) {
            let abs_start = search_from + start + prefix.len();
            if let Some(end) = line[abs_start..].find(')') {
                let date_str = &line[abs_start..abs_start + end];
                if let Ok(date) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                    timestamps.push(AgendaTimestamp { kind, date });
                }
                search_from = abs_start + end + 1;
            } else {
                break;
            }
        }
    }

    timestamps
}

/// Extract inline `#tag` tokens from a line (lowercased).
fn parse_line_tags(line: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'#' {
            // Must not be preceded by an alphanumeric char.
            if i > 0 && (bytes[i - 1] as char).is_alphanumeric() {
                i += 1;
                continue;
            }
            let start = i + 1;
            let mut end = start;
            while end < bytes.len() {
                let c = bytes[end] as char;
                if c.is_alphanumeric() || c == '_' || c == '-' {
                    end += 1;
                } else {
                    break;
                }
            }
            if end > start {
                tags.push(line[start..end].to_lowercase());
            }
            i = end;
        } else {
            i += 1;
        }
    }
    tags
}

fn is_completed_task(line: &str) -> bool {
    let t = line.trim_start();
    t.starts_with("- [x]") || t.starts_with("- [X]")
}

// ---------------------------------------------------------------------------
// scan_vault
// ---------------------------------------------------------------------------

/// Build a grouped agenda view from the index.
pub fn scan_vault(index: &SqliteIndex, today: NaiveDate) -> Result<AgendaView, AgendaError> {
    let pages = index.list_pages()?;

    let mut overdue = Vec::new();
    let mut today_items = Vec::new();
    let mut upcoming = Vec::new();

    for page in &pages {
        let content = match index.content_for_page_id(&page.page_id)? {
            Some(c) => c,
            None => continue,
        };

        let page_tags = index.tags_for_path(&page.path.to_string_lossy())?;

        for (line_idx, line) in content.lines().enumerate() {
            let timestamps = parse_timestamps(line);
            if timestamps.is_empty() {
                continue;
            }

            let completed = is_completed_task(line);
            let mut line_tags = parse_line_tags(line);
            for pt in &page_tags {
                if !line_tags.contains(pt) {
                    line_tags.push(pt.clone());
                }
            }

            for ts in timestamps {
                let item = AgendaItem {
                    page_id: page.page_id.clone(),
                    path: page.path.clone(),
                    page_title: page.title.clone(),
                    line: line_idx,
                    text: line.to_string(),
                    timestamp: ts.clone(),
                    completed,
                    tags: line_tags.clone(),
                };

                if ts.date < today {
                    overdue.push(item);
                } else if ts.date == today {
                    today_items.push(item);
                } else {
                    upcoming.push(item);
                }
            }
        }
    }

    // Overdue: oldest first. Upcoming: soonest first. Today: insertion order.
    overdue.sort_by_key(|i| i.timestamp.date);
    upcoming.sort_by_key(|i| i.timestamp.date);

    Ok(AgendaView {
        overdue,
        today: today_items,
        upcoming,
    })
}

// ---------------------------------------------------------------------------
// toggle_task
// ---------------------------------------------------------------------------

/// Toggle `- [ ]` ↔ `- [x]` on the given line. Returns the new completed state.
pub fn toggle_task(
    store: &impl NoteStore,
    path: &Path,
    line: usize,
) -> Result<bool, AgendaError> {
    let content = store.read(path)?;
    let lines: Vec<&str> = content.lines().collect();

    if line >= lines.len() {
        return Err(AgendaError::LineOutOfRange {
            path: path.to_path_buf(),
            line,
        });
    }

    let target = lines[line];
    let trimmed = target.trim_start();

    let (new_line, new_completed) = if trimmed.starts_with("- [ ]") {
        (target.replacen("- [ ]", "- [x]", 1), true)
    } else if trimmed.starts_with("- [x]") {
        (target.replacen("- [x]", "- [ ]", 1), false)
    } else if trimmed.starts_with("- [X]") {
        (target.replacen("- [X]", "- [ ]", 1), false)
    } else {
        return Err(AgendaError::NoCheckbox {
            path: path.to_path_buf(),
            line,
        });
    };

    let new_content = rebuild_content(&content, &lines, line, &new_line);
    store.write(path, &new_content)?;
    Ok(new_completed)
}

// ---------------------------------------------------------------------------
// reschedule_task
// ---------------------------------------------------------------------------

/// Replace the date inside the matching `@kind(…)` timestamp on the given line.
pub fn reschedule_task(
    store: &impl NoteStore,
    path: &Path,
    line: usize,
    kind: TimestampKind,
    new_date: NaiveDate,
) -> Result<(), AgendaError> {
    let content = store.read(path)?;
    let lines: Vec<&str> = content.lines().collect();

    if line >= lines.len() {
        return Err(AgendaError::LineOutOfRange {
            path: path.to_path_buf(),
            line,
        });
    }

    let target = lines[line];
    let prefix = match kind {
        TimestampKind::Due => "@due(",
        TimestampKind::Start => "@start(",
        TimestampKind::At => "@at(",
    };

    let Some(start) = target.find(prefix) else {
        return Err(AgendaError::NoTimestamp {
            path: path.to_path_buf(),
            line,
        });
    };

    let date_start = start + prefix.len();
    let Some(paren_end) = target[date_start..].find(')') else {
        return Err(AgendaError::NoTimestamp {
            path: path.to_path_buf(),
            line,
        });
    };

    let new_line = format!(
        "{}{}{}",
        &target[..date_start],
        new_date.format("%Y-%m-%d"),
        &target[date_start + paren_end..],
    );

    let new_content = rebuild_content(&content, &lines, line, &new_line);
    store.write(path, &new_content)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Rebuild file content with a single line replaced, preserving trailing newline.
fn rebuild_content(original: &str, lines: &[&str], idx: usize, replacement: &str) -> String {
    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    out[idx] = replacement.to_string();
    let mut joined = out.join("\n");
    if original.ends_with('\n') && !joined.ends_with('\n') {
        joined.push('\n');
    }
    joined
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SqliteIndex;
    use crate::parser::parse;
    use crate::store::LocalFileStore;
    use std::path::Path as StdPath;
    use tempfile::TempDir;

    fn make_index() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index)
    }

    fn index_doc(
        index: &mut SqliteIndex,
        path: &str,
        id: &str,
        title: &str,
        tags: &[&str],
        body: &str,
    ) {
        let tags_str = if tags.is_empty() {
            "[]".to_string()
        } else {
            let joined = tags
                .iter()
                .map(|t| format!("\"{t}\""))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{joined}]")
        };
        let raw = format!("---\nid: {id}\ntitle: \"{title}\"\ntags: {tags_str}\n---\n\n{body}\n");
        let doc = parse(&raw).unwrap();
        index.index_document(StdPath::new(path), &doc).unwrap();
    }

    #[test]
    fn scan_finds_overdue_and_upcoming_tasks() {
        let (_tmp, mut index) = make_index();
        let today = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();

        index_doc(
            &mut index,
            "pages/tasks.md",
            "t001",
            "Tasks",
            &[],
            "- [ ] overdue task @due(2025-06-10)\n- [ ] today task @due(2025-06-15)\n- [ ] upcoming task @due(2025-06-20)",
        );

        let view = scan_vault(&index, today).unwrap();

        assert_eq!(view.overdue.len(), 1);
        assert!(view.overdue[0].text.contains("overdue"));
        assert_eq!(
            view.overdue[0].timestamp.date,
            NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()
        );

        assert_eq!(view.today.len(), 1);
        assert!(view.today[0].text.contains("today task"));

        assert_eq!(view.upcoming.len(), 1);
        assert!(view.upcoming[0].text.contains("upcoming"));
        assert_eq!(
            view.upcoming[0].timestamp.date,
            NaiveDate::from_ymd_opt(2025, 6, 20).unwrap()
        );
    }

    #[test]
    fn scan_separates_completed_from_pending() {
        let (_tmp, mut index) = make_index();
        let today = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();

        index_doc(
            &mut index,
            "pages/mixed.md",
            "m001",
            "Mixed",
            &[],
            "- [ ] pending @due(2025-06-15)\n- [x] done @due(2025-06-15)",
        );

        let view = scan_vault(&index, today).unwrap();

        assert_eq!(view.today.len(), 2);
        let pending = view.today.iter().find(|i| !i.completed).unwrap();
        let done = view.today.iter().find(|i| i.completed).unwrap();
        assert!(pending.text.contains("pending"));
        assert!(done.text.contains("done"));
    }

    #[test]
    fn toggle_task_flips_checkbox() {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let path = store.pages_dir().join("toggle.md");
        store
            .write(&path, "- [ ] buy milk @due(2025-06-15)\n- [x] done item\n")
            .unwrap();

        // Unchecked → checked
        let result = toggle_task(&store, &path, 0).unwrap();
        assert!(result);
        let content = store.read(&path).unwrap();
        assert!(content.starts_with("- [x] buy milk"));

        // Checked → unchecked
        let result = toggle_task(&store, &path, 0).unwrap();
        assert!(!result);
        let content = store.read(&path).unwrap();
        assert!(content.starts_with("- [ ] buy milk"));
    }

    #[test]
    fn reschedule_updates_date() {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let path = store.pages_dir().join("resched.md");
        store
            .write(&path, "- [ ] task @due(2025-06-10)\n")
            .unwrap();

        let new_date = NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
        reschedule_task(&store, &path, 0, TimestampKind::Due, new_date).unwrap();

        let content = store.read(&path).unwrap();
        assert!(content.contains("@due(2025-07-01)"));
        assert!(!content.contains("@due(2025-06-10)"));
    }

    #[test]
    fn parse_timestamps_extracts_all_kinds() {
        let line = "- [ ] meeting @due(2025-06-15) @start(2025-06-10) @at(2025-06-12)";
        let ts = parse_timestamps(line);

        assert_eq!(ts.len(), 3);
        assert!(ts.iter().any(|t| t.kind == TimestampKind::Due
            && t.date == NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()));
        assert!(ts.iter().any(|t| t.kind == TimestampKind::Start
            && t.date == NaiveDate::from_ymd_opt(2025, 6, 10).unwrap()));
        assert!(ts.iter().any(|t| t.kind == TimestampKind::At
            && t.date == NaiveDate::from_ymd_opt(2025, 6, 12).unwrap()));
    }
}
