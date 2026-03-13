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
fn default_scrolloff() -> usize {
    3
}
fn default_word_wrap() -> bool {
    true
}
fn default_wrap_indicator() -> String {
    "↪".into()
}
fn default_max_results() -> u64 {
    100
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
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default = "default_autosave_debounce")]
    pub autosave_debounce_ms: u64,
    #[serde(default = "default_which_key_timeout")]
    pub which_key_timeout_ms: u64,
    #[serde(default)]
    pub auto_align: AutoAlignMode,
    #[serde(default = "default_scrolloff")]
    pub scrolloff: usize,
    #[serde(default = "default_word_wrap")]
    pub word_wrap: bool,
    #[serde(default = "default_wrap_indicator")]
    pub wrap_indicator: String,
    #[serde(default = "default_max_results")]
    pub max_results: u64,
    #[serde(default = "default_views")]
    pub views: Vec<ViewConfig>,
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

/// A named view backed by a BQL query.
#[derive(Debug, Clone, Deserialize)]
pub struct ViewConfig {
    pub name: String,
    pub query: String,
    /// Optional keybinding (e.g., "SPC a a"). If absent, only accessible via SPC v l.
    pub key: Option<String>,
}

fn default_views() -> Vec<ViewConfig> {
    vec![ViewConfig {
        name: "Agenda".to_string(),
        query: "tasks | where not done | sort due".to_string(),
        key: Some("a a".to_string()),
    }]
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct StartupConfig {
    #[serde(default)]
    pub mode: StartupMode,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
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

fn default_auto_commit_idle_minutes() -> u64 {
    5
}
fn default_max_commit_interval_minutes() -> u64 {
    60
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryConfig {
    #[serde(default = "default_auto_commit_idle_minutes")]
    pub auto_commit_idle_minutes: u64,
    #[serde(default = "default_max_commit_interval_minutes")]
    pub max_commit_interval_minutes: u64,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            auto_commit_idle_minutes: default_auto_commit_idle_minutes(),
            max_commit_interval_minutes: default_max_commit_interval_minutes(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: McpMode,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
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
            history: HistoryConfig::default(),
            autosave_debounce_ms: default_autosave_debounce(),
            which_key_timeout_ms: default_which_key_timeout(),
            auto_align: AutoAlignMode::default(),
            scrolloff: default_scrolloff(),
            word_wrap: default_word_wrap(),
            wrap_indicator: default_wrap_indicator(),
            max_results: default_max_results(),
            views: default_views(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::defaults()
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
            mode = "restore"
            [theme]
            name = "bloom-light"
        "#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.autosave_debounce_ms, 500);
        assert_eq!(config.theme.name, "bloom-light");
    }

    #[test]
    fn test_config_with_unknown_sections() {
        let toml_str = r#"
            [startup]
            mode = "restore"

            [editor]
            tab_width = 4
            auto_save_ms = 300

            [theme]
            name = "lichen"

            [calendar]
            week_starts = "monday"
        "#;
        let result: Result<Config, _> = toml::from_str(toml_str);
        match &result {
            Ok(c) => assert_eq!(c.theme.name, "lichen"),
            Err(e) => panic!("Config parse failed on unknown sections: {e}"),
        }
    }
}
