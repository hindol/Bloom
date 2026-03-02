/// Updates `[[page_id|display]]` hints across all vault pages when a page is renamed.

use crate::parser;
use crate::store::{NoteStore, StoreError};

/// Scan every `.md` page in the store and update any link whose target matches
/// `page_id` so that its display text becomes `new_title`.
///
/// Returns the number of files that were modified.
pub fn update_display_hints(
    store: &dyn NoteStore,
    page_id: &str,
    new_title: &str,
) -> Result<usize, StoreError> {
    let pages = store.list_pages()?;
    let mut updated_count = 0;

    for path in &pages {
        let content = store.read(path)?;
        let doc = match parser::parse(&content) {
            Ok(d) => d,
            Err(_) => continue,
        };

        // Collect links in this document that target `page_id` and need updating.
        let needs_update = doc.blocks.iter().any(|block| {
            block.links.iter().any(|link| {
                link.page_id == page_id && link.display.as_deref() != Some(new_title)
            })
        });

        if !needs_update {
            continue;
        }

        // Perform textual replacements on the raw content.
        let new_content = rewrite_links(&content, page_id, new_title);
        if new_content != content {
            store.write(path, &new_content)?;
            updated_count += 1;
        }
    }

    Ok(updated_count)
}

/// Replace all occurrences of `[[page_id]]` or `[[page_id|old_display]]` with
/// `[[page_id|new_title]]` in the raw markdown text.
fn rewrite_links(content: &str, page_id: &str, new_title: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut i = 0;
    let bytes = content.as_bytes();

    while i < bytes.len() {
        // Skip embed links (`![[`).
        if i + 3 <= bytes.len() && &bytes[i..i + 3] == b"![[" {
            result.push('!');
            i += 1;
            // fall through to the `[[` check below
        }

        if i + 2 <= bytes.len() && &bytes[i..i + 2] == b"[[" {
            let inner_start = i + 2;
            if let Some(close_rel) = content[inner_start..].find("]]") {
                let close = inner_start + close_rel;
                let inner = &content[inner_start..close];

                // Extract the target (before `|`) and check if it matches.
                let target = inner.split('|').next().unwrap_or("").trim();
                if target == page_id {
                    result.push_str(&format!("[[{page_id}|{new_title}]]"));
                    i = close + 2;
                    continue;
                }
            }
        }

        // Advance one character (handle UTF-8 properly).
        if content.is_char_boundary(i) {
            let ch = content[i..].chars().next().unwrap();
            result.push(ch);
            i += ch.len_utf8();
        } else {
            i += 1;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{LocalFileStore, NoteStore};
    use tempfile::TempDir;

    fn make_store() -> (TempDir, LocalFileStore) {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        (tmp, store)
    }

    fn page_content(title: &str, body: &str) -> String {
        format!("---\nid: {id}\ntitle: {title}\n---\n\n{body}", id = "abcd1234", title = title)
    }

    #[test]
    fn test_update_display_hints_basic() {
        let (_tmp, store) = make_store();

        // File 1: references target with old display text
        let f1 = store.pages_dir().join("file1.md");
        store
            .write(&f1, &page_content("File 1", "See [[target-page|Old Title]] for details."))
            .unwrap();

        // File 2: references target without display text
        let f2 = store.pages_dir().join("file2.md");
        store
            .write(&f2, &page_content("File 2", "Also [[target-page]] is great."))
            .unwrap();

        // File 3: does NOT reference the target
        let f3 = store.pages_dir().join("file3.md");
        store
            .write(&f3, &page_content("File 3", "No links here, just text."))
            .unwrap();

        let count = update_display_hints(&store, "target-page", "New Title").unwrap();
        assert_eq!(count, 2, "two files should have been updated");

        // Verify file1 updated
        let c1 = store.read(&f1).unwrap();
        assert!(
            c1.contains("[[target-page|New Title]]"),
            "file1 should have new display: {c1}"
        );
        assert!(
            !c1.contains("Old Title]]"),
            "file1 should not have old display"
        );

        // Verify file2 updated
        let c2 = store.read(&f2).unwrap();
        assert!(
            c2.contains("[[target-page|New Title]]"),
            "file2 should have new display: {c2}"
        );

        // Verify file3 untouched
        let c3 = store.read(&f3).unwrap();
        assert_eq!(
            c3,
            page_content("File 3", "No links here, just text."),
            "file3 should be untouched"
        );
    }

    #[test]
    fn test_already_correct_display_not_rewritten() {
        let (_tmp, store) = make_store();

        let f = store.pages_dir().join("up_to_date.md");
        let body = "Link [[target|Correct]] is fine.";
        store.write(&f, &page_content("Up to date", body)).unwrap();

        let count = update_display_hints(&store, "target", "Correct").unwrap();
        assert_eq!(count, 0, "no files should be updated when display is already correct");
    }

    #[test]
    fn test_multiple_links_in_one_file() {
        let (_tmp, store) = make_store();

        let f = store.pages_dir().join("multi.md");
        store
            .write(
                &f,
                &page_content("Multi", "A [[tp]] and B [[tp|old]] plus C [[other|x]]."),
            )
            .unwrap();

        let count = update_display_hints(&store, "tp", "Fresh").unwrap();
        assert_eq!(count, 1);

        let c = store.read(&f).unwrap();
        // Both tp links should be updated; the [[other|x]] link should be untouched.
        assert_eq!(
            c.contains("[[tp|Fresh]]"),
            true,
            "tp links should be updated: {c}"
        );
        assert!(c.contains("[[other|x]]"), "other link untouched");
        // Count occurrences of [[tp|Fresh]]
        assert_eq!(c.matches("[[tp|Fresh]]").count(), 2, "both tp links updated");
    }
}
