use std::collections::BTreeSet;

use bloom_core::render::{PaneFrame, PaneKind, RenderFrame};
use bloom_md::theme::ThemePalette;
use iced::mouse;
use iced::widget::canvas::{self, Action, Cache, Event, Geometry};
use iced::{Rectangle, Renderer, Size, Theme};

use crate::draw::{draw_text_right, drawer, inline, notification, overlay, pane};
use crate::layout::FrameLayout;
use crate::remote::RemoteHints;
use crate::theme::rgb_to_color;
use crate::{CHAR_WIDTH, Message};

/// Animation speed: fraction of remaining distance covered per frame.
const LERP_FACTOR: f32 = 0.6;
/// Snap threshold: if within this many pixels, jump to target.
const SNAP_THRESHOLD: f32 = 0.5;

fn lerp_snap(current: f32, target: f32) -> f32 {
    let diff = target - current;
    if diff.abs() < SNAP_THRESHOLD {
        target
    } else {
        current + diff * LERP_FACTOR
    }
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
        Self {
            cursor_y: 0.0,
            highlight_y: 0.0,
            scroll_y: 0.0,
            initialized: false,
        }
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
    /// Jump to target instantly (no lerp). Used for remote sessions.
    pub fn snap(&mut self, target_cursor_y: f32, target_scroll_y: f32) {
        self.cursor_y = target_cursor_y;
        self.highlight_y = target_cursor_y;
        self.scroll_y = target_scroll_y;
        self.initialized = true;
    }
    pub fn cursor_y(&self) -> f32 {
        self.cursor_y
    }
    pub fn highlight_y(&self) -> f32 {
        self.highlight_y
    }
}

// ---------------------------------------------------------------------------
// Base layer: panes + bottom drawers
// ---------------------------------------------------------------------------

pub(crate) struct BaseCanvas<'a> {
    pub(crate) frame: Option<&'a RenderFrame>,
    pub(crate) theme: &'a ThemePalette,
    pub(crate) cache: &'a Cache,
    pub(crate) anim: &'a AnimationState,
    pub(crate) remote: RemoteHints,
    pub(crate) cursor_visible: bool,
}

impl<'a> canvas::Program<Message> for BaseCanvas<'a> {
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        let Some(frame) = self.frame else {
            return None;
        };
        let Some(position) = cursor.position_in(bounds) else {
            return None;
        };

        match event {
            Event::Mouse(mouse::Event::WheelScrolled { delta })
                if editor_pane_at_position(frame, bounds.size(), position).is_some() =>
            {
                let lines = scroll_lines(delta);

                if lines != 0 {
                    return Some(Action::publish(Message::Scroll(lines)).and_capture());
                }

                None
            }
            _ => None,
        }
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let Some(frame) = self.frame else {
            return mouse::Interaction::default();
        };
        let Some(position) = cursor.position_in(bounds) else {
            return mouse::Interaction::default();
        };

        // Check if cursor is near a split border — show resize arrow.
        if let Some(dir) = split_border_hit(frame, position) {
            return dir;
        }

        if editor_pane_at_position(frame, bounds.size(), position).is_some() {
            mouse::Interaction::Text
        } else {
            mouse::Interaction::default()
        }
    }

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

            let Some(rf) = self.frame else {
                return;
            };

            if let Some(wizard) = rf.panes.iter().find_map(|p| match &p.kind {
                PaneKind::SetupWizard(w) => Some(w),
                _ => None,
            }) {
                overlay::draw_setup_wizard(frame, bounds.size(), wizard, self.theme);
                return;
            }

            // Compute layout once for the entire frame.
            let layout = FrameLayout::compute(bounds.size().width, bounds.size().height, rf);
            let modeline_bottom = layout.modeline.y + layout.modeline.height;

            // Compute how many status bars are above each pane's Y origin.
            // Panes are sorted by Y in the frame. For each pane, count panes
            // whose total_height boundary is at or above this pane's Y.
            for pf in &rf.panes {
                let status_bars_above = rf
                    .panes
                    .iter()
                    .filter(|other| other.rect.y + other.rect.total_height <= pf.rect.y)
                    .count();
                let (px, py, pw, ch) = pane::pane_pixel_rect(&pf.rect, status_bars_above, bounds.size());
                let anim = if pf.is_active && !self.remote.skip_animation() {
                    Some((self.anim.cursor_y(), self.anim.highlight_y()))
                } else {
                    None
                };
                pane::draw_pane(
                    frame,
                    pf,
                    self.theme,
                    anim,
                    self.cursor_visible,
                    px,
                    py,
                    pw,
                    ch,
                    modeline_bottom,
                );
            }

            draw_split_borders(frame, &rf.panes, self.theme);
            draw_hidden_count(frame, bounds.size(), rf.hidden_pane_count, self.theme);

            let drawer_rect = layout.drawer;
            if let Some(s) = &rf.context_strip {
                drawer::draw_context_strip(frame, bounds.size(), s, self.theme, drawer_rect);
            }
            if let Some(s) = &rf.temporal_strip {
                // Draw only the strip drawer (nodes, hints) — NOT the diff preview.
                // Diff preview is on a separate layer (DiffCanvas) to avoid
                // text blending issues within a single Canvas.
                drawer::draw_temporal_strip_drawer(frame, bounds.size(), s, self.theme, drawer_rect);
            }
            if let Some(wk) = &rf.which_key {
                drawer::draw_which_key(frame, bounds.size(), wk, self.theme, drawer_rect);
            }
        });
        vec![geometry]
    }
}

// ---------------------------------------------------------------------------
// Diff preview layer: renders between base and overlay.
// Opaque fill over the active pane content area when temporal history is active.
// ---------------------------------------------------------------------------

pub(crate) struct DiffCanvas<'a> {
    pub(crate) frame: Option<&'a RenderFrame>,
    pub(crate) theme: &'a ThemePalette,
    pub(crate) cache: &'a Cache,
}

impl<'a> canvas::Program<Message> for DiffCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let Some(rf) = self.frame else {
            return vec![];
        };
        let Some(strip) = &rf.temporal_strip else {
            return vec![];
        };

        use bloom_core::render::TemporalMode;
        if !matches!(strip.mode, TemporalMode::PageHistory | TemporalMode::BlockHistory) {
            return vec![];
        }
        if strip.preview_lines.is_empty() {
            return vec![];
        }

        let active = rf.panes.iter().find(|p| p.is_active);
        let layout = FrameLayout::compute(bounds.size().width, bounds.size().height, rf);

        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            drawer::draw_temporal_diff_preview(frame, bounds.size(), strip, self.theme, active, layout.drawer);
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
    pub(crate) remote: RemoteHints,
}

impl<'a> canvas::Program<Message> for OverlayCanvas<'a> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<Geometry> {
        let Some(rf) = self.frame else {
            return vec![];
        };

        let has_any = rf.picker.is_some()
            || rf.dialog.is_some()
            || rf.date_picker.is_some()
            || rf.view.is_some()
            || rf.inline_menu.is_some()
            || !rf.notifications.is_empty();

        if !has_any {
            return vec![];
        }

        let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
            let active = rf.panes.iter().find(|p| p.is_active);
            let scrim_alpha = if self.remote.opaque_scrim() { 1.0 } else { 0.50 };
            let layout = FrameLayout::compute(bounds.size().width, bounds.size().height, rf);
            let drawer_rect = layout.drawer;

            if let Some(m) = &rf.inline_menu {
                inline::draw_inline_menu(frame, bounds.size(), active, m, self.theme);
            }
            if let Some(dp) = &rf.date_picker {
                overlay::draw_date_picker(frame, bounds.size(), dp, self.theme, drawer_rect);
            }
            if let Some(p) = &rf.picker {
                overlay::draw_picker(frame, bounds.size(), p, self.theme, scrim_alpha, drawer_rect);
            }
            if let Some(d) = &rf.dialog {
                overlay::draw_dialog(frame, bounds.size(), d, self.theme);
            }
            if let Some(v) = &rf.view {
                overlay::draw_view(frame, bounds.size(), v, self.theme, drawer_rect);
            }
            if !rf.notifications.is_empty() {
                let modeline_y = Some(layout.modeline.y);
                notification::draw_notifications(frame, bounds.size(), &rf.notifications, self.theme, modeline_y);
            }
        });
        vec![geometry]
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn editor_pane_at_position(
    frame: &RenderFrame,
    window_size: Size,
    position: iced::Point,
) -> Option<&PaneFrame> {
    frame.panes.iter().find(|pane| {
        if !matches!(pane.kind, PaneKind::Editor) {
            return false;
        }

        let status_bars_above = frame
            .panes
            .iter()
            .filter(|other| other.rect.y + other.rect.total_height <= pane.rect.y)
            .count();
        let (pane_x, pane_y, pane_w, content_h) =
            pane::pane_pixel_rect(&pane.rect, status_bars_above, window_size);

        position.x >= pane_x
            && position.x <= pane_x + pane_w
            && position.y >= pane_y
            && position.y <= pane_y + content_h
    })
}

fn scroll_lines(delta: &mouse::ScrollDelta) -> i32 {
    let lines = match delta {
        mouse::ScrollDelta::Lines { y, .. } => (-*y * 3.0).round() as i32,
        mouse::ScrollDelta::Pixels { y, .. } => (-*y / crate::LINE_HEIGHT).round() as i32,
    };

    if lines == 0 {
        match delta {
            mouse::ScrollDelta::Lines { y, .. } if *y != 0.0 => (-y.signum()) as i32,
            mouse::ScrollDelta::Pixels { y, .. } if *y != 0.0 => (-y.signum()) as i32,
            _ => 0,
        }
    } else {
        lines
    }
}

/// Hit-test pixel position against split borders.
/// Returns the appropriate resize cursor if within `BORDER_HIT_PX` of a border.
const BORDER_HIT_PX: f32 = 4.0;

fn split_border_hit(rf: &RenderFrame, pos: iced::Point) -> Option<mouse::Interaction> {
    let panes = &rf.panes;
    for (i, l) in panes.iter().enumerate() {
        for r in panes.iter().skip(i + 1) {
            let lr = l.rect.x + l.rect.width;
            let rr = r.rect.x + r.rect.width;
            let lb = l.rect.y + l.rect.total_height;
            let rb = r.rect.y + r.rect.total_height;
            // Vertical border (left-right split).
            if lr == r.rect.x || rr == l.rect.x {
                let x = if lr == r.rect.x { lr } else { rr };
                let y1 = l.rect.y.max(r.rect.y);
                let y2 = lb.min(rb);
                if y2 > y1 {
                    let px = x as f32 * CHAR_WIDTH;
                    let py1 = y1 as f32 * crate::LINE_HEIGHT;
                    let py2 = y2 as f32 * crate::LINE_HEIGHT;
                    if (pos.x - px).abs() < BORDER_HIT_PX && pos.y >= py1 && pos.y <= py2 {
                        return Some(mouse::Interaction::ResizingHorizontally);
                    }
                }
            }
            // Horizontal border (top-bottom split).
            if lb == r.rect.y || rb == l.rect.y {
                let y = if lb == r.rect.y { lb } else { rb };
                let x1 = l.rect.x.max(r.rect.x);
                let x2 = (l.rect.x + l.rect.width).min(r.rect.x + r.rect.width);
                if x2 > x1 {
                    let py = y as f32 * crate::LINE_HEIGHT;
                    let px1 = x1 as f32 * CHAR_WIDTH;
                    let px2 = x2 as f32 * CHAR_WIDTH;
                    if (pos.y - py).abs() < BORDER_HIT_PX && pos.x >= px1 && pos.x <= px2 {
                        return Some(mouse::Interaction::ResizingVertically);
                    }
                }
            }
        }
    }
    None
}

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
                if y2 > y1 {
                    vert.insert((x, y1, y2));
                }
            }
            if lb == r.rect.y || rb == l.rect.y {
                let y = if lb == r.rect.y { lb } else { rb };
                let x1 = l.rect.x.max(r.rect.x);
                let x2 = (l.rect.x + l.rect.width).min(r.rect.x + r.rect.width);
                if x2 > x1 {
                    horiz.insert((y, x1, x2));
                }
            }
        }
    }
    for (x, y1, y2) in vert {
        crate::draw::draw_vline(
            frame,
            x as f32 * CHAR_WIDTH,
            y1 as f32 * crate::LINE_HEIGHT,
            y2 as f32 * crate::LINE_HEIGHT,
            rgb_to_color(&theme.subtle),
        );
    }
    for (y, x1, x2) in horiz {
        crate::draw::draw_hline(
            frame,
            x1 as f32 * CHAR_WIDTH,
            x2 as f32 * CHAR_WIDTH,
            y as f32 * crate::LINE_HEIGHT,
            rgb_to_color(&theme.subtle),
        );
    }
}

fn draw_hidden_count(frame: &mut canvas::Frame, size: Size, count: usize, theme: &ThemePalette) {
    if count == 0 {
        return;
    }
    draw_text_right(
        frame,
        size.width - CHAR_WIDTH,
        2.0,
        &format!("[{count} hidden]"),
        rgb_to_color(&theme.faded),
    );
}


