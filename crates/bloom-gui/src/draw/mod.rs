pub mod drawer;
pub mod inline;
pub mod notification;
pub mod overlay;
pub mod pane;

use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};

use crate::{CHAR_WIDTH, EDITOR_FONT, FONT_SIZE, TEXT_Y_OFFSET};

pub(crate) fn rect(x: f32, y: f32, width: f32, height: f32) -> Rectangle {
    Rectangle {
        x,
        y,
        width,
        height,
    }
}

pub(crate) fn inset(area: Rectangle, padding: f32) -> Rectangle {
    Rectangle {
        x: area.x + padding,
        y: area.y + padding,
        width: (area.width - padding * 2.0).max(0.0),
        height: (area.height - padding * 2.0).max(0.0),
    }
}

pub(crate) fn draw_text(
    frame: &mut Frame,
    x: f32,
    y: f32,
    content: impl Into<String>,
    color: Color,
) {
    frame.fill_text(canvas::Text {
        content: content.into(),
        position: Point::new(x, y + TEXT_Y_OFFSET),
        color,
        size: FONT_SIZE.into(),
        font: EDITOR_FONT,
        ..Default::default()
    });
}

pub(crate) fn draw_text_sized(
    frame: &mut Frame,
    x: f32,
    y: f32,
    content: impl Into<String>,
    color: Color,
    size: f32,
    row_height: f32,
) {
    let y_offset = (row_height - size) / 2.0;
    frame.fill_text(canvas::Text {
        content: content.into(),
        position: Point::new(x, y + y_offset.max(0.0)),
        color,
        size: size.into(),
        font: EDITOR_FONT,
        ..Default::default()
    });
}

pub(crate) fn draw_text_right(
    frame: &mut Frame,
    right_x: f32,
    y: f32,
    content: &str,
    color: Color,
) {
    draw_text(
        frame,
        (right_x - text_width(content)).max(0.0),
        y,
        content,
        color,
    );
}

pub(crate) fn draw_text_center(
    frame: &mut Frame,
    area: Rectangle,
    y: f32,
    content: &str,
    color: Color,
) {
    let x = area.x + ((area.width - text_width(content)).max(0.0) / 2.0);
    draw_text(frame, x, y, content, color);
}

pub(crate) fn fill_rect(frame: &mut Frame, area: Rectangle, color: Color) {
    frame.fill_rectangle(
        Point::new(area.x, area.y),
        Size::new(area.width, area.height),
        color,
    );
}

pub(crate) fn stroke_rect(frame: &mut Frame, area: Rectangle, color: Color) {
    frame.stroke(
        &Path::rectangle(
            Point::new(area.x, area.y),
            Size::new(area.width, area.height),
        ),
        Stroke::default().with_color(color).with_width(1.0),
    );
}

pub(crate) fn fill_panel(frame: &mut Frame, area: Rectangle, bg: Color, border: Color) {
    fill_rect(frame, area, bg);
    stroke_rect(frame, area, border);
}

pub(crate) fn draw_hline(frame: &mut Frame, x1: f32, x2: f32, y: f32, color: Color) {
    frame.stroke(
        &Path::line(Point::new(x1, y), Point::new(x2, y)),
        Stroke::default().with_color(color).with_width(1.0),
    );
}

pub(crate) fn draw_vline(frame: &mut Frame, x: f32, y1: f32, y2: f32, color: Color) {
    frame.stroke(
        &Path::line(Point::new(x, y1), Point::new(x, y2)),
        Stroke::default().with_color(color).with_width(1.0),
    );
}

pub(crate) fn text_width(text: &str) -> f32 {
    text.chars().count() as f32 * CHAR_WIDTH
}

pub(crate) fn chars_that_fit(width: f32) -> usize {
    (width / CHAR_WIDTH).floor().max(0.0) as usize
}

pub(crate) fn truncate_text(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    if max_chars == 0 {
        return String::new();
    }
    if max_chars == 1 {
        return "…".to_string();
    }

    let mut truncated = String::new();
    for ch in text.chars().take(max_chars - 1) {
        truncated.push(ch);
    }
    truncated.push('…');
    truncated
}

/// Convert a byte offset in `text` to the number of characters preceding it.
/// If `byte_offset` exceeds `text.len()`, returns the total char count.
pub(crate) fn byte_offset_to_char_col(text: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(text.len());
    text[..clamped].chars().count()
}

pub(crate) fn draw_bar_cursor(frame: &mut Frame, x: f32, y: f32, h: f32, color: Color) {
    fill_rect(frame, rect(x, y, 2.0, h), color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_width_empty() {
        assert_eq!(text_width(""), 0.0);
    }

    #[test]
    fn text_width_ascii() {
        assert_eq!(text_width("hello"), 5.0 * CHAR_WIDTH);
    }

    #[test]
    fn text_width_unicode() {
        // Each char counts as 1 for monospace approximation.
        assert_eq!(text_width("日本語"), 3.0 * CHAR_WIDTH);
    }

    #[test]
    fn chars_that_fit_zero_width() {
        assert_eq!(chars_that_fit(0.0), 0);
    }

    #[test]
    fn chars_that_fit_exact_multiple() {
        assert_eq!(chars_that_fit(CHAR_WIDTH * 10.0), 10);
    }

    #[test]
    fn chars_that_fit_partial() {
        // Slightly more than 5 chars of space → still 5
        assert_eq!(chars_that_fit(CHAR_WIDTH * 5.0 + 0.1), 5);
    }

    #[test]
    fn chars_that_fit_negative() {
        assert_eq!(chars_that_fit(-10.0), 0);
    }

    #[test]
    fn truncate_text_short_string() {
        assert_eq!(truncate_text("hi", 10), "hi");
    }

    #[test]
    fn truncate_text_exact_fit() {
        assert_eq!(truncate_text("hello", 5), "hello");
    }

    #[test]
    fn truncate_text_overflow() {
        assert_eq!(truncate_text("hello world", 8), "hello w…");
    }

    #[test]
    fn truncate_text_max_zero() {
        assert_eq!(truncate_text("hello", 0), "");
    }

    #[test]
    fn truncate_text_max_one() {
        assert_eq!(truncate_text("hello", 1), "…");
    }

    #[test]
    fn rect_helper() {
        let r = rect(10.0, 20.0, 100.0, 50.0);
        assert_eq!(r.x, 10.0);
        assert_eq!(r.y, 20.0);
        assert_eq!(r.width, 100.0);
        assert_eq!(r.height, 50.0);
    }

    #[test]
    fn inset_shrinks_rect() {
        let r = rect(0.0, 0.0, 100.0, 80.0);
        let i = inset(r, 10.0);
        assert_eq!(i.x, 10.0);
        assert_eq!(i.y, 10.0);
        assert_eq!(i.width, 80.0);
        assert_eq!(i.height, 60.0);
    }

    #[test]
    fn inset_clamps_to_zero() {
        let r = rect(0.0, 0.0, 10.0, 10.0);
        let i = inset(r, 20.0);
        assert_eq!(i.width, 0.0);
        assert_eq!(i.height, 0.0);
    }
}
