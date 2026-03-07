//! File storage, watcher, and disk writer.
//!
//! [`LocalFileStore`] provides filesystem operations for pages and journals.
//! [`DiskWriter`] runs on a dedicated thread with debounced atomic writes
//! (temp → fsync → rename). The [`watcher`] module wraps [`notify`] for
//! live file-change detection.

pub mod disk_writer;
pub mod local;
pub mod traits;
pub mod watcher;

pub use disk_writer::{DiskWriter, WriteComplete, WriteRequest};
pub use local::LocalFileStore;
pub use traits::{FileEvent, NoteStore};
