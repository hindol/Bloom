pub struct Viewport {
    pub first_visible_line: usize,
    pub height: usize,
    pub width: usize,
}

impl Viewport {
    pub fn new(height: usize, width: usize) -> Self {
        Self {
            first_visible_line: 0,
            height,
            width,
        }
    }

    /// Ensure cursor line is visible, adjusting scroll if needed.
    pub fn ensure_visible(&mut self, cursor_line: usize) {
        if cursor_line < self.first_visible_line {
            self.first_visible_line = cursor_line;
        } else if cursor_line >= self.first_visible_line + self.height {
            self.first_visible_line = cursor_line.saturating_sub(self.height.saturating_sub(1));
        }
    }

    /// Get range of visible lines.
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        self.first_visible_line..self.first_visible_line + self.height
    }
}