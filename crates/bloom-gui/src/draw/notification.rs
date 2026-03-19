use bloom_core::render::{Notification, NotificationLevel};
use bloom_md::theme::ThemePalette;
use iced::Size;

use crate::draw::{draw_text, fill_rect, rect, text_width, truncate_text};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, LINE_HEIGHT, STATUS_BAR_HEIGHT};

pub(crate) fn draw_notifications(
    frame: &mut iced::widget::canvas::Frame,
    size: Size,
    notifications: &[Notification],
    theme: &ThemePalette,
) {
    let gap = 4.0;
    let notif_h = LINE_HEIGHT + 4.0; // notification height with padding
    // Start stacking from bottom, above the modeline area (~24px from bottom).
    let bottom_margin = STATUS_BAR_HEIGHT + 8.0;
    let mut y = size.height - bottom_margin - notif_h;

    for notification in notifications.iter().rev().take(3) {
        if y < 0.0 {
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
        let x = (size.width - width - CHAR_WIDTH).max(0.0);
        let area = rect(x, y, width, notif_h);
        fill_rect(frame, area, bg);
        draw_text(frame, x + crate::SPACING_SM, y + 2.0, text, fg);

        y -= notif_h + gap;
    }
}
