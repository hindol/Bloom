use crate::error::BloomError;
use crate::index::Index;
use crate::types::*;
use std::fs;

use super::{MoveResult, TextEdit};

/// Move a block (identified by block ID) from one page to another.
///
/// - Finds the block in the source page by scanning for `^block-id`.
/// - Removes it from the source.
/// - Appends it to the target page.
/// - Updates any links that reference the block.
pub(crate) fn move_block(
    block_id: &BlockId,
    from_page: &PageId,
    to_page: &PageId,
    index: &Index,
) -> Result<MoveResult, BloomError> {
    let from_meta = index
        .find_page_by_id(from_page)
        .ok_or_else(|| BloomError::PageNotFound(from_page.to_hex()))?;
    let to_meta = index
        .find_page_by_id(to_page)
        .ok_or_else(|| BloomError::PageNotFound(to_page.to_hex()))?;

    let from_content = fs::read_to_string(&from_meta.path)?;
    let to_content = fs::read_to_string(&to_meta.path)?;

    // Find the line containing the block ID marker `^block-id`.
    let marker = format!("^{}", block_id.0);
    let mut block_line_idx = None;
    let mut byte_offset = 0usize;
    let mut block_byte_start = 0usize;
    let mut block_byte_end = 0usize;
    let mut block_text = String::new();

    for (i, line) in from_content.lines().enumerate() {
        let line_end = byte_offset + line.len() + 1; // +1 for newline
        if line.contains(&marker) {
            block_line_idx = Some(i);
            block_byte_start = byte_offset;
            block_byte_end = line_end.min(from_content.len());
            block_text = line.to_string();
            break;
        }
        byte_offset = line_end;
    }

    if block_line_idx.is_none() {
        return Err(BloomError::PageNotFound(format!(
            "block ^{} not found in page {}",
            block_id.0,
            from_page.to_hex()
        )));
    }

    // Source edit: remove the block line.
    let source_edits = vec![TextEdit {
        file_path: from_meta.path.clone(),
        range: block_byte_start..block_byte_end,
        new_text: String::new(),
    }];

    // Target edit: append the block at end.
    let target_edits = vec![TextEdit {
        file_path: to_meta.path.clone(),
        range: to_content.len()..to_content.len(),
        new_text: format!("\n{}", block_text),
    }];

    // Update links from other pages that reference this block.
    let mut link_updates = Vec::new();
    let backlinks = index.backlinks_to(from_page);
    for bl in &backlinks {
        let bl_content = fs::read_to_string(&bl.source_page.path).unwrap_or_default();
        // Look for links like [[PageTitle#^block-id]]
        let old_ref = format!("[[{}#^{}]]", from_meta.title, block_id.0);
        let new_ref = format!("[[{}#^{}]]", to_meta.title, block_id.0);
        if let Some(pos) = bl_content.find(&old_ref) {
            link_updates.push(TextEdit {
                file_path: bl.source_page.path.clone(),
                range: pos..pos + old_ref.len(),
                new_text: new_ref,
            });
        }
    }

    Ok(MoveResult {
        source_edits,
        target_edits,
        link_updates,
    })
}
