pub mod drawer;
pub mod inline;
pub mod notification;
pub mod overlay;
pub mod pane;

use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::{Color, Point, Rectangle, Size};

use crate::{CHAR_WIDTH, EDITOR_FONT, FONT_SIZE, LINE_HEIGHT, TEXT_Y_OFFSET};

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

pub(crate) fn draw_bar_cursor(frame: &mut Frame, x: f32, y: f32, color: Color) {
    fill_rect(frame, rect(x, y, 2.0, LINE_HEIGHT), color);
}

pub(crate) fn draw_overlay_scrim(frame: &mut Frame, size: Size, color: Color, alpha: f32) {
    fill_rect(
        frame,
        rect(0.0, 0.0, size.width, size.height),
        Color { a: alpha, ..color },
    );
}
