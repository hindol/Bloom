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

    /// Ensure cursor line is visible with a scrolloff margin.
    pub fn ensure_visible_with_scrolloff(&mut self, cursor_line: usize, scrolloff: usize) {
        let top_margin = self.first_visible_line + scrolloff;
        let bottom_margin = self.first_visible_line + self.height.saturating_sub(1).saturating_sub(scrolloff);
        if cursor_line < top_margin {
            self.first_visible_line = cursor_line.saturating_sub(scrolloff);
        } else if cursor_line > bottom_margin {
            self.first_visible_line = (cursor_line + scrolloff + 1).saturating_sub(self.height);
        }
    }

    /// Ensure cursor line is visible (no margin).
    pub fn ensure_visible(&mut self, cursor_line: usize) {
        self.ensure_visible_with_scrolloff(cursor_line, 0);
    }

    /// Get range of visible lines.
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        self.first_visible_line..self.first_visible_line + self.height
    }
}