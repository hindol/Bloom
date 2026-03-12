use crate::error::BloomError;
use crate::types::*;
use bloom_md::parser::traits::{DocumentParser, Frontmatter};
use bloom_store::traits::NoteStore;
use std::collections::HashMap;
use std::path::Path;

/// Adopt an unrecognized .md file by adding frontmatter if it's missing.
pub(crate) fn adopt_file(
    _vault_root: &Path,
    path: &Path,
    parser: &dyn DocumentParser,
    store: &dyn NoteStore,
) -> Result<PageMeta, BloomError> {
    let content = store.read(path)?;
    let existing_fm = parser.parse_frontmatter(&content);

    let (meta, new_content) = if let Some(fm) = existing_fm {
        // Frontmatter exists; extract metadata.
        let title = fm.title.clone().unwrap_or_else(|| {
            path.file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        let id = fm.id.clone().unwrap_or_else(|| {
            // Generate a deterministic ID from the file path.
            let hash = simple_hash(&path.to_string_lossy());
            PageId(hash)
        });
        let created = fm
            .created
            .unwrap_or_else(|| chrono::Local::now().date_naive());
        let meta = PageMeta {
            id,
            title,
            created,
            tags: fm.tags.clone(),
            path: path.to_path_buf(),
        };
        (meta, None)
    } else {
        // No frontmatter; add one.
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let hash = simple_hash(&path.to_string_lossy());
        let id = PageId(hash);
        let created = chrono::Local::now().date_naive();

        let fm = Frontmatter {
            id: Some(id.clone()),
            title: Some(title.clone()),
            created: Some(created),
            tags: Vec::new(),
            extra: HashMap::new(),
        };
        let fm_text = parser.serialize_frontmatter(&fm);
        let new_content = format!("{}\n{}", fm_text, content);

        let meta = PageMeta {
            id,
            title,
            created,
            tags: Vec::new(),
            path: path.to_path_buf(),
        };
        (meta, Some(new_content))
    };

    if let Some(nc) = new_content {
        store.write(path, &nc)?;
    }

    Ok(meta)
}

/// Simple 4-byte hash of a string for generating deterministic PageIds.
fn simple_hash(s: &str) -> [u8; 4] {
    let mut h: u32 = 5381;
    for b in s.bytes() {
        h = h.wrapping_mul(33).wrapping_add(b as u32);
    }
    h.to_le_bytes()
}
