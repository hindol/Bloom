use crate::index::{Index, UnlinkedMention};
use bloom_md::parser::traits::*;
use crate::types::*;
use std::ops::Range;
use std::path::PathBuf;

pub struct Linker {}

pub enum LinkResolution {
    Resolved {
        page: PageMeta,
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

impl Default for Linker {
    fn default() -> Self {
        Self::new()
    }
}

impl Linker {
    pub fn new() -> Self {
        Linker {}
    }

    /// Resolve a parsed link against the index.
    pub fn resolve(&self, link: &ParsedLink, index: &Index) -> LinkResolution {
        match index.find_page_by_id(&link.target) {
            Some(page) => LinkResolution::Resolved { page },
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
    pub fn promote_unlinked_mention(&self, mention: &UnlinkedMention, target: &PageId) -> TextEdit {
        let matched_text = &mention.context[mention.match_range.clone()];
        let new_text = format!("[[{}|{}]]", target.to_hex(), matched_text);
        TextEdit {
            file_path: mention.source_page.path.clone(),
            range: mention.match_range.clone(),
            new_text,
        }
    }

    /// Batch-promote multiple unlinked mentions.
    pub fn batch_promote(&self, mentions: &[UnlinkedMention], target: &PageId) -> Vec<TextEdit> {
        mentions
            .iter()
            .map(|m| self.promote_unlinked_mention(m, target))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_new() {
        let linker = Linker::new();
        // Should create without error
        let _ = linker;
    }

    // UC-28: promote_unlinked_mention
    #[test]
    fn test_promote_creates_link_text() {
        let linker = Linker::new();
        let target = crate::types::PageId::from_hex("aabbccdd").unwrap();
        let mention = crate::index::UnlinkedMention {
            source_page: crate::types::PageMeta {
                id: crate::types::PageId::from_hex("11223344").unwrap(),
                title: "Source".into(),
                created: chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
                tags: vec![],
                path: std::path::PathBuf::from("source.md"),
            },
            context: "Read about Text Editor Theory today".into(),
            line: 5,
            match_range: 11..31,
        };
        let edit = linker.promote_unlinked_mention(&mention, &target);
        assert!(edit.new_text.contains("[["));
        assert!(edit.new_text.contains("aabbccdd"));
    }
}
