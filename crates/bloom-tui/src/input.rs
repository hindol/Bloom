use bloom_core::types;
use crossterm::event::{self as ct, KeyModifiers};

/// Convert a crossterm key event into a bloom-core KeyEvent.
pub fn convert_key(event: ct::KeyEvent) -> Option<types::KeyEvent> {
    let modifiers = types::Modifiers {
        ctrl: event.modifiers.contains(KeyModifiers::CONTROL),
        alt: event.modifiers.contains(KeyModifiers::ALT),
        shift: event.modifiers.contains(KeyModifiers::SHIFT),
        meta: event.modifiers.contains(KeyModifiers::SUPER),
    };

    let code = match event.code {
        ct::KeyCode::Char(c) => types::KeyCode::Char(c),
        ct::KeyCode::Enter => types::KeyCode::Enter,
        ct::KeyCode::Esc => types::KeyCode::Esc,
        ct::KeyCode::Tab => types::KeyCode::Tab,
        ct::KeyCode::Backspace => types::KeyCode::Backspace,
        ct::KeyCode::Delete => types::KeyCode::Delete,
        ct::KeyCode::Up => types::KeyCode::Up,
        ct::KeyCode::Down => types::KeyCode::Down,
        ct::KeyCode::Left => types::KeyCode::Left,
        ct::KeyCode::Right => types::KeyCode::Right,
        ct::KeyCode::Home => types::KeyCode::Home,
        ct::KeyCode::End => types::KeyCode::End,
        ct::KeyCode::PageUp => types::KeyCode::PageUp,
        ct::KeyCode::PageDown => types::KeyCode::PageDown,
        ct::KeyCode::F(n) => types::KeyCode::F(n),
        _ => return None,
    };

    Some(types::KeyEvent { code, modifiers })
}
