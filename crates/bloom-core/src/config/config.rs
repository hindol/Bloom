use crate::error::BloomError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

fn default_autosave_debounce() -> u64 {
    300
}
fn default_which_key_timeout() -> u64 {
    500
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub startup: StartupConfig,
    #[serde(default)]
    pub font: FontConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub mcp: McpConfig,
    #[serde(default = "default_autosave_debounce")]
    pub autosave_debounce_ms: u64,
    #[serde(default = "default_which_key_timeout")]
    pub which_key_timeout_ms: u64,
    #[serde(default)]
    pub auto_align: AutoAlignMode,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
pub enum AutoAlignMode {
    #[default]
    #[serde(rename = "page")]
    Page,
    #[serde(rename = "block")]
    Block,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartupConfig {
    #[serde(default)]
    pub mode: StartupMode,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub enum StartupMode {
    #[default]
    Journal,
    Restore,
    Blank,
}

fn default_font_family() -> String {
    "JetBrains Mono".into()
}
fn default_font_size() -> u16 {
    14
}
fn default_line_height() -> f32 {
    1.6
}

#[derive(Debug, Clone, Deserialize)]
pub struct FontConfig {
    #[serde(default = "default_font_family")]
    pub family: String,
    #[serde(default = "default_font_size")]
    pub size: u16,
    #[serde(default = "default_line_height")]
    pub line_height: f32,
}

fn default_theme() -> String {
    "bloom-dark".into()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_theme")]
    pub name: String,
    #[serde(default)]
    pub overrides: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: McpMode,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub enum McpMode {
    #[default]
    ReadOnly,
    ReadWrite,
}

impl Config {
    /// Load configuration from a TOML file at the given path.
    pub fn load(path: &Path) -> Result<Self, BloomError> {
        let content = std::fs::read_to_string(path)?;
        let config: Config =
            toml::from_str(&content).map_err(|e| BloomError::ConfigError(e.to_string()))?;
        Ok(config)
    }

    /// Return the default configuration.
    pub fn defaults() -> Self {
        Self {
            startup: StartupConfig::default(),
            font: FontConfig::default(),
            theme: ThemeConfig::default(),
            mcp: McpConfig::default(),
            autosave_debounce_ms: default_autosave_debounce(),
            which_key_timeout_ms: default_which_key_timeout(),
            auto_align: AutoAlignMode::default(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::defaults()
    }
}

impl Default for StartupConfig {
    fn default() -> Self {
        Self {
            mode: StartupMode::default(),
        }
    }
}

impl Default for FontConfig {
    fn default() -> Self {
        Self {
            family: default_font_family(),
            size: default_font_size(),
            line_height: default_line_height(),
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: default_theme(),
            overrides: HashMap::new(),
        }
    }
}

impl Default for McpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: McpMode::default(),
            exclude_paths: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::defaults();
        assert_eq!(config.autosave_debounce_ms, 300);
        assert_eq!(config.which_key_timeout_ms, 500);
        assert_eq!(config.font.family, "JetBrains Mono");
        assert_eq!(config.font.size, 14);
    }

    #[test]
    fn test_config_from_toml() {
        let toml_str = r#"
            autosave_debounce_ms = 500
            [startup]
            mode = "Restore"
            [theme]
            name = "bloom-light"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.autosave_debounce_ms, 500);
        assert_eq!(config.theme.name, "bloom-light");
    }
}