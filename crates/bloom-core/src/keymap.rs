/// Platform-specific modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformModifier {
    Cmd,  // macOS
    Ctrl, // Windows/Linux
}

pub fn platform_modifier() -> PlatformModifier {
    if cfg!(target_os = "macos") {
        PlatformModifier::Cmd
    } else {
        PlatformModifier::Ctrl
    }
}

/// Common platform shortcuts mapped to editor actions.
pub struct PlatformKeymap {
    pub save: (PlatformModifier, char),
    pub quit: (PlatformModifier, char),
    pub copy: (PlatformModifier, char),
    pub paste: (PlatformModifier, char),
    pub undo: (PlatformModifier, char),
    pub redo: (PlatformModifier, char),
    pub find: (PlatformModifier, char),
}

impl PlatformKeymap {
    pub fn current() -> Self {
        let m = platform_modifier();
        Self {
            save: (m, 's'),
            quit: (m, 'q'),
            copy: (m, 'c'),
            paste: (m, 'v'),
            undo: (m, 'z'),
            redo: (
                m,
                if m == PlatformModifier::Cmd {
                    'Z'
                } else {
                    'y'
                },
            ),
            find: (m, 'f'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_modifier_is_cmd_on_macos() {
        let m = platform_modifier();
        if cfg!(target_os = "macos") {
            assert_eq!(m, PlatformModifier::Cmd);
        } else {
            assert_eq!(m, PlatformModifier::Ctrl);
        }
    }

    #[test]
    fn keymap_current_has_save_binding() {
        let km = PlatformKeymap::current();
        let m = platform_modifier();
        assert_eq!(km.save, (m, 's'));
        assert_eq!(km.quit, (m, 'q'));
        assert_eq!(km.copy, (m, 'c'));
        assert_eq!(km.paste, (m, 'v'));
        assert_eq!(km.undo, (m, 'z'));
        assert_eq!(km.find, (m, 'f'));
    }
}
