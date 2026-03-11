//! Auto-migration from legacy vault layout to `.index/` directory structure.
//!
//! Legacy layout:
//!   - `vault_root/.index.db`
//!   - `vault_root/.bloom.lock`
//!
//! New layout:
//!   - `vault_root/.index/bloom.db`
//!   - `vault_root/.index/bloom.lock`
//!   - `vault_root/.index/.git/`  (future, for time-travel history)
//!
//! Migration runs once on startup, before the lock is acquired.

use std::fs;
use std::path::Path;

use super::paths;

/// Migrate legacy `.index.db` and `.bloom.lock` into the `.index/` directory.
/// No-op if no legacy files exist or if migration is already done.
pub(crate) fn migrate_if_needed(vault_root: &Path) {
    let legacy_db = vault_root.join(".index.db");
    let legacy_lock = vault_root.join(".bloom.lock");

    let has_legacy = legacy_db.exists() || legacy_lock.exists();
    if !has_legacy {
        return;
    }

    let index_dir = paths::index_dir(vault_root);
    if let Err(e) = fs::create_dir_all(&index_dir) {
        tracing::error!(error = %e, "failed to create .index/ directory during migration");
        return;
    }

    // Migrate .index.db → .index/bloom.db
    if legacy_db.exists() {
        let new_db = paths::index_db(vault_root);
        if !new_db.exists() {
            match fs::rename(&legacy_db, &new_db) {
                Ok(()) => {
                    tracing::warn!(
                        from = %legacy_db.display(),
                        to = %new_db.display(),
                        "migrated legacy index database to .index/ directory"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to migrate .index.db → .index/bloom.db");
                }
            }
        } else {
            // New file already exists — remove the legacy one.
            tracing::warn!("both .index.db and .index/bloom.db exist; removing legacy .index.db");
            let _ = fs::remove_file(&legacy_db);
        }
    }

    // Migrate .bloom.lock → .index/bloom.lock
    // The lock file may be stale, so we just move it. The normal lock
    // acquisition logic handles stale PID detection.
    if legacy_lock.exists() {
        let new_lock = paths::lock_file(vault_root);
        if !new_lock.exists() {
            match fs::rename(&legacy_lock, &new_lock) {
                Ok(()) => {
                    tracing::warn!(
                        from = %legacy_lock.display(),
                        to = %new_lock.display(),
                        "migrated legacy lock file to .index/ directory"
                    );
                }
                Err(e) => {
                    tracing::error!(error = %e, "failed to migrate .bloom.lock → .index/bloom.lock");
                    // Non-fatal — lock acquisition will create a fresh lock.
                }
            }
        } else {
            let _ = fs::remove_file(&legacy_lock);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn migrate_moves_legacy_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create legacy layout.
        fs::write(root.join(".index.db"), "sqlite data").unwrap();
        fs::write(root.join(".bloom.lock"), "12345").unwrap();

        migrate_if_needed(root);

        // Legacy files should be gone.
        assert!(!root.join(".index.db").exists());
        assert!(!root.join(".bloom.lock").exists());

        // New files should exist.
        assert!(paths::index_db(root).exists());
        assert!(paths::lock_file(root).exists());

        // Content preserved.
        assert_eq!(fs::read_to_string(paths::index_db(root)).unwrap(), "sqlite data");
        assert_eq!(fs::read_to_string(paths::lock_file(root)).unwrap(), "12345");
    }

    #[test]
    fn migrate_noop_when_no_legacy_files() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        migrate_if_needed(root);

        // Nothing should be created.
        assert!(!paths::index_dir(root).exists());
    }

    #[test]
    fn migrate_removes_legacy_when_new_exists() {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        // Create both legacy and new layout.
        fs::write(root.join(".index.db"), "old").unwrap();
        fs::create_dir_all(paths::index_dir(root)).unwrap();
        fs::write(paths::index_db(root), "new").unwrap();

        migrate_if_needed(root);

        // Legacy removed, new preserved.
        assert!(!root.join(".index.db").exists());
        assert_eq!(fs::read_to_string(paths::index_db(root)).unwrap(), "new");
    }
}
