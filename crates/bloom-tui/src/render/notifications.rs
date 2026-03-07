use super::*;

pub(super) fn draw_notifications(
    f: &mut Frame,
    area: Rect,
    notifications: &[bloom_core::render::Notification],
    theme: &TuiTheme,
) {
    // Stack notifications bottom-right, above the status bar (last row).
    // Newest at bottom, older ones stack upward.
    let status_bar_height = 1u16;
    let mut y = area.bottom().saturating_sub(status_bar_height + 1);

    for notif in notifications.iter().rev() {
        if y < area.y + 1 {
            break;
        }
        let icon = match notif.level {
            NotificationLevel::Info => "\u{2713}",    // ✓
            NotificationLevel::Warning => "\u{26a0}", // ⚠
            NotificationLevel::Error => "\u{2717}",   // ✗
        };
        let text = format!(" {} {} ", icon, notif.message);
        let w = (text.width() as u16).min(area.width);
        let x = area.right().saturating_sub(w + 1);
        let notif_area = Rect::new(x, y, w, 1);

        let style = theme.notification_style(&notif.level);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(&text, style))),
            notif_area,
        );
        y = y.saturating_sub(1);
    }
}
