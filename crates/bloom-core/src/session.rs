// Session persistence — save and restore editor state across quit/relaunch.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionData {
    /// Open buffer file paths (ordered by most recently focused).
    pub buffers: Vec<String>,
    /// Currently focused buffer path.
    pub active_buffer: Option<String>,
    /// Cursor byte offset per buffer path.
    pub cursors: HashMap<String, usize>,
    /// Scroll offset per buffer path.
    pub scroll_offsets: HashMap<String, usize>,
    /// Window layout serialization.
    pub layout: SessionLayout,
    /// Last active theme name.
    pub theme: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SessionLayout {
    Single,
    VSplit {
        left: Box<SessionLayout>,
        right: Box<SessionLayout>,
    },
    HSplit {
        top: Box<SessionLayout>,
        bottom: Box<SessionLayout>,
    },
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn session_path(vault_root: &Path) -> PathBuf {
    vault_root.join(".bloom").join("session.json")
}

/// Save session to `<vault>/.bloom/session.json`.
pub fn save_session(vault_root: &Path, data: &SessionData) -> Result<(), SessionError> {
    let path = session_path(vault_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(data)?;
    std::fs::write(&path, json)?;
    Ok(())
}

/// Load session from `<vault>/.bloom/session.json`.
pub fn load_session(vault_root: &Path) -> Result<Option<SessionData>, SessionError> {
    let path = session_path(vault_root);
    if !path.exists() {
        return Ok(None);
    }
    let json = std::fs::read_to_string(&path)?;
    let data: SessionData = serde_json::from_str(&json)?;
    Ok(Some(data))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_session() -> SessionData {
        let mut cursors = HashMap::new();
        cursors.insert("pages/foo.md".to_string(), 42);
        let mut scroll_offsets = HashMap::new();
        scroll_offsets.insert("pages/foo.md".to_string(), 10);

        SessionData {
            buffers: vec!["pages/foo.md".to_string(), "journal/2025-01-01.md".to_string()],
            active_buffer: Some("pages/foo.md".to_string()),
            cursors,
            scroll_offsets,
            layout: SessionLayout::Single,
            theme: "bloom-dark".to_string(),
        }
    }

    #[test]
    fn save_and_load_session_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let data = sample_session();

        save_session(tmp.path(), &data).unwrap();
        let loaded = load_session(tmp.path()).unwrap();
        assert_eq!(loaded, Some(data));
    }

    #[test]
    fn session_file_missing_returns_none() {
        let tmp = TempDir::new().unwrap();
        let loaded = load_session(tmp.path()).unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn restore_session_opens_buffers() {
        use crate::editor::EditorState;
        let tmp = TempDir::new().unwrap();
        let vault = tmp.path();

        // Create files on disk.
        let pages = vault.join("pages");
        std::fs::create_dir_all(&pages).unwrap();
        std::fs::write(pages.join("foo.md"), "# Foo\nHello").unwrap();
        std::fs::write(pages.join("bar.md"), "# Bar\nWorld").unwrap();

        let mut state = EditorState::new("");
        state.vault_root = Some(vault.to_path_buf());

        let data = SessionData {
            buffers: vec![
                pages.join("foo.md").to_str().unwrap().to_string(),
                pages.join("bar.md").to_str().unwrap().to_string(),
            ],
            active_buffer: Some(pages.join("foo.md").to_str().unwrap().to_string()),
            cursors: {
                let mut m = HashMap::new();
                m.insert(pages.join("foo.md").to_str().unwrap().to_string(), 6);
                m
            },
            scroll_offsets: HashMap::new(),
            layout: SessionLayout::Single,
            theme: "bloom-dark".to_string(),
        };

        state.restore_session(&data);

        // Active buffer should be foo.md.
        assert_eq!(
            state.buffer.file_path.as_ref().map(|p| p.to_str().unwrap().to_string()),
            Some(pages.join("foo.md").to_str().unwrap().to_string()),
        );
        assert_eq!(state.cursor, 6);
        // bar.md should be in open_buffers.
        assert_eq!(state.open_buffers.len(), 1);
        assert_eq!(
            state.open_buffers[0].file_path.as_ref().map(|p| p.to_str().unwrap().to_string()),
            Some(pages.join("bar.md").to_str().unwrap().to_string()),
        );
    }
}
