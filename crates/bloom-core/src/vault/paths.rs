//! Centralized vault path constants.
//!
//! Every Bloom-internal path is derived from the vault root via helpers here.
//! Keeps path conventions in one place so changes propagate everywhere.

use std::path::{Path, PathBuf};

/// The `.index/` directory inside the vault root — contains bloom.db, bloom.lock,
/// and (in future) .git/ for time-travel history.
pub fn index_dir(vault_root: &Path) -> PathBuf {
    vault_root.join(".index")
}

/// SQLite database: `.index/bloom.db`
pub fn index_db(vault_root: &Path) -> PathBuf {
    index_dir(vault_root).join("bloom.db")
}

/// Single-instance lock file: `.index/bloom.lock`
pub fn lock_file(vault_root: &Path) -> PathBuf {
    index_dir(vault_root).join("bloom.lock")
}

/// Git history repo (future): `.index/.git/`
pub fn history_git_dir(vault_root: &Path) -> PathBuf {
    index_dir(vault_root).join(".git")
}
