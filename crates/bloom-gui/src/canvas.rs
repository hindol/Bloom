use std::collections::BTreeSet;

use bloom_core::render::{PaneFrame, PaneKind, RenderFrame};
use bloom_md::theme::ThemePalette;
use iced::widget::canvas::{self, Cache, Geometry};
use iced::{Rectangle, Renderer, Size, Theme};

use crate::draw::{drawer, inline, notification, overlay, pane, draw_text_right};
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, Message};

/// Animation speed: fraction of remaining distance covered per frame.
const LERP_FACTOR: f32 = 0.6;
/// Snap threshold: if within this many pixels, jump to target.
const SNAP_THRESHOLD: f32 = 0.5;

fn lerp_snap(current: f32, target: f32) -> f32 {
    let diff = target - current;
    if diff.abs() < SNAP_THRESHOLD { target } else { current + diff * LERP_FACTOR }
}

/// Smooth animation state for cursor and scroll.
pub(crate) struct AnimationState {
    cursor_y: f32,
    highlight_y: f32,
    scroll_y: f32,
    initialized: bool,
}

impl Default for AnimationState {
    fn default() -> Self {
        Self { cursor_y: 0.0, highlight_y: 0.0, scroll_y: 0.0, initialized: false }
    }
}

impl AnimationState {
    pub fn advance(&mut self, target_cursor_y: f32, target_scroll_y: f32) -> bool {
        if !self.initialized {
            self.cursor_y = target_cursor_y;
            self.highlight_y = target_cursor_y;
            self.scroll_y = target_scroll_y;
            self.initialized = true;
            return false;
        }
        let prev_c = self.cursor_y;
        let prev_s = self.scroll_y;
        self.cursor_y = lerp_snap(self.cursor_y, target_cursor_y);
        self.highlight_y = lerp_snap(self.highlight_y, target_cursor_y);
        self.scroll_y = lerp_snap(self.scroll_y, target_scroll_y);
        (self.cursor_y - prev_c).abs() > 0.01 || (self.scroll_y - prev_s).abs() > 0.01
    }
    pub fn cursor_y(&self) -> f32 { self.cursor_y }
    pub fn highlight_y(&self) -> f32 { self.highlight_y }
}

// ---------------------------------------------------------------------------
// Base layer: panes + bottom drawers
// ---------------------------------------------------------------------------

pub(crate) struct BaseCanvas<'a> {
    pub(crate) frame: Option<&'a RenderFrame>,
    pub(crate) theme: &'a ThemePalette,
    pub(crate) cache: &'a Cache,
    pub(crate) anim: &'a AnimationState,
}

impl<'a> canvas::Program<Message> for BaseCanvas<'a> {
    type State = ();

    fn draw(
        &self, _state: &(), renderer: &Renderer, _theme: &Theme,
        bounds: Rectangle, _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(iced::Point::ORIGIN, bounds.size(), rgb_to_color(&self.theme.background));

            let Some(rf) = self.frame else { return };

            if let Some(wizard) = rf.panes.iter().find_map(|p| match &p.kind {
                PaneKind::SetupWizard(w) => Some(w), _ => None,
            }) {
                overlay::draw_setup_wizard(frame, bounds.size(), wizard, self.theme);
                return;
            }

            for pf in &rf.panes {
                let anim = if pf.is_active {
                    Some((self.anim.cursor_y(), self.anim.highlight_y()))
                } else { None };
                pane::draw_pane(frame, pf, self.theme, anim);
            }

            draw_split_borders(frame, &rf.panes, self.theme);
            draw_hidden_count(frame, bounds.size(), rf.hidden_pane_count, self.theme);

            let active = rf.panes.iter().find(|p| p.is_active);

            if let Some(s) = &rf.context_strip {
                drawer::draw_context_strip(frame, bounds.size(), s, self.theme);
            }
            if let Some(s) = &rf.temporal_strip {
                drawer::draw_temporal_strip(frame, bounds.size(), s, self.theme, active);
            }
            if let Some(wk) = &rf.which_key {
                drawer::draw_which_key(frame, bounds.size(), wk, self.theme);
            }
        });
        vec![geometry]
    }
}

// ---------------------------------------------------------------------------
// Overlay layer: picker, dialog, date picker, view, inline menu, notifications
// Separate Canvas ⇒ composites cleanly over the base via Iced Stack.
// ---------------------------------------------------------------------------

pub(crate) struct OverlayCanvas<'a> {
    pub(crate) frame: Option<&'a RenderFrame>,
    pub(crate) theme: &'a ThemePalette,
    pub(crate) cache: &'a Cache,
}

impl<'a> canvas::Program<Message> for OverlayCanvas<'a> {
    type State = ();

    fn draw(
        &self, _state: &(), renderer: &Renderer, _theme: &Theme,
        bounds: Rectangle, _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let Some(rf) = self.frame else { return vec![] };

        let has_any = rf.picker.is_some()
            || rf.dialog.is_some()
            || rf.date_picker.is_some()
            || rf.view.is_some()
            || rf.inline_menu.is_some()
            || !rf.notifications.is_empty();

        if !has_any { return vec![] }

        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let active = rf.panes.iter().find(|p| p.is_active);

            if let Some(m) = &rf.inline_menu {
                inline::draw_inline_menu(frame, bounds.size(), active, m, self.theme);
            }
            if let Some(dp) = &rf.date_picker {
                overlay::draw_date_picker(frame, bounds.size(), dp, self.theme);
            }
            if let Some(p) = &rf.picker {
                overlay::draw_picker(frame, bounds.size(), p, self.theme);
            }
            if let Some(d) = &rf.dialog {
                overlay::draw_dialog(frame, bounds.size(), d, self.theme);
            }
            if let Some(v) = &rf.view {
                overlay::draw_view(frame, bounds.size(), v, self.theme);
            }
            if !rf.notifications.is_empty() {
                notification::draw_notifications(frame, bounds.size(), &rf.notifications, self.theme);
            }
        });
        vec![geometry]
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn draw_split_borders(frame: &mut canvas::Frame, panes: &[PaneFrame], theme: &ThemePalette) {
    let mut vert = BTreeSet::new();
    let mut horiz = BTreeSet::new();
    for (i, l) in panes.iter().enumerate() {
        for r in panes.iter().skip(i + 1) {
            let lr = l.rect.x + l.rect.width;
            let rr = r.rect.x + r.rect.width;
            let lb = l.rect.y + l.rect.total_height;
            let rb = r.rect.y + r.rect.total_height;
            if lr == r.rect.x || rr == l.rect.x {
                let x = if lr == r.rect.x { lr } else { rr };
                let y1 = l.rect.y.max(r.rect.y);
                let y2 = lb.min(rb);
                if y2 > y1 { vert.insert((x, y1, y2)); }
            }
            if lb == r.rect.y || rb == l.rect.y {
                let y = if lb == r.rect.y { lb } else { rb };
                let x1 = l.rect.x.max(r.rect.x);
                let x2 = (l.rect.x + l.rect.width).min(r.rect.x + r.rect.width);
                if x2 > x1 { horiz.insert((y, x1, x2)); }
            }
        }
    }
    for (x, y1, y2) in vert {
        crate::draw::draw_vline(frame, x as f32 * CHAR_WIDTH, y1 as f32 * crate::LINE_HEIGHT, y2 as f32 * crate::LINE_HEIGHT, rgb_to_color(&theme.faded));
    }
    for (y, x1, x2) in horiz {
        crate::draw::draw_hline(frame, x1 as f32 * CHAR_WIDTH, x2 as f32 * CHAR_WIDTH, y as f32 * crate::LINE_HEIGHT, rgb_to_color(&theme.faded));
    }
}

fn draw_hidden_count(frame: &mut canvas::Frame, size: Size, count: usize, theme: &ThemePalette) {
    if count == 0 { return }
    draw_text_right(frame, size.width - CHAR_WIDTH, 2.0, &format!("[{count} hidden]"), rgb_to_color(&theme.faded));
}
