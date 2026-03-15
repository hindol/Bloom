mod calendar;
mod context_strip;
mod dialog;
mod inline_menu;
mod notifications;
mod page_history;
mod pane;
mod picker;
mod status_bar;
mod temporal_strip;
mod timeline;
mod undo_tree;
mod view;
mod which_key;
mod wizard;

use bloom_core::render::{
    DialogFrame, InlineMenuAnchor, InlineMenuFrame, McpIndicator, NotificationLevel, PaneFrame,
    PaneKind, PickerFrame, RenderFrame, StatusBarContent, StatusBarFrame, WhichKeyFrame,
};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style as RStyle};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::theme::TuiTheme;

/// Render the full RenderFrame to the terminal.
pub fn draw(
    f: &mut Frame,
    frame: &RenderFrame,
    theme: &TuiTheme,
    config: &bloom_core::config::Config,
) {
    let area = f.area();

    // Layer 1: Clear all cells (reset content), then fill with background colour.
    // This prevents stale characters from previous frames bleeding through.
    f.render_widget(Clear, area);
    f.render_widget(
        Block::default().style(RStyle::default().bg(theme.background())),
        area,
    );

    // Layout: panes | temporal strip (optional) | which-key drawer (optional)
    // Both temporal strip and which-key are bottom drawers below the status bar.
    let ts_h = frame.temporal_strip.as_ref().map(|ts| ts.drawer_height()).unwrap_or(0);

    let wk_h = if let Some(wk) = &frame.which_key {
        let col_width = 24u16;
        let cols = (area.width.saturating_sub(4) / col_width).max(1);
        let rows_needed = (wk.entries.len() as u16).div_ceil(cols);
        (rows_needed + 2).min(area.height / 3).max(3)
    } else {
        0
    };

    let drawer_h = ts_h.max(wk_h); // only one is active at a time
    let pane_h = area.height.saturating_sub(drawer_h);
    let pane_area = Rect::new(area.x, area.y, area.width, pane_h);
    let wk_area = if wk_h > 0 {
        Some(Rect::new(area.x, area.y + pane_h, area.width, wk_h))
    } else {
        None
    };
    let ts_area = if ts_h > 0 {
        Some(Rect::new(area.x, area.y + pane_h, area.width, ts_h))
    } else {
        None
    };

    // Draw panes (each pane includes its own status bar)
    pane::draw_panes(
        f,
        pane_area,
        &frame.panes,
        frame.maximized,
        frame.hidden_pane_count,
        theme,
        config,
    );

    // Which-key drawer
    if let (Some(wk), Some(wk_rect)) = (&frame.which_key, wk_area) {
        which_key::draw_which_key(f, wk_rect, wk, theme);
    }

    // Context strip (journal day-hopping, temporal navigation)
    if let Some(strip) = &frame.context_strip {
        context_strip::draw_context_strip(f, pane_area, strip, theme);
    }

    // Temporal strip (below status bar, like which-key drawer)
    if let (Some(strip), Some(ts_rect)) = (&frame.temporal_strip, ts_area) {
        temporal_strip::draw_temporal_strip(f, pane_area, ts_rect, strip, theme);
    }

    // Overlays — drawn after panes, so their set_cursor_position() wins.
    if let Some(dp) = &frame.date_picker {
        calendar::draw_calendar(f, area, dp, theme);
    }
    if let Some(menu) = &frame.inline_menu {
        inline_menu::draw_inline_menu(f, area, menu, theme);
    }
    if let Some(picker) = &frame.picker {
        picker::draw_picker(f, area, picker, theme);
    }
    if let Some(dialog) = &frame.dialog {
        dialog::draw_dialog(f, area, dialog, theme);
    }
    if let Some(view_frame) = &frame.view {
        view::draw_view(f, view_frame, theme);
    }
    if !frame.notifications.is_empty() {
        notifications::draw_notifications(f, area, &frame.notifications, theme);
    }
}

pub(crate) fn truncate_with_ellipsis(s: &str, max: usize) -> String {
    truncate_to_width(s, max)
}

/// Truncate a string to fit within `max_width` display columns, appending `…` if truncated.
pub(crate) fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    if s.width() <= max_width {
        return s.to_string();
    }
    let ellipsis_w = 1; // '…' is 1 column wide
    let target = max_width.saturating_sub(ellipsis_w);
    let mut width = 0;
    let mut end = 0;
    for (i, ch) in s.char_indices() {
        let cw = ch.width().unwrap_or(0);
        if width + cw > target {
            break;
        }
        width += cw;
        end = i + ch.len_utf8();
    }
    format!("{}…", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_core::config::Config;
    use bloom_core::BloomEditor;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    /// Render a frame and verify every cell has the theme background colour.
    /// No cell should retain Style::Reset (terminal default).
    #[test]
    fn test_all_cells_have_background() {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("test.md"),
            "# Hello\n\nWorld\n",
        );

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = crate::theme::TuiTheme::new(editor.theme());
        let _expected_bg = Color::Rgb(
            editor.theme().background.0,
            editor.theme().background.1,
            editor.theme().background.2,
        );
        let cfg = editor.config.clone();

        terminal
            .draw(|f| {
                let area = f.area();
                let frame = editor.render(area.width, area.height);
                draw(f, &frame, &theme, &cfg);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut uncovered = Vec::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                // Every cell should have an explicit bg set (not Reset/default)
                if cell.bg == Color::Reset {
                    uncovered.push((x, y, cell.symbol().to_string()));
                }
            }
        }
        assert!(
            uncovered.is_empty(),
            "Found {} cells with no background set (Style::Reset). First 10: {:?}",
            uncovered.len(),
            &uncovered[..uncovered.len().min(10)]
        );
    }

    /// After switching to a shorter buffer, no stale content from the
    /// previous buffer should remain.
    #[test]
    fn test_no_stale_content_after_buffer_switch() {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();

        // Open a long file
        let id1 = bloom_core::uuid::generate_hex_id();
        let long_content = (0..50)
            .map(|i| format!("Line number {i} with some content here"))
            .collect::<Vec<_>>()
            .join("\n");
        editor.open_page_with_content(&id1, "Long", std::path::Path::new("long.md"), &long_content);

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = crate::theme::TuiTheme::new(editor.theme());
        let cfg = editor.config.clone();

        // Render the long file
        terminal
            .draw(|f| {
                let area = f.area();
                let frame = editor.render(area.width, area.height);
                draw(f, &frame, &theme, &cfg);
            })
            .unwrap();

        // Open a short file
        let id2 = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(&id2, "Short", std::path::Path::new("short.md"), "# Short\n");

        // Render again
        terminal
            .draw(|f| {
                let area = f.area();
                let frame = editor.render(area.width, area.height);
                draw(f, &frame, &theme, &cfg);
            })
            .unwrap();

        // Check: no cell below the short file's content should contain
        // text from the long file (e.g., "Line number")
        let buf = terminal.backend().buffer();
        let mut stale = Vec::new();
        for y in 0..buf.area.height {
            let mut row_text = String::new();
            for x in 0..buf.area.width {
                row_text.push_str(buf[(x, y)].symbol());
            }
            if row_text.contains("Line number") {
                stale.push((y, row_text.trim().to_string()));
            }
        }
        assert!(
            stale.is_empty(),
            "Found stale content from previous buffer: {:?}",
            stale
        );
    }

    /// Diagnostic: check every cell has an explicit bg (not Reset).
    #[test]
    fn test_no_reset_bg_anywhere() {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("test.md"),
            "# Hello\n\nWorld\nLine four\nLine five with **bold** and #tag\n",
        );

        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = crate::theme::TuiTheme::new(editor.theme());
        let cfg = editor.config.clone();

        terminal
            .draw(|tf| {
                let area = tf.area();
                let frame = editor.render(area.width, area.height);
                draw(tf, &frame, &theme, &cfg);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut issues = Vec::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.bg == Color::Reset {
                    issues.push(format!(
                        "({},{}) sym={:?} fg={:?}",
                        x,
                        y,
                        cell.symbol(),
                        cell.fg
                    ));
                }
            }
        }

        if !issues.is_empty() {
            let sample: Vec<_> = issues.iter().take(20).collect();
            panic!(
                "Found {} cells with bg=Reset. First 20:\n{}",
                issues.len(),
                sample
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }

    /// Regression: CRLF line endings must not cause short rows.
    #[test]
    fn test_no_reset_bg_with_crlf() {
        let config = Config::defaults();
        let mut editor = BloomEditor::new(config).unwrap();
        let id = bloom_core::uuid::generate_hex_id();
        editor.open_page_with_content(
            &id,
            "Test",
            std::path::Path::new("test.md"),
            "# Hello\r\n\r\nWorld\r\nLine four\r\nLine five with **bold** and #tag\r\n",
        );

        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let theme = crate::theme::TuiTheme::new(editor.theme());
        let cfg = editor.config.clone();

        terminal
            .draw(|tf| {
                let area = tf.area();
                let frame = editor.render(area.width, area.height);
                draw(tf, &frame, &theme, &cfg);
            })
            .unwrap();

        let buf = terminal.backend().buffer();
        let mut issues = Vec::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let cell = &buf[(x, y)];
                if cell.bg == Color::Reset {
                    issues.push(format!(
                        "({},{}) sym={:?} fg={:?}",
                        x,
                        y,
                        cell.symbol(),
                        cell.fg
                    ));
                }
            }
        }

        if !issues.is_empty() {
            let sample: Vec<_> = issues.iter().take(20).collect();
            panic!(
                "Found {} cells with bg=Reset (CRLF content). First 20:\n{}",
                issues.len(),
                sample
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }
    }
}
