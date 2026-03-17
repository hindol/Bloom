use bloom_core::render::Style;
use bloom_md::theme::{Rgb, ThemePalette};
use iced::Color;

pub(crate) fn rgb_to_color(rgb: &Rgb) -> Color {
    Color::from_rgb8(rgb.0, rgb.1, rgb.2)
}

pub(crate) fn style_to_color(style: &Style, theme: &ThemePalette) -> Color {
    let rgb = match style {
        Style::Normal => &theme.foreground,
        Style::Heading { level: 1 } => &theme.strong,
        Style::Heading { level: 2 } => &theme.salient,
        Style::Heading { .. } => &theme.foreground,
        Style::Bold | Style::Italic => &theme.foreground,
        Style::Code | Style::CodeBlock => &theme.foreground,
        Style::LinkText => &theme.strong,
        Style::LinkChrome | Style::SyntaxNoise => &theme.faded,
        Style::Tag | Style::TimestampKeyword | Style::TimestampParens => &theme.faded,
        Style::TimestampDate => &theme.foreground,
        Style::TimestampOverdue => &theme.accent_red,
        Style::BlockId | Style::BlockIdCaret => &theme.faded,
        Style::ListMarker => &theme.foreground,
        Style::CheckboxUnchecked => &theme.accent_yellow,
        Style::CheckboxChecked => &theme.accent_green,
        Style::CheckedTaskText => &theme.faded,
        Style::Blockquote => &theme.foreground,
        Style::BlockquoteMarker | Style::TablePipe | Style::TableAlignmentRow => &theme.faded,
        Style::Frontmatter
        | Style::FrontmatterKey
        | Style::FrontmatterId
        | Style::FrontmatterDate
        | Style::FrontmatterTags => &theme.faded,
        Style::FrontmatterTitle => &theme.foreground,
        Style::BrokenLink => &theme.critical,
        Style::SearchMatch | Style::SearchMatchCurrent => &theme.foreground,
        Style::DiffAdded => &theme.accent_green,
        Style::DiffRemoved => &theme.accent_red,
    };

    rgb_to_color(rgb)
}

/// Return an optional background color for styles that need a bg wash.
pub(crate) fn style_to_bg(style: &Style, theme: &ThemePalette) -> Option<Color> {
    match style {
        Style::Code | Style::CodeBlock => Some(rgb_to_color(&theme.subtle)),
        Style::LinkText => Some(rgb_to_color(&theme.modeline)),
        Style::SearchMatch => Some(rgb_to_color(&theme.ultralight)),
        Style::SearchMatchCurrent => Some(rgb_to_color(&theme.popout)),
        _ => None,
    }
}
