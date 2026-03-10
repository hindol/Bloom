//! Single-instance vault lock.
//!
//! Prevents two Bloom processes from opening the same vault simultaneously.
//! Creates a `.bloom.lock` file in the vault root containing the current PID.
//! On startup, if the lock file exists and the PID is still alive, the lock
//! is considered held and initialization fails. If the PID is stale (process
//! no longer running), the lock is taken over.

use std::fs;
use std::path::{Path, PathBuf};

/// A held vault lock. Dropping this releases the lock file.
pub struct VaultLock {
    path: PathBuf,
}

impl VaultLock {
    /// Acquire the vault lock. Returns an error if another Bloom instance
    /// holds the lock.
    pub fn acquire(vault_root: &Path) -> Result<Self, LockError> {
        let path = vault_root.join(".bloom.lock");

        if path.exists() {
            // Check if the existing lock is stale.
            match fs::read_to_string(&path) {
                Ok(contents) => {
                    if let Ok(pid) = contents.trim().parse::<u32>() {
                        if pid != std::process::id() && is_process_alive(pid) {
                            return Err(LockError::AlreadyLocked { pid });
                        }
                        // Stale lock — process is dead (or it's us). Take it over.
                        if pid != std::process::id() {
                            tracing::warn!(pid, "removing stale vault lock");
                        }
                    }
                    // Invalid content or dead PID — safe to overwrite.
                }
                Err(_) => {
                    // Can't read lock file — try to overwrite.
                }
            }
        }

        // Write our PID to the lock file.
        let pid = std::process::id();
        fs::write(&path, pid.to_string()).map_err(LockError::Io)?;

        tracing::info!(pid, path = %path.display(), "vault lock acquired");
        Ok(VaultLock { path })
    }

    /// Release the lock (also called on Drop).
    pub fn release(&self) {
        if self.path.exists() {
            let _ = fs::remove_file(&self.path);
            tracing::info!(path = %self.path.display(), "vault lock released");
        }
    }
}

impl Drop for VaultLock {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Debug)]
pub enum LockError {
    /// Another Bloom instance holds the lock.
    AlreadyLocked { pid: u32 },
    /// Filesystem error creating/reading the lock file.
    Io(std::io::Error),
}

impl std::fmt::Display for LockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LockError::AlreadyLocked { pid } => {
                write!(f, "another Bloom instance is running (PID {pid})")
            }
            LockError::Io(e) => write!(f, "lock file error: {e}"),
        }
    }
}

/// Check if a process with the given PID is still running.
fn is_process_alive(pid: u32) -> bool {
    // Cross-platform: try to check /proc on Linux, tasklist on Windows.
    // Falls back to assuming alive if we can't determine.
    #[cfg(target_os = "linux")]
    {
        std::path::Path::new(&format!("/proc/{pid}")).exists()
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, use kill(pid, 0) which checks existence without signalling.
        // Safety: signal 0 is a null signal — no side effects.
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(target_os = "windows")]
    {
        // Use CreateToolhelp32Snapshot would need windows-sys.
        // Simpler: just check if we can open the process.
        // For now, use tasklist via Command — not fast but only runs once on startup.
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.contains(&pid.to_string())
            })
            .unwrap_or(true) // if tasklist fails, assume alive to be safe
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        true // can't check — assume alive
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn acquire_and_release() {
        let dir = TempDir::new().unwrap();
        let lock = VaultLock::acquire(dir.path()).unwrap();
        let lock_path = dir.path().join(".bloom.lock");
        assert!(lock_path.exists());

        // Lock file contains our PID.
        let contents = fs::read_to_string(&lock_path).unwrap();
        assert_eq!(contents.trim(), std::process::id().to_string());

        lock.release();
        assert!(!lock_path.exists());
    }

    #[test]
    fn acquire_succeeds_when_same_pid() {
        let dir = TempDir::new().unwrap();
        // First acquire.
        let lock1 = VaultLock::acquire(dir.path()).unwrap();
        drop(lock1);

        // Re-acquire after release should work.
        let _lock2 = VaultLock::acquire(dir.path()).unwrap();
    }

    #[test]
    fn acquire_succeeds_with_stale_lock() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".bloom.lock");

        // Write a PID that almost certainly doesn't exist.
        fs::write(&lock_path, "4294967290").unwrap();

        // Should succeed — stale lock taken over.
        let lock = VaultLock::acquire(dir.path()).unwrap();
        let contents = fs::read_to_string(&lock_path).unwrap();
        assert_eq!(contents.trim(), std::process::id().to_string());
        drop(lock);
    }

    #[test]
    fn drop_releases_lock() {
        let dir = TempDir::new().unwrap();
        let lock_path = dir.path().join(".bloom.lock");

        {
            let _lock = VaultLock::acquire(dir.path()).unwrap();
            assert!(lock_path.exists());
        }
        // Dropped — lock file should be gone.
        assert!(!lock_path.exists());
    }
}
