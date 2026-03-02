use std::collections::BTreeMap;

use crate::render::{WhichKeyEntry, WhichKeyFrame};

pub const DEFAULT_POPUP_TIMEOUT_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WhichKeyConfig {
    pub popup_timeout_ms: u64,
}

impl Default for WhichKeyConfig {
    fn default() -> Self {
        Self {
            popup_timeout_ms: DEFAULT_POPUP_TIMEOUT_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhichKeyCommand {
    pub id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhichKeyError {
    EmptySequence,
}

impl std::fmt::Display for WhichKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WhichKeyError::EmptySequence => write!(f, "key sequence cannot be empty"),
        }
    }
}

impl std::error::Error for WhichKeyError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WhichKeyStep {
    Pending(WhichKeyFrame),
    Execute(WhichKeyCommand),
    UnknownPrefix { prefix: String },
}

/// Hierarchical registry for which-key style keymaps.
#[derive(Debug, Clone, Default)]
pub struct WhichKeyRegistry {
    root: WhichKeyNode,
    config: WhichKeyConfig,
}

impl WhichKeyRegistry {
    pub fn new(config: WhichKeyConfig) -> Self {
        Self {
            root: WhichKeyNode::default(),
            config,
        }
    }

    pub fn config(&self) -> WhichKeyConfig {
        self.config
    }

    pub fn register_group<I, K>(
        &mut self,
        sequence: I,
        description: impl Into<String>,
    ) -> Result<(), WhichKeyError>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        let path = collect_path(sequence)?;
        let mut node = &mut self.root;
        for key in path {
            node = node.children.entry(key).or_default();
        }
        node.is_group = true;
        node.group_description = Some(description.into());
        Ok(())
    }

    pub fn register_command<I, K>(
        &mut self,
        sequence: I,
        command_id: impl Into<String>,
        description: impl Into<String>,
    ) -> Result<(), WhichKeyError>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        let path = collect_path(sequence)?;
        let mut node = &mut self.root;
        for key in path {
            node = node.children.entry(key).or_default();
        }
        node.command = Some(WhichKeyCommand {
            id: command_id.into(),
            description: description.into(),
        });
        Ok(())
    }

    pub fn query<I, K>(&self, prefix: I) -> Option<WhichKeyFrame>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        let path: Vec<String> = prefix
            .into_iter()
            .map(|key| key.as_ref().to_string())
            .collect();
        let node = self.find_node(path.iter().map(String::as_str))?;

        let entries = node
            .children
            .iter()
            .map(|(key, child)| WhichKeyEntry {
                key: key.clone(),
                description: child.entry_description(),
                is_group: child.is_group_entry(),
            })
            .collect();

        Some(WhichKeyFrame {
            prefix: path.join(" "),
            entries,
        })
    }

    pub fn command<I, K>(&self, sequence: I) -> Option<WhichKeyCommand>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        self.find_node(sequence)
            .and_then(|node| node.command.as_ref().cloned())
    }

    pub fn has_prefix<I, K>(&self, prefix: I) -> bool
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        self.find_node(prefix).is_some()
    }

    pub fn has_children<I, K>(&self, prefix: I) -> bool
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        self.find_node(prefix)
            .map(|node| !node.children.is_empty())
            .unwrap_or(false)
    }

    fn find_node<'a, I, K>(&'a self, prefix: I) -> Option<&'a WhichKeyNode>
    where
        I: IntoIterator<Item = K>,
        K: AsRef<str>,
    {
        let mut node = &self.root;
        for key in prefix {
            node = node.children.get(key.as_ref())?;
        }
        Some(node)
    }

    /// Collect all leaf commands in the registry as (keys_string, description) pairs.
    pub fn all_commands(&self) -> Vec<(String, String)> {
        self.all_commands_detailed()
            .into_iter()
            .map(|(keys, _id, description)| (keys, description))
            .collect()
    }

    /// Collect all leaf commands in the registry as (keys_string, command_id, description).
    pub fn all_commands_detailed(&self) -> Vec<(String, String, String)> {
        let mut result = Vec::new();
        self.collect_commands_detailed(&self.root, &mut Vec::new(), &mut result);
        result
    }

    fn collect_commands_detailed(
        &self,
        node: &WhichKeyNode,
        path: &mut Vec<String>,
        result: &mut Vec<(String, String, String)>,
    ) {
        if let Some(ref cmd) = node.command {
            let keys = format!("SPC {}", path.join(" "));
            result.push((keys, cmd.id.clone(), cmd.description.clone()));
        }
        for (key, child) in &node.children {
            path.push(key.clone());
            self.collect_commands_detailed(child, path, result);
            path.pop();
        }
    }
}

/// Lightweight state machine for pending which-key traversal.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WhichKeyState {
    prefix: Vec<String>,
}

impl WhichKeyState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn prefix(&self) -> &[String] {
        &self.prefix
    }

    pub fn reset(&mut self) {
        self.prefix.clear();
    }

    pub fn advance<K: AsRef<str>>(&mut self, key: K, registry: &WhichKeyRegistry) -> WhichKeyStep {
        self.prefix.push(key.as_ref().to_string());

        if !registry.has_prefix(self.prefix.iter().map(String::as_str)) {
            let unknown = self.prefix.join(" ");
            self.reset();
            return WhichKeyStep::UnknownPrefix { prefix: unknown };
        }

        if let Some(command) = registry.command(self.prefix.iter().map(String::as_str)) {
            if !registry.has_children(self.prefix.iter().map(String::as_str)) {
                self.reset();
                return WhichKeyStep::Execute(command);
            }
        }

        let frame = registry
            .query(self.prefix.iter().map(String::as_str))
            .unwrap_or_else(|| WhichKeyFrame {
                prefix: self.prefix.join(" "),
                entries: Vec::new(),
            });
        WhichKeyStep::Pending(frame)
    }
}

#[derive(Debug, Clone, Default)]
struct WhichKeyNode {
    group_description: Option<String>,
    command: Option<WhichKeyCommand>,
    is_group: bool,
    children: BTreeMap<String, WhichKeyNode>,
}

impl WhichKeyNode {
    fn is_group_entry(&self) -> bool {
        self.is_group || !self.children.is_empty()
    }

    fn entry_description(&self) -> String {
        if self.is_group_entry() {
            self.group_description
                .as_ref()
                .or_else(|| self.command.as_ref().map(|command| &command.description))
                .cloned()
                .unwrap_or_default()
        } else {
            self.command
                .as_ref()
                .map(|command| command.description.clone())
                .or_else(|| self.group_description.clone())
                .unwrap_or_default()
        }
    }
}

fn collect_path<I, K>(sequence: I) -> Result<Vec<String>, WhichKeyError>
where
    I: IntoIterator<Item = K>,
    K: AsRef<str>,
{
    let path: Vec<String> = sequence
        .into_iter()
        .map(|key| key.as_ref().to_string())
        .collect();
    if path.is_empty() {
        Err(WhichKeyError::EmptySequence)
    } else {
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::WhichKeyEntry;

    #[test]
    fn prefix_traversal_advances_through_groups() {
        let mut registry = WhichKeyRegistry::default();
        registry.register_group(["SPC"], "leader").unwrap();
        registry.register_group(["SPC", "w"], "window").unwrap();
        registry
            .register_command(["SPC", "w", "v"], "window.split.vertical", "Vertical split")
            .unwrap();
        registry
            .register_command(
                ["SPC", "w", "s"],
                "window.split.horizontal",
                "Horizontal split",
            )
            .unwrap();

        let mut state = WhichKeyState::new();

        let first = state.advance("SPC", &registry);
        assert_eq!(
            first,
            WhichKeyStep::Pending(WhichKeyFrame {
                prefix: "SPC".into(),
                entries: vec![WhichKeyEntry {
                    key: "w".into(),
                    description: "window".into(),
                    is_group: true,
                }],
            })
        );

        let second = state.advance("w", &registry);
        assert_eq!(
            second,
            WhichKeyStep::Pending(WhichKeyFrame {
                prefix: "SPC w".into(),
                entries: vec![
                    WhichKeyEntry {
                        key: "s".into(),
                        description: "Horizontal split".into(),
                        is_group: false,
                    },
                    WhichKeyEntry {
                        key: "v".into(),
                        description: "Vertical split".into(),
                        is_group: false,
                    },
                ],
            })
        );
        assert_eq!(state.prefix(), &["SPC".to_string(), "w".to_string()]);
    }

    #[test]
    fn query_distinguishes_group_and_command_entries() {
        let mut registry = WhichKeyRegistry::default();
        registry.register_group(["SPC"], "leader").unwrap();
        registry.register_group(["SPC", "f"], "file").unwrap();
        registry
            .register_command(["SPC", "f", "f"], "file.find", "Find page")
            .unwrap();

        let top = registry.query(["SPC"]).unwrap();
        assert_eq!(
            top.entries,
            vec![WhichKeyEntry {
                key: "f".into(),
                description: "file".into(),
                is_group: true,
            }]
        );

        let file = registry.query(["SPC", "f"]).unwrap();
        assert_eq!(
            file.entries,
            vec![WhichKeyEntry {
                key: "f".into(),
                description: "Find page".into(),
                is_group: false,
            }]
        );
    }

    #[test]
    fn unknown_prefix_returns_none_and_resets_state() {
        let mut registry = WhichKeyRegistry::default();
        registry.register_group(["SPC"], "leader").unwrap();
        registry
            .register_command(["SPC", "f", "f"], "file.find", "Find page")
            .unwrap();

        assert!(registry.query(["SPC", "z"]).is_none());

        let mut state = WhichKeyState::new();
        let _ = state.advance("SPC", &registry);
        let unknown = state.advance("z", &registry);

        assert_eq!(
            unknown,
            WhichKeyStep::UnknownPrefix {
                prefix: "SPC z".into(),
            }
        );
        assert!(state.prefix().is_empty());
    }

    #[test]
    fn timeout_default_is_300ms() {
        assert_eq!(WhichKeyRegistry::default().config().popup_timeout_ms, 300);
    }
}
