mod palette;
mod resolve;

pub use palette::{
    Rgb, ThemePalette, AGED_PAPER, BLOOM_DARK, BLOOM_DARK_FADED, BLOOM_LIGHT, BLOOM_LIGHT_FADED,
    DRIFTWOOD, EMBER, INK, LICHEN, MOSS, NEWSPRINT, PARCHMENT, SLATE, SOLARIUM, TWILIGHT,
};
pub use resolve::{resolve, resolve_chrome, resolve_status_bar, Chrome, StyleProps};

/// All built-in theme names, in display order.
pub const THEME_NAMES: &[&str] = &[
    "bloom-dark",
    "bloom-dark-faded",
    "bloom-light",
    "bloom-light-faded",
    "parchment",
    "newsprint",
    "aged-paper",
    "moss",
    "slate",
    "solarium",
    "ink",
    "driftwood",
    "twilight",
    "lichen",
    "ember",
];

/// Look up a built-in palette by name.
pub fn palette_by_name(name: &str) -> Option<&'static ThemePalette> {
    match name {
        "bloom-dark" => Some(&BLOOM_DARK),
        "bloom-dark-faded" => Some(&BLOOM_DARK_FADED),
        "bloom-light" => Some(&BLOOM_LIGHT),
        "bloom-light-faded" => Some(&BLOOM_LIGHT_FADED),
        "parchment" => Some(&PARCHMENT),
        "newsprint" => Some(&NEWSPRINT),
        "aged-paper" => Some(&AGED_PAPER),
        "moss" => Some(&MOSS),
        "slate" => Some(&SLATE),
        "solarium" => Some(&SOLARIUM),
        "ink" => Some(&INK),
        "driftwood" => Some(&DRIFTWOOD),
        "twilight" => Some(&TWILIGHT),
        "lichen" => Some(&LICHEN),
        "ember" => Some(&EMBER),
        _ => None,
    }
}

/// Description for each theme (for the picker).
pub fn theme_description(name: &str) -> &'static str {
    match name {
        "bloom-dark" => "warm dark, medium contrast",
        "bloom-dark-faded" => "softer, Nord-influenced",
        "bloom-light" => "warm white, soft contrast",
        "bloom-light-faded" => "cool, muted light",
        "parchment" => "cream paper, sepia ink",
        "newsprint" => "cool grey paper, charcoal type",
        "aged-paper" => "dark brown, candlelight warmth",
        "moss" => "dark green-grey, forest floor",
        "slate" => "cool blue-grey, architectural",
        "solarium" => "warm solarized cream",
        "ink" => "pure monochrome, white on black",
        "driftwood" => "warm sand, bleached wood",
        "twilight" => "deep blue-purple, edge of night",
        "lichen" => "pale green-grey stone",
        "ember" => "charcoal with warm orange glow",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::traits::Style;

    #[test]
    fn test_all_builtin_palettes_resolve() {
        for name in THEME_NAMES {
            let palette = palette_by_name(name).unwrap();
            // Every Style variant must resolve without panic
            let styles = [
                Style::Normal,
                Style::Heading { level: 1 },
                Style::Heading { level: 2 },
                Style::Heading { level: 3 },
                Style::Bold,
                Style::Italic,
                Style::Code,
                Style::CodeBlock,
                Style::LinkText,
                Style::LinkChrome,
                Style::Tag,
                Style::TimestampKeyword,
                Style::TimestampDate,
                Style::TimestampOverdue,
                Style::TimestampParens,
                Style::BlockId,
                Style::BlockIdCaret,
                Style::ListMarker,
                Style::CheckboxUnchecked,
                Style::CheckboxChecked,
                Style::CheckedTaskText,
                Style::Blockquote,
                Style::BlockquoteMarker,
                Style::TablePipe,
                Style::TableAlignmentRow,
                Style::Frontmatter,
                Style::FrontmatterKey,
                Style::FrontmatterTitle,
                Style::FrontmatterId,
                Style::FrontmatterDate,
                Style::FrontmatterTags,
                Style::BrokenLink,
                Style::SyntaxNoise,
            ];
            for style in &styles {
                let props = resolve(style, palette);
                assert!(props.fg.is_some(), "{name}: {style:?} should have fg");
            }
        }
    }

    #[test]
    fn test_palette_lookup() {
        assert!(palette_by_name("bloom-dark").is_some());
        assert!(palette_by_name("bloom-light-faded").is_some());
        assert!(palette_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_status_bar_modes() {
        let palette = &BLOOM_DARK;
        for mode in &["NORMAL", "INSERT", "VISUAL", "COMMAND"] {
            let props = resolve_status_bar(mode, true, palette);
            assert!(props.fg.is_some());
            assert!(props.bg.is_some());
        }
    }
}
