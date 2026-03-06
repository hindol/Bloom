use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crossbeam::channel::Sender;
use rayon::prelude::*;

use crate::error::BloomError;
use crate::index::{FileFingerprint, Index, IndexEntry, RebuildStats};
use crate::parser::{self, traits::DocumentParser};
use crate::store;
use crate::types::*;

/// Result sent from the indexer thread to the UI thread on completion.
#[derive(Debug)]
pub struct IndexComplete {
    pub stats: RebuildStats,
    pub timing: IndexTiming,
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

/// Spawn the background indexer thread. Returns immediately.
/// The indexer sends an `IndexComplete` on the provided channel when done.
pub fn spawn_indexer(
    vault_root: PathBuf,
    index_path: PathBuf,
    tx: Sender<IndexComplete>,
) {
    std::thread::Builder::new()
        .name("bloom-indexer".into())
        .spawn(move || {
            let result = run_incremental(&vault_root, &index_path);
            match result {
                Ok(complete) => {
                    let _ = tx.send(complete);
                }
                Err(e) => {
                    tracing::error!("indexer error: {:?}", e);
                    // Send a zero-stats completion so the UI knows indexing finished
                    let _ = tx.send(IndexComplete {
                        stats: RebuildStats { pages: 0, links: 0, tags: 0 },
                        timing: IndexTiming {
                            scan_ms: 0,
                            read_parse_ms: 0,
                            write_ms: 0,
                            total_ms: 0,
                            files_scanned: 0,
                            files_changed: 0,
                        },
                    });
                }
            }
        })
        .expect("failed to spawn indexer thread");
}

fn run_incremental(vault_root: &Path, index_path: &Path) -> Result<IndexComplete, BloomError> {
    let t_total = Instant::now();

    let mut index = Index::open(index_path)?;
    let parser = parser::BloomMarkdownParser::new();

    // Phase 1: Scan — list files and compare fingerprints
    let t_scan = Instant::now();

    let store = store::local::LocalFileStore::new(vault_root.to_path_buf())?;
    use store::traits::NoteStore;
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

        // Compare against stored fingerprint
        let changed = match stored_fps.get(&rel_str) {
            Some(stored) => stored.mtime_secs != fp.mtime_secs || stored.size_bytes != fp.size_bytes,
            None => true, // new file
        };
        if changed {
            changed_paths.push(rel.to_path_buf());
        }
    }

    // Find deleted files (in stored but not in current)
    let deleted_paths: Vec<String> = stored_fps
        .keys()
        .filter(|k| !current_path_set.contains(*k))
        .cloned()
        .collect();

    let files_scanned = all_paths.len();
    let files_changed = changed_paths.len() + deleted_paths.len();
    let scan_ms = t_scan.elapsed().as_millis() as u64;

    // Phase 2: Read + Parse changed files in parallel
    let t_read = Instant::now();

    let entries: Vec<IndexEntry> = changed_paths
        .par_iter()
        .filter_map(|rel_path| {
            let full = vault_root.join(rel_path);
            let content = std::fs::read_to_string(&full).ok()?;
            let doc = parser.parse(&content);
            let fm = doc.frontmatter.as_ref();
            let page_id = fm.and_then(|f| f.id.clone())?;
            let title = fm.and_then(|f| f.title.clone()).unwrap_or_default();
            let created = fm
                .and_then(|f| f.created)
                .unwrap_or_else(|| chrono::Local::now().date_naive());
            let tags: Vec<TagName> = doc.tags.iter().map(|t| t.name.clone()).collect();
            let links: Vec<LinkTarget> = doc.links.iter().map(|l| LinkTarget {
                page: l.target.clone(),
                section: l.section.clone(),
                display_hint: l.display_hint.clone(),
            }).collect();
            let tasks: Vec<Task> = doc.tasks.iter().map(|t| Task {
                text: t.text.clone(),
                done: t.done,
                timestamps: t.timestamps.clone(),
                source_page: page_id.clone(),
                line: t.line,
            }).collect();
            let block_ids: Vec<(BlockId, usize)> = doc.block_ids.iter()
                .map(|b| (b.id.clone(), b.line))
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
            })
        })
        .collect();

    let read_parse_ms = t_read.elapsed().as_millis() as u64;

    // Phase 3: Batch write to SQLite
    let t_write = Instant::now();

    index.incremental_update(&entries, &deleted_paths)?;

    // Update fingerprints for changed files
    let fp_batch: Vec<(String, FileFingerprint)> = changed_paths
        .iter()
        .filter_map(|rel| {
            let rel_str = rel.display().to_string();
            current_fps.get(&rel_str).map(|fp| (rel_str, fp.clone()))
        })
        .collect();
    index.set_fingerprints_batch(&fp_batch);

    let write_ms = t_write.elapsed().as_millis() as u64;
    let total_ms = t_total.elapsed().as_millis() as u64;

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
    })
}
