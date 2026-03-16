use super::palette::{Rgb, ThemePalette};
use crate::parser::traits::Style;

/// Resolved style properties — UI-agnostic, ready for frontend conversion.
/// Both `fg` and `bg` are always set — no cell should inherit terminal defaults.
#[derive(Debug, Clone)]
pub struct StyleProps {
    pub fg: Rgb,
    pub bg: Rgb,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub strikethrough: bool,
}

impl StyleProps {
    /// Base style: palette foreground on palette background, no decorations.
    fn base(p: &ThemePalette) -> Self {
        Self {
            fg: p.foreground,
            bg: p.background,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
            strikethrough: false,
        }
    }
}

/// Resolve a `Style` variant to `StyleProps` using the face mapping table from THEMING.md.
pub fn resolve(style: &Style, p: &ThemePalette) -> StyleProps {
    let base = StyleProps::base(p);
    match style {
        Style::Normal => base,
        Style::Heading { level: 1 } => StyleProps {
            fg: p.strong,
            bold: true,
            ..base
        },
        Style::Heading { level: 2 } => StyleProps {
            fg: p.salient,
            bold: true,
            ..base
        },
        Style::Heading { .. } => StyleProps {
            bold: true,
            ..base
        },
        Style::Bold => StyleProps {
            bold: true,
            ..base
        },
        Style::Italic => StyleProps {
            italic: true,
            ..base
        },
        Style::Code => StyleProps {
            bg: p.subtle,
            ..base
        },
        Style::CodeBlock => StyleProps {
            bg: p.subtle,
            ..base
        },
        Style::LinkText => StyleProps {
            fg: p.strong,
            bg: p.modeline,
            underline: true,
            ..base
        },
        Style::LinkChrome => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::Tag => StyleProps {
            fg: p.faded,
            ..base
        },
        Style::TimestampKeyword => StyleProps {
            fg: p.faded,
            ..base
        },
        Style::TimestampDate => base,
        Style::TimestampOverdue => StyleProps {
            fg: p.accent_red,
            ..base
        },
        Style::TimestampParens => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::BlockId => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::BlockIdCaret => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::ListMarker => base,
        Style::CheckboxUnchecked => StyleProps {
            fg: p.accent_yellow,
            ..base
        },
        Style::CheckboxChecked => StyleProps {
            fg: p.accent_green,
            strikethrough: true,
            ..base
        },
        Style::CheckedTaskText => StyleProps {
            fg: p.faded,
            strikethrough: true,
            ..base
        },
        Style::Blockquote => StyleProps {
            italic: true,
            ..base
        },
        Style::BlockquoteMarker => StyleProps {
            fg: p.faded,
            ..base
        },
        Style::TablePipe => StyleProps {
            fg: p.faded,
            ..base
        },
        Style::TableAlignmentRow => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::Frontmatter => StyleProps {
            fg: p.faded,
            italic: true,
            ..base
        },
        Style::FrontmatterKey => StyleProps {
            fg: p.faded,
            italic: true,
            ..base
        },
        Style::FrontmatterTitle => StyleProps {
            bold: true,
            italic: true,
            ..base
        },
        Style::FrontmatterId => StyleProps {
            fg: p.faded,
            italic: true,
            dim: true,
            ..base
        },
        Style::FrontmatterDate => StyleProps {
            fg: p.faded,
            italic: true,
            ..base
        },
        Style::FrontmatterTags => StyleProps {
            fg: p.faded,
            ..base
        },
        Style::BrokenLink => StyleProps {
            fg: p.critical,
            strikethrough: true,
            ..base
        },
        Style::SyntaxNoise => StyleProps {
            fg: p.faded,
            dim: true,
            ..base
        },
        Style::SearchMatch => StyleProps {
            bg: p.mild,
            ..base
        },
        Style::SearchMatchCurrent => StyleProps {
            bg: p.mild,
            bold: true,
            underline: true,
            ..base
        },
        Style::DiffAdded => StyleProps {
            fg: p.accent_green,
            ..base
        },
        Style::DiffRemoved => StyleProps {
            fg: p.accent_red,
            strikethrough: true,
            ..base
        },
    }
}

/// Resolve status bar style per UI Chrome Mapping in THEMING.md.
pub fn resolve_status_bar(mode: &str, active: bool, p: &ThemePalette) -> StyleProps {
    let base = StyleProps::base(p);
    if !active {
        return StyleProps {
            fg: p.faded,
            bg: p.subtle,
            ..base
        };
    }
    match mode {
        "INSERT" => StyleProps {
            fg: p.background,
            bg: p.accent_green,
            ..base
        },
        "VISUAL" => StyleProps {
            fg: p.background,
            bg: p.popout,
            ..base
        },
        "COMMAND" => StyleProps {
            fg: p.background,
            bg: p.accent_blue,
            ..base
        },
        "QUERY" => StyleProps {
            fg: p.background,
            bg: p.salient,
            ..base
        },
        // Temporal modes share accent_yellow (per WINDOW_LAYOUTS.md)
        "JRNL" | "HIST" | "HISTORY" | "DAY" => StyleProps {
            fg: p.background,
            bg: p.accent_yellow,
            ..base
        },
        _ => StyleProps {
            bg: p.highlight,
            ..base
        },
    }
}

/// UI chrome element identifiers.
pub enum Chrome {
    PickerSurface,
    PickerSelected,
    PickerBorder,
    WhichKey,
    CurrentLine,
    WindowBorder,
    Faded,
    NotificationInfo,
    NotificationWarning,
    NotificationError,
}

/// Resolve UI chrome element styles per THEMING.md.
pub fn resolve_chrome(element: Chrome, p: &ThemePalette) -> StyleProps {
    let base = StyleProps::base(p);
    match element {
        Chrome::PickerSurface => StyleProps {
            bg: p.subtle,
            ..base
        },
        Chrome::PickerSelected => StyleProps {
            bg: p.mild,
            ..base
        },
        Chrome::PickerBorder | Chrome::WindowBorder | Chrome::Faded => StyleProps {
            fg: p.faded,
            ..base
        },
        Chrome::WhichKey => base,
        Chrome::CurrentLine => StyleProps {
            bg: p.highlight,
            ..base
        },
        Chrome::NotificationInfo => StyleProps {
            bg: p.subtle,
            ..base
        },
        Chrome::NotificationWarning => StyleProps {
            fg: p.background,
            bg: p.accent_yellow,
            ..base
        },
        Chrome::NotificationError => StyleProps {
            fg: p.background,
            bg: p.critical,
            ..base
        },
    }
}
