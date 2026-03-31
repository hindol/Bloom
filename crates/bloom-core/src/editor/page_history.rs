//! Page and block history — `SPC H h` / `SPC H b`.
//!
//! Opens a temporal strip showing the unified history: undo tree (recent,
//! branching) followed by git commits (older, linear). Preview shows diff.
//! Block history filters to a single block ID.

use crate::history::HistoryRequest;
use crate::*;

fn undo_stop_time(elapsed: std::time::Duration) -> TemporalStopTime {
    let relative_label = if elapsed.as_secs() < 60 {
        format!("{}s", elapsed.as_secs())
    } else if elapsed.as_secs() < 3600 {
        format!("{}m", elapsed.as_secs() / 60)
    } else {
        format!("{}h", elapsed.as_secs() / 3600)
    };
    TemporalStopTime {
        timestamp: None,
        absolute_label: None,
        relative_label: relative_label.clone(),
    }
}

fn git_stop_time(timestamp: i64) -> TemporalStopTime {
    let absolute = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string());
    let relative_label = chrono::DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.format("%b %-d").to_string())
        .unwrap_or_else(|| "?".to_string());
    TemporalStopTime {
        timestamp: Some(timestamp),
        relative_label: relative_label.clone(),
        absolute_label: absolute,
    }
}

fn branch_context(branch_count: usize) -> Option<TemporalBranchContext> {
    if branch_count == 0 {
        return None;
    }
    Some(TemporalBranchContext {
        status: if branch_count > 1 {
            TemporalBranchStatus::ForkNode
        } else {
            TemporalBranchStatus::CurrentPath
        },
        branch_count,
        summary: if branch_count > 1 {
            format!("fork with {branch_count} branches")
        } else {
            "single branch".into()
        },
    })
}

fn checkpoint_reason(message: &str) -> TemporalCheckpointReason {
    match message {
        "idle timeout" => TemporalCheckpointReason::IdleTimeout,
        "safety-net max interval" => TemporalCheckpointReason::MaxInterval,
        "session save" => TemporalCheckpointReason::SessionSave,
        "explicit checkpoint" => TemporalCheckpointReason::Explicit,
        _ => TemporalCheckpointReason::Unknown,
    }
}

impl BloomEditor {
    /// Open page history as a temporal strip.
    pub(crate) fn open_page_history(&mut self) {
        let page_id = match self.active_page() {
            Some(id) => id.clone(),
            None => return,
        };

        let current_content = match self.writer.buffers().get(&page_id) {
            Some(buf) => buf.text().to_string(),
            None => return,
        };

        // Collect undo tree nodes (recent history)
        let mut items: Vec<TemporalItem> = Vec::new();
        if let Some(buf) = self.writer.buffers().get(&page_id) {
            let tree = buf.undo_tree();
            let mut node_id = tree.current();
            let mut visited = std::collections::HashSet::new();
            while visited.insert(node_id) {
                let info = tree.node_info(node_id);
                let branch_count = tree.children(node_id).len();
                let time = undo_stop_time(info.timestamp.elapsed());
                items.push(TemporalItem {
                    label: time.relative_label.clone(),
                    detail: Some(info.description.clone()),
                    summary: info.description.clone(),
                    kind: render::StripNodeKind::UndoNode,
                    branch_count,
                    time,
                    scope_summary: TemporalScopeSummary::CurrentPage,
                    restore_effect: TemporalRestoreEffect::RestoreUndoNode,
                    branch: branch_context(branch_count),
                    checkpoint: None,
                    lineage: None,
                    content: Some(tree.node_snapshot_string(node_id)),
                    lineage_snapshot: None,
                    undo_node_id: Some(node_id),
                    git_oid: None,
                    skip: false,
                });
                match tree.parent(node_id) {
                    Some(parent) => node_id = parent,
                    None => break,
                }
            }
        }
        // Undo items are newest-first (from current to root). Reverse for left=older.
        items.reverse();

        // Request git history (async — will arrive via handle_history_complete)
        let uuid_hex = page_id.to_hex();
        if let Some(tx) = &self.history_tx {
            let _ = tx.send(HistoryRequest::PageHistory {
                uuid: uuid_hex,
                limit: 50,
            });
        }

        // Select the most recent item (rightmost)
        let selected = items.len().saturating_sub(1);

        self.temporal_strip = Some(TemporalStripState {
            mode: render::TemporalMode::PageHistory,
            items,
            selected,
            compact: true,
            page_id,
            current_content,
            block_id: None,
            block_line: None,
        });
    }

    /// Open block history as a temporal strip for the block under the cursor.
    pub(crate) fn open_block_history(&mut self) {
        let page_id = match self.active_page() {
            Some(id) => id.clone(),
            None => return,
        };

        let (cursor_line, _) = self.cursor_position();
        let (block_id_str, current_line_text) = {
            let Some(doc) = self.writer.buffers().document(&page_id) else {
                return;
            };
            if cursor_line >= doc.buffer().len_lines() {
                return;
            }
            let line_text = doc.buffer().line(cursor_line).to_string();
            match doc.block_id_at_line(cursor_line) {
                Some(entry) => (
                    entry.id.0.clone(),
                    line_text.trim_end_matches('\n').to_string(),
                ),
                None => {
                    self.push_notification(
                        "No block ID on this line".into(),
                        render::NotificationLevel::Warning,
                    );
                    return;
                }
            }
        };

        let block_pattern = format!("^{}", block_id_str);
        let mirror_pattern = format!("^={}", block_id_str);

        // Walk undo tree — collect versions where this block's line changed
        let mut items: Vec<TemporalItem> = Vec::new();
        let mut last_line_content: Option<String> = None;

        if let Some(buf) = self.writer.buffers().get(&page_id) {
            let tree = buf.undo_tree();
            let mut node_id = tree.current();
            let mut visited = std::collections::HashSet::new();
            while visited.insert(node_id) {
                let info = tree.node_info(node_id);
                let snapshot = tree.node_snapshot_string(node_id);
                // Find the line containing our block ID, or fall back to
                // same line number if the ID didn't exist in this version.
                let block_line_text =
                    extract_block_line(&snapshot, &block_pattern, &mirror_pattern, cursor_line);

                if let Some(ref line) = block_line_text {
                    // Only add if content changed from the next (newer) version
                    let changed = last_line_content.as_ref() != Some(line);
                    if changed {
                        let time = undo_stop_time(info.timestamp.elapsed());
                        let branch_count = tree.children(node_id).len();
                        items.push(TemporalItem {
                            label: time.relative_label.clone(),
                            detail: Some(info.description.clone()),
                            summary: info.description.clone(),
                            kind: render::StripNodeKind::UndoNode,
                            branch_count,
                            time,
                            scope_summary: TemporalScopeSummary::CurrentBlock,
                            restore_effect: TemporalRestoreEffect::ReplaceBlockLineCreatesUndoNode,
                            branch: branch_context(branch_count),
                            checkpoint: None,
                            lineage: None,
                            content: Some(line.clone()),
                            lineage_snapshot: None,
                            undo_node_id: Some(node_id),
                            git_oid: None,
                            skip: false,
                        });
                    }
                    last_line_content = Some(line.clone());
                }

                match tree.parent(node_id) {
                    Some(parent) => node_id = parent,
                    None => break,
                }
            }
        }
        items.reverse(); // oldest first (left)

        // Request git history for git-based block versions
        let uuid_hex = page_id.to_hex();
        if let Some(tx) = &self.history_tx {
            let _ = tx.send(HistoryRequest::PageHistory {
                uuid: uuid_hex,
                limit: 50,
            });
        }

        let selected = items.len().saturating_sub(1);

        self.temporal_strip = Some(TemporalStripState {
            mode: render::TemporalMode::BlockHistory,
            items,
            selected,
            compact: true,
            page_id,
            current_content: current_line_text,
            block_id: Some(block_id_str.clone()),
            block_line: Some(cursor_line),
        });
    }

    /// Append git history entries to the temporal strip when they arrive.
    pub(crate) fn append_git_history(&mut self, entries: &[history::PageHistoryEntry]) {
        let Some(ts) = &mut self.temporal_strip else {
            return;
        };
        if !matches!(
            ts.mode,
            render::TemporalMode::PageHistory | render::TemporalMode::BlockHistory
        ) {
            return;
        }

        // Git entries are newest-first. Insert at the BEGINNING (older = left).
        let git_items: Vec<TemporalItem> = entries
            .iter()
            .rev()
            .map(|entry| {
                let time = git_stop_time(entry.timestamp);
                let changed_pages = entry.changed_files.len();
                TemporalItem {
                    label: time.relative_label.clone(),
                    detail: Some(entry.message.clone()),
                    summary: entry.message.clone(),
                    kind: render::StripNodeKind::GitCommit,
                    branch_count: 0,
                    time,
                    scope_summary: TemporalScopeSummary::PageSet {
                        count: changed_pages,
                        includes_mirrors: false,
                    },
                    restore_effect: if matches!(ts.mode, render::TemporalMode::BlockHistory) {
                        TemporalRestoreEffect::ReplaceBlockLineCreatesUndoNode
                    } else {
                        TemporalRestoreEffect::ReplaceBufferCreatesUndoNode
                    },
                    branch: None,
                    checkpoint: Some(TemporalCheckpointContext {
                        reason: checkpoint_reason(&entry.message),
                        changed_pages,
                    }),
                    lineage: None,
                    content: None, // Loaded on-demand via BlobAt
                    lineage_snapshot: None,
                    undo_node_id: None,
                    git_oid: Some(entry.oid.clone()),
                    skip: false,
                }
            })
            .collect();

        let git_count = git_items.len();
        // Insert git items before undo items (older on the left)
        let mut new_items = git_items;
        new_items.append(&mut ts.items);
        ts.items = new_items;
        // Adjust selected to keep pointing at the same item
        ts.selected += git_count;

        // For block history: eagerly fire BlobAt for all git entries (newest
        // first) so we can mark unchanged commits as skip. The UI stays
        // interactive — blobs load in the background and nodes dim as they
        // resolve.
        if matches!(ts.mode, render::TemporalMode::BlockHistory) {
            let uuid_hex = ts.page_id.to_hex();
            if let Some(tx) = &self.history_tx {
                // Fire newest-first: git items are oldest-first in the array
                // (indices 0..git_count), so iterate in reverse.
                for i in (0..git_count).rev() {
                    if let Some(oid) = &ts.items[i].git_oid {
                        let _ = tx.send(HistoryRequest::BlobAt {
                            oid: oid.clone(),
                            uuid: uuid_hex.clone(),
                        });
                    }
                }
            }
        }
    }

    /// Handle keys when temporal strip is active.
    pub(crate) fn handle_temporal_strip_key(
        &mut self,
        key: &types::KeyEvent,
    ) -> Vec<keymap::dispatch::Action> {
        match &key.code {
            types::KeyCode::Char('h') | types::KeyCode::Left => {
                if let Some(ts) = &mut self.temporal_strip {
                    // Move left, skipping over `skip` items
                    let mut target = ts.selected;
                    while target > 0 {
                        target -= 1;
                        if !ts.items[target].skip {
                            break;
                        }
                    }
                    if target != ts.selected {
                        ts.selected = target;
                        self.load_temporal_content_if_needed();
                    }
                }
            }
            types::KeyCode::Char('l') | types::KeyCode::Right => {
                if let Some(ts) = &mut self.temporal_strip {
                    // Move right, skipping over `skip` items
                    let mut target = ts.selected;
                    while target + 1 < ts.items.len() {
                        target += 1;
                        if !ts.items[target].skip {
                            break;
                        }
                    }
                    if target != ts.selected {
                        ts.selected = target;
                    }
                }
            }
            types::KeyCode::Char('e') => {
                if let Some(ts) = &mut self.temporal_strip {
                    ts.compact = !ts.compact;
                }
            }
            types::KeyCode::Char('r') => {
                self.temporal_strip_restore();
            }
            types::KeyCode::Char('c') => {
                self.create_explicit_checkpoint();
            }
            types::KeyCode::Char('q') | types::KeyCode::Esc => {
                self.temporal_strip = None;
            }
            _ => {}
        }
        vec![keymap::dispatch::Action::Noop]
    }

    /// Restore the selected temporal item to the buffer.
    fn temporal_strip_restore(&mut self) {
        let (content, undo_node_id, page_id, mode) = {
            let Some(ts) = &self.temporal_strip else {
                return;
            };
            let Some(item) = ts.items.get(ts.selected) else {
                return;
            };
            (
                item.content.clone(),
                item.undo_node_id,
                ts.page_id.clone(),
                ts.mode,
            )
        };

        match mode {
            render::TemporalMode::PageHistory => {
                if let Some(node_id) = undo_node_id {
                    let cursor_idx = self.active_cursor_idx();
                    if let Some(mut doc) = self.writer.buffers_mut().document_mut(&page_id) {
                        doc.restore_state(node_id, cursor_idx);
                    }
                } else if let Some(content) = content {
                    let cursor_policy = self
                        .writer
                        .buffers()
                        .get(&page_id)
                        .map(|buf| {
                            crate::document::CursorPolicy::reanchor_to_cursor(
                                buf,
                                self.active_cursor_idx(),
                            )
                        })
                        .unwrap_or(crate::document::CursorPolicy::Explicit { idx: 0, pos: 0 });
                    self.writer.apply(crate::BufferMessage::Reload {
                        page_id: page_id.clone(),
                        content,
                        cursor_policy,
                    });
                } else {
                    return;
                }
            }
            render::TemporalMode::BlockHistory => {
                // Block restore: replace only the line containing the block ID
                let Some(new_line) = content else { return };
                let (cursor_line, _) = self.cursor_position();
                if let Some(mut doc) = self.writer.buffers_mut().document_mut(&page_id) {
                    let clean_line = crate::document::clean_text_from_canonical_markdown(&new_line);
                    doc.replace_trimmed_line(
                        cursor_line,
                        &clean_line,
                        crate::document::CursorPolicy::Preserve,
                    );
                }
            }
            render::TemporalMode::DayActivity => return,
        }

        self.temporal_strip = None;
        self.save_page(&page_id);
        self.push_notification(
            "Restored from history".into(),
            render::NotificationLevel::Info,
        );
    }

    /// Load content for the selected temporal item (git commits are lazy-loaded).
    pub(crate) fn load_temporal_content_if_needed(&self) {
        let Some(ts) = &self.temporal_strip else {
            return;
        };
        let Some(item) = ts.items.get(ts.selected) else {
            return;
        };
        if item.content.is_some() {
            return; // Already loaded
        }
        if let Some(oid) = &item.git_oid {
            if let Some(tx) = &self.history_tx {
                let _ = tx.send(HistoryRequest::BlobAt {
                    oid: oid.clone(),
                    uuid: ts.page_id.to_hex(),
                });
            }
        }
    }
}

pub(crate) fn temporal_item_identity(item: &TemporalItem) -> String {
    if let Some(oid) = &item.git_oid {
        format!("git:{oid}")
    } else if let Some(node_id) = item.undo_node_id {
        format!("undo:{node_id:?}")
    } else {
        format!("item:{}:{}", item.label, item.summary)
    }
}

pub(crate) fn parse_temporal_block_snapshot(content: &str) -> TemporalBlockSnapshot {
    let (clean, entries) = crate::document::deserialize_canonical_markdown(content);
    let lines: Vec<&str> = clean.lines().collect();
    let blocks = entries
        .into_iter()
        .map(|entry| TemporalSnapshotBlock {
            id: entry.id.0,
            is_mirror: entry.is_mirror,
            first_line: entry.first_line,
            last_line: entry.last_line,
            text: lines
                .get(entry.first_line..entry.last_line.saturating_add(1))
                .unwrap_or(&[])
                .join("\n"),
        })
        .collect();
    TemporalBlockSnapshot { blocks }
}

pub(crate) fn rebuild_block_history_projection(ts: &mut TemporalStripState) {
    if !matches!(ts.mode, render::TemporalMode::BlockHistory) {
        return;
    }
    let Some(block_id) = ts.block_id.clone() else {
        return;
    };
    let selected_key = ts.items.get(ts.selected).map(temporal_item_identity);
    let mut real_items: Vec<TemporalItem> = ts
        .items
        .iter()
        .filter(|item| !matches!(item.kind, render::StripNodeKind::LineageEvent))
        .cloned()
        .collect();

    for item in &mut real_items {
        if matches!(item.kind, render::StripNodeKind::GitCommit) {
            item.skip = false;
        }
    }

    for idx in 0..real_items.len().saturating_sub(1) {
        if matches!(real_items[idx].kind, render::StripNodeKind::GitCommit)
            && matches!(real_items[idx + 1].kind, render::StripNodeKind::GitCommit)
            && real_items[idx].content.is_some()
            && real_items[idx + 1].content == real_items[idx].content
        {
            real_items[idx + 1].skip = true;
        }
    }

    let lineage_items: Vec<Option<TemporalItem>> = (0..real_items.len().saturating_sub(1))
        .map(|idx| synthetic_lineage_item(&real_items[idx], &real_items[idx + 1], &block_id))
        .collect();

    for (idx, lineage_item) in lineage_items.iter().enumerate() {
        if lineage_item.is_some()
            && matches!(real_items[idx + 1].kind, render::StripNodeKind::GitCommit)
        {
            real_items[idx + 1].skip = true;
        }
    }

    let mut rebuilt = Vec::with_capacity(real_items.len() + lineage_items.len());
    for idx in 0..real_items.len() {
        rebuilt.push(real_items[idx].clone());
        if let Some(lineage_item) = lineage_items.get(idx).cloned().flatten() {
            rebuilt.push(lineage_item);
        }
    }

    ts.items = rebuilt;
    if let Some(key) = selected_key {
        if let Some(new_idx) = ts
            .items
            .iter()
            .position(|item| temporal_item_identity(item) == key)
        {
            ts.selected = new_idx;
        } else {
            ts.selected = ts.selected.min(ts.items.len().saturating_sub(1));
        }
    }
}

fn synthetic_lineage_item(
    older: &TemporalItem,
    newer: &TemporalItem,
    tracked_block_id: &str,
) -> Option<TemporalItem> {
    if !matches!(older.kind, render::StripNodeKind::GitCommit)
        || !matches!(newer.kind, render::StripNodeKind::GitCommit)
    {
        return None;
    }
    let older_snapshot = older.lineage_snapshot.as_ref()?;
    let newer_snapshot = newer.lineage_snapshot.as_ref()?;

    let (event, related_ids, summary) = if let Some(child_id) =
        detect_split_spawned_child(older_snapshot, newer_snapshot, tracked_block_id)
    {
        (
            TemporalLineageEventKind::SplitSpawnedChild,
            vec![child_id.clone()],
            format!("Split; spawned ^{child_id}"),
        )
    } else if let Some(parent_id) =
        detect_split_from_parent(older_snapshot, newer_snapshot, tracked_block_id)
    {
        (
            TemporalLineageEventKind::SplitFromParent,
            vec![parent_id.clone()],
            format!("Split from ^{parent_id}"),
        )
    } else if let Some(retired_id) =
        detect_merged_from(older_snapshot, newer_snapshot, tracked_block_id)
    {
        (
            TemporalLineageEventKind::MergedFrom,
            vec![retired_id.clone()],
            format!("Merged from ^{retired_id}"),
        )
    } else if let Some(survivor_id) =
        detect_merged_into(older_snapshot, newer_snapshot, tracked_block_id)
    {
        (
            TemporalLineageEventKind::MergedInto,
            vec![survivor_id.clone()],
            format!("Merged into ^{survivor_id}"),
        )
    } else if detect_moved(older_snapshot, newer_snapshot, tracked_block_id) {
        (
            TemporalLineageEventKind::Moved,
            Vec::new(),
            "Moved within page".to_string(),
        )
    } else {
        return None;
    };

    let older_oid = older.git_oid.as_deref().unwrap_or("older");
    let newer_oid = newer.git_oid.as_deref().unwrap_or("newer");
    let synthetic_id = format!(
        "lineage:{older_oid}:{newer_oid}:{tracked_block_id}:{:?}",
        event
    );

    Some(TemporalItem {
        label: newer.label.clone(),
        detail: Some(summary.clone()),
        summary,
        kind: render::StripNodeKind::LineageEvent,
        branch_count: 0,
        time: newer.time.clone(),
        scope_summary: TemporalScopeSummary::CurrentBlock,
        restore_effect: TemporalRestoreEffect::ReplaceBlockLineCreatesUndoNode,
        branch: None,
        checkpoint: None,
        lineage: Some(TemporalLineageContext {
            event,
            primary_id: tracked_block_id.to_string(),
            related_ids,
            page_context: None,
        }),
        content: newer.content.clone(),
        lineage_snapshot: newer.lineage_snapshot.clone(),
        undo_node_id: None,
        git_oid: Some(synthetic_id),
        skip: false,
    })
}

fn detect_split_spawned_child(
    older: &TemporalBlockSnapshot,
    newer: &TemporalBlockSnapshot,
    tracked_block_id: &str,
) -> Option<String> {
    let (_, older_block) = find_block(older, tracked_block_id)?;
    let (newer_idx, newer_block) = find_block(newer, tracked_block_id)?;
    let older_ids = block_id_set(older);
    adjacent_block_candidates(newer, newer_idx)
        .into_iter()
        .find(|block| !older_ids.contains(block.id.as_str()))
        .and_then(|child| {
            block_text_matches_join(&older_block.text, &newer_block.text, &child.text)
                .then(|| child.id.clone())
        })
}

fn detect_split_from_parent(
    older: &TemporalBlockSnapshot,
    newer: &TemporalBlockSnapshot,
    tracked_block_id: &str,
) -> Option<String> {
    if find_block(older, tracked_block_id).is_some() {
        return None;
    }
    let (tracked_idx, tracked_block) = find_block(newer, tracked_block_id)?;
    adjacent_block_candidates(newer, tracked_idx)
        .into_iter()
        .filter_map(|candidate| {
            let (_, older_parent) = find_block(older, &candidate.id)?;
            block_text_matches_join(&older_parent.text, &candidate.text, &tracked_block.text)
                .then(|| candidate.id.clone())
        })
        .next()
}

fn detect_merged_from(
    older: &TemporalBlockSnapshot,
    newer: &TemporalBlockSnapshot,
    tracked_block_id: &str,
) -> Option<String> {
    let (older_idx, older_block) = find_block(older, tracked_block_id)?;
    let (_, newer_block) = find_block(newer, tracked_block_id)?;
    let newer_ids = block_id_set(newer);
    adjacent_block_candidates(older, older_idx)
        .into_iter()
        .find(|block| !newer_ids.contains(block.id.as_str()))
        .and_then(|retired| {
            block_text_matches_join(&newer_block.text, &older_block.text, &retired.text)
                .then(|| retired.id.clone())
        })
}

fn detect_merged_into(
    older: &TemporalBlockSnapshot,
    newer: &TemporalBlockSnapshot,
    tracked_block_id: &str,
) -> Option<String> {
    let (older_idx, older_block) = find_block(older, tracked_block_id)?;
    if find_block(newer, tracked_block_id).is_some() {
        return None;
    }
    adjacent_block_candidates(older, older_idx)
        .into_iter()
        .filter_map(|candidate| {
            let (_, newer_survivor) = find_block(newer, &candidate.id)?;
            block_text_matches_join(&newer_survivor.text, &older_block.text, &candidate.text)
                .then(|| candidate.id.clone())
        })
        .next()
}

fn detect_moved(
    older: &TemporalBlockSnapshot,
    newer: &TemporalBlockSnapshot,
    tracked_block_id: &str,
) -> bool {
    let (older_idx, older_block) = match find_block(older, tracked_block_id) {
        Some(result) => result,
        None => return false,
    };
    let (newer_idx, newer_block) = match find_block(newer, tracked_block_id) {
        Some(result) => result,
        None => return false,
    };
    older_idx != newer_idx
        && normalize_block_text(&older_block.text) == normalize_block_text(&newer_block.text)
        && block_id_set(older) == block_id_set(newer)
}

fn find_block<'a>(
    snapshot: &'a TemporalBlockSnapshot,
    block_id: &str,
) -> Option<(usize, &'a TemporalSnapshotBlock)> {
    snapshot
        .blocks
        .iter()
        .enumerate()
        .find(|(_, block)| block.id == block_id)
}

fn adjacent_block_candidates(
    snapshot: &TemporalBlockSnapshot,
    idx: usize,
) -> Vec<&TemporalSnapshotBlock> {
    let mut candidates = Vec::new();
    if idx > 0 {
        candidates.push(&snapshot.blocks[idx - 1]);
    }
    if idx + 1 < snapshot.blocks.len() {
        candidates.push(&snapshot.blocks[idx + 1]);
    }
    candidates
}

fn block_id_set(snapshot: &TemporalBlockSnapshot) -> std::collections::HashSet<&str> {
    snapshot
        .blocks
        .iter()
        .map(|block| block.id.as_str())
        .collect()
}

fn block_text_matches_join(target: &str, left: &str, right: &str) -> bool {
    let target = normalize_block_text(target);
    let left = normalize_block_text(left);
    let right = normalize_block_text(right);
    join_normalized_parts([left.as_str(), right.as_str()]) == target
        || join_normalized_parts([right.as_str(), left.as_str()]) == target
}

fn join_normalized_parts(parts: [&str; 2]) -> String {
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_block_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Extract the block line from a full-page snapshot.
/// First tries to find the line by block ID pattern match.
/// Falls back to the same line number if the ID didn't exist yet.
pub(crate) fn extract_block_line(
    content: &str,
    block_pattern: &str,
    mirror_pattern: &str,
    fallback_line: usize,
) -> Option<String> {
    let clean_line = |line: &str| {
        crate::document::clean_text_from_canonical_markdown(&format!("{line}\n"))
            .trim_end_matches('\n')
            .to_string()
    };
    // Primary: find by block ID
    if let Some(line) = content
        .lines()
        .find(|l| l.contains(block_pattern) || l.contains(mirror_pattern))
    {
        return Some(clean_line(line));
    }
    // Fallback: same line number (block ID may not have existed in this version)
    content.lines().nth(fallback_line).map(clean_line)
}

#[cfg(test)]
mod tests {
    use crate::{TemporalCheckpointReason, TemporalRestoreEffect, TemporalScopeSummary};

    fn git_block_item(
        oid: &str,
        label: &str,
        raw_snapshot: &str,
        tracked_block_id: &str,
    ) -> crate::TemporalItem {
        let block_pattern = format!("^{tracked_block_id}");
        let mirror_pattern = format!("^={tracked_block_id}");
        crate::TemporalItem {
            label: label.into(),
            detail: Some("git".into()),
            summary: "git".into(),
            kind: crate::render::StripNodeKind::GitCommit,
            branch_count: 0,
            time: crate::TemporalStopTime {
                timestamp: None,
                relative_label: label.into(),
                absolute_label: None,
            },
            scope_summary: crate::TemporalScopeSummary::CurrentBlock,
            restore_effect: crate::TemporalRestoreEffect::ReplaceBlockLineCreatesUndoNode,
            branch: None,
            checkpoint: None,
            lineage: None,
            content: super::extract_block_line(raw_snapshot, &block_pattern, &mirror_pattern, 0),
            lineage_snapshot: Some(super::parse_temporal_block_snapshot(raw_snapshot)),
            undo_node_id: None,
            git_oid: Some(oid.into()),
            skip: false,
        }
    }

    #[test]
    fn append_git_history_populates_structured_metadata() {
        let config = crate::config::Config::defaults();
        let mut editor = crate::BloomEditor::new(config).unwrap();
        let page_id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &page_id,
            "Test",
            std::path::Path::new("[scratch]"),
            "hello\n",
        );
        editor.open_page_history();

        editor.append_git_history(&[crate::history::PageHistoryEntry {
            oid: "abc123".into(),
            message: "idle timeout".into(),
            timestamp: 1_710_000_000,
            changed_files: vec!["abc12345.md".into(), "def67890.md".into()],
        }]);

        let ts = editor
            .temporal_strip
            .as_ref()
            .expect("temporal strip should open");
        let item = ts.items.first().expect("git item should be inserted");
        assert_eq!(item.summary, "idle timeout");
        assert_eq!(
            item.scope_summary,
            TemporalScopeSummary::PageSet {
                count: 2,
                includes_mirrors: false,
            }
        );
        assert_eq!(
            item.restore_effect,
            TemporalRestoreEffect::ReplaceBufferCreatesUndoNode
        );
        assert_eq!(
            item.checkpoint.as_ref().map(|ctx| &ctx.reason),
            Some(&TemporalCheckpointReason::IdleTimeout)
        );
    }

    #[test]
    fn extract_block_line_strips_serialized_block_ids() {
        let line =
            super::extract_block_line("keep me ^abc123\nother line\n", "^abc123", "^=abc123", 0)
                .expect("line should be found");
        assert_eq!(line, "keep me");

        let mirror = super::extract_block_line("mirror text ^=abc123\n", "^abc123", "^=abc123", 0)
            .expect("mirror line should be found");
        assert_eq!(mirror, "mirror text");
    }

    #[test]
    fn synthetic_lineage_item_detects_split_from_parent() {
        let older = git_block_item("old", "older", "Hello world ^parent\n", "child");
        let newer = git_block_item("new", "newer", "Hello ^parent\n\nworld ^child\n", "child");

        let lineage =
            super::synthetic_lineage_item(&older, &newer, "child").expect("lineage stop expected");

        assert_eq!(lineage.kind, crate::render::StripNodeKind::LineageEvent);
        assert_eq!(
            lineage.lineage.as_ref().map(|ctx| &ctx.event),
            Some(&crate::TemporalLineageEventKind::SplitFromParent)
        );
        assert_eq!(
            lineage.lineage.as_ref().map(|ctx| ctx.related_ids.clone()),
            Some(vec!["parent".into()])
        );
        assert_eq!(lineage.summary, "Split from ^parent");
    }

    #[test]
    fn rebuild_block_history_projection_inserts_merge_lineage_stop() {
        let mut ts = crate::TemporalStripState {
            mode: crate::render::TemporalMode::BlockHistory,
            items: vec![
                git_block_item("old", "older", "alpha ^keep\n\nbeta ^retired\n", "keep"),
                git_block_item("new", "newer", "alpha beta ^keep\n", "keep"),
            ],
            selected: 1,
            compact: true,
            page_id: crate::uuid::generate_hex_id(),
            current_content: "alpha beta".into(),
            block_id: Some("keep".into()),
            block_line: Some(0),
        };

        super::rebuild_block_history_projection(&mut ts);

        assert_eq!(ts.items.len(), 3);
        assert_eq!(ts.items[1].kind, crate::render::StripNodeKind::LineageEvent);
        assert_eq!(
            ts.items[1].lineage.as_ref().map(|ctx| &ctx.event),
            Some(&crate::TemporalLineageEventKind::MergedFrom)
        );
        assert_eq!(ts.items[1].summary, "Merged from ^retired");
        assert!(
            ts.items[2].skip,
            "raw git stop should be dimmed behind lineage stop"
        );
        assert_eq!(
            ts.selected, 2,
            "selection should stay on the same real git item"
        );
    }

    #[test]
    fn git_history_restore_reanchors_cursor_by_line_and_column() {
        let config = crate::config::Config::defaults();
        let mut editor = crate::BloomEditor::new(config).unwrap();
        let page_id = crate::uuid::generate_hex_id();
        editor.open_page_with_content(
            &page_id,
            "Test",
            std::path::Path::new("[scratch]"),
            "short\nkeep me here\n",
        );

        let line_start = editor
            .writer
            .buffers()
            .get(&page_id)
            .unwrap()
            .text()
            .line_to_char(1);
        editor.set_cursor(line_start + 4);
        assert_eq!(editor.cursor_position(), (1, 4));

        editor.temporal_strip = Some(crate::TemporalStripState {
            mode: crate::render::TemporalMode::PageHistory,
            items: vec![crate::TemporalItem {
                label: "older".into(),
                detail: Some("git".into()),
                summary: "git".into(),
                kind: crate::render::StripNodeKind::GitCommit,
                branch_count: 0,
                time: crate::TemporalStopTime {
                    timestamp: None,
                    relative_label: "older".into(),
                    absolute_label: None,
                },
                scope_summary: crate::TemporalScopeSummary::CurrentPage,
                restore_effect: crate::TemporalRestoreEffect::ReplaceBufferCreatesUndoNode,
                branch: None,
                checkpoint: None,
                lineage: None,
                content: Some("this line got much longer before restore\nkeep me here\n".into()),
                lineage_snapshot: None,
                undo_node_id: None,
                git_oid: Some("abc123".into()),
                skip: false,
            }],
            selected: 0,
            compact: true,
            page_id: page_id.clone(),
            current_content: editor
                .writer
                .buffers()
                .get(&page_id)
                .unwrap()
                .text()
                .to_string(),
            block_id: None,
            block_line: None,
        });

        editor.temporal_strip_restore();

        assert!(editor.temporal_strip.is_none());
        assert_eq!(
            editor
                .writer
                .buffers()
                .get(&page_id)
                .unwrap()
                .text()
                .to_string(),
            "this line got much longer before restore\nkeep me here\n",
        );
        assert_eq!(
            editor.cursor_position(),
            (1, 4),
            "git history restore should keep the cursor on the same logical line/column",
        );
    }
}
