use crate::error::BloomError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub buffers: Vec<SessionBuffer>,
    pub layout: SessionLayout,
    pub active_pane: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionBuffer {
    pub page_path: PathBuf,
    pub cursor_line: usize,
    pub cursor_column: usize,
    pub scroll_offset: usize,
    pub pane: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionLayout {
    Leaf(u64),
    Split {
        direction: String,
        ratio: f32,
        left: Box<SessionLayout>,
        right: Box<SessionLayout>,
    },
}

impl SessionState {
    /// Serialize and save session state as JSON to the given path.
    pub fn save(&self, path: &Path) -> Result<(), BloomError> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| BloomError::ConfigError(format!("session serialize: {}", e)))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Load session state from a JSON file.
    pub fn load(path: &Path) -> Result<Self, BloomError> {
        let content = std::fs::read_to_string(path)?;
        let state: SessionState = serde_json::from_str(&content)
            .map_err(|e| BloomError::ConfigError(format!("session deserialize: {}", e)))?;
        Ok(state)
    }
}