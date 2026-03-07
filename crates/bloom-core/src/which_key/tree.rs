use std::collections::HashMap;

use crate::types::{KeyCode, KeyEvent};

pub type ActionId = String;

pub struct WhichKeyTree {
    root: WhichKeyNode,
}

struct WhichKeyNode {
    children: HashMap<String, WhichKeyChild>,
}

enum WhichKeyChild {
    Group { label: String, node: WhichKeyNode },
    Action { label: String, action: ActionId },
}

#[derive(Debug)]
pub enum WhichKeyLookup {
    /// Exact match — execute this action.
    Action(ActionId),
    /// Prefix match — show these next keys.
    Prefix(Vec<WhichKeyEntry>),
    /// No match.
    NoMatch,
}

#[derive(Debug)]
pub struct WhichKeyEntry {
    pub key: String,
    pub label: String,
    pub is_group: bool,
}

/// Frame data for rendering the which-key popup.
pub struct WhichKeyFrame {
    pub entries: Vec<WhichKeyEntry>,
    pub prefix: String,
    pub context: WhichKeyContext,
}

pub enum WhichKeyContext {
    Leader,
    VimOperator { operator: String },
}

impl WhichKeyNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
        }
    }

    fn entries(&self) -> Vec<WhichKeyEntry> {
        let mut entries: Vec<WhichKeyEntry> = self
            .children
            .iter()
            .map(|(key, child)| match child {
                WhichKeyChild::Group { label, .. } => WhichKeyEntry {
                    key: key.clone(),
                    label: label.clone(),
                    is_group: true,
                },
                WhichKeyChild::Action { label, .. } => WhichKeyEntry {
                    key: key.clone(),
                    label: label.clone(),
                    is_group: false,
                },
            })
            .collect();
        entries.sort_by(|a, b| a.key.cmp(&b.key));
        entries
    }
}

impl Default for WhichKeyTree {
    fn default() -> Self {
        Self::new()
    }
}

impl WhichKeyTree {
    pub fn new() -> Self {
        Self {
            root: WhichKeyNode::new(),
        }
    }

    /// Register a key sequence. E.g., `"f f"` maps to label `"Find file"` with the given action.
    pub fn register(&mut self, keys: &str, label: &str, action: ActionId) {
        let parts: Vec<&str> = keys.split_whitespace().collect();
        assert!(!parts.is_empty(), "keys must not be empty");

        let mut node = &mut self.root;

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            let key = part.to_string();

            if is_last {
                node.children.insert(
                    key,
                    WhichKeyChild::Action {
                        label: label.to_string(),
                        action,
                    },
                );
                return;
            }

            // Intermediate key — ensure a group exists
            let child = node
                .children
                .entry(key.clone())
                .or_insert_with(|| WhichKeyChild::Group {
                    label: format!("+{key}"), // default; override via set_group_label()
                    node: WhichKeyNode::new(),
                });

            match child {
                WhichKeyChild::Group {
                    node: ref mut n, ..
                } => {
                    node = n;
                }
                WhichKeyChild::Action { .. } => {
                    panic!("conflict: key '{key}' is already bound to an action");
                }
            }
        }
    }

    /// Set a descriptive label for a group key (e.g., "f" → "files").
    pub fn set_group_label(&mut self, key: &str, label: &str) {
        if let Some(WhichKeyChild::Group {
            label: ref mut l, ..
        }) = self.root.children.get_mut(key)
        {
            *l = label.to_string();
        }
    }

    /// Look up a key prefix. Returns the match result.
    pub fn lookup(&self, prefix: &[KeyEvent]) -> WhichKeyLookup {
        let mut node = &self.root;

        for (i, event) in prefix.iter().enumerate() {
            let key_str = key_event_to_string(event);
            match node.children.get(&key_str) {
                Some(WhichKeyChild::Group { node: ref n, .. }) => {
                    node = n;
                }
                Some(WhichKeyChild::Action { action, .. }) => {
                    if i == prefix.len() - 1 {
                        return WhichKeyLookup::Action(action.clone());
                    }
                    return WhichKeyLookup::NoMatch;
                }
                None => return WhichKeyLookup::NoMatch,
            }
        }

        WhichKeyLookup::Prefix(node.entries())
    }
}

fn key_event_to_string(event: &KeyEvent) -> String {
    // For Char keys, the case is already encoded in the character itself
    // (e.g. Shift+t → Char('T')). Strip shift so 'T' matches tree key "T".
    // Other modifiers (Ctrl, Alt) are preserved as prefixes.
    match &event.code {
        KeyCode::Char(c) => {
            let has_ctrl = event.modifiers.ctrl;
            let has_alt = event.modifiers.alt;
            let has_meta = event.modifiers.meta;
            if !has_ctrl && !has_alt && !has_meta {
                c.to_string()
            } else {
                // Build prefix without Shift (it's in the char)
                let mut prefix = String::new();
                if has_ctrl {
                    prefix.push_str("C-");
                }
                if has_alt {
                    prefix.push_str("A-");
                }
                if has_meta {
                    prefix.push_str("M-");
                }
                format!("{prefix}{c}")
            }
        }
        _ => event.to_string(),
    }
}

/// Build the default which-key tree with all leader keybindings.
pub fn default_tree() -> WhichKeyTree {
    let mut tree = WhichKeyTree::new();

    // Files
    tree.register("f f", "Find page", "find_page".into());
    tree.register("f r", "Rename page", "rename_page".into());
    tree.register("f D", "Delete page", "delete_page".into());

    // Buffers
    tree.register("b b", "Switch buffer", "switch_buffer".into());
    tree.register("b d", "Close buffer", "close_buffer".into());

    // Journal
    tree.register("j j", "Journal today", "journal_today".into());
    tree.register("j p", "Journal previous", "journal_prev".into());
    tree.register("j n", "Journal next", "journal_next".into());
    tree.register("j a", "Journal append", "journal_append".into());
    tree.register("j t", "Journal task", "journal_task".into());

    // Search
    tree.register("s s", "Search", "search".into());
    tree.register("s j", "Search journal", "search_journal".into());
    tree.register("s t", "Search tags", "search_tags".into());
    tree.register("s l", "Search backlinks", "search_backlinks".into());
    tree.register("s u", "Search unlinked", "search_unlinked".into());

    // Links
    tree.register("l l", "Insert link", "insert_link".into());
    tree.register("l y", "Yank link", "yank_link".into());
    tree.register("l Y", "Yank block link", "yank_block_link".into());
    tree.register("l t", "Timeline", "timeline".into());
    tree.register("l b", "Backlinks", "backlinks".into());

    // Tags
    tree.register("t a", "Add tag", "add_tag".into());
    tree.register("t r", "Remove tag", "remove_tag".into());

    // Agenda
    tree.register("a a", "Agenda", "agenda".into());

    // Insert
    tree.register("i d", "Insert @due()", "insert_due".into());
    tree.register("i s", "Insert @start()", "insert_start".into());
    tree.register("i a", "Insert @at()", "insert_at".into());

    // Windows
    tree.register("w v", "Split vertical", "split_vertical".into());
    tree.register("w s", "Split horizontal", "split_horizontal".into());
    tree.register("w h", "Navigate left", "navigate_left".into());
    tree.register("w j", "Navigate down", "navigate_down".into());
    tree.register("w k", "Navigate up", "navigate_up".into());
    tree.register("w l", "Navigate right", "navigate_right".into());
    tree.register("w d", "Close window", "close_window".into());
    tree.register("w o", "Close other windows", "close_other_windows".into());
    tree.register("w =", "Balance windows", "balance".into());
    tree.register("w m", "Maximize window", "maximize".into());
    tree.register("w >", "Widen window", "widen_window".into());
    tree.register("w <", "Narrow window", "narrow_window".into());
    tree.register("w +", "Taller window", "taller_window".into());
    tree.register("w -", "Shorter window", "shorter_window".into());
    tree.register("w x", "Swap with next window", "swap_window".into());
    tree.register("w R", "Rotate layout", "rotate_layout".into());
    tree.register("w H", "Move buffer left", "move_buffer_left".into());
    tree.register("w J", "Move buffer down", "move_buffer_down".into());
    tree.register("w K", "Move buffer up", "move_buffer_up".into());
    tree.register("w L", "Move buffer right", "move_buffer_right".into());

    // Refactor
    tree.register("r s", "Split page", "split_page".into());
    tree.register("r m", "Merge pages", "merge_pages".into());
    tree.register("r b", "Move block", "move_block".into());

    // Undo
    tree.register("u u", "Undo tree", "undo_tree".into());

    // New from template (single key, no sub-key)
    tree.register("n", "New from template", "new_from_template".into());

    // Toggles
    tree.register("T t", "Theme selector", "theme_selector".into());
    tree.register("T m", "Toggle MCP", "toggle_mcp".into());

    // Set descriptive group labels
    tree.set_group_label("f", "files");
    tree.set_group_label("b", "buffers");
    tree.set_group_label("j", "journal");
    tree.set_group_label("s", "search");
    tree.set_group_label("l", "links");
    tree.set_group_label("a", "agenda");
    tree.set_group_label("w", "windows");
    tree.set_group_label("r", "refactor");
    tree.set_group_label("u", "undo");
    tree.set_group_label("T", "toggles");
    tree.set_group_label("i", "insert");
    tree.set_group_label("t", "tags");

    tree
}

// ---------- CommandRegistry ----------

pub struct CommandRegistry {
    commands: HashMap<String, CommandDef>,
}

pub struct CommandDef {
    pub name: String,
    pub aliases: Vec<String>,
    pub description: String,
    pub args: CommandArgs,
}

pub enum CommandArgs {
    None,
    Required(String),
    Optional(String),
}

pub struct Completion {
    pub text: String,
    pub description: String,
}

impl Default for CommandRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandRegistry {
    pub fn new() -> Self {
        Self {
            commands: HashMap::new(),
        }
    }

    pub fn register(&mut self, def: CommandDef) {
        self.commands.insert(def.name.clone(), def);
    }

    pub fn complete(&self, prefix: &str) -> Vec<Completion> {
        let prefix_lower = prefix.to_lowercase();
        let mut completions: Vec<Completion> = self
            .commands
            .values()
            .filter(|def| {
                def.name.to_lowercase().starts_with(&prefix_lower)
                    || def
                        .aliases
                        .iter()
                        .any(|a| a.to_lowercase().starts_with(&prefix_lower))
            })
            .map(|def| Completion {
                text: def.name.clone(),
                description: def.description.clone(),
            })
            .collect();
        completions.sort_by(|a, b| a.text.cmp(&b.text));
        completions
    }

    pub fn complete_args(&self, command: &str, arg_prefix: &str) -> Vec<Completion> {
        let Some(def) = self.find(command) else {
            return Vec::new();
        };
        match &def.args {
            CommandArgs::None => Vec::new(),
            CommandArgs::Required(label) | CommandArgs::Optional(label) => {
                if label.to_lowercase().contains(&arg_prefix.to_lowercase())
                    || arg_prefix.is_empty()
                {
                    vec![Completion {
                        text: label.clone(),
                        description: format!("Argument for {}", def.name),
                    }]
                } else {
                    Vec::new()
                }
            }
        }
    }

    pub fn find(&self, name: &str) -> Option<&CommandDef> {
        if let Some(def) = self.commands.get(name) {
            return Some(def);
        }
        // Search aliases
        self.commands
            .values()
            .find(|def| def.aliases.iter().any(|a| a == name))
    }
}

pub fn default_registry() -> CommandRegistry {
    let mut reg = CommandRegistry::new();

    reg.register(CommandDef {
        name: "write".into(),
        aliases: vec!["w".into()],
        description: "Save current file".into(),
        args: CommandArgs::None,
    });

    reg.register(CommandDef {
        name: "quit".into(),
        aliases: vec!["q".into()],
        description: "Quit".into(),
        args: CommandArgs::None,
    });

    reg.register(CommandDef {
        name: "write-quit".into(),
        aliases: vec!["wq".into(), "x".into()],
        description: "Save and quit".into(),
        args: CommandArgs::None,
    });

    reg.register(CommandDef {
        name: "rebuild-index".into(),
        aliases: vec![],
        description: "Rebuild the search index".into(),
        args: CommandArgs::None,
    });

    reg.register(CommandDef {
        name: "theme".into(),
        aliases: vec![],
        description: "Switch color theme".into(),
        args: CommandArgs::Required("<theme-name>".into()),
    });

    reg.register(CommandDef {
        name: "import-logseq".into(),
        aliases: vec![],
        description: "Import from Logseq vault".into(),
        args: CommandArgs::Required("<path>".into()),
    });

    reg.register(CommandDef {
        name: "rename".into(),
        aliases: vec![],
        description: "Rename current page".into(),
        args: CommandArgs::Required("<new-name>".into()),
    });

    reg.register(CommandDef {
        name: "delete".into(),
        aliases: vec![],
        description: "Delete current page".into(),
        args: CommandArgs::None,
    });

    reg
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::KeyEvent;

    #[test]
    fn lookup_action() {
        let tree = default_tree();
        let result = tree.lookup(&[KeyEvent::char('f'), KeyEvent::char('f')]);
        match result {
            WhichKeyLookup::Action(id) => assert_eq!(id, "find_page"),
            _ => panic!("expected Action"),
        }
    }

    #[test]
    fn lookup_prefix() {
        let tree = default_tree();
        let result = tree.lookup(&[KeyEvent::char('f')]);
        match result {
            WhichKeyLookup::Prefix(entries) => {
                assert_eq!(entries.len(), 3); // f, r, D
                assert!(entries.iter().all(|e| !e.is_group));
            }
            _ => panic!("expected Prefix"),
        }
    }

    #[test]
    fn lookup_no_match() {
        let tree = default_tree();
        let result = tree.lookup(&[KeyEvent::char('z')]);
        assert!(matches!(result, WhichKeyLookup::NoMatch));
    }

    #[test]
    fn lookup_single_key_action() {
        let tree = default_tree();
        let result = tree.lookup(&[KeyEvent::char('n')]);
        match result {
            WhichKeyLookup::Action(id) => assert_eq!(id, "new_from_template"),
            _ => panic!("expected Action for 'n'"),
        }
    }

    #[test]
    fn lookup_empty_prefix_returns_root() {
        let tree = default_tree();
        let result = tree.lookup(&[]);
        match result {
            WhichKeyLookup::Prefix(entries) => {
                assert!(!entries.is_empty());
            }
            _ => panic!("expected Prefix for empty lookup"),
        }
    }

    #[test]
    fn registry_complete() {
        let reg = default_registry();
        let completions = reg.complete("wr");
        assert_eq!(completions.len(), 2);
        assert_eq!(completions[0].text, "write");
        assert_eq!(completions[1].text, "write-quit");
    }

    #[test]
    fn registry_find_by_alias() {
        let reg = default_registry();
        let def = reg.find("w").unwrap();
        assert_eq!(def.name, "write");
    }

    #[test]
    fn registry_find_by_name() {
        let reg = default_registry();
        let def = reg.find("quit").unwrap();
        assert_eq!(def.name, "quit");
    }

    #[test]
    fn registry_complete_args() {
        let reg = default_registry();
        let completions = reg.complete_args("theme", "");
        assert_eq!(completions.len(), 1);
        assert_eq!(completions[0].text, "<theme-name>");
    }

    #[test]
    fn registry_wq_alias() {
        let reg = default_registry();
        assert!(reg.find("wq").is_some());
        assert!(reg.find("x").is_some());
        assert_eq!(reg.find("wq").unwrap().name, "write-quit");
    }

    #[test]
    fn lookup_uppercase_with_shift_modifier() {
        // Crossterm sends Shift+t as Char('T') with shift=true.
        // The which-key tree registers "T" as a plain key.
        let tree = default_tree();
        let shift_t = KeyEvent {
            code: crate::types::KeyCode::Char('T'),
            modifiers: crate::types::Modifiers::shift(),
        };
        let result = tree.lookup(&[shift_t]);
        match result {
            WhichKeyLookup::Prefix(entries) => {
                assert!(
                    !entries.is_empty(),
                    "T should be a group with t, m children"
                );
            }
            _ => panic!("expected Prefix for Shift+T, got {result:?}"),
        }
    }
}
