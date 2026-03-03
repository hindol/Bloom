use crate::types::{KeyCode, KeyEvent};

use super::dispatch::Action;

/// Check if a key event matches a platform-level shortcut.
///
/// Platform shortcuts take priority over all other key handling.
pub fn platform_shortcut(key: &KeyEvent) -> Option<Action> {
    if key.modifiers.ctrl {
        match key.code {
            KeyCode::Char('s') => return Some(Action::Save),
            KeyCode::Char('q') => return Some(Action::Quit),
            KeyCode::Char('z') => return Some(Action::Undo),
            KeyCode::Char('y') => return Some(Action::Redo),
            _ => {}
        }
    }
    None
}