use bloom_core::render::RenderFrame;
use iced::Rectangle;

use crate::{LINE_HEIGHT, STATUS_BAR_HEIGHT};

/// Pre-computed pixel rects for one frame. Computed once, passed to all draw functions.
#[allow(dead_code)]
pub(crate) struct FrameLayout {
    /// Full window size.
    pub window: Rectangle,
    /// Content area: editor panes. y=0, height shrinks when drawer is open.
    pub content: Rectangle,
    /// Modeline (status bar). Adjacent below content, above drawer.
    pub modeline: Rectangle,
    /// Active bottom drawer (picker, which-key, calendar, temporal strip, view).
    /// None if no drawer is active.
    pub drawer: Option<Rectangle>,
}

impl FrameLayout {
    pub fn compute(window_width: f32, window_height: f32, frame: &RenderFrame) -> Self {
        let modeline_h = STATUS_BAR_HEIGHT + 4.0; // +4 for macOS corner clearance
        let drawer_h = Self::drawer_height(frame);

        let drawer_y = window_height - drawer_h;
        let modeline_y = drawer_y - modeline_h;
        let content_h = modeline_y.max(0.0);

        let window = Rectangle::new(
            iced::Point::ORIGIN,
            iced::Size::new(window_width, window_height),
        );
        let content = Rectangle::new(
            iced::Point::ORIGIN,
            iced::Size::new(window_width, content_h),
        );
        let modeline = Rectangle {
            x: 0.0,
            y: modeline_y,
            width: window_width,
            height: modeline_h,
        };
        let drawer = if drawer_h > 0.0 {
            Some(Rectangle {
                x: 0.0,
                y: drawer_y,
                width: window_width,
                height: drawer_h,
            })
        } else {
            None
        };

        Self {
            window,
            content,
            modeline,
            drawer,
        }
    }

    fn drawer_height(frame: &RenderFrame) -> f32 {
        // Priority: only one drawer is active at a time.
        if let Some(p) = &frame.picker {
            let rows = p.results.len().min(10) + 3;
            return rows as f32 * LINE_HEIGHT;
        }
        if let Some(v) = &frame.view {
            let rows = v.rows.len().min(12) + 3;
            return rows as f32 * LINE_HEIGHT;
        }
        if frame.date_picker.is_some() {
            return 10.0 * LINE_HEIGHT;
        }
        if let Some(wk) = &frame.which_key {
            let col_chars = 20usize;
            let total_chars = (frame
                .panes
                .first()
                .map(|p| p.rect.width as usize)
                .unwrap_or(80))
            .max(40);
            let cols = ((total_chars.saturating_sub(4)) / col_chars).max(1);
            let rows = wk.entries.len().div_ceil(cols).max(1) + 2;
            return rows as f32 * LINE_HEIGHT;
        }
        if let Some(s) = &frame.temporal_strip {
            return if s.compact { 4.0 } else { 6.0 } * LINE_HEIGHT;
        }
        if frame.context_strip.is_some() {
            return 3.0 * LINE_HEIGHT;
        }
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bloom_core::render::{PickerFrame, PickerRow, RenderFrame};

    fn empty_frame() -> RenderFrame {
        RenderFrame {
            panes: vec![],
            maximized: false,
            hidden_pane_count: 0,
            picker: None,
            inline_menu: None,
            which_key: None,
            date_picker: None,
            context_strip: None,
            temporal_strip: None,
            dialog: None,
            view: None,
            notifications: vec![],
            scrolloff: 3,
            word_wrap: true,
            wrap_indicator: "↪".into(),
            block_id_gutter: false,
            theme_name: "bloom-dark".into(),
            layout_tree: bloom_core::render::LayoutTree::Leaf(bloom_core::types::PaneId(0)),
            clipboard_text: None,
        }
    }

    fn picker_frame(n_results: usize) -> PickerFrame {
        PickerFrame {
            title: "Test".into(),
            query: String::new(),
            results: (0..n_results)
                .map(|i| PickerRow {
                    label: format!("item {i}"),
                    middle: None,
                    right: None,
                })
                .collect(),
            selected_index: 0,
            filters: vec![],
            preview: None,
            total_count: n_results,
            filtered_count: n_results,
            status_noun: "items".into(),
            min_query_len: 0,
            query_selected: false,
            wide: false,
        }
    }

    #[test]
    fn layout_no_drawer() {
        let layout = FrameLayout::compute(800.0, 600.0, &empty_frame());
        assert!(layout.drawer.is_none());
        assert_eq!(layout.window.width, 800.0);
        assert_eq!(layout.window.height, 600.0);
        assert!(layout.content.height > 0.0);
    }

    #[test]
    fn layout_vertical_stack_order() {
        let layout = FrameLayout::compute(1000.0, 800.0, &empty_frame());
        assert_eq!(layout.content.y, 0.0);
        assert!(layout.modeline.y >= layout.content.height - 0.1);
        assert!(layout.modeline.y + layout.modeline.height <= 800.0 + 0.1);
    }

    #[test]
    fn layout_with_picker_has_drawer() {
        let mut frame = empty_frame();
        frame.picker = Some(picker_frame(0));
        let layout = FrameLayout::compute(800.0, 600.0, &frame);
        assert!(layout.drawer.is_some());
        let drawer = layout.drawer.unwrap();
        assert!(drawer.height > 0.0);
        assert!(drawer.y >= layout.modeline.y + layout.modeline.height - 0.1);
    }

    #[test]
    fn layout_content_shrinks_with_drawer() {
        let no_drawer = FrameLayout::compute(800.0, 600.0, &empty_frame());
        let mut frame = empty_frame();
        frame.picker = Some(picker_frame(5));
        let with_drawer = FrameLayout::compute(800.0, 600.0, &frame);
        assert!(
            with_drawer.content.height < no_drawer.content.height,
            "content should shrink when drawer opens"
        );
    }

    #[test]
    fn layout_full_width() {
        let layout = FrameLayout::compute(1920.0, 1080.0, &empty_frame());
        assert_eq!(layout.content.width, 1920.0);
        assert_eq!(layout.modeline.width, 1920.0);
    }
}
