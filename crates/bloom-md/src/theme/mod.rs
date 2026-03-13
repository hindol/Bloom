mod palette;
mod resolve;

pub use palette::{
    Rgb, ThemePalette, AURORA, BLOOM_DARK, BLOOM_LIGHT, EMBER, FROST, LICHEN, PAPER, SAKURA,
    SOLARIUM, TWILIGHT, VERDANT,
};
pub use resolve::{resolve, resolve_chrome, resolve_status_bar, Chrome, StyleProps};

/// All built-in theme names, in display order (dark themes first, then light).
pub const THEME_NAMES: &[&str] = &[
    "bloom-dark",
    "bloom-light",
    "aurora",
    "frost",
    "ember",
    "solarium",
    "twilight",
    "sakura",
    "verdant",
    "lichen",
    "paper",
];

/// Look up a built-in palette by name.
pub fn palette_by_name(name: &str) -> Option<&'static ThemePalette> {
    match name {
        "bloom-dark" => Some(&BLOOM_DARK),
        "bloom-light" => Some(&BLOOM_LIGHT),
        "aurora" => Some(&AURORA),
        "frost" => Some(&FROST),
        "ember" => Some(&EMBER),
        "solarium" => Some(&SOLARIUM),
        "twilight" => Some(&TWILIGHT),
        "sakura" => Some(&SAKURA),
        "verdant" => Some(&VERDANT),
        "lichen" => Some(&LICHEN),
        "paper" => Some(&PAPER),
        _ => None,
    }
}

/// Description for each theme (for the picker).
pub fn theme_description(name: &str) -> &'static str {
    match name {
        "bloom-dark" => "warm dark, medium contrast",
        "bloom-light" => "warm white, clean reading",
        "aurora" => "cool Arctic dark, Nordic blue-grey",
        "frost" => "cool ice-blue light, crystalline",
        "ember" => "deep charcoal, warm orange glow",
        "solarium" => "warm golden light, sunlit study",
        "twilight" => "deep violet-blue, edge of night",
        "sakura" => "soft pink light, cherry blossom",
        "verdant" => "deep forest green, dense canopy",
        "lichen" => "sage green light, stone garden",
        "paper" => "pure monochrome light, minimalist",
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
        assert!(palette_by_name("bloom-light").is_some());
        assert!(palette_by_name("nonexistent").is_none());
    }

    #[test]
    fn test_status_bar_modes() {
        let palette = &BLOOM_DARK;
        for mode in &["NORMAL", "INSERT", "VISUAL", "COMMAND", "QUERY"] {
            let props = resolve_status_bar(mode, true, palette);
            assert!(props.fg.is_some());
            assert!(props.bg.is_some());
        }
    }
}
