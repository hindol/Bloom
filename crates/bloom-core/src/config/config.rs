use crate::error::BloomError;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;

// ── Config template & version ────────────────────────────────────────────

pub const CURRENT_CONFIG_VERSION: u32 = 1;

pub const CONFIG_TEMPLATE: &str = r#"# ──────────────────────────────────────────────────────────────
# Bloom Configuration
# ──────────────────────────────────────────────────────────────
# Every setting is listed with its default. Uncomment to customize.
# Config version — used for automatic migration. Do not edit.
config_version = 1

# ──── Startup ─────────────────────────────────────────────────

# What to show on launch.
# Options: "restore" (last session), "journal" (today), "blank"
# startup.mode = "journal"

# ──── Editor ──────────────────────────────────────────────────

# scrolloff = 3
# autosave_debounce_ms = 300
# word_wrap = true
# wrap_indicator = "↪"
# auto_align = "page"
# max_results = 100

# ──── Theme ───────────────────────────────────────────────────

# Built-in: bloom-dark, bloom-light, aurora, frost, ember,
#           solarium, twilight, sakura, verdant, lichen, paper
# theme.name = "bloom-dark"

# ──── Font (GUI) ──────────────────────────────────────────────

# font.family = "JetBrains Mono"
# font.size = 14
# font.line_height = 1.6

# ──── Which-Key ───────────────────────────────────────────────

# which_key_timeout_ms = 500

# ──── History (git-backed time travel) ────────────────────────

# [history]
# auto_commit_idle_minutes = 5
# max_commit_interval_minutes = 60

# ──── MCP Server ──────────────────────────────────────────────

# [mcp]
# enabled = false
# mode = "read-only"
# exclude_paths = []

# ──── Views (BQL) ─────────────────────────────────────────────

# [[views]]
# name = "Work Tasks"
# query = "tasks | where not done and tags has #work | sort due"
# key = "SPC v w"

# ──── Calendar ────────────────────────────────────────────────

# [calendar]
# week_starts = "monday"
"#;

// ── Default value helpers ────────────────────────────────────────────────

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
    pub config_version: u32,
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
        query: "tasks | where not done | sort due | group due.category".to_string(),
        key: Some("a a".to_string()),
    }]
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct StartupConfig {
    #[serde(default)]
    pub mode: StartupMode,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Deserialize, Default, PartialEq)]
pub struct McpConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub mode: McpMode,
    #[serde(default)]
    pub exclude_paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum McpMode {
    #[default]
    ReadOnly,
    ReadWrite,
}

impl Config {
    /// Load configuration from a TOML file at the given path.
    /// Runs version-based migration when `config_version` is outdated.
    pub fn load(path: &Path) -> Result<Self, BloomError> {
        let content = std::fs::read_to_string(path)?;
        let mut config: Config =
            toml::from_str(&content).map_err(|e| BloomError::ConfigError(e.to_string()))?;

        if config.config_version < CURRENT_CONFIG_VERSION {
            let migrated = migrate_config(&config);
            let _ = std::fs::write(path, &migrated);
            config.config_version = CURRENT_CONFIG_VERSION;
        }

        Ok(config)
    }

    /// Return the default configuration.
    pub fn defaults() -> Self {
        Self {
            config_version: CURRENT_CONFIG_VERSION,
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

// ── Migration ────────────────────────────────────────────────────────────

/// Rebuild `config.toml` from `CONFIG_TEMPLATE`, uncommenting lines whose
/// values differ from the compiled-in defaults.
pub fn migrate_config(config: &Config) -> String {
    let defaults = Config::defaults();
    let mut result = CONFIG_TEMPLATE.to_string();

    // ── Startup ──────────────────────────────────────────────────────
    if config.startup.mode != defaults.startup.mode {
        let mode_str = match config.startup.mode {
            StartupMode::Journal => "journal",
            StartupMode::Restore => "restore",
            StartupMode::Blank => "blank",
        };
        result = result.replace(
            "# startup.mode = \"journal\"",
            &format!("startup.mode = \"{}\"", mode_str),
        );
    }

    // ── Editor scalars ───────────────────────────────────────────────
    if config.scrolloff != defaults.scrolloff {
        result = result.replace(
            "# scrolloff = 3",
            &format!("scrolloff = {}", config.scrolloff),
        );
    }
    if config.autosave_debounce_ms != defaults.autosave_debounce_ms {
        result = result.replace(
            "# autosave_debounce_ms = 300",
            &format!("autosave_debounce_ms = {}", config.autosave_debounce_ms),
        );
    }
    if config.word_wrap != defaults.word_wrap {
        result = result.replace(
            "# word_wrap = true",
            &format!("word_wrap = {}", config.word_wrap),
        );
    }
    if config.wrap_indicator != defaults.wrap_indicator {
        result = result.replace(
            "# wrap_indicator = \"↪\"",
            &format!("wrap_indicator = \"{}\"", config.wrap_indicator),
        );
    }
    if config.auto_align != defaults.auto_align {
        let align_str = match config.auto_align {
            AutoAlignMode::Page => "page",
            AutoAlignMode::Block => "block",
            AutoAlignMode::None => "none",
        };
        result = result.replace(
            "# auto_align = \"page\"",
            &format!("auto_align = \"{}\"", align_str),
        );
    }
    if config.max_results != defaults.max_results {
        result = result.replace(
            "# max_results = 100",
            &format!("max_results = {}", config.max_results),
        );
    }

    // ── Theme ────────────────────────────────────────────────────────
    if config.theme.name != defaults.theme.name {
        result = result.replace(
            "# theme.name = \"bloom-dark\"",
            &format!("theme.name = \"{}\"", config.theme.name),
        );
    }
    if !config.theme.overrides.is_empty() {
        let mut overrides = String::from("\n[theme.overrides]\n");
        let mut keys: Vec<&String> = config.theme.overrides.keys().collect();
        keys.sort();
        for k in keys {
            overrides.push_str(&format!("{} = \"{}\"\n", k, config.theme.overrides[k]));
        }
        result.push_str(&overrides);
    }

    // ── Font ─────────────────────────────────────────────────────────
    if config.font.family != defaults.font.family {
        result = result.replace(
            "# font.family = \"JetBrains Mono\"",
            &format!("font.family = \"{}\"", config.font.family),
        );
    }
    if config.font.size != defaults.font.size {
        result = result.replace(
            "# font.size = 14",
            &format!("font.size = {}", config.font.size),
        );
    }
    if (config.font.line_height - defaults.font.line_height).abs() > f32::EPSILON {
        result = result.replace(
            "# font.line_height = 1.6",
            &format!("font.line_height = {}", config.font.line_height),
        );
    }

    // ── Which-Key ────────────────────────────────────────────────────
    if config.which_key_timeout_ms != defaults.which_key_timeout_ms {
        result = result.replace(
            "# which_key_timeout_ms = 500",
            &format!("which_key_timeout_ms = {}", config.which_key_timeout_ms),
        );
    }

    // ── History ──────────────────────────────────────────────────────
    if config.history != defaults.history {
        let replacement = format!(
            "[history]\nauto_commit_idle_minutes = {}\nmax_commit_interval_minutes = {}",
            config.history.auto_commit_idle_minutes, config.history.max_commit_interval_minutes,
        );
        result = result.replace(
            "# [history]\n# auto_commit_idle_minutes = 5\n# max_commit_interval_minutes = 60",
            &replacement,
        );
    }

    // ── MCP ──────────────────────────────────────────────────────────
    if config.mcp != defaults.mcp {
        let mode_str = match config.mcp.mode {
            McpMode::ReadOnly => "read-only",
            McpMode::ReadWrite => "read-write",
        };
        let exclude = if config.mcp.exclude_paths.is_empty() {
            "[]".to_string()
        } else {
            format!(
                "[{}]",
                config
                    .mcp
                    .exclude_paths
                    .iter()
                    .map(|p| format!("\"{}\"", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let replacement = format!(
            "[mcp]\nenabled = {}\nmode = \"{}\"\nexclude_paths = {}",
            config.mcp.enabled, mode_str, exclude,
        );
        result = result.replace(
            "# [mcp]\n# enabled = false\n# mode = \"read-only\"\n# exclude_paths = []",
            &replacement,
        );
    }

    // ── Views ────────────────────────────────────────────────────────
    // The default "Agenda" view comes from Rust defaults; the template only
    // shows a commented example. If the user has non-default views, append them.
    if !views_eq(&config.views, &defaults.views) {
        // Remove the commented example so it doesn't confuse users.
        result = result.replace(
            "# [[views]]\n# name = \"Work Tasks\"\n# query = \"tasks | where not done and tags has #work | sort due\"\n# key = \"SPC v w\"",
            "# Custom views (migrated from previous config):",
        );
        for v in &config.views {
            result.push_str("\n[[views]]\n");
            result.push_str(&format!("name = \"{}\"\n", v.name));
            result.push_str(&format!("query = \"{}\"\n", v.query));
            if let Some(key) = &v.key {
                result.push_str(&format!("key = \"{}\"\n", key));
            }
        }
    }

    result
}

fn views_eq(a: &[ViewConfig], b: &[ViewConfig]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .zip(b.iter())
        .all(|(va, vb)| va.name == vb.name && va.query == vb.query && va.key == vb.key)
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
        assert_eq!(config.config_version, CURRENT_CONFIG_VERSION);
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

    #[test]
    fn test_config_version_defaults_to_zero() {
        let toml_str = "[startup]\nmode = \"journal\"\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.config_version, 0);
    }

    #[test]
    fn test_config_version_parsed() {
        let toml_str = "config_version = 1\n";
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.config_version, 1);
    }

    #[test]
    fn test_template_parses_as_valid_config() {
        let config: Config = toml::from_str(CONFIG_TEMPLATE).unwrap();
        assert_eq!(config.config_version, CURRENT_CONFIG_VERSION);
        // All other fields should be defaults (since commented out).
        assert_eq!(config.startup.mode, StartupMode::Journal);
        assert_eq!(config.scrolloff, 3);
        assert_eq!(config.theme.name, "bloom-dark");
    }

    #[test]
    fn test_migrate_default_config_is_template() {
        let defaults = Config::defaults();
        let migrated = migrate_config(&defaults);
        // With all defaults, migration should produce the template unchanged.
        assert_eq!(migrated, CONFIG_TEMPLATE);
    }

    #[test]
    fn test_migrate_preserves_custom_scalars() {
        let mut config = Config::defaults();
        config.config_version = 0;
        config.scrolloff = 8;
        config.which_key_timeout_ms = 1000;
        config.theme.name = "aurora".to_string();

        let migrated = migrate_config(&config);

        assert!(migrated.contains("scrolloff = 8"));
        assert!(!migrated.contains("# scrolloff = 3"));
        assert!(migrated.contains("which_key_timeout_ms = 1000"));
        assert!(migrated.contains("theme.name = \"aurora\""));
        assert!(migrated.contains("config_version = 1"));
    }

    #[test]
    fn test_migrate_preserves_startup_mode() {
        let mut config = Config::defaults();
        config.startup.mode = StartupMode::Restore;

        let migrated = migrate_config(&config);
        assert!(migrated.contains("startup.mode = \"restore\""));
        assert!(!migrated.contains("# startup.mode"));
    }

    #[test]
    fn test_migrate_preserves_mcp() {
        let mut config = Config::defaults();
        config.mcp.enabled = true;
        config.mcp.mode = McpMode::ReadWrite;

        let migrated = migrate_config(&config);
        assert!(migrated.contains("[mcp]"));
        assert!(migrated.contains("enabled = true"));
        assert!(migrated.contains("mode = \"read-write\""));
        assert!(!migrated.contains("# [mcp]"));
    }

    #[test]
    fn test_migrate_preserves_history() {
        let mut config = Config::defaults();
        config.history.auto_commit_idle_minutes = 10;

        let migrated = migrate_config(&config);
        assert!(migrated.contains("[history]"));
        assert!(migrated.contains("auto_commit_idle_minutes = 10"));
    }

    #[test]
    fn test_migrate_preserves_font() {
        let mut config = Config::defaults();
        config.font.family = "Fira Code".to_string();
        config.font.size = 16;

        let migrated = migrate_config(&config);
        assert!(migrated.contains("font.family = \"Fira Code\""));
        assert!(migrated.contains("font.size = 16"));
    }

    #[test]
    fn test_migrated_output_parses() {
        let mut config = Config::defaults();
        config.config_version = 0;
        config.scrolloff = 10;
        config.theme.name = "frost".to_string();
        config.startup.mode = StartupMode::Restore;

        let migrated = migrate_config(&config);
        let reparsed: Config = toml::from_str(&migrated).unwrap();
        assert_eq!(reparsed.config_version, CURRENT_CONFIG_VERSION);
        assert_eq!(reparsed.scrolloff, 10);
        assert_eq!(reparsed.theme.name, "frost");
        assert_eq!(reparsed.startup.mode, StartupMode::Restore);
    }

    #[test]
    fn test_load_triggers_migration() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        // Write a v0 config (no config_version field).
        std::fs::write(
            &config_path,
            "[startup]\nmode = \"restore\"\n\n[theme]\nname = \"frost\"\n",
        )
        .unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.config_version, CURRENT_CONFIG_VERSION);
        assert_eq!(config.startup.mode, StartupMode::Restore);
        assert_eq!(config.theme.name, "frost");

        // File on disk should now be the migrated template.
        let on_disk = std::fs::read_to_string(&config_path).unwrap();
        assert!(on_disk.contains("config_version = 1"));
        assert!(on_disk.contains("startup.mode = \"restore\""));
        assert!(on_disk.contains("theme.name = \"frost\""));
    }

    #[test]
    fn test_load_skips_migration_when_current() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.toml");
        std::fs::write(&config_path, CONFIG_TEMPLATE).unwrap();

        let config = Config::load(&config_path).unwrap();
        assert_eq!(config.config_version, CURRENT_CONFIG_VERSION);

        // File should be untouched.
        let on_disk = std::fs::read_to_string(&config_path).unwrap();
        assert_eq!(on_disk, CONFIG_TEMPLATE);
    }
}
