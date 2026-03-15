use super::palette::{Rgb, ThemePalette};
use crate::parser::traits::Style;

/// Resolved style properties — UI-agnostic, ready for frontend conversion.
#[derive(Debug, Clone, Default)]
pub struct StyleProps {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub strikethrough: bool,
}

/// Resolve a `Style` variant to `StyleProps` using the face mapping table from THEMING.md.
pub fn resolve(style: &Style, p: &ThemePalette) -> StyleProps {
    match style {
        Style::Normal => StyleProps {
            fg: Some(p.foreground),
            ..Default::default()
        },
        Style::Heading { level: 1 } => StyleProps {
            fg: Some(p.strong),
            bold: true,
            ..Default::default()
        },
        Style::Heading { level: 2 } => StyleProps {
            fg: Some(p.salient),
            bold: true,
            ..Default::default()
        },
        Style::Heading { .. } => StyleProps {
            fg: Some(p.foreground),
            bold: true,
            ..Default::default()
        },
        Style::Bold => StyleProps {
            fg: Some(p.foreground),
            bold: true,
            ..Default::default()
        },
        Style::Italic => StyleProps {
            fg: Some(p.foreground),
            italic: true,
            ..Default::default()
        },
        Style::Code => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.subtle),
            ..Default::default()
        },
        Style::CodeBlock => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.subtle),
            ..Default::default()
        },
        Style::LinkText => StyleProps {
            fg: Some(p.strong),
            bg: Some(p.modeline),
            underline: true,
            ..Default::default()
        },
        Style::LinkChrome => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::Tag => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Style::TimestampKeyword => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Style::TimestampDate => StyleProps {
            fg: Some(p.foreground),
            ..Default::default()
        },
        Style::TimestampOverdue => StyleProps {
            fg: Some(p.accent_red),
            ..Default::default()
        },
        Style::TimestampParens => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::BlockId => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::BlockIdCaret => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::ListMarker => StyleProps {
            fg: Some(p.foreground),
            ..Default::default()
        },
        Style::CheckboxUnchecked => StyleProps {
            fg: Some(p.accent_yellow),
            ..Default::default()
        },
        Style::CheckboxChecked => StyleProps {
            fg: Some(p.accent_green),
            strikethrough: true,
            ..Default::default()
        },
        Style::CheckedTaskText => StyleProps {
            fg: Some(p.faded),
            strikethrough: true,
            ..Default::default()
        },
        Style::Blockquote => StyleProps {
            fg: Some(p.foreground),
            italic: true,
            ..Default::default()
        },
        Style::BlockquoteMarker => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Style::TablePipe => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Style::TableAlignmentRow => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::Frontmatter => StyleProps {
            fg: Some(p.faded),
            italic: true,
            ..Default::default()
        },
        Style::FrontmatterKey => StyleProps {
            fg: Some(p.faded),
            italic: true,
            ..Default::default()
        },
        Style::FrontmatterTitle => StyleProps {
            fg: Some(p.foreground),
            bold: true,
            italic: true,
            ..Default::default()
        },
        Style::FrontmatterId => StyleProps {
            fg: Some(p.faded),
            italic: true,
            dim: true,
            ..Default::default()
        },
        Style::FrontmatterDate => StyleProps {
            fg: Some(p.faded),
            italic: true,
            ..Default::default()
        },
        Style::FrontmatterTags => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Style::BrokenLink => StyleProps {
            fg: Some(p.critical),
            strikethrough: true,
            ..Default::default()
        },
        Style::SyntaxNoise => StyleProps {
            fg: Some(p.faded),
            dim: true,
            ..Default::default()
        },
        Style::SearchMatch => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.mild),
            ..Default::default()
        },
        Style::SearchMatchCurrent => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.mild),
            bold: true,
            underline: true,
            ..Default::default()
        },
    }
}

/// Resolve status bar style per UI Chrome Mapping in THEMING.md.
pub fn resolve_status_bar(mode: &str, active: bool, p: &ThemePalette) -> StyleProps {
    if !active {
        return StyleProps {
            fg: Some(p.faded),
            bg: Some(p.subtle),
            ..Default::default()
        };
    }
    match mode {
        "INSERT" => StyleProps {
            fg: Some(p.background),
            bg: Some(p.accent_green),
            ..Default::default()
        },
        "VISUAL" => StyleProps {
            fg: Some(p.background),
            bg: Some(p.popout),
            ..Default::default()
        },
        "COMMAND" => StyleProps {
            fg: Some(p.background),
            bg: Some(p.accent_blue),
            ..Default::default()
        },
        "QUERY" => StyleProps {
            fg: Some(p.background),
            bg: Some(p.salient),
            ..Default::default()
        },
        // Temporal modes share accent_yellow (per WINDOW_LAYOUTS.md)
        "JRNL" | "HIST" | "HISTORY" | "DAY" => StyleProps {
            fg: Some(p.background),
            bg: Some(p.accent_yellow),
            ..Default::default()
        },
        _ => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.highlight),
            ..Default::default()
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
    match element {
        Chrome::PickerSurface => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.subtle),
            ..Default::default()
        },
        Chrome::PickerSelected => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.mild),
            ..Default::default()
        },
        Chrome::PickerBorder | Chrome::WindowBorder | Chrome::Faded => StyleProps {
            fg: Some(p.faded),
            ..Default::default()
        },
        Chrome::WhichKey => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.background),
            ..Default::default()
        },
        Chrome::CurrentLine => StyleProps {
            bg: Some(p.highlight),
            ..Default::default()
        },
        Chrome::NotificationInfo => StyleProps {
            fg: Some(p.foreground),
            bg: Some(p.subtle),
            ..Default::default()
        },
        Chrome::NotificationWarning => StyleProps {
            fg: Some(p.background),
            bg: Some(p.accent_yellow),
            ..Default::default()
        },
        Chrome::NotificationError => StyleProps {
            fg: Some(p.background),
            bg: Some(p.critical),
            ..Default::default()
        },
    }
}
