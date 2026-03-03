use crate::error::BloomError;
use crate::store::traits::{FileEvent, NoteStore};
use crate::store::watcher::start_watcher;
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