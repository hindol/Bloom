use bloom_core::render::{Notification, NotificationLevel};
use bloom_md::theme::ThemePalette;
use iced::Size;

use crate::draw::{draw_text, fill_rect, rect, stroke_rect, text_width, truncate_text};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT, STATUS_BAR_HEIGHT};

pub(crate) fn draw_notifications(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    notifications: &[Notification],
    theme: &ThemePalette,
) {
    // Position above the status bar, with a small gap.
    let mut y = size.height - STATUS_BAR_HEIGHT - LINE_HEIGHT - 4.0;

    for notification in notifications.iter().rev().take(3) {
        let (prefix, bg, fg) = match notification.level {
            NotificationLevel::Info => (
                "✓",
                rgb_to_color(&theme.subtle),
                rgb_to_color(&theme.foreground),
            ),
            NotificationLevel::Warning => (
                "⚠",
                rgb_to_color(&theme.accent_yellow),
                rgb_to_color(&theme.background),
            ),
            NotificationLevel::Error => (
                "✗",
                rgb_to_color(&theme.critical),
                rgb_to_color(&theme.background),
            ),
        };

        let text = truncate_text(&format!(" {prefix} {} ", notification.message), 48);
        let width = text_width(&text) + CHAR_WIDTH;
        let x = (size.width - width - CHAR_WIDTH).max(0.0);
        let area = rect(x, y, width, LINE_HEIGHT);
        fill_rect(frame, area, bg);
        stroke_rect(frame, area, rgb_to_color(&theme.faded));
        draw_text(frame, x + CHAR_WIDTH / 2.0, y, text, fg);

        y -= LINE_HEIGHT + 4.0;
        if y < 0.0 {
            break;
        }
    }
}
