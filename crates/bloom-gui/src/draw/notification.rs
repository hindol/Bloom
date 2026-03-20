use bloom_core::render::{Notification, NotificationLevel};
use bloom_md::theme::ThemePalette;
use iced::Rectangle;

use crate::draw::{draw_text, fill_rect, rect, text_width, truncate_text};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT};

/// Draw notifications right-aligned and bottom-aligned within `area`.
pub(crate) fn draw_notifications(
    frame: &mut iced::widget::canvas::Frame,
    area: Rectangle,
    notifications: &[Notification],
    theme: &ThemePalette,
) {
    let gap = 4.0;
    let notif_h = LINE_HEIGHT + 4.0;
    let bottom_edge = area.y + area.height;
    let mut y = bottom_edge - notif_h;

    for notification in notifications.iter().rev().take(3) {
        if y < area.y {
            break;
        }

        let (prefix, bg, fg) = match notification.level {
            NotificationLevel::Info => (
                "✓",
                rgb_to_color(&theme.highlight),
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
        let right_edge = area.x + area.width;
        let x = (right_edge - width - CHAR_WIDTH).max(area.x);
        let pill = rect(x, y, width, notif_h);
        fill_rect(frame, pill, bg);
        draw_text(frame, x + crate::SPACING_SM, y + 2.0, text, fg);

        y -= notif_h + gap;
    }
}
