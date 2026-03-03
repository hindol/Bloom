pub mod merge;
pub mod move_block;
pub mod split;

use crate::error::BloomError;
use crate::index::Index;
use crate::parser::traits::*;
use crate::types::*;
use std::ops::Range;
use std::path::PathBuf;

pub struct Refactor {}

/// A text edit to apply to a file.
pub struct TextEdit {
    pub file_path: PathBuf,
    pub range: Range<usize>,
    pub new_text: String,
}

pub struct SplitResult {
    pub new_page_content: String,
    pub source_edits: Vec<TextEdit>,
    pub link_updates: Vec<TextEdit>,
}

pub struct MergeResult {
    pub target_edits: Vec<TextEdit>,
    pub link_redirects: Vec<TextEdit>,
    pub file_to_delete: PathBuf,
}

pub struct MoveResult {
    pub source_edits: Vec<TextEdit>,
    pub target_edits: Vec<TextEdit>,
    pub link_updates: Vec<TextEdit>,
}

impl Refactor {
    pub fn new() -> Self {
        Refactor {}
    }

    /// Extract a section into a new page. Returns edits to apply.
    pub fn split_page(
        &self,
        source_page: &PageId,
        section: &Section,
        new_title: &str,
        index: &Index,
        parser: &dyn DocumentParser,
    ) -> Result<SplitResult, BloomError> {
        split::split_page(source_page, section, new_title, index, parser)
    }

    /// Merge a source page into a target page.
    pub fn merge_pages(
        &self,
        source: &PageId,
        target: &PageId,
        index: &Index,
    ) -> Result<MergeResult, BloomError> {
        merge::merge_pages(source, target, index)
    }

    /// Move a block from one page to another.
    pub fn move_block(
        &self,
        block_id: &BlockId,
        from_page: &PageId,
        to_page: &PageId,
        index: &Index,
    ) -> Result<MoveResult, BloomError> {
        move_block::move_block(block_id, from_page, to_page, index)
    }
}

impl Default for Refactor {
    fn default() -> Self {
        Self::new()
    }
}