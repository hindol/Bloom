use bloom_core::render::Style;
use bloom_md::theme::{Rgb, ThemePalette};
use iced::Color;

pub(crate) fn rgb_to_color(rgb: &Rgb) -> Color {
    Color::from_rgb8(rgb.0, rgb.1, rgb.2)
}

pub(crate) fn style_to_color(style: &Style, theme: &ThemePalette) -> Color {
    // Tier 3 "dim" styles: faded blended 40% toward background (THEMING.md: faded + dim).
    let dimmed = || rgb_to_color(&theme.faded.blend(theme.background, 0.4));

    match style {
        Style::Normal => rgb_to_color(&theme.foreground),
        Style::Heading { level: 1 } => rgb_to_color(&theme.strong),
        Style::Heading { level: 2 } => rgb_to_color(&theme.salient),
        Style::Heading { .. } => rgb_to_color(&theme.foreground),
        Style::Bold | Style::Italic => rgb_to_color(&theme.foreground),
        Style::Code | Style::CodeBlock => rgb_to_color(&theme.foreground),
        Style::LinkText => rgb_to_color(&theme.strong),
        // Tier 3 — noise: dim
        Style::LinkChrome | Style::SyntaxNoise => dimmed(),
        Style::BlockId | Style::BlockIdCaret => dimmed(),
        Style::FrontmatterId => dimmed(),
        Style::TimestampParens => dimmed(),
        Style::TableAlignmentRow => dimmed(),
        // Tier 2 — contextual: faded (not dimmed)
        Style::Tag | Style::TimestampKeyword => rgb_to_color(&theme.faded),
        Style::TimestampDate => rgb_to_color(&theme.foreground),
        Style::TimestampOverdue => rgb_to_color(&theme.accent_red),
        Style::ListMarker => rgb_to_color(&theme.foreground),
        Style::CheckboxUnchecked => rgb_to_color(&theme.accent_yellow),
        Style::CheckboxChecked => rgb_to_color(&theme.accent_green),
        Style::CheckedTaskText => rgb_to_color(&theme.faded),
        Style::Blockquote => rgb_to_color(&theme.foreground),
        Style::BlockquoteMarker | Style::TablePipe => rgb_to_color(&theme.faded),
        Style::Frontmatter
        | Style::FrontmatterKey
        | Style::FrontmatterDate
        | Style::FrontmatterTags => rgb_to_color(&theme.faded),
        Style::FrontmatterTitle => rgb_to_color(&theme.foreground),
        Style::BrokenLink => rgb_to_color(&theme.critical),
        Style::SearchMatch | Style::SearchMatchCurrent => rgb_to_color(&theme.foreground),
        Style::DiffAdded => rgb_to_color(&theme.diff_added),
        Style::DiffRemoved => rgb_to_color(&theme.diff_removed),
    }
}

/// Compute a semi-transparent overlay color that, when composited over the
/// theme background, approximates the theme highlight color.  Used by the
/// CursorCanvas layer so the line highlight can be drawn *above* text content
/// with minimal text-color distortion (~1-2%).
///
/// Dark themes (highlight brighter than bg): white overlay with low alpha.
/// Light themes (highlight darker than bg): black overlay with low alpha.
pub(crate) fn highlight_overlay_color(theme: &ThemePalette) -> Color {
    let bg = &theme.background;
    let hl = &theme.highlight;
    let bg_lum = (bg.0 as f32 + bg.1 as f32 + bg.2 as f32) / 3.0;
    let hl_lum = (hl.0 as f32 + hl.1 as f32 + hl.2 as f32) / 3.0;

    if hl_lum >= bg_lum {
        // Highlight is lighter → white overlay
        let denom = 255.0 - bg_lum;
        let alpha = if denom > 0.0 {
            ((hl_lum - bg_lum) / denom).clamp(0.01, 0.15)
        } else {
            0.05
        };
        Color {
            r: 1.0,
            g: 1.0,
            b: 1.0,
            a: alpha,
        }
    } else {
        // Highlight is darker → black overlay
        let alpha = if bg_lum > 0.0 {
            ((bg_lum - hl_lum) / bg_lum).clamp(0.01, 0.15)
        } else {
            0.05
        };
        Color {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: alpha,
        }
    }
}
pub(crate) fn style_to_bg(style: &Style, theme: &ThemePalette) -> Option<Color> {
    match style {
        Style::Code | Style::CodeBlock => Some(rgb_to_color(&theme.subtle)),
        Style::LinkText => Some(rgb_to_color(&theme.modeline)),
        Style::SearchMatch => Some(rgb_to_color(&theme.ultralight)),
        Style::SearchMatchCurrent => Some(rgb_to_color(&theme.popout)),
        _ => None,
    }
}
