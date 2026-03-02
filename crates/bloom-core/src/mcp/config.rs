use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpConfig {
    pub enabled: bool,
    pub mode: String, // "read-only" or "read-write"
    pub exclude_paths: Vec<String>,
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: "read-write".into(),
            exclude_paths: vec![],
        }
    }
}
