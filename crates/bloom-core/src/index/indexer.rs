//! Background indexer thread.
//!
//! Runs on a dedicated OS thread, listening for [`IndexRequest`] messages
//! (full rebuild, incremental batch, shutdown). Uses file fingerprints for
//! change detection and [`rayon`] for parallel markdown parsing. Sends
//! [`IndexComplete`] back to the UI thread on completion.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossbeam::channel::{Receiver, Sender};
use rayon::prelude::*;

use crate::error::BloomError;
use crate::index::{FileFingerprint, Index, IndexEntry, RebuildStats};
use crate::types::*;
use bloom_md::parser::{self, traits::DocumentParser};

/// Requests sent from the UI thread to the indexer thread.
pub enum IndexRequest {
    /// Full rebuild: invalidate all fingerprints, re-scan everything.
    FullRebuild,
    /// Re-index specific files (from file watcher, debounced).
    IncrementalBatch(Vec<PathBuf>),
    /// Persist undo trees for the given pages. The indexer writes to SQLite.
    PersistUndo(Vec<bloom_buffer::UndoPersistData>),
    /// Delete undo data for specific pages (on buffer close).
    PruneUndoPages(Vec<String>),
    /// Delete undo data older than the given epoch milliseconds.
    PruneUndoBefore(i64),
    /// Shut down the indexer thread.
    Shutdown,
}

/// Re-export from bloom-buffer for convenience.
pub use bloom_buffer::{UndoNodeData, UndoPersistData};

/// Result sent from the indexer thread to the UI thread on completion.
#[derive(Debug)]
pub struct IndexComplete {
    pub stats: RebuildStats,
    pub timing: IndexTiming,
    /// If set, the indexer encountered an error. The UI should surface this.
    pub error: Option<String>,
}

#[derive(Debug)]
pub struct IndexTiming {
    pub scan_ms: u64,
    pub read_parse_ms: u64,
    pub write_ms: u64,
    pub total_ms: u64,
    pub files_scanned: usize,
    pub files_changed: usize,
}

/// Spawn the long-lived indexer thread. Returns the request sender.
/// The indexer performs an initial incremental scan, then loops waiting
/// for IndexRequest messages until Shutdown.
pub fn spawn_indexer(
    vault_root: PathBuf,
    index_path: PathBuf,
    completion_tx: Sender<IndexComplete>,
) -> Sender<IndexRequest> {
    let (request_tx, request_rx) = crossbeam::channel::unbounded();

    std::thread::Builder::new()
        .name("bloom-indexer".into())
        .spawn(move || {
            indexer_main(&vault_root, &index_path, &request_rx, &completion_tx);
        })
        .expect("failed to spawn indexer thread");

    request_tx
}

fn indexer_main(
    vault_root: &Path,
    index_path: &Path,
    request_rx: &Receiver<IndexRequest>,
    completion_tx: &Sender<IndexComplete>,
) {
    tracing::info!(vault = %vault_root.display(), "indexer thread started");
    let mut index = match Index::open(index_path) {
        Ok(i) => i,
        Err(e) => {
            tracing::error!(error = %e, "indexer: failed to open index");
            send_error_completion(completion_tx, format!("Failed to open index: {e}"));
            return;
        }
    };
    let parser = parser::BloomMarkdownParser::new();

    // Initial startup scan
    match run_incremental(vault_root, &mut index, &parser) {
        Ok(complete) => {
            tracing::info!(
                files_scanned = complete.timing.files_scanned,
                files_changed = complete.timing.files_changed,
                duration_ms = complete.timing.total_ms,
                "incremental scan complete"
            );
            let _ = completion_tx.send(complete);
        }
        Err(e) => {
            tracing::error!(error = %e, "indexer startup error");
            send_error_completion(completion_tx, format!("Startup scan failed: {e}"));
        }
    }

    // Long-lived loop: process requests until Shutdown
    while let Ok(request) = request_rx.recv() {
        match request {
            IndexRequest::FullRebuild => match run_full_rebuild(vault_root, &mut index, &parser) {
                Ok(complete) => {
                    tracing::info!(
                        pages = complete.stats.pages,
                        links = complete.stats.links,
                        duration_ms = complete.timing.total_ms,
                        "full rebuild complete"
                    );
                    let _ = index.prune_orphaned_access();
                    let _ = completion_tx.send(complete);
                }
                Err(e) => {
                    tracing::error!(error = %e, "indexer rebuild error");
                    send_error_completion(completion_tx, format!("Rebuild failed: {e}"));
                }
            },
            IndexRequest::IncrementalBatch(paths) => {
                match run_batch(vault_root, &mut index, &parser, &paths) {
                    Ok(complete) => {
                        tracing::debug!(
                            files_changed = complete.timing.files_changed,
                            "incremental batch complete"
                        );
                        let _ = completion_tx.send(complete);
                    }
                    Err(e) => {
                        tracing::error!(error = %e, "indexer batch error");
                        send_error_completion(completion_tx, format!("Batch index failed: {e}"));
                    }
                }
            }
            IndexRequest::PersistUndo(undo_data) => {
                if let Err(e) = persist_undo_trees(index.connection(), &undo_data) {
                    tracing::error!(error = %e, "failed to persist undo trees");
                } else {
                    tracing::debug!(pages = undo_data.len(), "undo trees persisted");
                }
            }
            IndexRequest::PruneUndoPages(page_ids) => {
                for page_id in &page_ids {
                    let _ = index
                        .connection()
                        .execute("DELETE FROM undo_tree WHERE page_id = ?1", [page_id]);
                    let _ = index
                        .connection()
                        .execute("DELETE FROM undo_tree_state WHERE page_id = ?1", [page_id]);
                }
                tracing::debug!(count = page_ids.len(), "undo trees pruned (buffer close)");
            }
            IndexRequest::PruneUndoBefore(epoch_ms) => {
                // Delete nodes older than the threshold, then clean up orphaned state rows.
                let _ = index
                    .connection()
                    .execute("DELETE FROM undo_tree WHERE timestamp_ms < ?1", [epoch_ms]);
                let _ = index.connection().execute(
                    "DELETE FROM undo_tree_state WHERE page_id NOT IN (SELECT DISTINCT page_id FROM undo_tree)",
                    [],
                );
                tracing::debug!(before_epoch_ms = epoch_ms, "undo trees pruned (age)");
            }
            IndexRequest::Shutdown => break,
        }
    }
    tracing::info!("indexer thread stopped");
}

/// Write serialized undo trees to SQLite in a single transaction.
fn persist_undo_trees(
    conn: &rusqlite::Connection,
    data: &[UndoPersistData],
) -> Result<(), rusqlite::Error> {
    let tx = conn.unchecked_transaction()?;

    for page in data {
        tx.execute("DELETE FROM undo_tree WHERE page_id = ?1", [&page.page_id])?;
        tx.execute(
            "DELETE FROM undo_tree_state WHERE page_id = ?1",
            [&page.page_id],
        )?;

        let mut stmt = tx.prepare_cached(
            "INSERT INTO undo_tree (page_id, node_id, parent_id, content, timestamp_ms, description)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )?;
        for node in &page.nodes {
            stmt.execute(rusqlite::params![
                page.page_id,
                node.node_id,
                node.parent_id,
                node.content,
                node.timestamp_ms,
                node.description,
            ])?;
        }

        tx.execute(
            "INSERT INTO undo_tree_state (page_id, current_node_id) VALUES (?1, ?2)",
            rusqlite::params![page.page_id, page.current_node_id],
        )?;
    }

    tx.commit()?;
    Ok(())
}

fn send_error_completion(tx: &Sender<IndexComplete>, error: String) {
    let _ = tx.send(IndexComplete {
        stats: RebuildStats {
            pages: 0,
            links: 0,
            tags: 0,
        },
        timing: IndexTiming {
            scan_ms: 0,
            read_parse_ms: 0,
            write_ms: 0,
            total_ms: 0,
            files_scanned: 0,
            files_changed: 0,
        },
        error: Some(error),
    });
}

/// Full wipe-and-reinsert: read ALL files, delete all index data, rebuild from scratch.
/// Ignores fingerprints entirely — every file is re-read and re-parsed.
fn run_full_rebuild(
    vault_root: &Path,
    index: &mut Index,
    parser: &parser::BloomMarkdownParser,
) -> Result<IndexComplete, BloomError> {
    let t_total = Instant::now();

    // Phase 1: List all files
    let t_scan = Instant::now();
    let store = bloom_store::local::LocalFileStore::new(vault_root.to_path_buf())?;
    use bloom_store::traits::NoteStore;
    let mut all_paths = store.list_pages().unwrap_or_default();
    all_paths.extend(store.list_journals().unwrap_or_default());
    let files_scanned = all_paths.len();
    let rel_paths: Vec<PathBuf> = all_paths
        .iter()
        .map(|p| p.strip_prefix(vault_root).unwrap_or(p).to_path_buf())
        .collect();
    let scan_ms = t_scan.elapsed().as_millis() as u64;

    // Phase 2: Read + Parse ALL files (no fingerprint check)
    let t_read = Instant::now();
    let entries = parse_paths(vault_root, &rel_paths, parser, true);
    let read_parse_ms = t_read.elapsed().as_millis() as u64;

    // Phase 3: Wipe all index tables and reinsert
    let t_write = Instant::now();
    let stats = index.rebuild(&entries)?;

    // Rebuild all fingerprints from scratch
    for rel in &rel_paths {
        let full = vault_root.join(rel);
        if let Ok(meta) = std::fs::metadata(&full) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let fp = FileFingerprint {
                mtime_secs: mtime,
                size_bytes: meta.len(),
            };
            index.set_fingerprint(&rel.display().to_string(), &fp);
        }
    }
    let write_ms = t_write.elapsed().as_millis() as u64;
    let total_ms = t_total.elapsed().as_millis() as u64;

    Ok(IndexComplete {
        stats,
        timing: IndexTiming {
            scan_ms,
            read_parse_ms,
            write_ms,
            total_ms,
            files_scanned,
            files_changed: files_scanned, // all files re-indexed
        },
        error: None,
    })
}

/// Incremental scan: compare fingerprints, read/parse changed files, write to index.
fn run_incremental(
    vault_root: &Path,
    index: &mut Index,
    parser: &parser::BloomMarkdownParser,
) -> Result<IndexComplete, BloomError> {
    let t_total = Instant::now();

    // Phase 1: Scan
    let t_scan = Instant::now();

    let store = bloom_store::local::LocalFileStore::new(vault_root.to_path_buf())?;
    use bloom_store::traits::NoteStore;
    let mut all_paths = store.list_pages().unwrap_or_default();
    all_paths.extend(store.list_journals().unwrap_or_default());

    let stored_fps = index.all_fingerprints();
    let mut changed_paths: Vec<PathBuf> = Vec::new();
    let mut current_fps: HashMap<String, FileFingerprint> = HashMap::new();
    let mut current_path_set: std::collections::HashSet<String> = std::collections::HashSet::new();

    for path in &all_paths {
        let rel = path.strip_prefix(vault_root).unwrap_or(path);
        let rel_str = rel.display().to_string();
        current_path_set.insert(rel_str.clone());

        let full_path = vault_root.join(rel);
        let meta = match std::fs::metadata(&full_path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let size = meta.len();

        let fp = FileFingerprint {
            mtime_secs: mtime,
            size_bytes: size,
        };
        current_fps.insert(rel_str.clone(), fp.clone());

        let changed = match stored_fps.get(&rel_str) {
            Some(stored) => {
                stored.mtime_secs != fp.mtime_secs || stored.size_bytes != fp.size_bytes
            }
            None => true,
        };
        if changed {
            changed_paths.push(rel.to_path_buf());
        }
    }

    let deleted_paths: Vec<String> = stored_fps
        .keys()
        .filter(|k| !current_path_set.contains(*k))
        .cloned()
        .collect();

    let files_scanned = all_paths.len();
    let files_changed = changed_paths.len() + deleted_paths.len();
    let scan_ms = t_scan.elapsed().as_millis() as u64;

    // Phase 2: Read + Parse
    let t_read = Instant::now();
    let entries = parse_paths(vault_root, &changed_paths, parser, true);
    let read_parse_ms = t_read.elapsed().as_millis() as u64;

    // Phase 3: Write
    let t_write = Instant::now();

    // First run (empty fingerprint table) or all files changed: use bulk rebuild
    // path (single DELETE + bulk INSERT) instead of per-file upsert.
    let is_first_run = stored_fps.is_empty();
    if is_first_run && deleted_paths.is_empty() {
        tracing::info!(
            entries = entries.len(),
            "using bulk rebuild path (first run)"
        );
        index.rebuild(&entries)?;
    } else {
        index.incremental_update(&entries, &deleted_paths)?;
    }

    let fp_batch: Vec<(String, FileFingerprint)> = changed_paths
        .iter()
        .filter_map(|rel| {
            let rel_str = rel.display().to_string();
            current_fps.get(&rel_str).map(|fp| (rel_str, fp.clone()))
        })
        .collect();
    index.set_fingerprints_batch(&fp_batch);

    // Phase 4: Mirror promotion/demotion
    apply_mirror_markers(vault_root, index);

    let write_ms = t_write.elapsed().as_millis() as u64;
    let total_ms = t_total.elapsed().as_millis() as u64;

    tracing::info!(
        scan_ms,
        read_parse_ms,
        write_ms,
        total_ms,
        "incremental scan phase timing"
    );

    Ok(IndexComplete {
        stats: RebuildStats {
            pages: entries.len(),
            links: entries.iter().map(|e| e.links.len()).sum(),
            tags: entries.iter().map(|e| e.tags.len()).sum(),
        },
        timing: IndexTiming {
            scan_ms,
            read_parse_ms,
            write_ms,
            total_ms,
            files_scanned,
            files_changed,
        },
        error: None,
    })
}

/// Process a batch of specific file paths (from file watcher).
fn run_batch(
    vault_root: &Path,
    index: &mut Index,
    parser: &parser::BloomMarkdownParser,
    rel_paths: &[PathBuf],
) -> Result<IndexComplete, BloomError> {
    let t_total = Instant::now();

    // Separate existing files from deleted files
    let mut existing: Vec<PathBuf> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    for rel in rel_paths {
        let full = vault_root.join(rel);
        if full.exists() {
            existing.push(rel.clone());
        } else {
            deleted.push(rel.display().to_string());
        }
    }

    let t_read = Instant::now();
    let entries = parse_paths(vault_root, &existing, parser, true);
    let read_parse_ms = t_read.elapsed().as_millis() as u64;

    let t_write = Instant::now();
    index.incremental_update(&entries, &deleted)?;

    // Update fingerprints for changed files
    for rel in &existing {
        let full = vault_root.join(rel);
        if let Ok(meta) = std::fs::metadata(&full) {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let fp = FileFingerprint {
                mtime_secs: mtime,
                size_bytes: meta.len(),
            };
            index.set_fingerprint(&rel.display().to_string(), &fp);
        }
    }

    // Mirror promotion/demotion
    apply_mirror_markers(vault_root, index);

    let write_ms = t_write.elapsed().as_millis() as u64;
    let total_ms = t_total.elapsed().as_millis() as u64;

    Ok(IndexComplete {
        stats: RebuildStats {
            pages: entries.len(),
            links: entries.iter().map(|e| e.links.len()).sum(),
            tags: entries.iter().map(|e| e.tags.len()).sum(),
        },
        timing: IndexTiming {
            scan_ms: 0,
            read_parse_ms,
            write_ms,
            total_ms,
            files_scanned: rel_paths.len(),
            files_changed: entries.len() + deleted.len(),
        },
        error: None,
    })
}

/// Promote solo blocks (^) to mirrored (^=) and demote orphaned mirrors (^=) back to (^).
/// Rewrites files on disk and updates the index.
fn apply_mirror_markers(vault_root: &Path, index: &mut Index) {
    // Promotions: ^ → ^= (block appears in multiple pages but not marked as mirror)
    let promotions = index.find_blocks_needing_promotion();
    // Demotions: ^= → ^ (block only in one page but still marked as mirror)
    let demotions = index.find_blocks_needing_demotion();

    if promotions.is_empty() && demotions.is_empty() {
        return;
    }

    // Group actions by file path to batch file writes
    let mut file_actions: std::collections::HashMap<PathBuf, Vec<(&str, &str, usize)>> =
        std::collections::HashMap::new();
    for action in &promotions {
        file_actions
            .entry(action.path.clone())
            .or_default()
            .push(("promote", &action.block_id, action.line));
    }
    for action in &demotions {
        file_actions
            .entry(action.path.clone())
            .or_default()
            .push(("demote", &action.block_id, action.line));
    }

    let mut files_modified = 0;
    for (rel_path, actions) in &file_actions {
        let full = vault_root.join(rel_path);
        let content = match std::fs::read_to_string(&full) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
        let mut changed = false;

        for (kind, block_id, line_idx) in actions {
            if *line_idx >= lines.len() {
                continue;
            }
            let line = &lines[*line_idx];
            let solo_marker = format!(" ^{}", block_id);
            let mirror_marker = format!(" ^={}", block_id);

            match *kind {
                "promote" if line.ends_with(&solo_marker) => {
                    let new_line =
                        format!("{}{}", &line[..line.len() - solo_marker.len()], mirror_marker);
                    lines[*line_idx] = new_line;
                    changed = true;
                }
                "demote" if line.ends_with(&mirror_marker) => {
                    let new_line =
                        format!("{}{}", &line[..line.len() - mirror_marker.len()], solo_marker);
                    lines[*line_idx] = new_line;
                    changed = true;
                }
                _ => {} // line changed since indexing — skip
            }
        }

        if changed {
            let has_trailing = content.ends_with('\n');
            let sep = if content.contains("\r\n") { "\r\n" } else { "\n" };
            let mut out = lines.join(sep);
            if has_trailing {
                out.push_str(sep);
            }
            if bloom_store::disk_writer::atomic_write(&full, &out).is_ok() {
                files_modified += 1;
            }
        }
    }

    if files_modified > 0 {
        tracing::info!(
            promotions = promotions.len(),
            demotions = demotions.len(),
            files_modified,
            "mirror markers updated"
        );

        // Update is_mirror flags in the index to match the files
        for action in &promotions {
            let _ = index.connection().execute(
                "UPDATE block_ids SET is_mirror = 1 WHERE block_id = ?1 AND page_id = ?2",
                rusqlite::params![action.block_id, action.page_id],
            );
        }
        for action in &demotions {
            let _ = index.connection().execute(
                "UPDATE block_ids SET is_mirror = 0 WHERE block_id = ?1 AND page_id = ?2",
                rusqlite::params![action.block_id, action.page_id],
            );
        }
    }
}

/// Read and parse a set of relative paths into IndexEntry objects.
/// If `assign_ids` is true, files with blocks missing IDs get IDs assigned
/// and are written back to disk (atomic write, on the indexer thread).
fn parse_paths(
    vault_root: &Path,
    rel_paths: &[PathBuf],
    parser: &parser::BloomMarkdownParser,
    assign_ids: bool,
) -> Vec<IndexEntry> {
    rel_paths
        .par_iter()
        .filter_map(|rel_path| {
            let full = vault_root.join(rel_path);
            let content = std::fs::read_to_string(&full).ok()?;
            let doc = parser.parse(&content);

            // Assign block IDs on the indexer thread if needed.
            let content = if assign_ids {
                if let Some(new_content) = crate::block_id_gen::assign_block_ids(&content, &doc) {
                    if bloom_store::disk_writer::atomic_write(&full, &new_content).is_ok() {
                        tracing::debug!(path = %rel_path.display(), "block IDs assigned by indexer");
                        new_content
                    } else {
                        content
                    }
                } else {
                    content
                }
            } else {
                content
            };

            // Re-parse if content changed (block IDs were added).
            let doc = if assign_ids {
                parser.parse(&content)
            } else {
                doc
            };
            let fm = doc.frontmatter.as_ref();
            let page_id = fm.and_then(|f| f.id.clone())?;
            let title = fm.and_then(|f| f.title.clone()).unwrap_or_default();
            let created = fm
                .and_then(|f| f.created)
                .unwrap_or_else(|| chrono::Local::now().date_naive());
            let tags: Vec<TagName> = doc.tags.iter().map(|t| t.name.clone()).collect();
            let links: Vec<LinkTarget> = doc
                .links
                .iter()
                .map(|l| LinkTarget {
                    page: l.target.clone(),
                    display_hint: l.display_hint.clone(),
                })
                .collect();
            let tasks: Vec<Task> = doc
                .tasks
                .iter()
                .map(|t| Task {
                    text: t.text.clone(),
                    done: t.done,
                    timestamps: t.timestamps.clone(),
                    source_page: page_id.clone(),
                    line: t.line,
                })
                .collect();
            let block_ids: Vec<(BlockId, usize, bool)> = doc
                .block_ids
                .iter()
                .map(|b| (b.id.clone(), b.line, b.is_mirror))
                .collect();
            let block_links: Vec<(BlockId, String)> = doc
                .block_links
                .iter()
                .map(|bl| (bl.block_id.clone(), bl.display_hint.clone()))
                .collect();

            Some(IndexEntry {
                meta: PageMeta {
                    id: page_id,
                    title,
                    created,
                    tags: tags.clone(),
                    path: rel_path.to_path_buf(),
                },
                content,
                links,
                tags,
                tasks,
                block_ids,
                block_links,
            })
        })
        .collect()
}
