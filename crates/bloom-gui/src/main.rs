//! Bloom GUI — Iced (Canvas) frontend over bloom-core.

mod canvas;
mod draw;
mod keys;
mod remote;
mod theme;

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::event_loop::{FrontendEvent, LoopAction};
use bloom_core::render::RenderFrame;
use bloom_core::types::KeyEvent;
use bloom_core::BloomEditor;
use bloom_md::theme::{ThemePalette, BLOOM_DARK};
use crossbeam::channel::{Receiver, Sender};
use iced::widget::canvas::{Cache, Canvas};
use iced::{keyboard, window, Element, Font, Length, Size, Subscription, Task};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::canvas::{AnimationState, BaseCanvas, DiffCanvas, OverlayCanvas};
use crate::keys::convert_key;
use crate::remote::RemoteHints;

pub(crate) const FONT_SIZE: f32 = 13.0;
pub(crate) const LINE_HEIGHT: f32 = FONT_SIZE * 1.4;
/// Vertical offset to center text within a LINE_HEIGHT row.
pub(crate) const TEXT_Y_OFFSET: f32 = (LINE_HEIGHT - FONT_SIZE) / 2.0;
/// Status bar is taller than content lines for visual prominence.
pub(crate) const STATUS_BAR_HEIGHT: f32 = LINE_HEIGHT * 1.5;
pub(crate) const CHAR_WIDTH: f32 = FONT_SIZE * 0.6;
pub(crate) const GUTTER_CHARS: usize = 5;
pub(crate) const GUTTER_WIDTH: f32 = GUTTER_CHARS as f32 * CHAR_WIDTH;
/// Bottom safe area to avoid macOS window corner radius clipping.
pub(crate) const BOTTOM_SAFE_AREA: f32 = 6.0;
pub(crate) const EDITOR_FONT: Font = Font::with_name("JetBrains Mono");

#[allow(dead_code)]
pub(crate) struct FontMetrics {
    pub char_width: f32,
    pub line_height: f32,
    pub font_size: f32,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            char_width: FONT_SIZE * 0.6,
            line_height: FONT_SIZE * 1.4,
            font_size: FONT_SIZE,
        }
    }
}

const JETBRAINS_MONO: &[u8] = include_bytes!("../fonts/JetBrainsMono-Regular.ttf");
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

fn title(state: &BloomApp) -> String {
    state
        .frame
        .as_ref()
        .and_then(|frame| frame.panes.iter().find(|pane| pane.is_active))
        .map(|pane| format!("{} — Bloom", pane.title))
        .unwrap_or_else(|| "Bloom".to_string())
}

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title(title)
        .font(JETBRAINS_MONO)
        .subscription(subscription)
        .window(window::Settings {
            size: Size::new(1200.0, 800.0),
            min_size: Some(Size::new(400.0, 300.0)),
            ..Default::default()
        })
        .antialiasing(true)
        .run()
}

struct BloomApp {
    frontend_tx: Sender<FrontendEvent>,
    frame_rx: Receiver<Box<RenderFrame>>,
    frame: Option<Box<RenderFrame>>,
    theme: &'static ThemePalette,
    base_cache: Cache,
    overlay_cache: Cache,
    diff_cache: Cache,
    quit_flag: Arc<AtomicBool>,
    anim: AnimationState,
    animating: bool,
    cursor_visible: bool,
    blink_timer: Option<Instant>,
    /// Last window size sent to core (for resize detection).
    last_size: (u16, u16),
    /// Remote session rendering hints (detected once at startup).
    remote: RemoteHints,
    /// Font metrics for layout calculations (prep for proportional fonts).
    #[allow(dead_code)]
    font_metrics: FontMetrics,
}

#[derive(Debug, Clone)]
enum Message {
    KeyboardEvent(keyboard::Event),
    /// Animation tick (~120Hz, only while animating).
    AnimTick,
    /// Window was resized.
    WindowResized(Size),
    /// Mouse wheel scroll in the editor area.
    Scroll(i32),
}

fn boot() -> (BloomApp, Task<Message>) {
    let config_path_str = default_vault_path();
    let config_path = std::path::Path::new(&config_path_str).join("config.toml");
    let config = if config_path.exists() {
        Config::load(&config_path).unwrap_or_else(|_| Config::defaults())
    } else {
        Config::defaults()
    };

    let mut editor = BloomEditor::new(config).expect("failed to create editor");

    let vault_path = default_vault_path();
    let vault_root = std::path::Path::new(&vault_path);
    if vault_root.join("config.toml").exists() {
        let _ = editor.init_vault(vault_root);
        editor.startup();
    }

    let (frontend_tx, frontend_rx) = crossbeam::channel::unbounded();
    let (frame_tx, frame_rx) = crossbeam::channel::unbounded::<Box<RenderFrame>>();
    let quit_flag = Arc::new(AtomicBool::new(false));
    let quit_flag_thread = quit_flag.clone();

    std::thread::Builder::new()
        .name("bloom-editor".into())
        .spawn(move || {
            bloom_core::event_loop::run_event_loop(
                &mut editor,
                &frontend_rx,
                |action| match action {
                    LoopAction::Render(frame) => {
                        let _ = frame_tx.send(frame);
                        true
                    }
                    LoopAction::Quit => {
                        quit_flag_thread.store(true, Ordering::SeqCst);
                        false
                    }
                },
            );
        })
        .expect("failed to spawn editor thread");

    let initial_cols = (1200.0 / CHAR_WIDTH) as u16;
    let initial_rows = ((800.0 - BOTTOM_SAFE_AREA) / LINE_HEIGHT) as u16;
    let _ = frontend_tx.send(FrontendEvent::Resize {
        cols: initial_cols,
        rows: initial_rows,
    });

    let remote = RemoteHints::detect();

    (
        BloomApp {
            frontend_tx,
            frame_rx,
            frame: None,
            theme: &BLOOM_DARK,
            base_cache: Cache::default(),
            diff_cache: Cache::default(),
            overlay_cache: Cache::default(),
            quit_flag,
            anim: AnimationState::default(),
            animating: true,
            cursor_visible: true,
            blink_timer: None,
            last_size: (initial_cols, initial_rows),
            remote,
            font_metrics: FontMetrics::default(),
        },
        Task::none(),
    )
}

fn update(state: &mut BloomApp, message: Message) -> Task<Message> {
    // Always drain pending frames — catches background events (indexer, file
    // watcher) promptly regardless of which message woke us.
    state.drain_frames();

    if state.quit_flag.load(Ordering::SeqCst) {
        return iced::exit();
    }

    match message {
        Message::AnimTick => {
            // Advance animation toward logical cursor/scroll positions.
            // Remote sessions: snap instantly (no lerp), stop after one tick.
            let still_moving = if let Some(frame) = &state.frame {
                if let Some(pane) = frame.panes.iter().find(|p| p.is_active) {
                    let status_bars_above = frame
                        .panes
                        .iter()
                        .filter(|other| other.rect.y + other.rect.total_height <= pane.rect.y)
                        .count();
                    let pane_y = pane.rect.y as f32 * LINE_HEIGHT
                        + status_bars_above as f32 * (STATUS_BAR_HEIGHT - LINE_HEIGHT);
                    let cursor_row = pane.cursor.line.saturating_sub(pane.scroll_offset);
                    let target_cursor_y = crate::draw::pane::cursor_y_in_pane(
                        &pane.visible_lines, cursor_row, pane_y,
                    );
                    let target_scroll_y = pane.scroll_offset as f32 * LINE_HEIGHT;
                    if state.remote.skip_animation() {
                        state.anim.snap(target_cursor_y, target_scroll_y);
                        false
                    } else {
                        state.anim.advance(target_cursor_y, target_scroll_y)
                    }
                } else {
                    false
                }
            } else {
                false
            };
            let insert_mode = state.is_insert_mode();
            let blink_changed = state.tick_cursor_blink(insert_mode);
            state.animating = still_moving || insert_mode || state.frame.is_none();
            if still_moving || blink_changed {
                state.clear_caches();
            }
        }
        Message::KeyboardEvent(event) => {
            if let keyboard::Event::KeyPressed {
                key,
                modified_key,
                modifiers,
                ..
            } = event
            {
                state.reset_cursor_blink();

                if !state.handle_platform_shortcut(&key, &modified_key, modifiers) {
                    // Try modified_key first (has Shift applied for characters),
                    // fall back to key for named keys like Escape where
                    // modified_key may differ.
                    let bloom_key = convert_key(modified_key.clone(), modifiers)
                        .or_else(|| convert_key(key.clone(), modifiers));
                    if let Some(key_event) = bloom_key {
                        state.send_key_event(key_event);
                    } else {
                        eprintln!("[bloom-gui] unhandled key: key={key:?} modified_key={modified_key:?} modifiers={modifiers:?}");
                    }
                }

                // Key sent — bump to animation speed so response frame is picked up
                // on the next tick (8ms), and start the animation subscription.
                state.animating = true;
                state.clear_caches();
            }
        }
        Message::Scroll(lines) => {
            if lines != 0 {
                state.scroll(lines);
                state.animating = true;
                state.clear_caches();
            }
        }
        Message::WindowResized(size) => {
            let cols = (size.width / CHAR_WIDTH).max(1.0) as u16;
            let rows = ((size.height - BOTTOM_SAFE_AREA) / LINE_HEIGHT).max(1.0) as u16;
            if (cols, rows) != state.last_size {
                state.last_size = (cols, rows);
                let _ = state.frontend_tx.send(FrontendEvent::Resize { cols, rows });
                state.animating = true;
            }
        }
    }

    Task::none()
}

fn view(state: &BloomApp) -> Element<'_, Message> {
    let base = Canvas::new(BaseCanvas {
        frame: state.frame.as_deref(),
        theme: state.theme,
        cache: &state.base_cache,
        anim: &state.anim,
        remote: state.remote,
        cursor_visible: !state.is_insert_mode() || state.cursor_visible,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let diff = Canvas::new(DiffCanvas {
        frame: state.frame.as_deref(),
        theme: state.theme,
        cache: &state.diff_cache,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    let overlay = Canvas::new(OverlayCanvas {
        frame: state.frame.as_deref(),
        theme: state.theme,
        cache: &state.overlay_cache,
        remote: state.remote,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    iced::widget::stack![base, diff, overlay]
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn subscription(state: &BloomApp) -> Subscription<Message> {
    let mut subs = vec![
        // Use listen_raw to get keyboard events regardless of widget capture
        // status. keyboard::listen() filters to Status::Ignored, which misses
        // events when Canvas widgets are in the Stack.
        iced::event::listen_raw(|event, _status, _window| {
            if let iced::Event::Keyboard(kb_event) = event {
                Some(Message::KeyboardEvent(kb_event))
            } else {
                None
            }
        }),
        window::resize_events().map(|(_, size)| Message::WindowResized(size)),
    ];

    // VSync-aligned frame tick — fires at the display's native refresh rate
    // (60Hz, 120Hz, etc.). Automatically adapts when the window moves between
    // monitors. Only active while animating — zero CPU when idle.
    if state.animating {
        subs.push(window::frames().map(|_| Message::AnimTick));
    }

    Subscription::batch(subs)
}

fn key_matches_any(key: &keyboard::Key, expected: &[&str]) -> bool {
    match key.as_ref() {
        keyboard::Key::Character(value) => expected
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate)),
        _ => false,
    }
}

fn is_primary_shortcut(modifiers: keyboard::Modifiers) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.macos_command()
    } else {
        modifiers.control()
    }
}

impl BloomApp {
    fn clear_caches(&mut self) {
        self.base_cache.clear();
        self.diff_cache.clear();
        self.overlay_cache.clear();
    }

    fn is_insert_mode(&self) -> bool {
        self.frame
            .as_ref()
            .and_then(|frame| frame.panes.iter().find(|pane| pane.is_active))
            .map(|pane| pane.status_bar.mode.as_str())
            == Some("INSERT")
    }

    fn tick_cursor_blink(&mut self, insert_mode: bool) -> bool {
        if !insert_mode {
            let changed = !self.cursor_visible || self.blink_timer.is_some();
            self.cursor_visible = true;
            self.blink_timer = None;
            return changed;
        }

        let now = Instant::now();

        match self.blink_timer {
            Some(last) if now.duration_since(last) >= CURSOR_BLINK_INTERVAL => {
                self.cursor_visible = !self.cursor_visible;
                self.blink_timer = Some(now);
                true
            }
            Some(_) => false,
            None => {
                let changed = !self.cursor_visible;
                self.cursor_visible = true;
                self.blink_timer = Some(now);
                changed
            }
        }
    }

    fn reset_cursor_blink(&mut self) {
        self.cursor_visible = true;
        self.blink_timer = Some(Instant::now());
    }

    fn send_key_event(&self, key_event: KeyEvent) {
        let _ = self.frontend_tx.send(FrontendEvent::Key(key_event));
    }

    /// Scroll the viewport by `lines` (positive = down, negative = up).
    /// Each line generates a `j`/`k` key event that moves the cursor, which in
    /// turn updates `scroll_offset` in core when the cursor crosses a viewport
    /// edge. The VSync animation system (`AnimationState`) lerps both the cursor
    /// and scroll positions, providing smooth visual transitions.
    fn scroll(&self, lines: i32) {
        let key = if lines > 0 { 'j' } else { 'k' };

        for _ in 0..lines.abs() {
            self.send_key_event(KeyEvent::char(key));
        }
    }

    fn handle_platform_shortcut(
        &mut self,
        key: &keyboard::Key,
        modified_key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
    ) -> bool {
        if !is_primary_shortcut(modifiers) {
            return false;
        }

        if key_matches_any(key, &["s"]) || key_matches_any(modified_key, &["s"]) {
            self.send_key_event(KeyEvent::ctrl('s'));
            return true;
        }

        if key_matches_any(key, &["q"]) || key_matches_any(modified_key, &["q"]) {
            let _ = self.frontend_tx.send(FrontendEvent::Quit);
            return true;
        }

        if key_matches_any(key, &["+", "="]) || key_matches_any(modified_key, &["+", "="]) {
            eprintln!("TODO: increase font size shortcut");
            return true;
        }

        if key_matches_any(key, &["-"]) || key_matches_any(modified_key, &["-"]) {
            eprintln!("TODO: decrease font size shortcut");
            return true;
        }

        if key_matches_any(key, &["0"]) || key_matches_any(modified_key, &["0"]) {
            eprintln!("TODO: reset font size shortcut");
            return true;
        }

        false
    }

    /// Drain any pending frames from the editor thread (non-blocking).
    /// Called on every update to catch frames promptly.
    fn drain_frames(&mut self) {
        let mut got_frame = false;
        while let Ok(frame) = self.frame_rx.try_recv() {
            if let Some(palette) = bloom_md::theme::palette_by_name(&frame.theme_name) {
                self.theme = palette;
            }
            self.frame = Some(frame);
            got_frame = true;
        }
        if got_frame {
            self.animating = true;
            self.clear_caches();
        }
    }
}
