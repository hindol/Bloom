//! Section mirroring — heading-level `^=` structural sync.
//!
//! When a `^=` heading exists in multiple pages, structural changes (add/delete
//! blocks) within the section are propagated to all peers. Individual block
//! content sync is handled by the existing leaf mirror machinery.
//!
//! See `docs/BLOCK_IDENTITY.md` § Section Mirroring for the design.

use bloom_md::parser::traits::Section;
use bloom_md::types::BlockId;

use crate::parse_tree::ParseTree;

/// One-way diff: source section children are truth.
pub struct SectionDiff {
    /// Blocks present in source but missing in peer: (block_id, source_line_idx).
    pub inserts: Vec<(BlockId, usize)>,
    /// Blocks present in peer but not in source — should be removed.
    pub removals: Vec<BlockId>,
}

impl SectionDiff {
    pub fn is_empty(&self) -> bool {
        self.inserts.is_empty() && self.removals.is_empty()
    }
}

/// Collect ordered child block IDs within a section's line_range from the ParseTree.
/// Excludes the heading's own block ID.
pub fn section_child_ids(
    tree: &ParseTree,
    section: &Section,
) -> Vec<(BlockId, usize)> {
    let mut children = Vec::new();
    let start = section.line_range.start + 1; // skip the heading line itself
    let end = section.line_range.end;
    for line_idx in start..end.min(tree.len()) {
        if let Some(ld) = tree.line(line_idx) {
            if let Some(bid) = &ld.elements.block_id {
                children.push((bid.id.clone(), line_idx));
            }
        }
    }
    children
}

/// Pure diff: source children are truth.
pub fn structural_diff(
    source: &[(BlockId, usize)],
    peer: &[(BlockId, usize)],
) -> SectionDiff {
    use std::collections::HashSet;

    let peer_set: HashSet<&str> = peer.iter().map(|(id, _)| id.0.as_str()).collect();
    let source_set: HashSet<&str> = source.iter().map(|(id, _)| id.0.as_str()).collect();

    let inserts: Vec<(BlockId, usize)> = source
        .iter()
        .filter(|(id, _)| !peer_set.contains(id.0.as_str()))
        .map(|(id, line)| (id.clone(), *line))
        .collect();

    let removals: Vec<BlockId> = peer
        .iter()
        .filter(|(id, _)| !source_set.contains(id.0.as_str()))
        .map(|(id, _)| id.clone())
        .collect();

    SectionDiff { inserts, removals }
}

// --- impl BloomEditor ---

use crate::types;
use crate::BloomEditor;

impl BloomEditor {
    /// After leaf mirror propagation, check if the active page has any `^=`
    /// heading sections. If so, diff each section's children against all peer
    /// pages and apply structural changes (insert/remove blocks).
    pub(crate) fn propagate_section_structure(&mut self, page_id: &types::PageId) {
        // Refresh source parse tree (may be dirty after ensure_block_ids).
        self.refresh_parse_tree(page_id);

        let mirror_sections = {
            let Some(tree) = self.writer.buffers().parse_tree(page_id) else {
                return;
            };
            tree.mirror_sections()
        };
        if mirror_sections.is_empty() {
            return;
        }

        // Collect all work upfront to avoid holding borrows across mutations.
        struct SyncWork {
            heading_bid: BlockId,
            source_children: Vec<(BlockId, usize)>,
            peers: Vec<(types::PageMeta, usize)>,
        }

        let work: Vec<SyncWork> = mirror_sections
            .iter()
            .filter_map(|section| {
                let heading_bid = section.block_id.as_ref()?.clone();
                let source_children = {
                    let tree = self.writer.buffers().parse_tree(page_id)?;
                    section_child_ids(tree, section)
                };
                let peers = self
                    .index
                    .as_ref()?
                    .find_all_pages_by_block_id(&heading_bid);
                Some(SyncWork {
                    heading_bid,
                    source_children,
                    peers,
                })
            })
            .collect();

        let mut sync_count = 0usize;

        for sw in &work {
            for (meta, _peer_heading_line) in &sw.peers {
                if meta.id == *page_id {
                    continue;
                }

                self.ensure_peer_buffer_loaded(meta);
                self.refresh_parse_tree(&meta.id);

                let peer_section = {
                    let Some(tree) = self.writer.buffers().parse_tree(&meta.id) else {
                        continue;
                    };
                    tree.section_by_block_id(&sw.heading_bid)
                };
                let Some(peer_section) = peer_section else {
                    continue;
                };

                let peer_children = {
                    let Some(tree) = self.writer.buffers().parse_tree(&meta.id) else {
                        continue;
                    };
                    section_child_ids(tree, &peer_section)
                };

                let diff = structural_diff(&sw.source_children, &peer_children);
                if diff.is_empty() {
                    continue;
                }

                self.apply_removals(&meta.id, &diff.removals, &peer_section);
                self.apply_insertions(
                    page_id,
                    &meta.id,
                    &diff.inserts,
                    &sw.source_children,
                    &sw.heading_bid,
                );

                self.save_page(&meta.id);
                sync_count += 1;
            }
        }

        if sync_count > 0 {
            self.push_notification(
                format!(
                    "🪞 Section synced to {sync_count} page{}",
                    if sync_count == 1 { "" } else { "s" }
                ),
                crate::render::NotificationLevel::Info,
            );
        }
    }

    /// Load a peer page's buffer from disk if not already open.
    fn ensure_peer_buffer_loaded(&mut self, meta: &types::PageMeta) {
        if self.writer.buffers().get(&meta.id).is_some() {
            return;
        }
        let full = self
            .vault_root
            .as_ref()
            .map(|r| r.join(&meta.path))
            .unwrap_or_else(|| meta.path.clone());
        if let Ok(content) = std::fs::read_to_string(&full) {
            self.writer.apply(crate::BufferMessage::Open {
                page_id: meta.id.clone(),
                title: meta.title.clone(),
                path: full,
                content,
            });
        }
    }

    /// Refresh a single parse tree if dirty.
    fn refresh_parse_tree(&mut self, page_id: &types::PageId) {
        let needs_refresh = self
            .writer
            .buffers()
            .parse_tree(page_id)
            .is_some_and(|pt| pt.is_dirty());
        if !needs_refresh {
            return;
        }
        let line_count = self
            .writer
            .buffers()
            .get(page_id)
            .map(|b| b.len_lines())
            .unwrap_or(0);
        let line_texts: Vec<String> = (0..line_count)
            .map(|i| {
                self.writer
                    .buffers()
                    .get(page_id)
                    .map(|b| b.line(i).to_string())
                    .unwrap_or_default()
            })
            .collect();
        if let Some(pt) = self.writer.buffers_mut().parse_tree_mut(page_id) {
            pt.refresh(
                |i| line_texts.get(i).cloned().unwrap_or_default(),
                line_count,
            );
        }
    }

    /// Remove blocks from peer that are no longer in the source section.
    fn apply_removals(
        &mut self,
        peer_id: &types::PageId,
        removals: &[BlockId],
        peer_section: &Section,
    ) {
        if removals.is_empty() {
            return;
        }
        use std::collections::HashSet;
        let removal_set: HashSet<&str> = removals.iter().map(|id| id.0.as_str()).collect();

        // Find lines to remove (within peer section, in reverse order)
        let mut lines_to_remove: Vec<usize> = Vec::new();
        {
            let Some(tree) = self.writer.buffers().parse_tree(peer_id) else {
                return;
            };
            let start = peer_section.line_range.start + 1;
            let end = peer_section.line_range.end.min(tree.len());
            for line_idx in start..end {
                if let Some(ld) = tree.line(line_idx) {
                    if let Some(bid) = &ld.elements.block_id {
                        if removal_set.contains(bid.id.0.as_str()) {
                            lines_to_remove.push(line_idx);
                        }
                    }
                }
            }
        }

        // Remove in reverse order to preserve line indices
        lines_to_remove.reverse();
        for line_idx in lines_to_remove {
            if let Some(buf) = self.writer.buffers_mut().get_mut(peer_id) {
                if line_idx < buf.len_lines() {
                    let ls = buf.text().line_to_char(line_idx);
                    let le = if line_idx + 1 < buf.len_lines() {
                        buf.text().line_to_char(line_idx + 1)
                    } else {
                        buf.len_chars()
                    };
                    if ls < le {
                        buf.delete(ls..le);
                    }
                }
            }
        }
    }

    /// Insert blocks from source that are missing in peer.
    fn apply_insertions(
        &mut self,
        source_id: &types::PageId,
        peer_id: &types::PageId,
        inserts: &[(BlockId, usize)],
        source_children: &[(BlockId, usize)],
        heading_bid: &BlockId,
    ) {
        if inserts.is_empty() {
            return;
        }

        for (insert_bid, source_line) in inserts {
            // Get the block text from source
            let block_text = {
                let Some(buf) = self.writer.buffers().get(source_id) else {
                    continue;
                };
                if *source_line >= buf.len_lines() {
                    continue;
                }
                let mut text = buf.line(*source_line).to_string();
                // Ensure the block has ^= marker (auto-promote)
                if text.contains(&format!(" ^{}", insert_bid.0))
                    && !text.contains(&format!(" ^={}", insert_bid.0))
                {
                    text = text.replace(
                        &format!(" ^{}", insert_bid.0),
                        &format!(" ^={}", insert_bid.0),
                    );
                }
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text
            };

            // Find insertion point in peer: after the preceding sibling, or
            // right after the heading if this is the first child.
            let insert_char_pos = {
                let Some(buf) = self.writer.buffers().get(peer_id) else {
                    continue;
                };
                // Find the source child that precedes this insert
                let preceding_bid = source_children
                    .iter()
                    .take_while(|(id, _)| id != insert_bid)
                    .last()
                    .map(|(id, _)| id);

                if let Some(prev_bid) = preceding_bid {
                    // Find the preceding sibling's line in peer, insert after it
                    let Some(tree) = self.writer.buffers().parse_tree(peer_id) else {
                        continue;
                    };
                    let prev_line = (0..tree.len()).find(|&i| {
                        tree.line(i)
                            .and_then(|ld| ld.elements.block_id.as_ref())
                            .is_some_and(|b| b.id == *prev_bid)
                    });
                    match prev_line {
                        Some(pl) if pl + 1 < buf.len_lines() => {
                            buf.text().line_to_char(pl + 1)
                        }
                        _ => buf.len_chars(), // fallback: end of buffer
                    }
                } else {
                    // No preceding sibling — insert right after the heading
                    let Some(tree) = self.writer.buffers().parse_tree(peer_id) else {
                        continue;
                    };
                    let heading_line = (0..tree.len()).find(|&i| {
                        tree.line(i)
                            .and_then(|ld| ld.elements.block_id.as_ref())
                            .is_some_and(|b| b.id == *heading_bid)
                    });
                    match heading_line {
                        Some(hl) if hl + 1 < buf.len_lines() => {
                            buf.text().line_to_char(hl + 1)
                        }
                        _ => buf.len_chars(),
                    }
                }
            };

            // Insert the block text
            if let Some(buf) = self.writer.buffers_mut().get_mut(peer_id) {
                buf.insert(insert_char_pos, &block_text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_identical_sections() {
        let source = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
        ];
        let peer = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
        ];
        let diff = structural_diff(&source, &peer);
        assert!(diff.is_empty());
    }

    #[test]
    fn diff_insert_new_block() {
        let source = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
            (BlockId("t0003".into()), 4),
        ];
        let peer = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
        ];
        let diff = structural_diff(&source, &peer);
        assert_eq!(diff.inserts.len(), 1);
        assert_eq!(diff.inserts[0].0, BlockId("t0003".into()));
        assert!(diff.removals.is_empty());
    }

    #[test]
    fn diff_remove_block() {
        let source = vec![(BlockId("t0001".into()), 2)];
        let peer = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
        ];
        let diff = structural_diff(&source, &peer);
        assert!(diff.inserts.is_empty());
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0], BlockId("t0002".into()));
    }

    #[test]
    fn diff_insert_and_remove() {
        let source = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0003".into()), 3),
        ];
        let peer = vec![
            (BlockId("t0001".into()), 2),
            (BlockId("t0002".into()), 3),
        ];
        let diff = structural_diff(&source, &peer);
        assert_eq!(diff.inserts.len(), 1);
        assert_eq!(diff.inserts[0].0, BlockId("t0003".into()));
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0], BlockId("t0002".into()));
    }

    #[test]
    fn section_child_ids_basic() {
        let text = "## Tasks ^=head1\n- [ ] Task A ^=t0001\n- [ ] Task B ^=t0002\n\n## Other\n";
        let tree = ParseTree::build(text);
        let sections = tree.sections();
        let task_section = sections.iter().find(|s| s.title.contains("Tasks")).unwrap();
        let children = section_child_ids(&tree, task_section);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].0, BlockId("t0001".into()));
        assert_eq!(children[1].0, BlockId("t0002".into()));
    }

    #[test]
    fn section_child_ids_excludes_heading() {
        let text = "## Tasks ^=head1\n- [ ] Task ^=t0001\n";
        let tree = ParseTree::build(text);
        let sections = tree.sections();
        let children = section_child_ids(&tree, &sections[0]);
        // heading's own block ID should not appear in children
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, BlockId("t0001".into()));
    }
}
