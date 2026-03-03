use bloom_core::parser::traits::Style;
use ratatui::style::{Color, Modifier, Style as RStyle};

/// 16-slot theme palette per THEMING.md.
pub struct ThemePalette {
    pub foreground: Color,
    pub background: Color,
    pub modeline: Color,
    pub highlight: Color,
    pub critical: Color,
    pub popout: Color,
    pub strong: Color,
    pub salient: Color,
    pub faded: Color,
    pub subtle: Color,
    pub mild: Color,
    pub ultralight: Color,
    pub accent_red: Color,
    pub accent_green: Color,
    pub accent_blue: Color,
    pub accent_yellow: Color,
}

impl ThemePalette {
    /// Bloom Dark — Lambda `dark` variant.
    pub fn bloom_dark() -> Self {
        Self {
            foreground: Color::Rgb(0xEB, 0xE9, 0xE7),
            background: Color::Rgb(0x14, 0x14, 0x14),
            modeline: Color::Rgb(0x1A, 0x19, 0x19),
            highlight: Color::Rgb(0x21, 0x22, 0x28),
            critical: Color::Rgb(0xCF, 0x67, 0x52),
            popout: Color::Rgb(0x7A, 0x9E, 0xFF),
            strong: Color::Rgb(0xF5, 0xF2, 0xF0),
            salient: Color::Rgb(0xF4, 0xBF, 0x4F),
            faded: Color::Rgb(0xA3, 0xA3, 0xA3),
            subtle: Color::Rgb(0x37, 0x37, 0x3E),
            mild: Color::Rgb(0x47, 0x46, 0x48),
            ultralight: Color::Rgb(0x2C, 0x2C, 0x34),
            accent_red: Color::Rgb(0xEC, 0x6A, 0x5E),
            accent_green: Color::Rgb(0x62, 0xC5, 0x54),
            accent_blue: Color::Rgb(0x81, 0xA1, 0xC1),
            accent_yellow: Color::Rgb(0xF2, 0xDA, 0x61),
        }
    }

    /// Map a bloom-core `Style` variant to a ratatui `Style` per THEMING.md face mapping.
    pub fn style_for(&self, style: &Style) -> RStyle {
        match style {
            Style::Normal => RStyle::default().fg(self.foreground),
            Style::Heading { level: 1 } => {
                RStyle::default().fg(self.strong).add_modifier(Modifier::BOLD)
            }
            Style::Heading { level: 2 } => {
                RStyle::default().fg(self.salient).add_modifier(Modifier::BOLD)
            }
            Style::Heading { .. } => {
                RStyle::default().fg(self.foreground).add_modifier(Modifier::BOLD)
            }
            Style::Bold => RStyle::default().fg(self.foreground).add_modifier(Modifier::BOLD),
            Style::Italic => RStyle::default().fg(self.foreground).add_modifier(Modifier::ITALIC),
            Style::Code => RStyle::default().fg(self.foreground).bg(self.subtle),
            Style::CodeBlock => RStyle::default().fg(self.foreground).bg(self.subtle),
            Style::Link => RStyle::default()
                .fg(self.strong)
                .bg(self.modeline)
                .add_modifier(Modifier::UNDERLINED),
            Style::Tag => RStyle::default().fg(self.faded),
            Style::Timestamp => RStyle::default().fg(self.faded),
            Style::BlockId => RStyle::default()
                .fg(self.faded)
                .add_modifier(Modifier::DIM),
            Style::ListMarker => RStyle::default().fg(self.foreground),
            Style::CheckboxUnchecked => RStyle::default().fg(self.accent_yellow),
            Style::CheckboxChecked => RStyle::default()
                .fg(self.accent_green)
                .add_modifier(Modifier::CROSSED_OUT),
            Style::Frontmatter => RStyle::default()
                .fg(self.faded)
                .add_modifier(Modifier::ITALIC),
            Style::BrokenLink => RStyle::default()
                .fg(self.critical)
                .add_modifier(Modifier::CROSSED_OUT),
            Style::SyntaxNoise => RStyle::default()
                .fg(self.faded)
                .add_modifier(Modifier::DIM),
        }
    }

    /// Status bar style for the given mode (active pane).
    pub fn status_bar_style(&self, mode: &str) -> RStyle {
        match mode {
            "INSERT" => RStyle::default().fg(self.background).bg(self.accent_green),
            "VISUAL" => RStyle::default().fg(self.background).bg(self.popout),
            "COMMAND" => RStyle::default().fg(self.background).bg(self.accent_blue),
            // NORMAL and fallback
            _ => RStyle::default().fg(self.foreground).bg(self.modeline),
        }
    }

    /// Status bar style for inactive pane (compact, dim).
    pub fn status_bar_inactive(&self) -> RStyle {
        RStyle::default().fg(self.faded).bg(self.subtle)
    }

    /// Border style.
    pub fn border_style(&self) -> RStyle {
        RStyle::default().fg(self.faded)
    }

    /// Picker surface background.
    pub fn picker_style(&self) -> RStyle {
        RStyle::default().fg(self.foreground).bg(self.subtle)
    }

    /// Picker selected row.
    pub fn picker_selected(&self) -> RStyle {
        RStyle::default().fg(self.foreground).bg(self.mild)
    }

    /// Which-key popup style.
    pub fn which_key_style(&self) -> RStyle {
        RStyle::default().fg(self.foreground).bg(self.subtle)
    }

    /// Faded text (tildes, line numbers).
    pub fn faded_style(&self) -> RStyle {
        RStyle::default().fg(self.faded)
    }

    /// Highlight current line.
    pub fn current_line_style(&self) -> RStyle {
        RStyle::default().bg(self.highlight)
    }

    /// Notification style based on level.
    pub fn notification_style(&self, level: &bloom_core::render::NotificationLevel) -> RStyle {
        match level {
            bloom_core::render::NotificationLevel::Info => {
                RStyle::default().fg(self.foreground).bg(self.subtle)
            }
            bloom_core::render::NotificationLevel::Warning => {
                RStyle::default().fg(self.background).bg(self.accent_yellow)
            }
            bloom_core::render::NotificationLevel::Error => {
                RStyle::default().fg(self.background).bg(self.critical)
            }
        }
    }
}
