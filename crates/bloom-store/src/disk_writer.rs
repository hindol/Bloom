//! Debounced atomic disk writer.
//!
//! Coalesces multiple [`WriteRequest`]s per path within a configurable debounce
//! window, then performs an atomic write sequence (temp file → fsync → rename)
//! to prevent data corruption. Sends a [`WriteComplete`] ack per successful write.

use crossbeam::channel;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// A write request sent from the editor to the [`DiskWriter`] thread.
pub struct WriteRequest {
    pub path: PathBuf,
    pub content: String,
}

/// Sent back to the editor after a successful atomic write.
#[derive(Debug, Clone)]
pub struct WriteComplete {
    pub path: PathBuf,
    pub mtime: SystemTime,
    pub size: u64,
}

/// Debounced atomic disk writer running on a dedicated thread.
///
/// Coalesces multiple writes per path within a configurable debounce window,
/// then performs temp → fsync → rename to prevent corruption. Create via
/// [`new`](Self::new), then spawn with [`start`](Self::start) on an OS thread.
pub struct DiskWriter {
    rx: channel::Receiver<WriteRequest>,
    ack_tx: channel::Sender<WriteComplete>,
    debounce_ms: u64,
}

impl DiskWriter {
    pub fn new(
        debounce_ms: u64,
    ) -> (
        Self,
        channel::Sender<WriteRequest>,
        channel::Receiver<WriteComplete>,
    ) {
        let (tx, rx) = channel::unbounded();
        let (ack_tx, ack_rx) = channel::unbounded();
        (
            Self {
                rx,
                ack_tx,
                debounce_ms,
            },
            tx,
            ack_rx,
        )
    }

    /// Run on a dedicated OS thread. Debounces writes per path, then does
    /// atomic write → fsync → rename.
    pub fn start(self) {
        tracing::info!("disk writer thread started");
        let debounce = Duration::from_millis(self.debounce_ms);
        // Track latest content and when we first saw a pending write for each path.
        let mut pending: HashMap<PathBuf, (String, Instant)> = HashMap::new();

        loop {
            let timeout = pending
                .values()
                .map(|(_, t)| debounce.saturating_sub(t.elapsed()))
                .min()
                .unwrap_or(debounce);

            match self.rx.recv_timeout(timeout) {
                Ok(req) => {
                    let entry = pending
                        .entry(req.path)
                        .or_insert_with(|| (String::new(), Instant::now()));
                    entry.0 = req.content;
                }
                Err(channel::RecvTimeoutError::Timeout) => {}
                Err(channel::RecvTimeoutError::Disconnected) => {
                    // Flush remaining writes before exiting.
                    for (path, (content, _)) in pending.drain() {
                        if atomic_write(&path, &content).is_ok() {
                            self.send_ack(&path);
                        }
                    }
                    return;
                }
            }

            // Flush any entries whose debounce window has elapsed.
            let now = Instant::now();
            let ready: Vec<PathBuf> = pending
                .iter()
                .filter(|(_, (_, t))| now.duration_since(*t) >= debounce)
                .map(|(p, _)| p.clone())
                .collect();

            for path in ready {
                if let Some((content, _)) = pending.remove(&path) {
                    let size = content.len();
                    if let Err(e) = atomic_write(&path, &content) {
                        tracing::error!(path = %path.display(), error = %e, "atomic write failed");
                    } else {
                        tracing::debug!(path = %path.display(), size_bytes = size, "atomic write complete");
                        self.send_ack(&path);
                    }
                }
            }
        }
    }

    fn send_ack(&self, path: &PathBuf) {
        if let Ok(meta) = fs::metadata(path) {
            let mtime = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            let size = meta.len();
            let _ = self.ack_tx.send(WriteComplete {
                path: path.clone(),
                mtime,
                size,
            });
        }
    }
}

/// Write content to a temp file, fsync, then rename over the target.
pub fn atomic_write(path: &PathBuf, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    let mut file = fs::File::create(&tmp)?;
    file.write_all(content.as_bytes())?;
    file.sync_all()?;
    fs::rename(&tmp, path)?;
    Ok(())
}
