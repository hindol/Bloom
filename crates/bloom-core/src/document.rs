//! Local document-model owner for mutable buffer state.
//!
//! This layer keeps low-level text mutation in `bloom-buffer`, while owning
//! parser lifecycle and block-ID coordination for one open document inside
//! `bloom-core`.

use std::{collections::HashSet, ops::Range};

use bloom_md::parser::traits::DocumentParser;

use crate::{block_id_gen, parse_tree::ParseTree, BufferSlot, ManagedBuffer};

pub(crate) enum CursorUpdate {
    Preserve,
    Set { idx: usize, pos: usize },
}

pub(crate) struct EditRequest<'a> {
    pub range: Range<usize>,
    pub replacement: &'a str,
    pub cursor: CursorUpdate,
}

pub(crate) struct Document<'a> {
    managed: &'a ManagedBuffer,
}

impl<'a> Document<'a> {
    pub(crate) fn new(managed: &'a ManagedBuffer) -> Self {
        Self { managed }
    }

    pub(crate) fn buffer(&self) -> &bloom_buffer::Buffer {
        self.managed.slot.as_buffer()
    }

    pub(crate) fn parse_tree(&self) -> &ParseTree {
        &self.managed.parse_tree
    }

    pub(crate) fn is_read_only(&self) -> bool {
        self.managed.slot.is_read_only()
    }
}

pub(crate) struct DocumentMut<'a> {
    managed: &'a mut ManagedBuffer,
}

impl<'a> DocumentMut<'a> {
    pub(crate) fn new(managed: &'a mut ManagedBuffer) -> Self {
        Self { managed }
    }

    pub(crate) fn document(&self) -> Document<'_> {
        Document::new(self.managed)
    }

    pub(crate) fn apply_edit(&mut self, request: EditRequest<'_>) -> bool {
        let undo_cursor_idx = match request.cursor {
            CursorUpdate::Set { idx, .. } => idx,
            CursorUpdate::Preserve => 0,
        };
        let Some((line_before, old_end, new_end)) = (|| {
            let buf = self.mutable_buffer()?;
            let line_before = char_pos_to_line(buf, request.range.start);
            let lines_before = buf.len_lines();

            if request.replacement.is_empty() && !request.range.is_empty() {
                buf.delete_with_undo_cursor(request.range.clone(), undo_cursor_idx);
            } else if request.range.is_empty() {
                buf.insert_with_undo_cursor(
                    request.range.start,
                    request.replacement,
                    undo_cursor_idx,
                );
            } else {
                buf.replace_with_undo_cursor(
                    request.range.clone(),
                    request.replacement,
                    undo_cursor_idx,
                );
            }

            if let CursorUpdate::Set { idx, pos } = request.cursor {
                buf.ensure_cursors(idx + 1);
                buf.set_cursor(idx, pos.min(buf.len_chars()));
            }

            let lines_after = buf.len_lines();
            let delta = lines_after as isize - lines_before as isize;
            let old_end = (line_before + 1).min(lines_before);
            let new_end = ((old_end as isize + delta).max(line_before as isize + 1)) as usize;
            Some((line_before, old_end, new_end))
        })() else {
            return false;
        };

        self.managed
            .parse_tree
            .mark_dirty(line_before, old_end, new_end);
        true
    }

    pub(crate) fn replace_trimmed_line(
        &mut self,
        line_idx: usize,
        new_text: &str,
        cursor: CursorUpdate,
    ) -> bool {
        let Some((line_start, old_len)) = ({
            let buf = self.managed.slot.as_buffer();
            if line_idx >= buf.len_lines() {
                return false;
            }
            let old_line = buf.line(line_idx).to_string();
            let old_trimmed = old_line.trim_end_matches('\n');
            Some((buf.text().line_to_char(line_idx), old_trimmed.len()))
        }) else {
            return false;
        };

        self.apply_edit(EditRequest {
            range: line_start..line_start + old_len,
            replacement: new_text,
            cursor,
        })
    }

    pub(crate) fn replace_all(&mut self, content: &str, cursor: CursorUpdate) -> bool {
        let len = self.managed.slot.as_buffer().len_chars();
        self.apply_edit(EditRequest {
            range: 0..len,
            replacement: content,
            cursor,
        })
    }

    pub(crate) fn insert_at(&mut self, pos: usize, text: &str, cursor: CursorUpdate) -> bool {
        self.apply_edit(EditRequest {
            range: pos..pos,
            replacement: text,
            cursor,
        })
    }

    pub(crate) fn delete_line(&mut self, line_idx: usize, cursor: CursorUpdate) -> bool {
        let Some(range) = ({
            let buf = self.managed.slot.as_buffer();
            if line_idx >= buf.len_lines() {
                return false;
            }
            let line_start = buf.text().line_to_char(line_idx);
            let line_end = if line_idx + 1 < buf.len_lines() {
                buf.text().line_to_char(line_idx + 1)
            } else {
                buf.len_chars()
            };
            (line_start < line_end).then_some(line_start..line_end)
        }) else {
            return false;
        };

        self.apply_edit(EditRequest {
            range,
            replacement: "",
            cursor,
        })
    }

    pub(crate) fn begin_edit_group(&mut self, cursor_idx: usize) -> bool {
        let Some(buf) = self.mutable_buffer() else {
            return false;
        };
        buf.begin_edit_group_with_cursor(cursor_idx);
        true
    }

    pub(crate) fn end_edit_group(&mut self, cursor_idx: usize) -> bool {
        let Some(buf) = self.mutable_buffer() else {
            return false;
        };
        buf.end_edit_group_with_cursor(cursor_idx);
        true
    }

    pub(crate) fn restore_edit_group_checkpoint(&mut self) -> bool {
        let Some(restored) = (|| {
            let buf = self.mutable_buffer()?;
            Some(buf.restore_edit_group_checkpoint())
        })() else {
            return false;
        };

        if restored {
            self.rebuild_parse_tree();
        }
        restored
    }

    pub(crate) fn restore_state(
        &mut self,
        node_id: bloom_buffer::UndoNodeId,
        cursor_idx: usize,
    ) -> bool {
        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            buf.restore_state_with_cursor(node_id, cursor_idx);
            Some(())
        })() else {
            return false;
        };
        self.rebuild_parse_tree();
        true
    }

    pub(crate) fn undo(&mut self, cursor_idx: usize) -> bool {
        let Some(changed) = (|| {
            let buf = self.mutable_buffer()?;
            Some(buf.undo_with_cursor(cursor_idx))
        })() else {
            return false;
        };

        if changed {
            self.rebuild_parse_tree();
        }
        changed
    }

    pub(crate) fn redo(&mut self, cursor_idx: usize) -> bool {
        let Some(changed) = (|| {
            let buf = self.mutable_buffer()?;
            Some(buf.redo_with_cursor(cursor_idx))
        })() else {
            return false;
        };

        if changed {
            self.rebuild_parse_tree();
        }
        changed
    }

    pub(crate) fn mark_clean(&mut self) -> bool {
        let Some(buf) = self.mutable_buffer() else {
            return false;
        };
        buf.mark_clean();
        true
    }

    pub(crate) fn reload(&mut self, content: &str) {
        match &mut self.managed.slot {
            BufferSlot::Mutable(buf) => {
                *buf = bloom_buffer::Buffer::from_text(content);
            }
            BufferSlot::Frozen(buf) => {
                *buf = bloom_buffer::Buffer::from_text(content).freeze();
            }
        }
        self.managed.parse_tree = ParseTree::build(content);
    }

    pub(crate) fn align_page(&mut self) -> bool {
        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            crate::align::auto_align_page(buf);
            Some(())
        })() else {
            return false;
        };
        self.rebuild_parse_tree();
        true
    }

    pub(crate) fn align_block(&mut self, cursor_line: usize) -> bool {
        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            crate::align::auto_align_block(buf, cursor_line);
            Some(())
        })() else {
            return false;
        };
        self.rebuild_parse_tree();
        true
    }

    pub(crate) fn refresh_parse_tree_if_dirty(&mut self) {
        if !self.managed.parse_tree.is_dirty() {
            return;
        }

        let line_count = self.managed.slot.len_lines();
        let line_texts: Vec<String> = (0..line_count)
            .map(|i| self.managed.slot.line_text(i))
            .collect();

        self.managed.parse_tree.refresh(
            |i| line_texts.get(i).cloned().unwrap_or_default(),
            line_count,
        );
    }

    pub(crate) fn rebuild_parse_tree(&mut self) {
        let text = self.managed.slot.text_string();
        self.managed.parse_tree = ParseTree::build(&text);
    }

    pub(crate) fn ensure_block_ids<P: DocumentParser>(
        &mut self,
        parser: &P,
        known_ids: Option<&mut HashSet<String>>,
    ) -> bool {
        if self.document().is_read_only() {
            return false;
        }

        let text = self.document().buffer().text().to_string();
        let doc = parser.parse(&text);
        let insertions = block_id_gen::compute_block_id_assignments(&doc, known_ids.as_deref());
        if insertions.is_empty() {
            return false;
        }

        if let Some(known_ids) = known_ids {
            for insertion in &insertions {
                known_ids.insert(insertion.id.clone());
            }
        }

        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            buf.begin_edit_group();
            for insertion in insertions.iter().rev() {
                if insertion.line >= buf.len_lines() {
                    continue;
                }
                let line_start = buf.text().line_to_char(insertion.line);
                let line_slice = buf.line(insertion.line);
                let mut content_chars = line_slice.len_chars();
                let chars: Vec<char> = line_slice.chars().collect();
                while content_chars > 0
                    && matches!(chars[content_chars - 1], '\n' | '\r' | ' ' | '\t')
                {
                    content_chars -= 1;
                }
                let insert_at = line_start + content_chars;
                let insertion_text = format!(" ^{}", insertion.id);
                buf.insert(insert_at, &insertion_text);
            }
            buf.end_edit_group();
            Some(())
        })() else {
            return false;
        };

        self.rebuild_parse_tree();
        true
    }

    fn mutable_buffer(&mut self) -> Option<&mut bloom_buffer::Buffer> {
        match &mut self.managed.slot {
            BufferSlot::Mutable(buf) => Some(buf),
            BufferSlot::Frozen(_) => None,
        }
    }
}

fn char_pos_to_line(buf: &bloom_buffer::Buffer, pos: usize) -> usize {
    if buf.len_chars() == 0 {
        0
    } else {
        buf.text()
            .char_to_line(pos.min(buf.len_chars().saturating_sub(1)))
    }
}
