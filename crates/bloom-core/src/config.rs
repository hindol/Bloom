use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloomConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub mcp: crate::mcp::config::McpConfig,
    #[serde(default)]
    pub keybindings: KeybindingConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// "normal" or "insert" — startup mode
    #[serde(default = "default_startup_mode")]
    pub startup_mode: String,
    /// Restore previous session on launch
    #[serde(default)]
    pub restore_session: bool,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            startup_mode: default_startup_mode(),
            restore_session: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme_name")]
    pub name: String,
    #[serde(default)]
    pub overrides: std::collections::HashMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme_name(),
            overrides: std::collections::HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct KeybindingConfig {
    #[serde(default)]
    pub overrides: Vec<KeybindingOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindingOverride {
    pub keys: String,
    pub command: String,
}

fn default_startup_mode() -> String {
    "normal".into()
}
fn default_theme_name() -> String {
    "bloom-dark".into()
}

impl Default for BloomConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            theme: ThemeConfig::default(),
            mcp: crate::mcp::config::McpConfig::default(),
            keybindings: KeybindingConfig::default(),
        }
    }
}

/// Load config from `<vault>/.bloom/config.toml`, or return default.
pub fn load_config(vault_root: &Path) -> BloomConfig {
    let path = config_path(vault_root);
    match std::fs::read_to_string(&path) {
        Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
        Err(_) => BloomConfig::default(),
    }
}

/// Save config to `<vault>/.bloom/config.toml`.
pub fn save_config(vault_root: &Path, config: &BloomConfig) -> Result<(), std::io::Error> {
    let path = config_path(vault_root);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let contents = toml::to_string_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
    std::fs::write(&path, contents)
}

fn config_path(vault_root: &Path) -> PathBuf {
    vault_root.join(".bloom").join("config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_sensible_values() {
        let config = BloomConfig::default();
        assert_eq!(config.general.startup_mode, "normal");
        assert!(!config.general.restore_session);
        assert_eq!(config.theme.name, "bloom-dark");
        assert!(config.theme.overrides.is_empty());
        assert!(config.keybindings.overrides.is_empty());
        assert!(!config.mcp.enabled);
    }

    #[test]
    fn load_missing_config_returns_default() {
        let tmp = tempfile::tempdir().unwrap();
        let config = load_config(tmp.path());
        assert_eq!(config.general.startup_mode, "normal");
        assert_eq!(config.theme.name, "bloom-dark");
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let mut config = BloomConfig::default();
        config.general.startup_mode = "insert".into();
        config.general.restore_session = true;
        config.theme.name = "solarized".into();

        save_config(tmp.path(), &config).unwrap();
        let loaded = load_config(tmp.path());

        assert_eq!(loaded.general.startup_mode, "insert");
        assert!(loaded.general.restore_session);
        assert_eq!(loaded.theme.name, "solarized");
    }

    #[test]
    fn custom_overrides_parsed() {
        let tmp = tempfile::tempdir().unwrap();
        let toml_content = r##"
[general]
startup_mode = "insert"

[theme]
name = "custom"

[theme.overrides]
background = "#000000"

[[keybindings.overrides]]
keys = "ctrl+p"
command = "open_picker"
"##;
        let path = tmp.path().join(".bloom");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("config.toml"), toml_content).unwrap();

        let config = load_config(tmp.path());
        assert_eq!(config.general.startup_mode, "insert");
        assert_eq!(config.theme.name, "custom");
        assert_eq!(
            config.theme.overrides.get("background").unwrap(),
            "#000000"
        );
        assert_eq!(config.keybindings.overrides.len(), 1);
        assert_eq!(config.keybindings.overrides[0].keys, "ctrl+p");
        assert_eq!(config.keybindings.overrides[0].command, "open_picker");
    }
}
