use crate::index::{Index, UnlinkedMention};
use crate::parser::traits::*;
use crate::types::*;
use std::ops::Range;
use std::path::PathBuf;

pub struct Linker {}

pub enum LinkResolution {
    Resolved {
        page: PageMeta,
        section: Option<Section>,
    },
    Orphaned {
        display_hint: String,
    },
}

pub struct HintUpdate {
    pub file_path: PathBuf,
    pub old_text: String,
    pub new_text: String,
}

pub struct TextEdit {
    pub file_path: PathBuf,
    pub range: Range<usize>,
    pub new_text: String,
}

impl Linker {
    pub fn new() -> Self {
        Linker {}
    }

    /// Resolve a parsed link against the index.
    pub fn resolve(&self, link: &ParsedLink, index: &Index) -> LinkResolution {
        match index.find_page_by_id(&link.target) {
            Some(page) => {
                let section = link.section.as_ref().and_then(|block_id| {
                    // If a section block ID is specified, try to find matching section
                    // For now we return None; full implementation would query the index
                    let _ = block_id;
                    None
                });
                LinkResolution::Resolved { page, section }
            }
            None => LinkResolution::Orphaned {
                display_hint: link.display_hint.clone(),
            },
        }
    }

    /// When a page title changes, update display hints in all pages that link to it.
    pub fn update_display_hints(
        &self,
        old_title: &str,
        new_title: &str,
        page_id: &PageId,
        index: &Index,
    ) -> Vec<HintUpdate> {
        let backlinks = index.backlinks_to(page_id);
        let hex = page_id.to_hex();
        backlinks
            .into_iter()
            .map(|bl| {
                let old_text = format!("[[{}|{}]]", hex, old_title);
                let new_text = format!("[[{}|{}]]", hex, new_title);
                HintUpdate {
                    file_path: bl.source_page.path.clone(),
                    old_text,
                    new_text,
                }
            })
            .collect()
    }

    /// Promote an unlinked mention to a wiki-link.
    pub fn promote_unlinked_mention(
        &self,
        mention: &UnlinkedMention,
        target: &PageId,
    ) -> TextEdit {
        let matched_text = &mention.context[mention.match_range.clone()];
        let new_text = format!("[[{}|{}]]", target.to_hex(), matched_text);
        TextEdit {
            file_path: mention.source_page.path.clone(),
            range: mention.match_range.clone(),
            new_text,
        }
    }

    /// Batch-promote multiple unlinked mentions.
    pub fn batch_promote(
        &self,
        mentions: &[UnlinkedMention],
        target: &PageId,
    ) -> Vec<TextEdit> {
        mentions
            .iter()
            .map(|m| self.promote_unlinked_mention(m, target))
            .collect()
    }
}