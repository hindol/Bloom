use iced::{
    mouse,
    widget::canvas::{self, Frame, Geometry},
    Color, Font, Point, Rectangle, Renderer, Size, Theme,
};

use crate::paint::PaintSnapshot;

pub struct IcedCanvasProgram {
    snapshot: PaintSnapshot,
    foreground: Color,
    cursor_color: Color,
}

impl IcedCanvasProgram {
    pub fn new(snapshot: PaintSnapshot, foreground: Color, cursor_color: Color) -> Self {
        Self {
            snapshot,
            foreground,
            cursor_color,
        }
    }
}

impl canvas::Program<()> for IcedCanvasProgram {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        for grapheme in &self.snapshot.graphemes {
            frame.fill_text(canvas::Text {
                content: grapheme.text.clone(),
                position: Point::new(grapheme.origin.x.get(), grapheme.origin.y.get()),
                color: self.foreground,
                size: grapheme.font.size().get().into(),
                font: Font::with_name(grapheme.font.family()),
                ..Default::default()
            });
        }

        let cursor = self.snapshot.cursor;
        frame.fill_rectangle(
            Point::new(cursor.origin.x.get(), cursor.origin.y.get()),
            Size::new(cursor.size.width.get(), cursor.size.height.get()),
            self.cursor_color,
        );

        vec![frame.into_geometry()]
    }
}
