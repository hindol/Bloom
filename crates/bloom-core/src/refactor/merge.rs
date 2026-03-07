use crate::error::BloomError;
use crate::index::Index;
use crate::types::*;
use std::fs;

use super::{MergeResult, TextEdit};

/// Merge a source page into a target page.
///
/// - Appends the source page content (minus frontmatter) to the target page.
/// - Redirects all backlinks pointing to the source so they point to the target.
/// - Returns the source file path for deletion by the caller.
pub(crate) fn merge_pages(
    source: &PageId,
    target: &PageId,
    index: &Index,
) -> Result<MergeResult, BloomError> {
    let source_meta = index
        .find_page_by_id(source)
        .ok_or_else(|| BloomError::PageNotFound(source.to_hex()))?;
    let target_meta = index
        .find_page_by_id(target)
        .ok_or_else(|| BloomError::PageNotFound(target.to_hex()))?;

    let source_content = fs::read_to_string(&source_meta.path)?;
    let target_content = fs::read_to_string(&target_meta.path)?;

    // Strip frontmatter from source content.
    let source_body = strip_frontmatter(&source_content);

    // Append source body to end of target.
    let append_text = format!("\n\n## {}\n\n{}", source_meta.title, source_body.trim());
    let target_edits = vec![TextEdit {
        file_path: target_meta.path.clone(),
        range: target_content.len()..target_content.len(),
        new_text: append_text,
    }];

    // Redirect all backlinks pointing to source → target.
    let mut link_redirects = Vec::new();
    let backlinks = index.backlinks_to(source);
    for bl in &backlinks {
        if bl.source_page.id == *target {
            continue; // skip the target page itself
        }
        let bl_content = fs::read_to_string(&bl.source_page.path).unwrap_or_default();
        let old_link = format!("[[{}]]", source_meta.title);
        let new_link = format!("[[{}]]", target_meta.title);
        if let Some(pos) = bl_content.find(&old_link) {
            link_redirects.push(TextEdit {
                file_path: bl.source_page.path.clone(),
                range: pos..pos + old_link.len(),
                new_text: new_link,
            });
        }
    }

    Ok(MergeResult {
        target_edits,
        link_redirects,
        file_to_delete: source_meta.path.clone(),
    })
}

/// Strip YAML frontmatter (delimited by `---`) from content, returning the body.
fn strip_frontmatter(content: &str) -> &str {
    if !content.starts_with("---") {
        return content;
    }
    // Find the closing `---`.
    if let Some(end) = content[3..].find("\n---") {
        let after = end + 3 + 4; // skip past closing ---
        if after < content.len() {
            return &content[after..];
        }
    }
    content
}
