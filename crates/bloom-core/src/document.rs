//! Local document-model owner for mutable buffer state.
//!
//! This layer keeps low-level text mutation in `bloom-buffer`, while owning
//! parser lifecycle, hidden block-ID metadata, and canonical disk-text
//! serialization for one open document inside `bloom-core`.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use bloom_md::{
    parser::{
        extensions::parse_block_id,
        markdown::BloomMarkdownParser,
        traits::{DocumentParser, ParsedBlock, Section},
    },
    types::BlockId,
};

use crate::{parse_tree::ParseTree, BufferSlot, ManagedBuffer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BlockIdEntry {
    pub id: BlockId,
    pub first_line: usize,
    pub last_line: usize,
    pub is_mirror: bool,
}

pub(crate) struct DocumentState {
    pub parse_tree: ParseTree,
    pub block_ids: Vec<BlockIdEntry>,
    pub block_id_history: HashMap<bloom_buffer::UndoNodeId, Vec<BlockIdEntry>>,
}

impl DocumentState {
    pub(crate) fn from_clean_text(text: &str) -> Self {
        Self {
            parse_tree: ParseTree::build(text),
            block_ids: Vec::new(),
            block_id_history: HashMap::new(),
        }
    }

    pub(crate) fn from_markdown_disk_text(text: &str) -> (String, Self) {
        let (clean_text, block_ids) = deserialize_canonical_markdown(text);
        let mut state = Self {
            parse_tree: ParseTree::build(&clean_text),
            block_ids,
            block_id_history: HashMap::new(),
        };
        state.sort_block_ids();
        (clean_text, state)
    }

    fn sort_block_ids(&mut self) {
        self.block_ids
            .sort_by_key(|entry| (entry.first_line, entry.last_line, entry.id.0.clone()));
    }
}

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
        &self.managed.document.parse_tree
    }

    pub(crate) fn is_read_only(&self) -> bool {
        self.managed.slot.is_read_only()
    }

    pub(crate) fn block_ids(&self) -> &[BlockIdEntry] {
        &self.managed.document.block_ids
    }

    pub(crate) fn block_id_at_line(&self, line: usize) -> Option<&BlockIdEntry> {
        self.managed
            .document
            .block_ids
            .iter()
            .find(|entry| line >= entry.first_line && line <= entry.last_line)
    }

    pub(crate) fn block_id(&self, id: &BlockId) -> Option<&BlockIdEntry> {
        self.managed
            .document
            .block_ids
            .iter()
            .find(|entry| entry.id == *id)
    }

    pub(crate) fn sections(&self) -> Vec<Section> {
        self.parse_tree()
            .sections()
            .into_iter()
            .map(|mut section| {
                section.block_id = self
                    .block_id_at_line(section.line_range.start)
                    .map(|entry| entry.id.clone());
                section
            })
            .collect()
    }

    pub(crate) fn mirror_sections(&self) -> Vec<Section> {
        self.sections()
            .into_iter()
            .filter(|section| {
                self.block_id_at_line(section.line_range.start)
                    .is_some_and(|entry| entry.is_mirror)
            })
            .collect()
    }

    pub(crate) fn section_by_block_id(&self, block_id: &BlockId) -> Option<Section> {
        self.sections()
            .into_iter()
            .find(|section| section.block_id.as_ref() == Some(block_id))
    }

    pub(crate) fn canonical_text(&self) -> String {
        serialize_canonical(self.buffer(), self.block_ids())
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
        let shifted = {
            let buf = self.managed.slot.as_buffer();
            transform_entries(
                &self.managed.document.block_ids,
                buf,
                request.range.clone(),
                request.replacement,
            )
        };

        let undo_cursor_idx = match request.cursor {
            CursorUpdate::Set { idx, .. } => idx,
            CursorUpdate::Preserve => 0,
        };

        let changed = (|| {
            let buf = self.mutable_buffer()?;

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

            Some(())
        })()
        .is_some();

        if !changed {
            return false;
        }

        self.reconcile_after_text_change(shifted, false, None);
        if !self.managed.slot.as_buffer().in_edit_group() {
            self.capture_history_snapshot();
        }
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
        let Some(before_node) = self.mutable_buffer().map(|buf| buf.current_undo_node()) else {
            return false;
        };

        let Some(buf) = self.mutable_buffer() else {
            return false;
        };
        buf.end_edit_group_with_cursor(cursor_idx);
        let after_node = buf.current_undo_node();
        if after_node != before_node {
            self.capture_history_snapshot();
        }
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
            self.restore_history_snapshot_for_current_node();
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
        self.restore_history_snapshot_for_current_node();
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
            self.restore_history_snapshot_for_current_node();
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
            self.restore_history_snapshot_for_current_node();
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
        self.reload_clean_text(content);
    }

    pub(crate) fn reload_from_disk_markdown(&mut self, content: &str) {
        let (clean_text, mut state) = DocumentState::from_markdown_disk_text(content);
        replace_slot_with_text(&mut self.managed.slot, &clean_text);
        let current_node = self.managed.slot.as_buffer().current_undo_node();
        state
            .block_id_history
            .insert(current_node, state.block_ids.clone());
        self.managed.document = state;
    }

    pub(crate) fn align_page(&mut self) -> bool {
        let Some(before_node) = self.mutable_buffer().map(|buf| buf.current_undo_node()) else {
            return false;
        };

        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            crate::align::auto_align_page(buf);
            Some(())
        })() else {
            return false;
        };

        let shifted = self.managed.document.block_ids.clone();
        self.reconcile_after_text_change(shifted, false, None);
        let after_node = self.managed.slot.as_buffer().current_undo_node();
        if after_node != before_node {
            self.capture_history_snapshot();
        }
        true
    }

    pub(crate) fn align_block(&mut self, cursor_line: usize) -> bool {
        let Some(before_node) = self.mutable_buffer().map(|buf| buf.current_undo_node()) else {
            return false;
        };

        let Some(()) = (|| {
            let buf = self.mutable_buffer()?;
            crate::align::auto_align_block(buf, cursor_line);
            Some(())
        })() else {
            return false;
        };

        let shifted = self.managed.document.block_ids.clone();
        self.reconcile_after_text_change(shifted, false, None);
        let after_node = self.managed.slot.as_buffer().current_undo_node();
        if after_node != before_node {
            self.capture_history_snapshot();
        }
        true
    }

    pub(crate) fn refresh_parse_tree_if_dirty(&mut self) {
        if !self.managed.document.parse_tree.is_dirty() {
            return;
        }
        let shifted = self.managed.document.block_ids.clone();
        self.reconcile_after_text_change(shifted, false, None);
    }

    pub(crate) fn ensure_block_ids<P: DocumentParser>(
        &mut self,
        _parser: &P,
        known_ids: Option<&mut HashSet<String>>,
    ) -> bool {
        if self.document().is_read_only() {
            return false;
        }

        let before = self.managed.document.block_ids.clone();
        self.reconcile_after_text_change(before.clone(), true, known_ids.as_deref());
        let changed = self.managed.document.block_ids != before;
        if changed {
            self.capture_history_snapshot();
        }
        changed
    }

    pub(crate) fn set_block_id_at_line(
        &mut self,
        line: usize,
        id: BlockId,
        is_mirror: bool,
    ) -> bool {
        let clean_text = self.managed.slot.text_string();
        let parsed = markdown_parser().parse(&clean_text);
        let Some(block) = parsed
            .blocks
            .iter()
            .find(|block| line >= block.first_line && line <= block.last_line)
        else {
            return false;
        };

        let mut replaced = false;
        for entry in &mut self.managed.document.block_ids {
            if ranges_overlap(
                entry.first_line..entry.last_line + 1,
                block.first_line..block.last_line + 1,
            ) {
                entry.id = id.clone();
                entry.first_line = block.first_line;
                entry.last_line = block.last_line;
                entry.is_mirror = is_mirror;
                replaced = true;
                break;
            }
        }

        if !replaced {
            self.managed.document.block_ids.push(BlockIdEntry {
                id,
                first_line: block.first_line,
                last_line: block.last_line,
                is_mirror,
            });
        }

        self.managed.document.sort_block_ids();
        self.capture_history_snapshot();
        true
    }

    fn mutable_buffer(&mut self) -> Option<&mut bloom_buffer::Buffer> {
        match &mut self.managed.slot {
            BufferSlot::Mutable(buf) => Some(buf),
            BufferSlot::Frozen(_) => None,
        }
    }

    fn reload_clean_text(&mut self, content: &str) {
        replace_slot_with_text(&mut self.managed.slot, content);
        let mut state = DocumentState::from_clean_text(content);
        let current_node = self.managed.slot.as_buffer().current_undo_node();
        state
            .block_id_history
            .insert(current_node, state.block_ids.clone());
        self.managed.document = state;
    }

    fn reconcile_after_text_change(
        &mut self,
        shifted: Vec<BlockIdEntry>,
        assign_missing: bool,
        known_ids: Option<&HashSet<String>>,
    ) {
        let clean_text = self.managed.slot.text_string();
        let parsed = markdown_parser().parse(&clean_text);
        self.managed.document.parse_tree = ParseTree::build(&clean_text);
        self.managed.document.block_ids =
            place_entries_in_blocks(shifted, &parsed.blocks, assign_missing, known_ids);
        self.managed.document.sort_block_ids();
    }

    fn capture_history_snapshot(&mut self) {
        let node_id = self.managed.slot.as_buffer().current_undo_node();
        self.managed
            .document
            .block_id_history
            .insert(node_id, self.managed.document.block_ids.clone());
    }

    fn restore_history_snapshot_for_current_node(&mut self) {
        let node_id = self.managed.slot.as_buffer().current_undo_node();
        if let Some(entries) = self
            .managed
            .document
            .block_id_history
            .get(&node_id)
            .cloned()
        {
            self.managed.document.block_ids = entries;
            self.managed.document.parse_tree = ParseTree::build(&self.managed.slot.text_string());
        } else {
            let shifted = self.managed.document.block_ids.clone();
            self.reconcile_after_text_change(shifted, false, None);
            self.capture_history_snapshot();
        }
    }
}

pub(crate) fn clean_text_from_canonical_markdown(text: &str) -> String {
    deserialize_canonical_markdown(text).0
}

pub(crate) fn deserialize_canonical_markdown(text: &str) -> (String, Vec<BlockIdEntry>) {
    let parsed = markdown_parser().parse(text);
    let mut entries = Vec::new();
    let mut claimed_blocks = HashSet::new();

    for parsed_id in &parsed.block_ids {
        if let Some((block_idx, block)) = parsed.blocks.iter().enumerate().find(|(_, block)| {
            parsed_id.line >= block.first_line && parsed_id.line <= block.last_line
        }) {
            if claimed_blocks.insert(block_idx) {
                entries.push(BlockIdEntry {
                    id: parsed_id.id.clone(),
                    first_line: block.first_line,
                    last_line: block.last_line,
                    is_mirror: parsed_id.is_mirror,
                });
            }
        }
    }

    let clean = if text.is_empty() {
        String::new()
    } else {
        text.split_inclusive('\n')
            .enumerate()
            .map(|(line_idx, line)| strip_recognized_block_id_suffix(line, line_idx))
            .collect::<Vec<_>>()
            .join("")
    };

    entries.sort_by_key(|entry| (entry.first_line, entry.last_line, entry.id.0.clone()));
    (clean, entries)
}

fn serialize_canonical(buf: &bloom_buffer::Buffer, entries: &[BlockIdEntry]) -> String {
    let mut markers_by_line: HashMap<usize, (String, bool)> = HashMap::new();
    for entry in entries {
        markers_by_line.insert(entry.last_line, (entry.id.0.clone(), entry.is_mirror));
    }

    let mut out = String::new();
    for line_idx in 0..buf.len_lines() {
        let line = buf.line(line_idx).to_string();
        let (body, newline) = split_line_ending(&line);
        let trimmed_body = body.trim_end_matches([' ', '\t', '\r']);
        out.push_str(trimmed_body);
        if let Some((id, is_mirror)) = markers_by_line.get(&line_idx) {
            out.push_str(" ^");
            if *is_mirror {
                out.push('=');
            }
            out.push_str(id);
        }
        out.push_str(newline);
    }
    out
}

fn split_line_ending(line: &str) -> (&str, &str) {
    if let Some(stripped) = line.strip_suffix("\r\n") {
        (stripped, "\r\n")
    } else if let Some(stripped) = line.strip_suffix('\n') {
        (stripped, "\n")
    } else {
        (line, "")
    }
}

fn strip_recognized_block_id_suffix(line: &str, line_number: usize) -> String {
    let (body, newline) = split_line_ending(line);
    let stripped = if let Some(parsed) = parse_block_id(body, line_number) {
        let trailing_marker = if parsed.is_mirror {
            format!(" ^={}", parsed.id.0)
        } else {
            format!(" ^{}", parsed.id.0)
        };
        if let Some(prefix) = body.strip_suffix(&trailing_marker) {
            prefix.to_string()
        } else {
            let standalone_marker = if parsed.is_mirror {
                format!("^={}", parsed.id.0)
            } else {
                format!("^{}", parsed.id.0)
            };
            if body.trim() == standalone_marker {
                body[..body.find('^').unwrap_or(0)].to_string()
            } else {
                body.to_string()
            }
        }
    } else {
        body.to_string()
    };

    format!("{stripped}{newline}")
}

fn transform_entries(
    entries: &[BlockIdEntry],
    buf: &bloom_buffer::Buffer,
    edit_range: Range<usize>,
    replacement: &str,
) -> Vec<BlockIdEntry> {
    let edit_start_line = char_pos_to_line(buf, edit_range.start);
    let edit_end_line = if edit_range.end > edit_range.start && buf.len_chars() > 0 {
        buf.text().char_to_line(
            edit_range
                .end
                .saturating_sub(1)
                .min(buf.len_chars().saturating_sub(1)),
        )
    } else {
        edit_start_line
    };
    let removed_newlines = if edit_range.is_empty() {
        0
    } else {
        buf.text()
            .slice(edit_range.clone())
            .chars()
            .filter(|c| *c == '\n')
            .count()
    };
    let added_newlines = replacement.chars().filter(|c| *c == '\n').count();
    let delta = added_newlines as isize - removed_newlines as isize;
    let replacement_end_line = edit_start_line + added_newlines;

    entries
        .iter()
        .filter_map(|entry| {
            if entry.last_line < edit_start_line {
                return Some(entry.clone());
            }

            if entry.first_line > edit_end_line {
                return Some(BlockIdEntry {
                    first_line: shift_line(entry.first_line, delta),
                    last_line: shift_line(entry.last_line, delta),
                    ..entry.clone()
                });
            }

            if removed_newlines > 0
                && !edit_range.is_empty()
                && entry.first_line >= edit_start_line
                && entry.last_line <= edit_end_line
            {
                return None;
            }

            let new_first = entry.first_line.min(edit_start_line);
            let new_last = if entry.last_line > edit_end_line {
                shift_line(entry.last_line, delta)
            } else {
                replacement_end_line
            };

            Some(BlockIdEntry {
                first_line: new_first,
                last_line: new_last.max(new_first),
                ..entry.clone()
            })
        })
        .collect()
}

fn place_entries_in_blocks(
    shifted: Vec<BlockIdEntry>,
    blocks: &[ParsedBlock],
    assign_missing: bool,
    known_ids: Option<&HashSet<String>>,
) -> Vec<BlockIdEntry> {
    let mut result = Vec::new();
    let mut claimed_blocks = HashSet::new();

    for entry in shifted {
        if let Some((block_idx, block)) = blocks.iter().enumerate().find(|(_, block)| {
            entry.first_line >= block.first_line && entry.first_line <= block.last_line
        }) {
            if claimed_blocks.insert(block_idx) {
                result.push(BlockIdEntry {
                    id: entry.id,
                    first_line: block.first_line,
                    last_line: block.last_line,
                    is_mirror: entry.is_mirror,
                });
            }
        }
    }

    if assign_missing {
        let mut existing_ids: HashSet<String> =
            result.iter().map(|entry| entry.id.0.clone()).collect();
        if let Some(known_ids) = known_ids {
            existing_ids.extend(known_ids.iter().cloned());
        }

        for (block_idx, block) in blocks.iter().enumerate() {
            if claimed_blocks.contains(&block_idx) {
                continue;
            }
            let new_id = bloom_buffer::block_id::next_block_id(&existing_ids);
            existing_ids.insert(new_id.clone());
            result.push(BlockIdEntry {
                id: BlockId(new_id),
                first_line: block.first_line,
                last_line: block.last_line,
                is_mirror: false,
            });
        }
    }

    result.sort_by_key(|entry| (entry.first_line, entry.last_line, entry.id.0.clone()));
    result
}

fn shift_line(line: usize, delta: isize) -> usize {
    (line as isize + delta).max(0) as usize
}

fn ranges_overlap(a: Range<usize>, b: Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
}

fn replace_slot_with_text(slot: &mut BufferSlot, content: &str) {
    match slot {
        BufferSlot::Mutable(buf) => {
            *buf = bloom_buffer::Buffer::from_text(content);
        }
        BufferSlot::Frozen(buf) => {
            *buf = bloom_buffer::Buffer::from_text(content).freeze();
        }
    }
}

fn markdown_parser() -> BloomMarkdownParser {
    BloomMarkdownParser::new()
}

fn char_pos_to_line(buf: &bloom_buffer::Buffer, pos: usize) -> usize {
    if buf.len_chars() == 0 {
        0
    } else {
        buf.text()
            .char_to_line(pos.min(buf.len_chars().saturating_sub(1)))
    }
}
