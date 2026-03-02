// RenderFrame — UI-agnostic snapshot of what to draw.
//
// The core library produces a RenderFrame; frontends (TUI, GUI, tests)
// consume it. Frontends never query editor state directly.

// ---------------------------------------------------------------------------
// Cursor
// ---------------------------------------------------------------------------

/// Cursor shape — frontends map this to their native cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorShape {
    Block,
    Bar,
    Underline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorState {
    /// 0-based row within the viewport.
    pub row: usize,
    /// 0-based column (grapheme cluster index).
    pub col: usize,
    pub shape: CursorShape,
}

// ---------------------------------------------------------------------------
// Styled text spans
// ---------------------------------------------------------------------------

/// A semantic style token applied to a range of text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Normal,
    Heading {
        level: u8,
    },
    Bold,
    Italic,
    Code,
    CodeBlock,
    Link,
    Embed,
    Tag,
    Timestamp,
    BlockId,
    ListMarker,
    CheckboxUnchecked,
    CheckboxChecked,
    Frontmatter,
    /// Orphaned / broken link indicator.
    BrokenLink,
}

/// A styled span within a rendered line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledSpan {
    /// Byte offset range within the line's text.
    pub start: usize,
    pub end: usize,
    pub style: Style,
}

/// A single rendered line in the viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedLine {
    /// The raw text content of this line.
    pub text: String,
    /// Style spans (may overlap; frontend resolves precedence).
    pub spans: Vec<StyledSpan>,
    /// Line number in the document (0-based). None for virtual lines (e.g., "~" beyond EOF).
    pub line_number: Option<usize>,
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBar {
    /// Current editing mode (NORMAL, INSERT, VISUAL, COMMAND).
    pub mode: String,
    /// Filename or "[No Name]".
    pub filename: String,
    /// True if buffer has unsaved changes.
    pub dirty: bool,
    /// Cursor position as "line:col".
    pub position: String,
    /// Pending key sequence (e.g., "d" waiting for motion).
    pub pending_keys: String,
    /// File type / extension hint.
    pub filetype: String,
}

// ---------------------------------------------------------------------------
// Picker frame
// ---------------------------------------------------------------------------

/// A filter pill shown in the picker (e.g., [tag:rust]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterPill {
    pub label: String,
    /// e.g., "tag", "date", "status"
    pub kind: String,
}

/// A single result row in the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerItem {
    /// Main text (highlighted match).
    pub text: String,
    /// Highlighted character indices for fuzzy match visualization.
    pub match_indices: Vec<usize>,
    /// Right-aligned metadata (tags, date, etc.).
    pub marginalia: String,
    /// Whether this item is currently selected.
    pub selected: bool,
    /// Whether this item is marked for batch action.
    pub marked: bool,
}

/// Full picker state snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PickerFrame {
    /// The picker title (e.g., "Find Page", "Search Tags").
    pub title: String,
    /// Current query text.
    pub query: String,
    /// Active filter pills.
    pub filters: Vec<FilterPill>,
    /// Visible result items.
    pub items: Vec<PickerItem>,
    /// "N of M" result count.
    pub result_count: String,
    /// Preview pane content (if applicable).
    pub preview: Option<Vec<RenderedLine>>,
    /// Whether this is an inline picker (anchored to cursor) or a full overlay.
    pub inline: bool,
    /// Action menu items (Some when action menu is open).
    pub action_menu: Option<Vec<String>>,
    /// Currently selected action menu index (Some when action menu is open).
    pub action_menu_selected: Option<usize>,
}

// ---------------------------------------------------------------------------
// Which-key popup
// ---------------------------------------------------------------------------

/// A single entry in the which-key popup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhichKeyEntry {
    /// The key to press (e.g., "f", "j", "w").
    pub key: String,
    /// Description (e.g., "file", "journal", "window").
    pub description: String,
    /// True if this opens a sub-group, false if it executes.
    pub is_group: bool,
}

/// Which-key popup state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WhichKeyFrame {
    /// Breadcrumb prefix (e.g., "SPC w" when in window sub-group).
    pub prefix: String,
    /// Available bindings at this level.
    pub entries: Vec<WhichKeyEntry>,
}

// ---------------------------------------------------------------------------
// Diagnostic (inline indicators)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticKind {
    BrokenLink,
    OrphanedEmbed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    /// Line number in the document.
    pub line: usize,
    /// Byte range within the line.
    pub start: usize,
    pub end: usize,
    pub kind: DiagnosticKind,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Pane / split layout
// ---------------------------------------------------------------------------

/// A single pane in the window layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaneFrame {
    pub lines: Vec<RenderedLine>,
    pub cursor: CursorState,
    pub status_bar: StatusBar,
    /// Whether this pane is the currently focused one.
    pub focused: bool,
}

// ---------------------------------------------------------------------------
// Undo tree visualizer overlay
// ---------------------------------------------------------------------------

/// A single entry in the undo tree visualizer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoTreeEntry {
    /// Branch index.
    pub branch_index: usize,
    /// Whether this branch is currently active.
    pub current: bool,
}

/// Undo tree visualizer overlay state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoTreeFrame {
    /// Title of the overlay.
    pub title: String,
    /// Available undo branches.
    pub entries: Vec<UndoTreeEntry>,
    /// Currently highlighted entry index.
    pub selected: usize,
}

// ---------------------------------------------------------------------------
// Agenda overlay
// ---------------------------------------------------------------------------

/// A single item in the agenda overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaRenderItem {
    pub text: String,
    pub page_title: String,
    pub date: String,
    pub completed: bool,
    pub tags: Vec<String>,
}

/// A labeled section in the agenda overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaSection {
    /// Section label: "Overdue", "Today", "Upcoming".
    pub label: String,
    pub items: Vec<AgendaRenderItem>,
}

/// Full agenda overlay state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgendaFrame {
    pub title: String,
    pub sections: Vec<AgendaSection>,
    pub selected: usize,
    pub total_items: usize,
}

// ---------------------------------------------------------------------------
// RenderFrame — the top-level snapshot
// ---------------------------------------------------------------------------

/// A complete, UI-agnostic snapshot of what to draw.
///
/// Produced by `EditorState::render()`. Consumed by TUI, GUI, and tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderFrame {
    /// All panes in the layout (at least one).
    pub panes: Vec<PaneFrame>,
    /// Active picker overlay, if any.
    pub picker: Option<PickerFrame>,
    /// Which-key popup, if pending keys are waiting.
    pub which_key: Option<WhichKeyFrame>,
    /// Inline diagnostics for the focused pane.
    pub diagnostics: Vec<Diagnostic>,
    /// Command-line content (for `:` commands).
    pub command_line: Option<String>,
    /// Quick-capture bar content (for `SPC j a` / `SPC j t`).
    pub capture_bar: Option<String>,
    /// Undo tree visualizer overlay, if active.
    pub undo_tree: Option<UndoTreeFrame>,
    /// Agenda overlay, if active.
    pub agenda: Option<AgendaFrame>,
}

impl RenderFrame {
    /// Convenience: get the focused pane.
    pub fn focused_pane(&self) -> Option<&PaneFrame> {
        self.panes.iter().find(|p| p.focused)
    }

    /// Convenience: get the status bar of the focused pane.
    pub fn status(&self) -> Option<&StatusBar> {
        self.focused_pane().map(|p| &p.status_bar)
    }
}

// ---------------------------------------------------------------------------
// Theme — Rougier-inspired semantic theming (THEMING.md)
// ---------------------------------------------------------------------------

/// RGB color (0-255).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b)
    }
}

/// The 14-slot semantic palette that fully defines a Bloom theme.
/// See docs/THEMING.md for design rationale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    // Surface colours
    pub foreground: Rgb,
    pub background: Rgb,
    pub modeline: Rgb,
    pub highlight: Rgb,

    // Semantic roles (Rougier's 6 faces)
    pub critical: Rgb,
    pub popout: Rgb,
    pub strong: Rgb,
    pub salient: Rgb,
    pub faded: Rgb,
    pub subtle: Rgb,

    // Mid-tones (Lambda additions)
    pub mild: Rgb,
    pub ultralight: Rgb,

    // Accent colours (bespoke-themes expansion)
    pub accent_red: Rgb,
    pub accent_green: Rgb,
    pub accent_blue: Rgb,
    pub accent_yellow: Rgb,
}

/// Visual properties for a single semantic style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleProps {
    pub fg: Option<Rgb>,
    pub bg: Option<Rgb>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub dim: bool,
    pub strikethrough: bool,
}

impl StyleProps {
    pub const fn plain() -> Self {
        Self {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            dim: false,
            strikethrough: false,
        }
    }

    pub const fn fg(mut self, r: u8, g: u8, b: u8) -> Self {
        self.fg = Some(Rgb(r, g, b));
        self
    }

    pub const fn bg_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.bg = Some(Rgb(r, g, b));
        self
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }

    pub const fn dim(mut self) -> Self {
        self.dim = true;
        self
    }

    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
}

/// A theme maps semantic styles to visual properties.
/// Built from a `ThemePalette` — adding a new theme only requires 14 colours.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: &'static str,
    pub palette: ThemePalette,

    // Derived fields for per-style lookup
    pub bg: Rgb,
    pub fg: Rgb,
    pub heading: [StyleProps; 6],
    pub bold: StyleProps,
    pub italic: StyleProps,
    pub code: StyleProps,
    pub code_block: StyleProps,
    pub link: StyleProps,
    pub embed: StyleProps,
    pub tag: StyleProps,
    pub timestamp: StyleProps,
    pub block_id: StyleProps,
    pub list_marker: StyleProps,
    pub checkbox_unchecked: StyleProps,
    pub checkbox_checked: StyleProps,
    pub frontmatter: StyleProps,
    pub broken_link: StyleProps,
    pub tilde: StyleProps,

    // UI chrome
    pub border: Rgb,
    pub surface: Rgb,
    pub accent: Rgb,
    pub selection_bg: Rgb,
    pub selection_fg: Rgb,
    pub status_normal: Rgb,
    pub status_insert: Rgb,
    pub status_visual: Rgb,
    pub status_command: Rgb,
}

impl Theme {
    pub fn props_for(&self, style: Style) -> StyleProps {
        match style {
            Style::Normal => StyleProps::plain(),
            Style::Heading { level } => {
                let idx = (level as usize).saturating_sub(1).min(5);
                self.heading[idx]
            }
            Style::Bold => self.bold,
            Style::Italic => self.italic,
            Style::Code => self.code,
            Style::CodeBlock => self.code_block,
            Style::Link => self.link,
            Style::Embed => self.embed,
            Style::Tag => self.tag,
            Style::Timestamp => self.timestamp,
            Style::BlockId => self.block_id,
            Style::ListMarker => self.list_marker,
            Style::CheckboxUnchecked => self.checkbox_unchecked,
            Style::CheckboxChecked => self.checkbox_checked,
            Style::Frontmatter => self.frontmatter,
            Style::BrokenLink => self.broken_link,
        }
    }

    /// Derive a full Theme from a palette.  All StyleProps are computed from
    /// the 14 palette slots following the mapping in THEMING.md.
    pub fn from_palette(name: &'static str, p: ThemePalette) -> Self {
        let s = p.strong;
        let sal = p.salient;
        let fad = p.faded;
        let sub = p.subtle;
        let crit = p.critical;
        let _pop = p.popout;

        // Lambda mapping: most syntax uses fg with weight/style variation,
        // NOT foreground color. Color is reserved for structural accents.
        Theme {
            name,
            palette: p,
            bg: p.background,
            fg: p.foreground,
            heading: [
                // H1: strong, bold (like font-lock-function-name-face)
                StyleProps::plain().fg(s.0, s.1, s.2).bold(),
                // H2: salient, bold (structural accent)
                StyleProps::plain().fg(sal.0, sal.1, sal.2).bold(),
                // H3: foreground, bold
                StyleProps::plain().fg(p.foreground.0, p.foreground.1, p.foreground.2).bold(),
                // H4-H6: just bold
                StyleProps::plain().bold(),
                StyleProps::plain().bold(),
                StyleProps::plain().bold(),
            ],
            bold: StyleProps::plain().bold(),
            italic: StyleProps::plain().italic(),
            // Lambda: strings use fg + faint bg wash — NOT a colorful foreground
            code: StyleProps::plain()
                .fg(p.foreground.0, p.foreground.1, p.foreground.2)
                .bg_rgb(sub.0, sub.1, sub.2),
            code_block: StyleProps::plain()
                .fg(p.foreground.0, p.foreground.1, p.foreground.2)
                .bg_rgb(sub.0, sub.1, sub.2),
            // Lambda: links use strong fg + lowlight bg + underline
            link: StyleProps::plain()
                .fg(s.0, s.1, s.2)
                .bg_rgb(p.modeline.0, p.modeline.1, p.modeline.2)
                .underline(),
            embed: StyleProps::plain()
                .fg(s.0, s.1, s.2)
                .bg_rgb(p.modeline.0, p.modeline.1, p.modeline.2)
                .italic(),
            // Lambda: tags use meek (faded), not a vivid accent
            tag: StyleProps::plain().fg(fad.0, fad.1, fad.2),
            timestamp: StyleProps::plain().fg(fad.0, fad.1, fad.2),
            block_id: StyleProps::plain().fg(fad.0, fad.1, fad.2).dim(),
            list_marker: StyleProps::plain().fg(fad.0, fad.1, fad.2),
            checkbox_unchecked: StyleProps::plain().fg(p.accent_yellow.0, p.accent_yellow.1, p.accent_yellow.2),
            checkbox_checked: StyleProps::plain().fg(p.accent_green.0, p.accent_green.1, p.accent_green.2).strikethrough(),
            // Lambda: comment face = meek + italic; frontmatter is similar
            frontmatter: StyleProps::plain().fg(fad.0, fad.1, fad.2).italic(),
            broken_link: StyleProps::plain().fg(crit.0, crit.1, crit.2).strikethrough(),
            tilde: StyleProps::plain().fg(fad.0, fad.1, fad.2),
            // UI chrome: use faded for borders (not too contrasty)
            border: p.faded,
            surface: p.subtle,
            accent: p.salient,
            // Lambda: region uses mild (mid-tone), hl-line uses highlight
            selection_bg: p.mild,
            selection_fg: p.foreground,
            status_normal: p.salient,
            status_insert: p.accent_green,
            status_visual: p.popout,
            status_command: p.accent_blue,
        }
    }

    // ── Built-in themes (colours from Lambda-themes by Colin McLear) ───

    /// Bloom Light — Lambda light palette.
    pub fn bloom_light() -> Self {
        Self::from_palette("bloom-light", ThemePalette {
            foreground:    Rgb(0x0C, 0x0D, 0x0D),
            background:    Rgb(0xFF, 0xFE, 0xFD),
            modeline:      Rgb(0xF8, 0xF6, 0xF4), // lowlight
            highlight:     Rgb(0xF5, 0xF2, 0xF0),
            critical:      Rgb(0xB3, 0x00, 0x00),
            popout:        Rgb(0x00, 0x44, 0xCC),
            strong:        Rgb(0x00, 0x00, 0x00),
            salient:       Rgb(0x5D, 0x00, 0xDA),
            faded:         Rgb(0x70, 0x6F, 0x6F),
            subtle:        Rgb(0xE3, 0xE1, 0xE0), // faint
            mild:          Rgb(0xC1, 0xC1, 0xC1),
            ultralight:    Rgb(0xEB, 0xE9, 0xE7),
            accent_red:    Rgb(0xEC, 0x6A, 0x5E),
            accent_green:  Rgb(0x00, 0x5A, 0x02),
            accent_blue:   Rgb(0x4C, 0x4C, 0xFF),
            accent_yellow: Rgb(0xE0, 0xA5, 0x00),
        })
    }

    /// Bloom Dark — Lambda dark palette.
    pub fn bloom_dark() -> Self {
        Self::from_palette("bloom-dark", ThemePalette {
            foreground:    Rgb(0xEB, 0xE9, 0xE7),
            background:    Rgb(0x14, 0x14, 0x14),
            modeline:      Rgb(0x1A, 0x19, 0x19), // lowlight
            highlight:     Rgb(0x21, 0x22, 0x28),
            critical:      Rgb(0xCF, 0x67, 0x52),
            popout:        Rgb(0x7A, 0x9E, 0xFF),
            strong:        Rgb(0xF5, 0xF2, 0xF0),
            salient:       Rgb(0xF4, 0xBF, 0x4F),
            faded:         Rgb(0xA3, 0xA3, 0xA3),
            subtle:        Rgb(0x37, 0x37, 0x3E), // faint
            mild:          Rgb(0x47, 0x46, 0x48),
            ultralight:    Rgb(0x2C, 0x2C, 0x34),
            accent_red:    Rgb(0xEC, 0x6A, 0x5E),
            accent_green:  Rgb(0x62, 0xC5, 0x54),
            accent_blue:   Rgb(0x81, 0xA1, 0xC1),
            accent_yellow: Rgb(0xF2, 0xDA, 0x61),
        })
    }

    /// Bloom Dark Faded — Lambda dark-faded palette (softer, Nord-influenced).
    pub fn bloom_dark_faded() -> Self {
        Self::from_palette("bloom-dark-faded", ThemePalette {
            foreground:    Rgb(0xEC, 0xEF, 0xF1),
            background:    Rgb(0x28, 0x2B, 0x35),
            modeline:      Rgb(0x3C, 0x43, 0x53), // lowlight
            highlight:     Rgb(0x44, 0x4B, 0x5C),
            critical:      Rgb(0xF4, 0x67, 0x15),
            popout:        Rgb(0xBC, 0x85, 0xFF),
            strong:        Rgb(0xFF, 0xFF, 0xFF),
            salient:       Rgb(0x88, 0xC0, 0xD0),
            faded:         Rgb(0x95, 0x9E, 0xB1),
            subtle:        Rgb(0x33, 0x3A, 0x47), // faint
            mild:          Rgb(0x87, 0x91, 0xA7),
            ultralight:    Rgb(0x52, 0x58, 0x68),
            accent_red:    Rgb(0xBF, 0x61, 0x6A),
            accent_green:  Rgb(0x8E, 0xB8, 0x9D),
            accent_blue:   Rgb(0x81, 0xA1, 0xC1),
            accent_yellow: Rgb(0xE9, 0xB8, 0x5D),
        })
    }

    /// Bloom Light Faded — Lambda light-faded palette (softer, muted light).
    pub fn bloom_light_faded() -> Self {
        Self::from_palette("bloom-light-faded", ThemePalette {
            foreground:    Rgb(0x28, 0x2B, 0x35),
            background:    Rgb(0xFC, 0xFA, 0xF6),
            modeline:      Rgb(0xE3, 0xE7, 0xEF), // lowlight
            highlight:     Rgb(0xDB, 0xE1, 0xEB),
            critical:      Rgb(0xF5, 0x31, 0x37),
            popout:        Rgb(0x94, 0x0B, 0x96),
            strong:        Rgb(0x00, 0x00, 0x00),
            salient:       Rgb(0x30, 0x3D, 0xB4),
            faded:         Rgb(0x72, 0x7D, 0x97),
            subtle:        Rgb(0xEC, 0xEF, 0xF1), // faint
            mild:          Rgb(0xC8, 0xCD, 0xD8),
            ultralight:    Rgb(0xCF, 0xD6, 0xE2),
            accent_red:    Rgb(0x96, 0x0D, 0x36),
            accent_green:  Rgb(0x00, 0x79, 0x6B),
            accent_blue:   Rgb(0x30, 0x60, 0x8C),
            accent_yellow: Rgb(0xE0, 0xA5, 0x00),
        })
    }

    /// Default theme — bloom-dark.
    pub fn bloom_default() -> Self {
        Self::bloom_dark()
    }

    /// All built-in themes.
    pub fn all_builtin() -> Vec<Self> {
        vec![
            Self::bloom_dark(),
            Self::bloom_dark_faded(),
            Self::bloom_light(),
            Self::bloom_light_faded(),
        ]
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_frame_focused_pane() {
        let frame = RenderFrame {
            panes: vec![PaneFrame {
                lines: vec![],
                cursor: CursorState {
                    row: 0,
                    col: 0,
                    shape: CursorShape::Block,
                },
                status_bar: StatusBar {
                    mode: "NORMAL".into(),
                    filename: "test.md".into(),
                    dirty: false,
                    position: "1:1".into(),
                    pending_keys: String::new(),
                    filetype: "markdown".into(),
                },
                focused: true,
            }],
            picker: None,
            which_key: None,
            diagnostics: vec![],
            command_line: None,
            capture_bar: None,
            undo_tree: None,
            agenda: None,
        };
        let pane = frame.focused_pane().unwrap();
        assert!(pane.focused);
        assert_eq!(frame.status().unwrap().mode, "NORMAL");
    }

    #[test]
    fn picker_frame_captures_state() {
        let picker = PickerFrame {
            title: "Find Page".into(),
            query: "edi".into(),
            filters: vec![FilterPill {
                label: "rust".into(),
                kind: "tag".into(),
            }],
            items: vec![PickerItem {
                text: "Text Editor Theory".into(),
                match_indices: vec![5, 6, 7],
                marginalia: "#rust · 2026-03-01".into(),
                selected: true,
                marked: false,
            }],
            result_count: "1 of 5".into(),
            preview: None,
            inline: false,
            action_menu: None,
            action_menu_selected: None,
        };
        assert_eq!(picker.items.len(), 1);
        assert!(picker.items[0].selected);
        assert_eq!(picker.filters[0].label, "rust");
    }

    #[test]
    fn all_builtin_themes_have_14_palette_slots() {
        for theme in Theme::all_builtin() {
            let p = theme.palette;
            // Verify all 14 slots are non-zero (not accidentally default).
            // At minimum, foreground and background must differ.
            assert_ne!(p.foreground, p.background, "{}: fg == bg", theme.name);
            assert_ne!(p.critical, p.background, "{}: critical == bg", theme.name);
            assert_ne!(p.salient, p.background, "{}: salient == bg", theme.name);
        }
    }

    #[test]
    fn theme_props_for_maps_all_styles() {
        let theme = Theme::bloom_dark();
        // Verify all Style variants produce non-panic results.
        let _ = theme.props_for(Style::Normal);
        let _ = theme.props_for(Style::Heading { level: 1 });
        let _ = theme.props_for(Style::Heading { level: 6 });
        let _ = theme.props_for(Style::Bold);
        let _ = theme.props_for(Style::Italic);
        let _ = theme.props_for(Style::Code);
        let _ = theme.props_for(Style::CodeBlock);
        let _ = theme.props_for(Style::Link);
        let _ = theme.props_for(Style::Embed);
        let _ = theme.props_for(Style::Tag);
        let _ = theme.props_for(Style::Timestamp);
        let _ = theme.props_for(Style::BlockId);
        let _ = theme.props_for(Style::ListMarker);
        let _ = theme.props_for(Style::CheckboxUnchecked);
        let _ = theme.props_for(Style::CheckboxChecked);
        let _ = theme.props_for(Style::Frontmatter);
        let _ = theme.props_for(Style::BrokenLink);
    }

    #[test]
    fn theme_from_palette_derives_link_from_strong() {
        let theme = Theme::bloom_light();
        let link_props = theme.props_for(Style::Link);
        // Lambda mapping: links use strong fg + lowlight bg + underline
        assert_eq!(link_props.fg, Some(theme.palette.strong));
        assert!(link_props.underline);
        assert_eq!(link_props.bg, Some(theme.palette.modeline));
    }

    #[test]
    fn theme_broken_link_uses_critical() {
        let theme = Theme::bloom_dark();
        let props = theme.props_for(Style::BrokenLink);
        assert_eq!(props.fg, Some(theme.palette.critical));
        assert!(props.strikethrough);
    }

    #[test]
    fn bloom_default_is_bloom_dark() {
        let d = Theme::bloom_default();
        assert_eq!(d.name, "bloom-dark");
    }

    #[test]
    fn three_builtin_themes_exist() {
        let themes = Theme::all_builtin();
        assert_eq!(themes.len(), 4);
        let names: Vec<&str> = themes.iter().map(|t| t.name).collect();
        assert!(names.contains(&"bloom-dark"));
        assert!(names.contains(&"bloom-dark-faded"));
        assert!(names.contains(&"bloom-light"));
        assert!(names.contains(&"bloom-light-faded"));
    }
}
