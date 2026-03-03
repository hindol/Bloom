use crate::error::BloomError;
use std::fs;
use std::path::Path;

use super::Vault;

/// Create a new vault with the standard directory structure.
pub(crate) fn create_vault(root: &Path) -> Result<Vault, BloomError> {
    fs::create_dir_all(root.join("pages"))?;
    fs::create_dir_all(root.join("journal"))?;
    fs::create_dir_all(root.join(".bloom"))?;
    fs::create_dir_all(root.join("images"))?;

    // Write default .gitignore.
    let gitignore_path = root.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(&gitignore_path, super::Vault::gitignore_content())?;
    }

    Ok(Vault {
        root: root.to_path_buf(),
    })
}

/// Open an existing vault, verifying the root directory exists.
pub(crate) fn open_vault(root: &Path) -> Result<Vault, BloomError> {
    if !root.exists() {
        return Err(BloomError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("vault root not found: {}", root.display()),
        )));
    }
    Ok(Vault {
        root: root.to_path_buf(),
    })
}