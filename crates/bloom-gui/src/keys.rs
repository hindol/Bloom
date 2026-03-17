use iced::keyboard;

pub(crate) fn convert_key(
    key: keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> Option<bloom_core::types::KeyEvent> {
    use bloom_core::types::{KeyCode, Modifiers as BloomMods};

    let code = match key.as_ref() {
        keyboard::Key::Named(named) => match named {
            keyboard::key::Named::Escape => KeyCode::Esc,
            keyboard::key::Named::Enter => KeyCode::Enter,
            keyboard::key::Named::Backspace => KeyCode::Backspace,
            keyboard::key::Named::Delete => KeyCode::Delete,
            keyboard::key::Named::Tab => KeyCode::Tab,
            keyboard::key::Named::ArrowUp => KeyCode::Up,
            keyboard::key::Named::ArrowDown => KeyCode::Down,
            keyboard::key::Named::ArrowLeft => KeyCode::Left,
            keyboard::key::Named::ArrowRight => KeyCode::Right,
            keyboard::key::Named::Home => KeyCode::Home,
            keyboard::key::Named::End => KeyCode::End,
            keyboard::key::Named::PageUp => KeyCode::PageUp,
            keyboard::key::Named::PageDown => KeyCode::PageDown,
            keyboard::key::Named::Space => KeyCode::Char(' '),
            _ => return None,
        },
        keyboard::Key::Character(s) => {
            let ch = s.chars().next()?;
            KeyCode::Char(ch)
        }
        keyboard::Key::Unidentified => return None,
    };

    let bloom_mods = BloomMods {
        ctrl: modifiers.control(),
        alt: modifiers.alt(),
        shift: modifiers.shift(),
        meta: modifiers.macos_command(),
    };

    Some(bloom_core::types::KeyEvent {
        code,
        modifiers: bloom_mods,
    })
}
