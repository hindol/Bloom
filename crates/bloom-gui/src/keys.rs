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

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_core::types::KeyCode;

    fn no_mods() -> keyboard::Modifiers {
        keyboard::Modifiers::default()
    }

    fn ctrl() -> keyboard::Modifiers {
        let mut m = keyboard::Modifiers::default();
        m.insert(keyboard::Modifiers::CTRL);
        m
    }

    fn shift() -> keyboard::Modifiers {
        let mut m = keyboard::Modifiers::default();
        m.insert(keyboard::Modifiers::SHIFT);
        m
    }

    #[test]
    fn escape_maps_correctly() {
        let key = keyboard::Key::Named(keyboard::key::Named::Escape);
        let result = convert_key(key, no_mods()).unwrap();
        assert_eq!(result.code, KeyCode::Esc);
        assert!(!result.modifiers.ctrl);
    }

    #[test]
    fn enter_maps_correctly() {
        let key = keyboard::Key::Named(keyboard::key::Named::Enter);
        let result = convert_key(key, no_mods()).unwrap();
        assert_eq!(result.code, KeyCode::Enter);
    }

    #[test]
    fn character_key() {
        let key = keyboard::Key::Character("a".into());
        let result = convert_key(key, no_mods()).unwrap();
        assert_eq!(result.code, KeyCode::Char('a'));
        assert!(!result.modifiers.shift);
    }

    #[test]
    fn ctrl_modifier_propagates() {
        let key = keyboard::Key::Character("s".into());
        let result = convert_key(key, ctrl()).unwrap();
        assert_eq!(result.code, KeyCode::Char('s'));
        assert!(result.modifiers.ctrl);
    }

    #[test]
    fn shift_modifier_propagates() {
        let key = keyboard::Key::Character("A".into());
        let result = convert_key(key, shift()).unwrap();
        assert_eq!(result.code, KeyCode::Char('A'));
        assert!(result.modifiers.shift);
    }

    #[test]
    fn space_is_char_space() {
        let key = keyboard::Key::Named(keyboard::key::Named::Space);
        let result = convert_key(key, no_mods()).unwrap();
        assert_eq!(result.code, KeyCode::Char(' '));
    }

    #[test]
    fn arrow_keys() {
        let cases = [
            (keyboard::key::Named::ArrowUp, KeyCode::Up),
            (keyboard::key::Named::ArrowDown, KeyCode::Down),
            (keyboard::key::Named::ArrowLeft, KeyCode::Left),
            (keyboard::key::Named::ArrowRight, KeyCode::Right),
        ];
        for (named, expected) in cases {
            let result = convert_key(keyboard::Key::Named(named), no_mods()).unwrap();
            assert_eq!(result.code, expected);
        }
    }

    #[test]
    fn unidentified_returns_none() {
        assert!(convert_key(keyboard::Key::Unidentified, no_mods()).is_none());
    }
}
