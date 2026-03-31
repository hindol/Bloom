//! Background history thread.
//!
//! Runs on a dedicated OS thread, owning the [`bloom_history::HistoryRepo`]
//! handle. Receives [`HistoryRequest`] messages (commit, shutdown) from the
//! UI thread and sends [`HistoryComplete`] results back.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crossbeam::channel::{Receiver, Sender};

use bloom_history::HistoryRepo;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryFlushReason {
    IdleTimeout,
    MaxInterval,
}

/// Requests sent from the UI thread to the history thread.
pub enum HistoryRequest {
    /// A file was written to disk — mark the vault as dirty.
    /// The history thread debounces these and auto-commits after idle.
    FileDirty,
    /// Commit all changed files now (e.g. on quit, journal rotation).
    /// `files`: `(uuid_hex, content)` pairs to stage.
    CommitNow {
        files: Vec<(String, String)>,
        message: String,
    },
    /// Retrieve commit history for a specific page (by UUID).
    PageHistory { uuid: String, limit: usize },
    /// Retrieve file content at a specific commit.
    BlobAt { oid: String, uuid: String },
    /// Shut down the history thread.
    Shutdown,
}

/// Results sent from the history thread back to the UI thread.
#[derive(Debug, Clone)]
pub enum HistoryComplete {
    /// The history thread has decided a durable checkpoint is due.
    FlushRequested { reason: HistoryFlushReason },
    /// A commit was created successfully.
    CommitFinished { oid: String },
    /// No commit was created because the tree was unchanged.
    CommitSkipped,
    /// Page history results.
    PageHistory { entries: Vec<PageHistoryEntry> },
    /// File content at a specific commit.
    BlobAt {
        oid: String,
        uuid: String,
        content: Option<String>,
    },
    /// An error occurred.
    Error { message: String },
    /// The thread has shut down.
    ShutDown,
}

/// A single entry in a page's commit history.
#[derive(Debug, Clone)]
pub struct PageHistoryEntry {
    pub oid: String,
    pub message: String,
    pub timestamp: i64,
    pub changed_files: Vec<String>,
}

/// Spawn the long-lived history thread. Returns the request sender.
///
/// - `index_dir`: path to `.index/` directory (contains `.git/`)
/// - `idle_commit_minutes`: auto-commit after this many minutes of idle
/// - `max_commit_minutes`: safety-net commit after this interval regardless
pub fn spawn_history_thread(
    index_dir: PathBuf,
    idle_commit_minutes: u64,
    max_commit_minutes: u64,
    completion_tx: Sender<HistoryComplete>,
) -> Sender<HistoryRequest> {
    let (request_tx, request_rx) = crossbeam::channel::unbounded();

    std::thread::Builder::new()
        .name("bloom-history".into())
        .spawn(move || {
            history_main(
                &index_dir,
                idle_commit_minutes,
                max_commit_minutes,
                &request_rx,
                &completion_tx,
            );
        })
        .expect("failed to spawn history thread");

    request_tx
}

fn history_main(
    index_dir: &std::path::Path,
    idle_commit_minutes: u64,
    max_commit_minutes: u64,
    request_rx: &Receiver<HistoryRequest>,
    completion_tx: &Sender<HistoryComplete>,
) {
    tracing::info!(path = %index_dir.display(), "history thread started");

    let repo = match HistoryRepo::open(index_dir) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!(error = %e, "failed to open history repository");
            let _ = completion_tx.send(HistoryComplete::Error {
                message: e.to_string(),
            });
            return;
        }
    };

    let idle_timeout = Duration::from_secs(idle_commit_minutes * 60);
    let max_interval = Duration::from_secs(max_commit_minutes * 60);

    // Track when the vault was last dirtied and when we last committed.
    let mut dirty = false;
    let mut last_dirty_at: Option<Instant> = None;
    let mut last_commit_at = Instant::now();
    let mut awaiting_snapshot = false;

    // Pending files to commit (collected from CommitNow requests).
    let mut pending_files: Vec<(String, String)> = Vec::new();
    let mut pending_message: Option<String> = None;

    loop {
        // Compute how long to wait before the next auto-commit check.
        let timeout = if dirty {
            let since_dirty = last_dirty_at.map(|t| t.elapsed()).unwrap_or(Duration::ZERO);
            let since_commit = last_commit_at.elapsed();

            // Commit if idle long enough OR if max interval exceeded.
            let idle_remaining = idle_timeout.saturating_sub(since_dirty);
            let max_remaining = max_interval.saturating_sub(since_commit);
            idle_remaining
                .min(max_remaining)
                .max(Duration::from_millis(100))
        } else {
            // Not dirty — just wait for a message.
            Duration::from_secs(1)
        };

        match request_rx.recv_timeout(timeout) {
            Ok(HistoryRequest::FileDirty) => {
                dirty = true;
                last_dirty_at = Some(Instant::now());
            }
            Ok(HistoryRequest::CommitNow { files, message }) => {
                pending_files = files;
                pending_message = Some(message);
                awaiting_snapshot = false;
                // Commit immediately below.
            }
            Ok(HistoryRequest::PageHistory { uuid, limit }) => {
                match repo.page_history(Some(&uuid), limit) {
                    Ok(entries) => {
                        let entries = entries
                            .into_iter()
                            .map(|c| PageHistoryEntry {
                                oid: c.oid,
                                message: c.message,
                                timestamp: c.timestamp,
                                changed_files: c.changed_files,
                            })
                            .collect();
                        let _ = completion_tx.send(HistoryComplete::PageHistory { entries });
                    }
                    Err(e) => {
                        let _ = completion_tx.send(HistoryComplete::Error {
                            message: e.to_string(),
                        });
                    }
                }
                continue;
            }
            Ok(HistoryRequest::BlobAt { oid, uuid }) => {
                match repo.blob_at(&oid, &uuid) {
                    Ok(content) => {
                        let _ = completion_tx.send(HistoryComplete::BlobAt { oid, uuid, content });
                    }
                    Err(e) => {
                        let _ = completion_tx.send(HistoryComplete::Error {
                            message: e.to_string(),
                        });
                    }
                }
                continue;
            }
            Ok(HistoryRequest::Shutdown) => {
                // Final commit if dirty, then exit.
                if dirty || !pending_files.is_empty() {
                    do_commit(
                        &repo,
                        &pending_files,
                        pending_message.as_deref().unwrap_or("shutdown"),
                        completion_tx,
                    );
                }
                let _ = completion_tx.send(HistoryComplete::ShutDown);
                tracing::info!("history thread shutting down");
                return;
            }
            Err(crossbeam::channel::RecvTimeoutError::Timeout) => {
                // Check if we should auto-commit.
            }
            Err(crossbeam::channel::RecvTimeoutError::Disconnected) => {
                tracing::info!("history request channel disconnected, shutting down");
                return;
            }
        }

        // Handle explicit CommitNow.
        if pending_message.is_some() {
            let msg = pending_message.take().unwrap();
            do_commit(&repo, &pending_files, &msg, completion_tx);
            pending_files.clear();
            dirty = false;
            last_dirty_at = None;
            last_commit_at = Instant::now();
            awaiting_snapshot = false;
            continue;
        }

        // Auto-commit check: idle timeout or safety-net max interval.
        if dirty && !awaiting_snapshot {
            let idle_elapsed = last_dirty_at
                .map(|t| t.elapsed() >= idle_timeout)
                .unwrap_or(false);
            let max_elapsed = last_commit_at.elapsed() >= max_interval;

            if idle_elapsed || max_elapsed {
                let reason = if max_elapsed {
                    HistoryFlushReason::MaxInterval
                } else {
                    HistoryFlushReason::IdleTimeout
                };
                tracing::debug!(?reason, "auto-commit triggered");
                let _ = completion_tx.send(HistoryComplete::FlushRequested { reason });
                awaiting_snapshot = true;
            }
        }
    }
}

fn do_commit(
    repo: &HistoryRepo,
    files: &[(String, String)],
    message: &str,
    completion_tx: &Sender<HistoryComplete>,
) {
    let file_refs: Vec<(&str, &str)> = files
        .iter()
        .map(|(u, c)| (u.as_str(), c.as_str()))
        .collect();

    match repo.commit_all(&file_refs, message, None) {
        Ok(oid) => {
            if let Some(id) = oid {
                tracing::info!(oid = %id, "history commit created");
                let _ = completion_tx.send(HistoryComplete::CommitFinished { oid: id });
            } else {
                tracing::debug!("history commit skipped (no changes)");
                let _ = completion_tx.send(HistoryComplete::CommitSkipped);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, "history commit failed");
            let _ = completion_tx.send(HistoryComplete::Error {
                message: e.to_string(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_thread_requests_flush_after_idle_timeout() {
        let dir = tempfile::TempDir::new().unwrap();
        let (completion_tx, completion_rx) = crossbeam::channel::unbounded();
        let request_tx = spawn_history_thread(dir.path().to_path_buf(), 0, 60, completion_tx);

        request_tx.send(HistoryRequest::FileDirty).unwrap();

        let completion = completion_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("expected flush request");
        assert!(matches!(
            completion,
            HistoryComplete::FlushRequested {
                reason: HistoryFlushReason::IdleTimeout
            }
        ));

        request_tx.send(HistoryRequest::Shutdown).unwrap();
        let _ = completion_rx.recv_timeout(Duration::from_secs(2));
    }

    #[test]
    fn history_thread_reports_commit_skipped_for_unchanged_snapshot() {
        let dir = tempfile::TempDir::new().unwrap();
        let (completion_tx, completion_rx) = crossbeam::channel::unbounded();
        let request_tx = spawn_history_thread(dir.path().to_path_buf(), 1, 60, completion_tx);

        request_tx
            .send(HistoryRequest::CommitNow {
                files: vec![("page0001".into(), "hello".into())],
                message: "first checkpoint".into(),
            })
            .unwrap();
        let first = completion_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("expected first commit result");
        assert!(matches!(first, HistoryComplete::CommitFinished { .. }));

        request_tx
            .send(HistoryRequest::CommitNow {
                files: vec![("page0001".into(), "hello".into())],
                message: "duplicate checkpoint".into(),
            })
            .unwrap();
        let second = completion_rx
            .recv_timeout(Duration::from_secs(2))
            .expect("expected second commit result");
        assert!(matches!(second, HistoryComplete::CommitSkipped));

        request_tx.send(HistoryRequest::Shutdown).unwrap();
        let _ = completion_rx.recv_timeout(Duration::from_secs(2));
    }
}
