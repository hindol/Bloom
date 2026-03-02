// File storage layer — NoteStore trait + LocalFileStore.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use chrono::Local;

// ---------------------------------------------------------------------------
// StoreError
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("File not found: {0}")]
    NotFound(PathBuf),
    #[error("Invalid UTF-8 in file: {0}")]
    InvalidUtf8(PathBuf),
}

// ---------------------------------------------------------------------------
// WriteTracker — filters self-triggered watcher events
// ---------------------------------------------------------------------------

/// Tracks recent writes made by Bloom to filter out self-triggered watcher events.
#[derive(Debug, Clone, Default)]
pub struct WriteTracker {
    recent: Arc<Mutex<HashMap<PathBuf, Instant>>>,
}

impl WriteTracker {
    pub fn new() -> Self {
        Self {
            recent: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Record that Bloom just wrote to this path.
    pub fn record(&self, path: &Path) {
        if let Ok(mut map) = self.recent.lock() {
            map.insert(path.to_path_buf(), Instant::now());
        }
    }

    /// Check if this path was written by Bloom within the last 2 seconds.
    /// If so, removes the entry and returns true.
    pub fn was_self_write(&self, path: &Path) -> bool {
        if let Ok(mut map) = self.recent.lock() {
            if let Some(when) = map.remove(path) {
                return when.elapsed().as_secs() < 2;
            }
        }
        false
    }

    /// Remove entries older than 5 seconds to prevent unbounded growth.
    pub fn cleanup(&self) {
        if let Ok(mut map) = self.recent.lock() {
            map.retain(|_, when| when.elapsed().as_secs() < 5);
        }
    }
}

// ---------------------------------------------------------------------------
// NoteStore trait
// ---------------------------------------------------------------------------

pub trait NoteStore {
    fn read(&self, path: &Path) -> Result<String, StoreError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), StoreError>;
    fn delete(&self, path: &Path) -> Result<(), StoreError>;
    fn list_pages(&self) -> Result<Vec<PathBuf>, StoreError>;
    fn list_journal(&self) -> Result<Vec<PathBuf>, StoreError>;
    fn exists(&self, path: &Path) -> bool;
}

// ---------------------------------------------------------------------------
// LocalFileStore
// ---------------------------------------------------------------------------

/// Vault subdirectories created on initialisation.
const VAULT_DIRS: &[&str] = &["pages", "journal", "templates", "images", ".index"];

pub struct LocalFileStore {
    root: PathBuf,
    write_tracker: Option<WriteTracker>,
}

impl LocalFileStore {
    /// Create a new store rooted at `root`, creating the vault directory
    /// structure (`pages/`, `journal/`, `templates/`, `images/`, `.index/`)
    /// if it does not already exist.
    pub fn new(root: PathBuf) -> Result<Self, StoreError> {
        for dir in VAULT_DIRS {
            fs::create_dir_all(root.join(dir))?;
        }
        Ok(Self {
            root,
            write_tracker: None,
        })
    }

    pub fn with_write_tracker(mut self, tracker: WriteTracker) -> Self {
        self.write_tracker = Some(tracker);
        self
    }

    pub fn pages_dir(&self) -> PathBuf {
        self.root.join("pages")
    }

    pub fn journal_dir(&self) -> PathBuf {
        self.root.join("journal")
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn today_journal_path(&self) -> PathBuf {
        let today = Local::now().format("%Y-%m-%d").to_string();
        self.journal_dir().join(format!("{today}.md"))
    }

    /// List all `.md` files in a subdirectory of the vault root.
    /// Returns paths relative to the vault root.
    fn list_md_files(&self, subdir: &str, sort: bool) -> Result<Vec<PathBuf>, StoreError> {
        let dir = self.root.join(subdir);
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().map_or(false, |ext| ext == "md"))
            .collect();
        if sort {
            paths.sort();
        }
        Ok(paths)
    }
}

impl NoteStore for LocalFileStore {
    fn read(&self, path: &Path) -> Result<String, StoreError> {
        if !path.exists() {
            return Err(StoreError::NotFound(path.to_path_buf()));
        }
        let bytes = fs::read(path)?;
        String::from_utf8(bytes).map_err(|_| StoreError::InvalidUtf8(path.to_path_buf()))
    }

    /// Atomic write: write to a `.tmp` sibling, fsync, then rename over target.
    fn write(&self, path: &Path, content: &str) -> Result<(), StoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = path.with_extension("md.tmp");

        // Write → fsync → rename (atomic write pattern from ARCHITECTURE.md)
        {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(content.as_bytes())?;
            f.sync_all()?;
        }
        fs::rename(&tmp_path, path)?;
        if let Some(ref tracker) = self.write_tracker {
            tracker.record(path);
        }
        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), StoreError> {
        if !path.exists() {
            return Err(StoreError::NotFound(path.to_path_buf()));
        }
        fs::remove_file(path)?;
        Ok(())
    }

    fn list_pages(&self) -> Result<Vec<PathBuf>, StoreError> {
        self.list_md_files("pages", false)
    }

    fn list_journal(&self) -> Result<Vec<PathBuf>, StoreError> {
        self.list_md_files("journal", true)
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }
}

// ---------------------------------------------------------------------------
// Filename sanitisation (GOALS.md G3)
// ---------------------------------------------------------------------------

/// Characters invalid on any OS that are replaced with `-`.
const INVALID_CHARS: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
const MAX_FILENAME_LEN: usize = 200;

/// Sanitise a page title into a safe filename (without `.md` extension).
pub fn sanitize_filename(title: &str) -> String {
    let mut name: String = title
        .chars()
        .map(|c| if INVALID_CHARS.contains(&c) { '-' } else { c })
        .collect();

    // Trim leading/trailing whitespace and dots
    name = name.trim().trim_matches('.').trim().to_string();

    if name.len() > MAX_FILENAME_LEN {
        let mut hasher = DefaultHasher::new();
        title.hash(&mut hasher);
        let hash = format!("{:x}", hasher.finish());
        let hash_suffix = &hash[..6.min(hash.len())];
        // Find the last char boundary at or before MAX_FILENAME_LEN to avoid
        // splitting a multi-byte UTF-8 character.
        let mut trunc = MAX_FILENAME_LEN;
        while trunc > 0 && !name.is_char_boundary(trunc) {
            trunc -= 1;
        }
        name.truncate(trunc);
        name.push_str(hash_suffix);
    }

    name
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_store() -> (TempDir, LocalFileStore) {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    #[test]
    fn test_vault_structure_created() {
        let (tmp, _store) = make_store();
        for dir in VAULT_DIRS {
            assert!(tmp.path().join(dir).is_dir(), "{dir} should exist");
        }
    }

    #[test]
    fn test_write_and_read() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("hello.md");
        let content = "# Hello\n\nWorld!";
        store.write(&path, content).unwrap();
        let read_back = store.read(&path).unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_atomic_write_no_partial() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("big.md");
        // Write a large file; if atomic, the result must be exactly what we wrote.
        let content: String = "x".repeat(1_000_000);
        store.write(&path, &content).unwrap();
        let read_back = store.read(&path).unwrap();
        assert_eq!(read_back.len(), content.len());
        assert_eq!(read_back, content);
        // The tmp file must not linger after a successful write.
        assert!(!path.with_extension("md.tmp").exists());
    }

    #[test]
    fn test_list_pages() {
        let (_tmp, store) = make_store();
        store.write(&store.pages_dir().join("a.md"), "a").unwrap();
        store.write(&store.pages_dir().join("b.md"), "b").unwrap();
        // Non-md file should be ignored
        fs::write(store.pages_dir().join("c.txt"), "c").unwrap();
        let pages = store.list_pages().unwrap();
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn test_list_journal_sorted() {
        let (_tmp, store) = make_store();
        store
            .write(&store.journal_dir().join("2026-02-28.md"), "a")
            .unwrap();
        store
            .write(&store.journal_dir().join("2026-01-15.md"), "b")
            .unwrap();
        store
            .write(&store.journal_dir().join("2026-03-01.md"), "c")
            .unwrap();
        let journal = store.list_journal().unwrap();
        assert_eq!(journal.len(), 3);
        // Should be sorted by name (= date order)
        let names: Vec<&str> = journal
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec!["2026-01-15.md", "2026-02-28.md", "2026-03-01.md"]
        );
    }

    #[test]
    fn test_delete() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("del.md");
        store.write(&path, "bye").unwrap();
        assert!(store.exists(&path));
        store.delete(&path).unwrap();
        assert!(!store.exists(&path));
    }

    #[test]
    fn test_delete_not_found() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("nope.md");
        assert!(matches!(store.delete(&path), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn test_read_not_found() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("nope.md");
        assert!(matches!(store.read(&path), Err(StoreError::NotFound(_))));
    }

    #[test]
    fn test_sanitize_special_chars() {
        assert_eq!(sanitize_filename("foo/bar\\baz:qux"), "foo-bar-baz-qux");
        assert_eq!(sanitize_filename("a*b?c\"d<e>f|g"), "a-b-c-d-e-f-g");
    }

    #[test]
    fn test_sanitize_trim() {
        assert_eq!(sanitize_filename("  hello  "), "hello");
        assert_eq!(sanitize_filename("...dots..."), "dots");
        assert_eq!(sanitize_filename(" . spaced dot . "), "spaced dot");
    }

    #[test]
    fn test_sanitize_long_title() {
        let long_title = "a".repeat(300);
        let result = sanitize_filename(&long_title);
        // Should be truncated at 200 + 6 char hash suffix
        assert!(result.len() <= MAX_FILENAME_LEN + 6);
        assert!(result.len() > MAX_FILENAME_LEN);
        assert!(result.starts_with(&"a".repeat(200)));
    }

    #[test]
    fn test_sanitize_unicode() {
        // Unicode should pass through unchanged (not invalid chars)
        assert_eq!(sanitize_filename("日本語ノート"), "日本語ノート");
        assert_eq!(sanitize_filename("café"), "café");
    }

    #[test]
    fn test_today_journal_path_format() {
        let (_tmp, store) = make_store();
        let path = store.today_journal_path();
        let name = path.file_name().unwrap().to_str().unwrap();
        // Should match YYYY-MM-DD.md
        assert!(
            name.len() == "YYYY-MM-DD.md".len(),
            "journal filename should be YYYY-MM-DD.md, got: {name}"
        );
        assert!(name.ends_with(".md"));
        // Verify it's today's date
        let expected = Local::now().format("%Y-%m-%d.md").to_string();
        assert_eq!(name, expected);
    }

    #[test]
    fn test_exists() {
        let (_tmp, store) = make_store();
        let path = store.pages_dir().join("exists.md");
        assert!(!store.exists(&path));
        store.write(&path, "hi").unwrap();
        assert!(store.exists(&path));
    }

    #[test]
    fn write_tracker_records_and_consumes() {
        let tracker = WriteTracker::new();
        let path = PathBuf::from("/tmp/test.md");
        assert!(!tracker.was_self_write(&path));
        tracker.record(&path);
        assert!(tracker.was_self_write(&path));
        assert!(!tracker.was_self_write(&path)); // consumed
    }
}
