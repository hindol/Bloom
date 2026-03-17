use std::collections::BTreeSet;

use bloom_core::render::{PaneFrame, PaneKind, RenderFrame};
use bloom_md::theme::ThemePalette;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::{Rectangle, Renderer, Size, Theme};

use crate::draw::{drawer, inline, notification, overlay, pane, draw_text_right};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, Message};

/// Animation speed: fraction of remaining distance covered per frame.
/// 0.6 at 60fps ≈ 50ms to converge (3 frames to reach 94% of target).
const LERP_FACTOR: f32 = 0.6;
/// Snap threshold: if within this many pixels, jump to target (no sub-pixel jitter).
const SNAP_THRESHOLD: f32 = 0.5;

fn lerp_snap(current: f32, target: f32) -> f32 {
    let diff = target - current;
    if diff.abs() < SNAP_THRESHOLD {
        target
    } else {
        current + diff * LERP_FACTOR
    }
}

/// Smooth animation state for cursor and scroll, driven by the app update loop.
pub(crate) struct AnimationState {
    cursor_y: f32,
    highlight_y: f32,
    scroll_y: f32,
    initialized: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self {
            cursor_y: 0.0,
            highlight_y: 0.0,
            scroll_y: 0.0,
            initialized: false,
        }
    }
}

impl AnimationState {
    /// Advance visual positions toward logical targets. Returns true if still animating.
    pub fn advance(
        &mut self,
        target_cursor_y: f32,
        target_scroll_y: f32,
    ) -> bool {
        if !self.initialized {
            self.cursor_y = target_cursor_y;
            self.highlight_y = target_cursor_y;
            self.scroll_y = target_scroll_y;
            self.initialized = true;
            return false;
        }

        let prev_cursor = self.cursor_y;
        let prev_scroll = self.scroll_y;

        self.cursor_y = lerp_snap(self.cursor_y, target_cursor_y);
        self.highlight_y = lerp_snap(self.highlight_y, target_cursor_y);
        self.scroll_y = lerp_snap(self.scroll_y, target_scroll_y);

        // Still animating if any value changed.
        (self.cursor_y - prev_cursor).abs() > 0.01
            || (self.scroll_y - prev_scroll).abs() > 0.01
    }

    pub fn cursor_y(&self) -> f32 {
        self.cursor_y
    }
    pub fn highlight_y(&self) -> f32 {
        self.highlight_y
    }
}

pub(crate) struct EditorCanvas<'a> {
    pub(crate) frame: Option<&'a RenderFrame>,
    pub(crate) theme: &'a ThemePalette,
    pub(crate) cache: &'a Cache,
    pub(crate) anim: &'a AnimationState,
}

impl<'a> canvas::Program<Message> for EditorCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(
                iced::Point::ORIGIN,
                bounds.size(),
                rgb_to_color(&self.theme.background),
            );

            let Some(render_frame) = self.frame else {
                return;
            };

            if let Some(wizard) = render_frame.panes.iter().find_map(|pane| match &pane.kind {
                PaneKind::SetupWizard(wizard) => Some(wizard),
                _ => None,
            }) {
                overlay::draw_setup_wizard(frame, bounds.size(), wizard, self.theme);
                return;
            }

            for pane_frame in &render_frame.panes {
                let anim = if pane_frame.is_active {
                    Some((self.anim.cursor_y(), self.anim.highlight_y()))
                } else {
                    None
                };
                pane::draw_pane(frame, pane_frame, self.theme, anim);
            }

            self.draw_split_borders(frame, &render_frame.panes);
            self.draw_hidden_pane_count(frame, bounds.size(), render_frame.hidden_pane_count);

            let active_pane = render_frame.panes.iter().find(|pane| pane.is_active);

            if let Some(strip) = &render_frame.context_strip {
                drawer::draw_context_strip(frame, bounds.size(), strip, self.theme);
            }
            if let Some(strip) = &render_frame.temporal_strip {
                drawer::draw_temporal_strip(frame, bounds.size(), strip, self.theme, active_pane);
            }
            if let Some(which_key) = &render_frame.which_key {
                drawer::draw_which_key(frame, bounds.size(), which_key, self.theme);
            }
            if let Some(menu) = &render_frame.inline_menu {
                inline::draw_inline_menu(frame, bounds.size(), active_pane, menu, self.theme);
            }
            if let Some(date_picker) = &render_frame.date_picker {
                overlay::draw_date_picker(frame, bounds.size(), date_picker, self.theme);
            }
            if let Some(picker) = &render_frame.picker {
                overlay::draw_picker(frame, bounds.size(), picker, self.theme);
            }
            if let Some(dialog) = &render_frame.dialog {
                overlay::draw_dialog(frame, bounds.size(), dialog, self.theme);
            }
            if let Some(view) = &render_frame.view {
                overlay::draw_view(frame, bounds.size(), view, self.theme);
            }
            if !render_frame.notifications.is_empty() {
                notification::draw_notifications(
                    frame,
                    bounds.size(),
                    &render_frame.notifications,
                    self.theme,
                );
            }
        });

        vec![geometry]
    }
}

impl<'a> EditorCanvas<'a> {
    fn draw_split_borders(&self, frame: &mut canvas::Frame, panes: &[PaneFrame]) {
        let mut vertical = BTreeSet::new();
        let mut horizontal = BTreeSet::new();

        for (index, left) in panes.iter().enumerate() {
            for right in panes.iter().skip(index + 1) {
                let left_right = left.rect.x + left.rect.width;
                let right_right = right.rect.x + right.rect.width;
                let left_bottom = left.rect.y + left.rect.total_height;
                let right_bottom = right.rect.y + right.rect.total_height;

                if left_right == right.rect.x || right_right == left.rect.x {
                    let x = if left_right == right.rect.x {
                        left_right
                    } else {
                        right_right
                    };
                    let y1 = left.rect.y.max(right.rect.y);
                    let y2 = left_bottom.min(right_bottom);
                    if y2 > y1 {
                        vertical.insert((x, y1, y2));
                    }
                }

                if left_bottom == right.rect.y || right_bottom == left.rect.y {
                    let y = if left_bottom == right.rect.y {
                        left_bottom
                    } else {
                        right_bottom
                    };
                    let x1 = left.rect.x.max(right.rect.x);
                    let x2 = (left.rect.x + left.rect.width).min(right.rect.x + right.rect.width);
                    if x2 > x1 {
                        horizontal.insert((y, x1, x2));
                    }
                }
            }
        }

        for (x, y1, y2) in vertical {
            crate::draw::draw_vline(
                frame,
                x as f32 * CHAR_WIDTH,
                y1 as f32 * crate::LINE_HEIGHT,
                y2 as f32 * crate::LINE_HEIGHT,
                rgb_to_color(&self.theme.faded),
            );
        }

        for (y, x1, x2) in horizontal {
            crate::draw::draw_hline(
                frame,
                x1 as f32 * CHAR_WIDTH,
                x2 as f32 * CHAR_WIDTH,
                y as f32 * crate::LINE_HEIGHT,
                rgb_to_color(&self.theme.faded),
            );
        }
    }

    fn draw_hidden_pane_count(&self, frame: &mut canvas::Frame, size: Size, hidden_count: usize) {
        if hidden_count == 0 {
            return;
        }

        let indicator = format!("[{hidden_count} hidden]");
        draw_text_right(
            frame,
            size.width - CHAR_WIDTH,
            2.0,
            &indicator,
            rgb_to_color(&self.theme.faded),
        );
    }
}
