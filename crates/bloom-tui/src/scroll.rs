pub struct ScreenScroll {
    pub first_screen_row: usize,
}

impl ScreenScroll {
    pub fn new() -> Self {
        Self {
            first_screen_row: 0,
        }
    }

    pub fn ensure_visible(
        &mut self,
        cursor_screen_row: usize,
        visible_height: usize,
        scrolloff: usize,
    ) {
        if visible_height == 0 {
            return;
        }
        let effective_scrolloff = scrolloff.min(visible_height.saturating_sub(1) / 2);
        let top = self.first_screen_row + effective_scrolloff;
        let bottom = self
            .first_screen_row
            .saturating_add(visible_height)
            .saturating_sub(1)
            .saturating_sub(effective_scrolloff);
        if cursor_screen_row < top {
            self.first_screen_row = cursor_screen_row.saturating_sub(effective_scrolloff);
        } else if cursor_screen_row > bottom {
            self.first_screen_row =
                (cursor_screen_row + effective_scrolloff + 1).saturating_sub(visible_height);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_at_zero() {
        let s = ScreenScroll::new();
        assert_eq!(s.first_screen_row, 0);
    }

    #[test]
    fn scrolls_down() {
        let mut s = ScreenScroll::new();
        s.ensure_visible(25, 20, 3);
        // cursor at 25, visible 20, scrolloff 3 -> first = 25+3+1-20 = 9
        assert_eq!(s.first_screen_row, 9);
    }

    #[test]
    fn scrolls_up() {
        let mut s = ScreenScroll {
            first_screen_row: 20,
        };
        s.ensure_visible(18, 20, 3);
        // cursor at 18, top boundary = 20+3 = 23 -> cursor < top -> first = 18-3 = 15
        assert_eq!(s.first_screen_row, 15);
    }

    #[test]
    fn no_scroll_when_visible() {
        let mut s = ScreenScroll {
            first_screen_row: 0,
        };
        s.ensure_visible(10, 20, 3);
        assert_eq!(s.first_screen_row, 0);
    }
}
