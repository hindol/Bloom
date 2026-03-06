use bloom_core::parser::traits::Style;
use bloom_core::render::NotificationLevel;
use bloom_core::theme::{self, Chrome, Rgb, StyleProps, ThemePalette};
use ratatui::style::{Color, Modifier, Style as RStyle};

/// Convert core `Rgb` to ratatui `Color`.
fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.0, c.1, c.2)
}

/// Convert core `StyleProps` to ratatui `Style`.
pub fn to_rstyle(props: &StyleProps) -> RStyle {
    let mut s = RStyle::default();
    if let Some(fg) = props.fg {
        s = s.fg(rgb(fg));
    }
    if let Some(bg) = props.bg {
        s = s.bg(rgb(bg));
    }
    if props.bold {
        s = s.add_modifier(Modifier::BOLD);
    }
    if props.italic {
        s = s.add_modifier(Modifier::ITALIC);
    }
    if props.underline {
        s = s.add_modifier(Modifier::UNDERLINED);
    }
    if props.dim {
        s = s.add_modifier(Modifier::DIM);
    }
    if props.strikethrough {
        s = s.add_modifier(Modifier::CROSSED_OUT);
    }
    s
}

/// Thin wrapper around a core `ThemePalette` that produces ratatui styles.
pub struct TuiTheme<'a> {
    pub palette: &'a ThemePalette,
}

impl<'a> TuiTheme<'a> {
    pub fn new(palette: &'a ThemePalette) -> Self {
        Self { palette }
    }

    /// Resolve a content `Style` to ratatui `Style`.
    /// Every returned style carries an explicit bg (defaults to palette background).
    pub fn style_for(&self, style: &Style) -> RStyle {
        if matches!(style, Style::Heading { level: 1 }) {
            let mut props = theme::resolve(style, self.palette);
            props.fg = Some(self.palette.salient);
            return to_rstyle(&props);
        }
        to_rstyle(&theme::resolve(style, self.palette))
    }

    /// Status bar style.
    pub fn status_bar_style(&self, mode: &str, active: bool) -> RStyle {
        to_rstyle(&theme::resolve_status_bar(mode, active, self.palette))
    }

    /// Background colour for filling.
    pub fn background(&self) -> Color {
        rgb(self.palette.background)
    }

    /// Salient colour (for which-key group labels, headings in wizard, etc.).
    pub fn salient(&self) -> Color {
        rgb(self.palette.salient)
    }

    /// Strong colour.
    pub fn strong(&self) -> Color {
        rgb(self.palette.strong)
    }

    /// Foreground colour.
    pub fn foreground(&self) -> Color {
        rgb(self.palette.foreground)
    }

    /// Critical colour.
    pub fn critical(&self) -> Color {
        rgb(self.palette.critical)
    }

    /// Accent green.
    pub fn accent_green(&self) -> Color {
        rgb(self.palette.accent_green)
    }

    /// Accent yellow.
    pub fn accent_yellow(&self) -> Color {
        rgb(self.palette.accent_yellow)
    }

    /// Modeline colour.
    pub fn modeline(&self) -> Color {
        rgb(self.palette.modeline)
    }

    /// Mild colour.
    pub fn mild(&self) -> Color {
        rgb(self.palette.mild)
    }

    /// Faded colour.
    pub fn faded(&self) -> Color {
        rgb(self.palette.faded)
    }

    /// Accent red.
    pub fn accent_red(&self) -> Color {
        rgb(self.palette.accent_red)
    }

    /// Highlight colour.
    pub fn highlight(&self) -> Color {
        rgb(self.palette.highlight)
    }

    pub fn faded_style(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::Faded, self.palette))
    }

    pub fn border_style(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::WindowBorder, self.palette))
    }

    pub fn picker_style(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::PickerSurface, self.palette))
    }

    pub fn picker_selected(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::PickerSelected, self.palette))
    }

    pub fn which_key_style(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::WhichKey, self.palette))
    }

    pub fn current_line_style(&self) -> RStyle {
        to_rstyle(&theme::resolve_chrome(Chrome::CurrentLine, self.palette))
    }

    pub fn notification_style(&self, level: &NotificationLevel) -> RStyle {
        let chrome = match level {
            NotificationLevel::Info => Chrome::NotificationInfo,
            NotificationLevel::Warning => Chrome::NotificationWarning,
            NotificationLevel::Error => Chrome::NotificationError,
        };
        to_rstyle(&theme::resolve_chrome(chrome, self.palette))
    }
}
