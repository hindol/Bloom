//! Background FTS worker — runs SQLite full-text queries off the UI thread.
//!
//! Architecture:
//! - A dedicated thread opens a **read-only** SQLite connection to the same DB.
//! - The editor sends `(generation, query)` via a crossbeam channel on each keystroke.
//! - The worker skips stale queries (generation < latest received) for natural debounce.
//! - Results are sent back and the editor polls the receiver in `poll_fts_results()`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;

use crossbeam_channel::{Receiver, Sender};

use crate::index::{IndexError, SqliteIndex};
use crate::picker::FullTextSearchSource;

/// A query request sent to the worker thread.
struct FtsRequest {
    generation: u64,
    query: String,
}

/// A result sent back from the worker thread.
pub struct FtsResult {
    pub generation: u64,
    pub source: FullTextSearchSource,
}

/// Handle held by the editor to communicate with the background FTS worker.
pub struct FtsWorker {
    tx: Sender<FtsRequest>,
    pub rx: Receiver<FtsResult>,
    generation: Arc<AtomicU64>,
}

impl std::fmt::Debug for FtsWorker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FtsWorker").finish_non_exhaustive()
    }
}

impl FtsWorker {
    /// Spawn a background FTS worker for the given index database.
    pub fn spawn(db_path: &Path) -> Result<Self, IndexError> {
        let db_path = db_path.to_path_buf();
        let generation = Arc::new(AtomicU64::new(0));
        let gen_clone = Arc::clone(&generation);

        // Bounded(4): small buffer; worker skips stale queries by generation.
        let (req_tx, req_rx) = crossbeam_channel::bounded::<FtsRequest>(4);
        // Bounded(64): prevents unbounded memory growth if results pile up.
        let (res_tx, res_rx) = crossbeam_channel::bounded::<FtsResult>(64);

        thread::Builder::new()
            .name("fts-worker".into())
            .spawn(move || {
                worker_loop(db_path, req_rx, res_tx, gen_clone);
            })
            .map_err(|e| IndexError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;

        Ok(Self {
            tx: req_tx,
            rx: res_rx,
            generation,
        })
    }

    /// Submit a new search query. Supersedes any previous in-flight query.
    pub fn search(&self, query: &str) {
        let next_gen = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let request = FtsRequest {
            generation: next_gen,
            query: query.to_string(),
        };
        // Bounded(1): if full, the old request is stale — just force-send.
        // crossbeam bounded channel will block on send if full, so use try_send
        // and on Full, just drop the old one by receiving it first via a separate
        // drain receiver we don't have. Instead, use an unbounded channel approach:
        // We use bounded(2) and always try_send; if full we accept the drop.
        let _ = self.tx.try_send(request);
    }

    /// Current generation counter (for staleness checks).
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::SeqCst)
    }
}

fn worker_loop(
    db_path: PathBuf,
    rx: Receiver<FtsRequest>,
    tx: Sender<FtsResult>,
    generation: Arc<AtomicU64>,
) {
    // Open a read-only connection to the same database.
    let index = match SqliteIndex::open(&db_path) {
        Ok(idx) => idx,
        Err(_) => return,
    };

    for request in rx {
        // Skip stale requests — a newer query has been submitted.
        let current_gen = generation.load(Ordering::SeqCst);
        if request.generation < current_gen {
            continue;
        }

        let source = FullTextSearchSource::from_index(&index, &request.query)
            .unwrap_or_else(|_| FullTextSearchSource::empty());

        let _ = tx.send(FtsResult {
            generation: request.generation,
            source,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;
    use crate::picker::PickerSource;
    use std::path::Path as StdPath;
    use tempfile::TempDir;

    fn make_index_with_content() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let mut index = SqliteIndex::open(&db_path).unwrap();

        let raw = "---\nid: aabb1122\ntitle: \"Rope Buffers\"\ntags: []\n---\n\nRopes are O(log n) for inserts.";
        let doc = parse(raw).unwrap();
        index
            .index_document(StdPath::new("pages/rope.md"), &doc)
            .unwrap();
        (tmp, index)
    }

    #[test]
    fn worker_returns_results() {
        let (tmp, index) = make_index_with_content();
        let worker = FtsWorker::spawn(index.db_path()).unwrap();
        worker.search("rope");

        // Poll for result (give the thread a moment).
        let result = worker.rx.recv_timeout(std::time::Duration::from_secs(2)).unwrap();
        assert!(!result.source.items().is_empty());
    }

    #[test]
    fn stale_queries_are_skipped() {
        let (tmp, index) = make_index_with_content();
        let worker = FtsWorker::spawn(index.db_path()).unwrap();

        // Fire multiple queries rapidly — only the last should matter.
        worker.search("nonexistent1");
        worker.search("nonexistent2");
        worker.search("rope");

        // Drain all results; the final one should have rope results.
        let mut last_result = None;
        for _ in 0..10 {
            match worker.rx.recv_timeout(std::time::Duration::from_millis(500)) {
                Ok(r) => last_result = Some(r),
                Err(_) => break,
            }
        }
        let result = last_result.unwrap();
        // The result with the highest generation should be present.
        assert!(!result.source.items().is_empty());
    }
}
