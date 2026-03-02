use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::index::{Backlink, IndexError, IndexedPage, IndexedPageContent, SqliteIndex};

#[path = "timeline.rs"]
pub mod timeline;

/// Maximum embed expansion depth. Embeds nested beyond this level are rendered
/// as plain links to prevent infinite recursion (e.g., A embeds B which embeds A).
pub const MAX_EMBED_DEPTH: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnlinkedMention {
    pub source_page_id: String,
    pub source_path: PathBuf,
    pub source_title: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTarget {
    pub page_id: String,
    pub sub_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLinkTarget {
    pub page: IndexedPage,
    pub sub_id: Option<String>,
}

pub struct Resolver<'a> {
    index: &'a SqliteIndex,
}

impl<'a> Resolver<'a> {
    pub fn new(index: &'a SqliteIndex) -> Self {
        Self { index }
    }

    pub fn backlinks_for_page_id(&self, page_id: &str) -> Result<Vec<Backlink>, IndexError> {
        self.index.backlinks_for(page_id)
    }

    pub fn unlinked_mentions_for_title(
        &self,
        page_title: &str,
    ) -> Result<Vec<UnlinkedMention>, IndexError> {
        // Titles shorter than 3 chars produce too many false-positive matches.
        if page_title.trim().chars().count() < 3 {
            return Ok(Vec::new());
        }

        let Some(target_page) = self.index.page_for_title(page_title)? else {
            return Ok(Vec::new());
        };

        let linked_source_ids: HashSet<String> = self
            .index
            .backlinks_for(&target_page.page_id)?
            .into_iter()
            .map(|backlink| backlink.source_page_id)
            .collect();

        let query = fts_phrase_query(&target_page.title);
        let mut mentions = Vec::new();
        for hit in self.index.search(&query)? {
            if hit.page_id == target_page.page_id || linked_source_ids.contains(&hit.page_id) {
                continue;
            }

            mentions.push(UnlinkedMention {
                source_page_id: hit.page_id,
                source_path: hit.path,
                source_title: hit.title,
                snippet: hit.snippet,
            });
        }

        Ok(mentions)
    }

    pub fn resolve_page_id(&self, page_id: &str) -> Result<Option<IndexedPage>, IndexError> {
        self.index.page_for_id(page_id)
    }

    pub fn page_content_for_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<IndexedPageContent>, IndexError> {
        self.index.page_content_for_path(path)
    }

    pub fn resolve_link_target(
        &self,
        raw_target: &str,
    ) -> Result<Option<ResolvedLinkTarget>, IndexError> {
        let Some(target) = parse_link_target(raw_target) else {
            return Ok(None);
        };

        let Some(page) = self.index.page_for_id(&target.page_id)? else {
            return Ok(None);
        };

        Ok(Some(ResolvedLinkTarget {
            page,
            sub_id: target.sub_id,
        }))
    }

    /// Resolve an embed target, respecting transclusion depth.
    /// Returns `None` if `current_depth >= MAX_EMBED_DEPTH`, preventing
    /// infinite recursion from circular embeds.
    pub fn resolve_embed(
        &self,
        raw_target: &str,
        current_depth: usize,
    ) -> Result<Option<ResolvedLinkTarget>, IndexError> {
        if current_depth >= MAX_EMBED_DEPTH {
            return Ok(None);
        }
        self.resolve_link_target(raw_target)
    }
}

pub fn parse_link_target(raw_target: &str) -> Option<LinkTarget> {
    let mut target = raw_target.trim();
    if target.is_empty() {
        return None;
    }

    if let Some(stripped) = target.strip_prefix("![[") {
        target = stripped;
    } else if let Some(stripped) = target.strip_prefix("[[") {
        target = stripped;
    }

    if let Some(stripped) = target.strip_suffix("]]") {
        target = stripped;
    }

    let (target, _) = target.split_once('|').unwrap_or((target, ""));
    let (page_id, sub_id) = if let Some((page_id, sub_id)) = target.split_once('#') {
        (page_id.trim(), Some(sub_id.trim()))
    } else {
        (target.trim(), None)
    };

    if page_id.is_empty() {
        return None;
    }

    Some(LinkTarget {
        page_id: page_id.to_string(),
        sub_id: sub_id.and_then(|value| {
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        }),
    })
}

fn fts_phrase_query(term: &str) -> String {
    let escaped = term.trim().replace('\"', "\"\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use tempfile::TempDir;

    use super::*;
    use crate::document::Document;
    use crate::parser::parse;

    fn make_index() -> (TempDir, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index)
    }

    fn make_doc(id: &str, title: &str, body: &str) -> Document {
        let raw = format!("---\nid: {id}\ntitle: \"{title}\"\ntags: []\n---\n\n{body}\n");
        parse(&raw).unwrap()
    }

    #[test]
    fn backlinks_lookup_for_page_id() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("target01", "Target", "base page");
        let source_a = make_doc("srca0001", "Source A", "See [[target01|Target]].");
        let source_b = make_doc("srcb0001", "Source B", "Embed ![[target01|Target]].");

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/source-a.md"), &source_a)
            .unwrap();
        index
            .index_document(Path::new("pages/source-b.md"), &source_b)
            .unwrap();

        let resolver = Resolver::new(&index);
        let backlinks = resolver.backlinks_for_page_id("target01").unwrap();

        assert_eq!(backlinks.len(), 2);
        assert_eq!(backlinks[0].source_page_id, "srca0001");
        assert_eq!(backlinks[0].source_path, Path::new("pages/source-a.md"));
        assert_eq!(backlinks[1].source_page_id, "srcb0001");
        assert_eq!(backlinks[1].source_path, Path::new("pages/source-b.md"));
    }

    #[test]
    fn unlinked_mentions_for_title_excludes_existing_links() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("target01", "Rust Notes", "Target body.");
        let linked = make_doc(
            "linked01",
            "Already Linked",
            "See [[target01|Rust Notes]] for details.",
        );
        let mention_a = make_doc(
            "mention01",
            "Mention One",
            "I reviewed rust notes during planning.",
        );
        let mention_b = make_doc("mention02", "Mention Two", "RUST NOTES are still useful.");

        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/linked.md"), &linked)
            .unwrap();
        index
            .index_document(Path::new("pages/mention-a.md"), &mention_a)
            .unwrap();
        index
            .index_document(Path::new("pages/mention-b.md"), &mention_b)
            .unwrap();

        let resolver = Resolver::new(&index);
        let mentions = resolver.unlinked_mentions_for_title("Rust Notes").unwrap();

        let mut mention_ids: Vec<_> = mentions
            .iter()
            .map(|mention| mention.source_page_id.clone())
            .collect();
        mention_ids.sort();

        assert_eq!(mention_ids, vec!["mention01", "mention02"]);
        assert!(
            !mentions
                .iter()
                .any(|mention| mention.source_page_id == "linked01")
        );
        assert!(
            resolver
                .unlinked_mentions_for_title("Missing Title")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn resolve_link_targets_with_optional_sub_id() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("target01", "Rust Notes", "Target body.");
        index
            .index_document(Path::new("pages/target.md"), &target)
            .unwrap();

        let resolver = Resolver::new(&index);
        let by_page_id = resolver.resolve_page_id("target01").unwrap().unwrap();
        assert_eq!(by_page_id.path, Path::new("pages/target.md"));

        let resolved = resolver
            .resolve_link_target("[[target01#sec-1|Rust Notes]]")
            .unwrap()
            .unwrap();
        assert_eq!(resolved.page.page_id, "target01");
        assert_eq!(resolved.page.title, "Rust Notes");
        assert_eq!(resolved.sub_id.as_deref(), Some("sec-1"));

        assert_eq!(
            parse_link_target("![[target01|Rust Notes]]").unwrap(),
            LinkTarget {
                page_id: "target01".to_string(),
                sub_id: None,
            }
        );
        assert!(parse_link_target("[[|invalid]]").is_none());
        assert!(
            resolver
                .resolve_link_target("[[missing01|Missing]]")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn unlinked_mentions_skips_short_titles() {
        let (_tmp, mut index) = make_index();
        let target = make_doc("short001", "Go", "Go page body.");
        let mention = make_doc("mention01", "Other", "I like Go programming.");

        index
            .index_document(Path::new("pages/go.md"), &target)
            .unwrap();
        index
            .index_document(Path::new("pages/other.md"), &mention)
            .unwrap();

        let resolver = Resolver::new(&index);
        let mentions = resolver.unlinked_mentions_for_title("Go").unwrap();
        assert!(
            mentions.is_empty(),
            "Short titles should not produce unlinked mentions"
        );
    }
}
