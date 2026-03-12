use crate::traits::{FileEvent, NoteStore};
use crate::watcher::start_watcher;
use bloom_error::BloomError;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub struct LocalFileStore {
    root: PathBuf,
    watcher_rx: crossbeam::channel::Receiver<FileEvent>,
    #[allow(dead_code)]
    watcher_tx: crossbeam::channel::Sender<FileEvent>,
    #[allow(dead_code)]
    _watcher: notify::RecommendedWatcher,
}

impl LocalFileStore {
    pub fn new(root: PathBuf) -> Result<Self, BloomError> {
        let (watcher_tx, watcher_rx) = crossbeam::channel::unbounded();
        let _watcher = start_watcher(&root, watcher_tx.clone())?;
        Ok(Self {
            root,
            watcher_rx,
            watcher_tx,
            _watcher,
        })
    }

    /// Recursively collect all `.md` files under `dir`.
    fn collect_md_files(&self, dir: &Path) -> Result<Vec<PathBuf>, BloomError> {
        let mut results = Vec::new();
        if !dir.exists() {
            return Ok(results);
        }
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                results.extend(self.collect_md_files(&path)?);
            } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
                results.push(path);
            }
        }
        Ok(results)
    }
}

impl NoteStore for LocalFileStore {
    fn read(&self, path: &Path) -> Result<String, BloomError> {
        let full = self.root.join(path);
        Ok(fs::read_to_string(full)?)
    }

    fn write(&self, path: &Path, content: &str) -> Result<(), BloomError> {
        let full = self.root.join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent)?;
        }
        // Atomic write: write to tmp, fsync, rename
        let tmp = full.with_extension("tmp");
        let mut file = fs::File::create(&tmp)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
        fs::rename(&tmp, &full)?;
        Ok(())
    }

    fn delete(&self, path: &Path) -> Result<(), BloomError> {
        let full = self.root.join(path);
        fs::remove_file(full)?;
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> Result<(), BloomError> {
        let full_from = self.root.join(from);
        let full_to = self.root.join(to);
        if let Some(parent) = full_to.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(full_from, full_to)?;
        Ok(())
    }

    fn list_pages(&self) -> Result<Vec<PathBuf>, BloomError> {
        self.collect_md_files(&self.root.join("pages"))
    }

    fn list_journals(&self) -> Result<Vec<PathBuf>, BloomError> {
        self.collect_md_files(&self.root.join("journal"))
    }

    fn exists(&self, path: &Path) -> bool {
        self.root.join(path).exists()
    }

    fn watch(&self) -> crossbeam::channel::Receiver<FileEvent> {
        self.watcher_rx.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::NoteStore;
    use tempfile::TempDir;

    fn setup() -> (TempDir, LocalFileStore) {
        let dir = TempDir::new().unwrap();
        let pages_dir = dir.path().join("pages");
        let journal_dir = dir.path().join("journal");
        std::fs::create_dir_all(&pages_dir).unwrap();
        std::fs::create_dir_all(&journal_dir).unwrap();
        let store = LocalFileStore::new(dir.path().to_path_buf()).unwrap();
        (dir, store)
    }

    // UC-86: Read/write
    #[test]
    fn test_write_and_read() {
        let (_dir, store) = setup();
        let path = Path::new("pages").join("test.md");
        store.write(&path, "hello world").unwrap();
        let content = store.read(&path).unwrap();
        assert_eq!(content, "hello world");
    }

    #[test]
    fn test_exists() {
        let (_dir, store) = setup();
        let path = Path::new("pages").join("test.md");
        assert!(!store.exists(&path));
        store.write(&path, "content").unwrap();
        assert!(store.exists(&path));
    }

    #[test]
    fn test_delete() {
        let (_dir, store) = setup();
        let path = Path::new("pages").join("test.md");
        store.write(&path, "content").unwrap();
        store.delete(&path).unwrap();
        assert!(!store.exists(&path));
    }

    #[test]
    fn test_list_pages() {
        let (dir, store) = setup();
        std::fs::write(dir.path().join("pages").join("a.md"), "content").unwrap();
        std::fs::write(dir.path().join("pages").join("b.md"), "content").unwrap();
        let pages = store.list_pages().unwrap();
        assert_eq!(pages.len(), 2);
    }

    #[test]
    fn test_list_journals() {
        let (dir, store) = setup();
        std::fs::write(dir.path().join("journal").join("2026-03-01.md"), "content").unwrap();
        let journals = store.list_journals().unwrap();
        assert_eq!(journals.len(), 1);
    }

    #[test]
    fn test_rename() {
        let (_dir, store) = setup();
        let from = Path::new("pages").join("old.md");
        let to = Path::new("pages").join("new.md");
        store.write(&from, "content").unwrap();
        store.rename(&from, &to).unwrap();
        assert!(!store.exists(&from));
        assert!(store.exists(&to));
    }
}
