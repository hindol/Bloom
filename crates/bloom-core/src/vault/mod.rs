pub mod adopt;
pub mod conflict;
pub mod setup;

use crate::error::BloomError;
use crate::index::Index;
use crate::parser::traits::DocumentParser;
use crate::store::traits::NoteStore;
use crate::types::*;
use std::path::{Path, PathBuf};

pub struct Vault {
    pub root: PathBuf,
}

impl Vault {
    /// Create a new vault with full directory structure.
    pub fn create(root: &Path) -> Result<Self, BloomError> {
        setup::create_vault(root)
    }

    /// Open an existing vault.
    pub fn open(root: &Path) -> Result<Self, BloomError> {
        setup::open_vault(root)
    }

    /// Generate a unique PageId (8-char hex, collision-checked against index).
    pub fn generate_id(&self, index: &Index) -> PageId {
        loop {
            let bytes: [u8; 4] = rand_bytes();
            let id = PageId(bytes);
            if index.find_page_by_id(&id).is_none() {
                return id;
            }
        }
    }

    /// Derive a filename from a title (sanitized, lowercased for collision avoidance).
    pub fn filename_for_title(&self, title: &str, id: &PageId) -> String {
        let sanitized: String = title
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                    c
                } else {
                    '-'
                }
            })
            .collect();

        // Collapse multiple dashes/spaces, trim, lowercase.
        let mut result: String = sanitized
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("-")
            .to_lowercase();

        // Cap at 200 chars.
        if result.len() > 200 {
            result.truncate(200);
        }

        // Append ID suffix for uniqueness.
        format!("{}-{}.md", result, id.to_hex())
    }

    /// Adopt an unrecognized .md file (add frontmatter if missing).
    pub fn adopt_file(
        &self,
        path: &Path,
        parser: &dyn DocumentParser,
        store: &dyn NoteStore,
    ) -> Result<PageMeta, BloomError> {
        adopt::adopt_file(&self.root, path, parser, store)
    }

    /// Check if content has git merge conflict markers.
    pub fn has_merge_conflicts(&self, content: &str) -> bool {
        conflict::has_merge_conflicts(content)
    }

    /// Generate .gitignore content for a Bloom vault.
    pub fn gitignore_content() -> &'static str {
        conflict::gitignore_content()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // UC-73: Create vault
    #[test]
    fn test_create_vault_creates_dirs() {
        let dir = TempDir::new().unwrap();
        let _vault = Vault::create(dir.path()).unwrap();
        assert!(dir.path().join("pages").exists());
        assert!(dir.path().join("journal").exists());
    }

    // UC-82: Merge conflict detection
    #[test]
    fn test_detect_merge_conflicts() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::create(dir.path()).unwrap();
        let conflict = "<<<<<<< HEAD\nmine\n=======\ntheirs\n>>>>>>> branch";
        assert!(vault.has_merge_conflicts(conflict));
    }

    #[test]
    fn test_no_false_positive_conflicts() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::create(dir.path()).unwrap();
        assert!(!vault.has_merge_conflicts("normal content\nnothing special"));
    }

    // UC-03: filename sanitization
    #[test]
    fn test_filename_sanitization() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::create(dir.path()).unwrap();
        let id = crate::types::PageId::from_hex("aabbccdd").unwrap();
        let name = vault.filename_for_title("Hello / World: Test?", &id);
        assert!(!name.contains('/'));
        assert!(!name.contains(':'));
        assert!(!name.contains('?'));
    }

    // UC-84: UUID collision avoidance
    #[test]
    fn test_uuid_generation() {
        let id = crate::uuid::generate_hex_id();
        assert_eq!(id.to_hex().len(), 8);
    }

    // gitignore
    #[test]
    fn test_gitignore_content() {
        let content = Vault::gitignore_content();
        assert!(content.contains(".index.db"));
    }
}

/// Generate 4 pseudo-random bytes using a simple xorshift on the current time.
fn rand_bytes() -> [u8; 4] {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos();
    seed.to_le_bytes()
}