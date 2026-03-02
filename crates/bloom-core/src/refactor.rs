//! Refactoring operations for Bloom vaults.

use std::path::{Path, PathBuf};

use crate::document::Frontmatter;
use crate::index::SqliteIndex;
use crate::parser;
use crate::store::{sanitize_filename, LocalFileStore, NoteStore};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum RefactorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Store error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("Index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("Not found: {0}")]
    NotFound(String),
}

// ---------------------------------------------------------------------------
// Report types
// ---------------------------------------------------------------------------

pub struct RenameReport {
    pub files_modified: usize,
    pub occurrences: usize,
}

pub struct DeleteReport {
    pub files_modified: usize,
    pub occurrences: usize,
}

pub enum SplitMode {
    Link,
    Embed,
}

pub struct SplitReport {
    pub new_page_id: String,
    pub new_path: PathBuf,
}

pub struct MergeReport {
    pub lines_moved: usize,
    pub links_rewritten: usize,
}

pub struct MoveReport {
    pub embeds_rewritten: usize,
}

// ---------------------------------------------------------------------------
// 1. Tag rename
// ---------------------------------------------------------------------------

pub fn rename_tag(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    old_tag: &str,
    new_tag: &str,
) -> Result<RenameReport, RefactorError> {
    let paths = index.paths_for_tag(&old_tag.to_lowercase())?;
    let mut files_modified = 0;
    let mut occurrences = 0;

    for path in &paths {
        let content = store.read(path)?;
        let (new_content, count) = replace_tag_in_text(&content, old_tag, Some(new_tag));
        if count > 0 {
            store.write(path, &new_content)?;
            reindex(store, index, path)?;
            files_modified += 1;
            occurrences += count;
        }
    }

    Ok(RenameReport {
        files_modified,
        occurrences,
    })
}

// ---------------------------------------------------------------------------
// 2. Tag delete
// ---------------------------------------------------------------------------

pub fn delete_tag(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    tag: &str,
) -> Result<DeleteReport, RefactorError> {
    let paths = index.paths_for_tag(&tag.to_lowercase())?;
    let mut files_modified = 0;
    let mut occurrences = 0;

    for path in &paths {
        let content = store.read(path)?;
        let (new_content, count) = replace_tag_in_text(&content, tag, None);
        if count > 0 {
            store.write(path, &new_content)?;
            reindex(store, index, path)?;
            files_modified += 1;
            occurrences += count;
        }
    }

    Ok(DeleteReport {
        files_modified,
        occurrences,
    })
}

// ---------------------------------------------------------------------------
// 3. Page split
// ---------------------------------------------------------------------------

pub fn split_page(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    source_path: &Path,
    heading_line: usize,
    mode: SplitMode,
) -> Result<SplitReport, RefactorError> {
    let content = store.read(source_path)?;
    let lines: Vec<&str> = content.lines().collect();

    if heading_line == 0 || heading_line > lines.len() {
        return Err(RefactorError::NotFound(format!(
            "line {} out of range (1..{})",
            heading_line,
            lines.len()
        )));
    }

    let h_idx = heading_line - 1;
    let heading = lines[h_idx];
    let level = heading.chars().take_while(|&c| c == '#').count();
    if level == 0 || !heading[level..].starts_with(' ') {
        return Err(RefactorError::Parse(format!(
            "line {} is not a heading",
            heading_line
        )));
    }
    let heading_text = heading[level..].trim().to_string();

    // Find end of section (next same-or-higher-level heading or EOF).
    let mut end_idx = lines.len();
    for i in (h_idx + 1)..lines.len() {
        let l = lines[i].chars().take_while(|&c| c == '#').count();
        if l > 0 && l <= level && lines[i].len() > l && lines[i].as_bytes()[l] == b' ' {
            end_idx = i;
            break;
        }
    }

    // Section body = lines after the heading, up to end_idx.
    let section_body: String = lines[h_idx + 1..end_idx].join("\n");

    // Create new page.
    let fm = Frontmatter::new(&heading_text);
    let new_page_id = fm.id.clone();
    let filename = sanitize_filename(&heading_text);
    let new_path = store.pages_dir().join(format!("{filename}.md"));

    let trimmed_body = section_body.trim();
    let new_content = if trimmed_body.is_empty() {
        format!(
            "---\nid: {}\ntitle: \"{}\"\ntags: []\n---\n",
            new_page_id, heading_text
        )
    } else {
        format!(
            "---\nid: {}\ntitle: \"{}\"\ntags: []\n---\n\n{}\n",
            new_page_id, heading_text, trimmed_body
        )
    };

    // Build replacement reference.
    let reference = match mode {
        SplitMode::Link => format!("[[{}|{}]]", new_page_id, heading_text),
        SplitMode::Embed => format!("![[{}|{}]]", new_page_id, heading_text),
    };

    // Reconstruct source: replace heading line, drop extracted body.
    let mut new_source_lines: Vec<String> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i == h_idx {
            new_source_lines.push(reference.clone());
        } else if i > h_idx && i < end_idx {
            continue;
        } else {
            new_source_lines.push(line.to_string());
        }
    }
    let mut new_source = new_source_lines.join("\n");
    if content.ends_with('\n') {
        new_source.push('\n');
    }

    store.write(source_path, &new_source)?;
    store.write(&new_path, &new_content)?;

    reindex(store, index, source_path)?;
    reindex(store, index, &new_path)?;

    Ok(SplitReport {
        new_page_id,
        new_path,
    })
}

// ---------------------------------------------------------------------------
// 4. Page merge
// ---------------------------------------------------------------------------

pub fn merge_pages(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    source_path: &Path,
    target_path: &Path,
) -> Result<MergeReport, RefactorError> {
    let source_content = store.read(source_path)?;
    let target_content = store.read(target_path)?;

    let source_doc =
        parser::parse(&source_content).map_err(|e| RefactorError::Parse(e.to_string()))?;
    let target_doc =
        parser::parse(&target_content).map_err(|e| RefactorError::Parse(e.to_string()))?;

    let source_id = source_doc.frontmatter.id.clone();
    let target_id = target_doc.frontmatter.id.clone();

    // Strip frontmatter from source.
    let (_, body) = split_fm_body(&source_content);
    let body_trimmed = body.trim();
    let lines_moved = body_trimmed.lines().count();

    // Append source body to target.
    let mut merged = target_content.clone();
    if !merged.ends_with('\n') {
        merged.push('\n');
    }
    if !body_trimmed.is_empty() {
        merged.push('\n');
        merged.push_str(body_trimmed);
        merged.push('\n');
    }
    store.write(target_path, &merged)?;

    // Rewrite links from source_id → target_id in all linking pages.
    let backlinks = index.backlinks_for(&source_id)?;
    let mut links_rewritten = 0;

    for bl in &backlinks {
        if bl.source_path == source_path || bl.source_path == target_path {
            continue;
        }
        let bl_content = store.read(&bl.source_path)?;
        let new_bl = rewrite_page_id_refs(&bl_content, &source_id, &target_id);
        if new_bl != bl_content {
            store.write(&bl.source_path, &new_bl)?;
            reindex(store, index, &bl.source_path)?;
            links_rewritten += 1;
        }
    }

    // Delete source and re-index target.
    store.delete(source_path)?;
    index.remove_document(source_path)?;
    reindex(store, index, target_path)?;

    Ok(MergeReport {
        lines_moved,
        links_rewritten,
    })
}

// ---------------------------------------------------------------------------
// 5. Block move
// ---------------------------------------------------------------------------

pub fn move_block(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    source_path: &Path,
    block_id: &str,
    target_path: &Path,
) -> Result<MoveReport, RefactorError> {
    let source_content = store.read(source_path)?;
    let target_content = store.read(target_path)?;

    let source_doc =
        parser::parse(&source_content).map_err(|e| RefactorError::Parse(e.to_string()))?;
    let target_doc =
        parser::parse(&target_content).map_err(|e| RefactorError::Parse(e.to_string()))?;

    let source_id = source_doc.frontmatter.id.clone();
    let target_id = target_doc.frontmatter.id.clone();

    // Find paragraph containing ^block_id.
    let marker = format!("^{}", block_id);
    let lines: Vec<&str> = source_content.lines().collect();
    let marker_idx = lines
        .iter()
        .position(|l| l.contains(&marker))
        .ok_or_else(|| RefactorError::NotFound(format!("block ^{} not found", block_id)))?;

    // Paragraph boundaries (delimited by blank lines).
    let mut para_start = marker_idx;
    while para_start > 0 && !lines[para_start - 1].trim().is_empty() {
        para_start -= 1;
    }
    let mut para_end = marker_idx + 1;
    while para_end < lines.len() && !lines[para_end].trim().is_empty() {
        para_end += 1;
    }

    let block_text: String = lines[para_start..para_end].join("\n");

    // Remove block from source.
    let mut src_lines: Vec<&str> = Vec::new();
    src_lines.extend_from_slice(&lines[..para_start]);
    src_lines.extend_from_slice(&lines[para_end..]);
    let mut new_source = src_lines.join("\n");
    if source_content.ends_with('\n') && !new_source.ends_with('\n') {
        new_source.push('\n');
    }

    // Append block to target.
    let mut new_target = target_content.clone();
    if !new_target.ends_with('\n') {
        new_target.push('\n');
    }
    new_target.push('\n');
    new_target.push_str(&block_text);
    new_target.push('\n');

    store.write(source_path, &new_source)?;
    store.write(target_path, &new_target)?;

    // Rewrite embeds ![[source_id#block_id → ![[target_id#block_id.
    let old_pattern = format!("![[{}#{}", source_id, block_id);
    let new_pattern = format!("![[{}#{}", target_id, block_id);

    let backlinks = index.backlinks_for(&source_id)?;
    let mut embeds_rewritten = 0;

    for bl in &backlinks {
        if bl.source_path == source_path || bl.source_path == target_path {
            continue;
        }
        let bl_content = store.read(&bl.source_path)?;
        if bl_content.contains(&old_pattern) {
            let new_bl = bl_content.replace(&old_pattern, &new_pattern);
            store.write(&bl.source_path, &new_bl)?;
            reindex(store, index, &bl.source_path)?;
            embeds_rewritten += 1;
        }
    }

    reindex(store, index, source_path)?;
    reindex(store, index, target_path)?;

    Ok(MoveReport { embeds_rewritten })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Re-read a file from disk and re-index it.
fn reindex(
    store: &LocalFileStore,
    index: &mut SqliteIndex,
    path: &Path,
) -> Result<(), RefactorError> {
    let content = store.read(path)?;
    let doc = parser::parse(&content).map_err(|e| RefactorError::Parse(e.to_string()))?;
    index.index_document(path, &doc)?;
    Ok(())
}

/// Replace (rename) or delete a tag in raw text, handling both frontmatter
/// and inline occurrences. Returns `(new_content, replacement_count)`.
fn replace_tag_in_text(content: &str, old_tag: &str, new_tag: Option<&str>) -> (String, usize) {
    let (fm, body) = split_fm_body(content);
    let mut count = 0;

    let new_fm = if let Some(fm_str) = fm {
        let (replaced, c) = replace_tag_in_frontmatter(fm_str, old_tag, new_tag);
        count += c;
        replaced
    } else {
        String::new()
    };

    let (new_body, c) = replace_inline_tags(body, old_tag, new_tag);
    count += c;

    let mut result = new_fm;
    result.push_str(&new_body);
    (result, count)
}

/// Split content into `(frontmatter_including_delimiters, body)`.
fn split_fm_body(content: &str) -> (Option<&str>, &str) {
    if !content.starts_with("---") {
        return (None, content);
    }
    let after_open = &content[3..];
    if let Some(pos) = after_open.find("\n---") {
        let close_end = 3 + pos + 4; // past "\n---"
        let fm_end = if close_end < content.len() && content.as_bytes()[close_end] == b'\n' {
            close_end + 1
        } else {
            close_end
        };
        (Some(&content[..fm_end]), &content[fm_end..])
    } else {
        (None, content)
    }
}

/// Replace or remove a tag inside the raw frontmatter section.
fn replace_tag_in_frontmatter(
    fm: &str,
    old_tag: &str,
    new_tag: Option<&str>,
) -> (String, usize) {
    let mut count = 0;
    let mut out_lines: Vec<String> = Vec::new();
    let mut in_tags_block = false;

    for line in fm.lines() {
        let trimmed = line.trim();

        // Flow-style: tags: [tag1, tag2]
        if trimmed.starts_with("tags:") && trimmed.contains('[') {
            let (new_line, c) = replace_tag_in_flow(line, old_tag, new_tag);
            out_lines.push(new_line);
            count += c;
            in_tags_block = false;
            continue;
        }

        // Block-style start
        if trimmed.starts_with("tags:") {
            in_tags_block = true;
            out_lines.push(line.to_string());
            continue;
        }

        // Block-style items
        if in_tags_block && trimmed.starts_with("- ") {
            let val = trimmed[2..].trim().trim_matches('"').trim_matches('\'');
            if val.eq_ignore_ascii_case(old_tag) {
                count += 1;
                if let Some(new) = new_tag {
                    out_lines.push(line.replacen(val, new, 1));
                }
                // else: delete line
                continue;
            }
        } else if in_tags_block && !trimmed.is_empty() && !trimmed.starts_with("- ") {
            in_tags_block = false;
        }

        out_lines.push(line.to_string());
    }

    let mut result = out_lines.join("\n");
    if fm.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    (result, count)
}

/// Replace or remove a tag inside a YAML flow-style array (`tags: [a, b]`).
fn replace_tag_in_flow(line: &str, old_tag: &str, new_tag: Option<&str>) -> (String, usize) {
    let open = match line.find('[') {
        Some(p) => p,
        None => return (line.to_string(), 0),
    };
    let close = match line.find(']') {
        Some(p) => p,
        None => return (line.to_string(), 0),
    };

    let inner = &line[open + 1..close];
    let items: Vec<&str> = inner.split(',').collect();
    let mut count = 0;
    let mut new_items: Vec<String> = Vec::new();

    for item in &items {
        let trimmed = item.trim();
        let bare = trimmed.trim_matches('"').trim_matches('\'').trim();
        if bare.eq_ignore_ascii_case(old_tag) {
            count += 1;
            if let Some(new) = new_tag {
                if trimmed.starts_with('"') {
                    new_items.push(format!("\"{}\"", new));
                } else {
                    new_items.push(new.to_string());
                }
            }
        } else if !bare.is_empty() {
            new_items.push(trimmed.to_string());
        }
    }

    let new_line = format!(
        "{}[{}]{}",
        &line[..open],
        new_items.join(", "),
        &line[close + 1..]
    );
    (new_line, count)
}

/// Replace or remove inline `#tag` occurrences in body text.
fn replace_inline_tags(body: &str, old_tag: &str, new_tag: Option<&str>) -> (String, usize) {
    let needle = format!("#{}", old_tag);
    let mut result = String::new();
    let mut count = 0;
    let mut i = 0;

    while i < body.len() {
        if let Some(candidate) = body.get(i..i + needle.len()) {
            if candidate.eq_ignore_ascii_case(&needle) {
                let end = i + needle.len();
                let boundary = if end >= body.len() {
                    true
                } else {
                    let b = body.as_bytes()[end];
                    !b.is_ascii_alphanumeric() && b != b'_' && b != b'-' && b != b'/'
                };
                if boundary {
                    count += 1;
                    match new_tag {
                        Some(new) => {
                            result.push('#');
                            result.push_str(new);
                        }
                        None => {
                            // Consume one trailing space to avoid double spaces.
                            if end < body.len() && body.as_bytes()[end] == b' ' {
                                i = end + 1;
                                continue;
                            }
                        }
                    }
                    i = end;
                    continue;
                }
            }
        }
        let ch = body[i..].chars().next().unwrap();
        result.push(ch);
        i += ch.len_utf8();
    }

    (result, count)
}

/// Rewrite all `[[old_id` references (links and embeds) to `[[new_id`.
fn rewrite_page_id_refs(content: &str, old_id: &str, new_id: &str) -> String {
    content.replace(
        &format!("[[{}", old_id),
        &format!("[[{}", new_id),
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SqliteIndex;
    use crate::parser;
    use crate::store::{LocalFileStore, NoteStore};
    use tempfile::TempDir;

    fn setup() -> (TempDir, LocalFileStore, SqliteIndex) {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, store, index)
    }

    fn write_page(
        store: &LocalFileStore,
        index: &mut SqliteIndex,
        name: &str,
        content: &str,
    ) -> PathBuf {
        let path = store.pages_dir().join(name);
        store.write(&path, content).unwrap();
        let doc = parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();
        path
    }

    // -- Tag rename --

    #[test]
    fn rename_tag_updates_frontmatter_and_inline() {
        let (_tmp, store, mut index) = setup();
        let content =
            "---\nid: aaa11111\ntitle: \"Test\"\ntags: [rust]\n---\n\nHello #rust world.\n";
        let path = write_page(&store, &mut index, "test.md", content);

        let report = rename_tag(&store, &mut index, "rust", "rs").unwrap();
        assert_eq!(report.files_modified, 1);
        assert_eq!(report.occurrences, 2);

        let updated = store.read(&path).unwrap();
        assert!(updated.contains("tags: [rs]"), "frontmatter: {}", updated);
        assert!(updated.contains("#rs"), "inline: {}", updated);
        assert!(!updated.contains("rust"), "old tag present: {}", updated);
    }

    // -- Tag delete --

    #[test]
    fn delete_tag_removes_from_all_locations() {
        let (_tmp, store, mut index) = setup();
        let content =
            "---\nid: bbb22222\ntitle: \"Del\"\ntags: [rust]\n---\n\nHello #rust world.\n";
        let path = write_page(&store, &mut index, "del.md", content);

        let report = delete_tag(&store, &mut index, "rust").unwrap();
        assert_eq!(report.files_modified, 1);
        assert_eq!(report.occurrences, 2);

        let updated = store.read(&path).unwrap();
        assert!(updated.contains("tags: []"), "frontmatter: {}", updated);
        assert!(!updated.contains("#rust"), "inline: {}", updated);
    }

    // -- Page split --

    #[test]
    fn split_page_creates_new_page_with_section() {
        let (_tmp, store, mut index) = setup();
        let content = "\
---
id: ccc33333
title: \"Main\"
tags: []
---

## Section One

Content of section one.

## Section Two

Content of section two.
";
        let path = write_page(&store, &mut index, "main.md", content);

        let report = split_page(&store, &mut index, &path, 7, SplitMode::Link).unwrap();
        assert!(!report.new_page_id.is_empty());
        assert!(store.exists(&report.new_path));

        let new_content = store.read(&report.new_path).unwrap();
        assert!(
            new_content.contains("Content of section one"),
            "new page body: {}",
            new_content
        );
        assert!(
            new_content.contains("title: \"Section One\""),
            "new page title: {}",
            new_content
        );
    }

    #[test]
    fn split_page_replaces_section_with_link() {
        let (_tmp, store, mut index) = setup();
        let content = "\
---
id: ddd44444
title: \"Main\"
tags: []
---

## Section One

Content of section one.

## Section Two

Content of section two.
";
        let path = write_page(&store, &mut index, "main2.md", content);

        let report = split_page(&store, &mut index, &path, 7, SplitMode::Link).unwrap();
        let updated = store.read(&path).unwrap();
        let expected_link = format!("[[{}|Section One]]", report.new_page_id);
        assert!(
            updated.contains(&expected_link),
            "source missing link: {}",
            updated
        );
        assert!(
            !updated.contains("Content of section one"),
            "extracted content still in source"
        );
        assert!(
            updated.contains("Content of section two"),
            "other section should remain"
        );
    }

    // -- Page merge --

    #[test]
    fn merge_pages_combines_content_and_redirects() {
        let (_tmp, store, mut index) = setup();
        let source =
            "---\nid: eee55555\ntitle: \"Source\"\ntags: []\n---\n\nSource body line.\n";
        let target =
            "---\nid: fff66666\ntitle: \"Target\"\ntags: []\n---\n\nTarget body.\n";
        let linker =
            "---\nid: ggg77777\ntitle: \"Linker\"\ntags: []\n---\n\nSee [[eee55555|Source]].\n";

        let source_path = write_page(&store, &mut index, "source.md", source);
        let target_path = write_page(&store, &mut index, "target.md", target);
        let _linker_path = write_page(&store, &mut index, "linker.md", linker);

        let report = merge_pages(&store, &mut index, &source_path, &target_path).unwrap();

        assert!(report.lines_moved > 0);
        assert_eq!(report.links_rewritten, 1);

        // Source deleted.
        assert!(!store.exists(&source_path));

        // Target has merged content.
        let merged = store.read(&target_path).unwrap();
        assert!(merged.contains("Source body line"), "merged: {}", merged);
        assert!(merged.contains("Target body"), "merged: {}", merged);

        // Linker now points to target.
        let linker_content = store.read(&_linker_path).unwrap();
        assert!(
            linker_content.contains("[[fff66666|Source]]"),
            "link not rewritten: {}",
            linker_content
        );
    }

    // -- Block move --

    #[test]
    fn move_block_transfers_and_updates_embeds() {
        let (_tmp, store, mut index) = setup();

        let source = "\
---
id: hhh88888
title: \"Src\"
tags: []
---

Keep this.

Move this paragraph. ^blk1

Also keep.
";
        let target = "\
---
id: iii99999
title: \"Tgt\"
tags: []
---

Existing target.
";
        let embedder = "\
---
id: jjj00000
title: \"Emb\"
tags: []
---

Embed: ![[hhh88888#blk1]]
";

        let src_path = write_page(&store, &mut index, "src.md", source);
        let tgt_path = write_page(&store, &mut index, "tgt.md", target);
        let emb_path = write_page(&store, &mut index, "emb.md", embedder);

        let report = move_block(&store, &mut index, &src_path, "blk1", &tgt_path).unwrap();
        assert_eq!(report.embeds_rewritten, 1);

        // Block removed from source.
        let src_content = store.read(&src_path).unwrap();
        assert!(!src_content.contains("^blk1"), "block still in source");
        assert!(src_content.contains("Keep this"), "other content lost");

        // Block present in target.
        let tgt_content = store.read(&tgt_path).unwrap();
        assert!(tgt_content.contains("^blk1"), "block not in target");

        // Embed rewritten.
        let emb_content = store.read(&emb_path).unwrap();
        assert!(
            emb_content.contains("![[iii99999#blk1]]"),
            "embed not rewritten: {}",
            emb_content
        );
    }
}
