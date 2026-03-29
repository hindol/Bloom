//! Section mirroring — heading-level `^=` structural sync.
//!
//! When a `^=` heading exists in multiple pages, structural changes (add/delete
//! blocks) within the section are propagated to all peers. Individual block
//! content sync is handled by the existing leaf mirror machinery.
//!
//! See `docs/BLOCK_IDENTITY.md` § Section Mirroring for the design.

use bloom_md::parser::traits::Section;
use bloom_md::types::BlockId;

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

/// Collect ordered child block IDs within a section's line_range from the document layer.
/// Excludes the heading's own block ID.
pub(crate) fn section_child_ids(
    doc: &crate::document::Document<'_>,
    section: &Section,
) -> Vec<(BlockId, usize)> {
    let start = section.line_range.start + 1;
    let end = section.line_range.end;
    let mut children: Vec<(BlockId, usize)> = doc
        .block_ids()
        .iter()
        .filter(|entry| entry.first_line >= start && entry.last_line < end)
        .map(|entry| (entry.id.clone(), entry.last_line))
        .collect();
    children.sort_by_key(|(_, line)| *line);
    children
}

/// Pure diff: source children are truth.
pub fn structural_diff(source: &[(BlockId, usize)], peer: &[(BlockId, usize)]) -> SectionDiff {
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
            let Some(doc) = self.writer.buffers().document(page_id) else {
                return;
            };
            doc.mirror_sections()
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
                    let doc = self.writer.buffers().document(page_id)?;
                    section_child_ids(&doc, section)
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
                    let Some(doc) = self.writer.buffers().document(&meta.id) else {
                        continue;
                    };
                    doc.section_by_block_id(&sw.heading_bid)
                };
                let Some(peer_section) = peer_section else {
                    continue;
                };

                let peer_children = {
                    let Some(doc) = self.writer.buffers().document(&meta.id) else {
                        continue;
                    };
                    section_child_ids(&doc, &peer_section)
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
        if let Some(mut doc) = self.writer.buffers_mut().document_mut(page_id) {
            doc.refresh_parse_tree_if_dirty();
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
            let Some(doc) = self.writer.buffers().document(peer_id) else {
                return;
            };
            let start = peer_section.line_range.start + 1;
            let end = peer_section.line_range.end;
            for entry in doc.block_ids() {
                if entry.first_line >= start
                    && entry.last_line < end
                    && removal_set.contains(entry.id.0.as_str())
                {
                    lines_to_remove.push(entry.last_line);
                }
            }
        }

        // Remove in reverse order to preserve line indices
        lines_to_remove.reverse();
        for line_idx in lines_to_remove {
            if let Some(mut doc) = self.writer.buffers_mut().document_mut(peer_id) {
                if doc.delete_line(line_idx, crate::document::CursorUpdate::Preserve) {
                    self.refresh_parse_tree(peer_id);
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
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                text
            };

            // Find insertion point in peer: after the preceding sibling, or
            // right after the heading if this is the first child.
            let (insert_char_pos, inserted_line) = {
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
                    let Some(doc) = self.writer.buffers().document(peer_id) else {
                        continue;
                    };
                    let prev_line = doc.block_id(prev_bid).map(|entry| entry.last_line);
                    match prev_line {
                        Some(pl) if pl + 1 < buf.len_lines() => {
                            (buf.text().line_to_char(pl + 1), pl + 1)
                        }
                        Some(pl) => (buf.len_chars(), pl + 1),
                        _ => (buf.len_chars(), buf.len_lines().saturating_sub(1)),
                    }
                } else {
                    // No preceding sibling — insert right after the heading
                    let Some(doc) = self.writer.buffers().document(peer_id) else {
                        continue;
                    };
                    let heading_line = doc.block_id(heading_bid).map(|entry| entry.last_line);
                    match heading_line {
                        Some(hl) if hl + 1 < buf.len_lines() => {
                            (buf.text().line_to_char(hl + 1), hl + 1)
                        }
                        Some(hl) => (buf.len_chars(), hl + 1),
                        _ => (buf.len_chars(), buf.len_lines().saturating_sub(1)),
                    }
                }
            };

            // Insert the block text
            if let Some(mut doc) = self.writer.buffers_mut().document_mut(peer_id) {
                if doc.insert_at(
                    insert_char_pos,
                    &block_text,
                    crate::document::CursorUpdate::Preserve,
                ) {
                    let _ = doc.set_block_id_at_line(inserted_line, insert_bid.clone(), true);
                    self.refresh_parse_tree(peer_id);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    use bloom_buffer::Buffer;

    use crate::{document::DocumentState, BufferInfo, BufferSlot, ManagedBuffer};

    fn test_document(text: &str) -> crate::document::Document<'_> {
        let (clean_text, document) = DocumentState::from_markdown_disk_text(text);
        let managed = Box::new(ManagedBuffer {
            slot: BufferSlot::Mutable(Buffer::from_text(&clean_text)),
            info: BufferInfo {
                page_id: crate::types::PageId::from_hex("aaaaaaaa").unwrap(),
                title: "Test".to_string(),
                path: std::path::PathBuf::from("test.md"),
                dirty: false,
                last_focused: Instant::now(),
            },
            document,
        });
        crate::document::Document::new(Box::leak(managed))
    }

    #[test]
    fn diff_identical_sections() {
        let source = vec![(BlockId("t0001".into()), 2), (BlockId("t0002".into()), 3)];
        let peer = vec![(BlockId("t0001".into()), 2), (BlockId("t0002".into()), 3)];
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
        let peer = vec![(BlockId("t0001".into()), 2), (BlockId("t0002".into()), 3)];
        let diff = structural_diff(&source, &peer);
        assert_eq!(diff.inserts.len(), 1);
        assert_eq!(diff.inserts[0].0, BlockId("t0003".into()));
        assert!(diff.removals.is_empty());
    }

    #[test]
    fn diff_remove_block() {
        let source = vec![(BlockId("t0001".into()), 2)];
        let peer = vec![(BlockId("t0001".into()), 2), (BlockId("t0002".into()), 3)];
        let diff = structural_diff(&source, &peer);
        assert!(diff.inserts.is_empty());
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0], BlockId("t0002".into()));
    }

    #[test]
    fn diff_insert_and_remove() {
        let source = vec![(BlockId("t0001".into()), 2), (BlockId("t0003".into()), 3)];
        let peer = vec![(BlockId("t0001".into()), 2), (BlockId("t0002".into()), 3)];
        let diff = structural_diff(&source, &peer);
        assert_eq!(diff.inserts.len(), 1);
        assert_eq!(diff.inserts[0].0, BlockId("t0003".into()));
        assert_eq!(diff.removals.len(), 1);
        assert_eq!(diff.removals[0], BlockId("t0002".into()));
    }

    #[test]
    fn section_child_ids_basic() {
        let text = "## Tasks ^=head1\n- [ ] Task A ^=t0001\n- [ ] Task B ^=t0002\n\n## Other\n";
        let doc = test_document(text);
        let sections = doc.mirror_sections();
        let task_section = sections.iter().find(|s| s.title.contains("Tasks")).unwrap();
        let children = section_child_ids(&doc, task_section);
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].0, BlockId("t0001".into()));
        assert_eq!(children[1].0, BlockId("t0002".into()));
    }

    #[test]
    fn section_child_ids_excludes_heading() {
        let text = "## Tasks ^=head1\n- [ ] Task ^=t0001\n";
        let doc = test_document(text);
        let sections = doc.mirror_sections();
        let children = section_child_ids(&doc, &sections[0]);
        // heading's own block ID should not appear in children
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].0, BlockId("t0001".into()));
    }
}
