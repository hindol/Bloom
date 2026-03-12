use crate::error::BloomError;
use crate::index::Index;
use bloom_md::parser::traits::*;
use crate::types::*;
use std::fs;

use super::{SplitResult, TextEdit};

/// Extract a section from a source page into a new page.
///
/// - Reads the source file from disk.
/// - Builds new page content with frontmatter and the extracted section body.
/// - Replaces the section in the source with a link to the new page.
/// - Updates any backlinks from other pages that pointed into the extracted section.
pub(crate) fn split_page(
    source_page: &PageId,
    section: &Section,
    new_title: &str,
    index: &Index,
    parser: &dyn DocumentParser,
) -> Result<SplitResult, BloomError> {
    let source_meta = index
        .find_page_by_id(source_page)
        .ok_or_else(|| BloomError::PageNotFound(source_page.to_hex()))?;

    let source_content = fs::read_to_string(&source_meta.path)?;
    let lines: Vec<&str> = source_content.lines().collect();

    // Compute byte range of the section in the source content.
    let section_start_line = section.line_range.start;
    let section_end_line = section.line_range.end.min(lines.len());

    let byte_start = lines[..section_start_line]
        .iter()
        .map(|l| l.len() + 1) // +1 for newline
        .sum::<usize>();
    let section_text: String = lines[section_start_line..section_end_line].join("\n");
    let byte_end = byte_start + section_text.len();

    // Build frontmatter for the new page.
    let new_fm = Frontmatter {
        id: None,
        title: Some(new_title.to_string()),
        created: Some(chrono::Local::now().date_naive()),
        tags: Vec::new(),
        extra: std::collections::HashMap::new(),
    };
    let fm_text = parser.serialize_frontmatter(&new_fm);

    // Strip the heading line from the section body for the new page (the title is in frontmatter).
    let body = if section_end_line > section_start_line + 1 {
        lines[section_start_line + 1..section_end_line].join("\n")
    } else {
        String::new()
    };
    let new_page_content = format!("{}\n{}", fm_text, body);

    // Source edit: replace the section with a link to the new page.
    let link_text = format!("[[{}]]", new_title);
    let source_edits = vec![TextEdit {
        file_path: source_meta.path.clone(),
        range: byte_start..byte_end,
        new_text: link_text,
    }];

    // Update backlinks from other pages that pointed into sections of the extracted content.
    let mut link_updates = Vec::new();
    let backlinks = index.backlinks_to(source_page);
    for bl in &backlinks {
        if bl.line >= section_start_line && bl.line < section_end_line {
            // This backlink pointed into the extracted section; redirect it.
            let bl_content = fs::read_to_string(&bl.source_page.path).unwrap_or_default();
            if let Some(pos) = bl_content.find(&format!("[[{}]]", source_meta.title)) {
                let old = format!("[[{}]]", source_meta.title);
                link_updates.push(TextEdit {
                    file_path: bl.source_page.path.clone(),
                    range: pos..pos + old.len(),
                    new_text: format!("[[{}]]", new_title),
                });
            }
        }
    }

    Ok(SplitResult {
        new_page_content,
        source_edits,
        link_updates,
    })
}
