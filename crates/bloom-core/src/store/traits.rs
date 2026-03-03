use crate::error::BloomError;
use std::path::{Path, PathBuf};

/// Events emitted when files change on disk.
#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Trait for reading/writing/listing notes. Concrete: LocalFileStore.
pub trait NoteStore: Send + Sync {
    fn read(&self, path: &Path) -> Result<String, BloomError>;
    fn write(&self, path: &Path, content: &str) -> Result<(), BloomError>;
    fn delete(&self, path: &Path) -> Result<(), BloomError>;
    fn rename(&self, from: &Path, to: &Path) -> Result<(), BloomError>;
    fn list_pages(&self) -> Result<Vec<PathBuf>, BloomError>;
    fn list_journals(&self) -> Result<Vec<PathBuf>, BloomError>;
    fn exists(&self, path: &Path) -> bool;
    fn watch(&self) -> crossbeam::channel::Receiver<FileEvent>;
}