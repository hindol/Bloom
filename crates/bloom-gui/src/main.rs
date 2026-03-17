//! Bloom GUI — Iced (Canvas) frontend over bloom-core.

mod canvas;
mod draw;
mod keys;
mod theme;

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::event_loop::{FrontendEvent, LoopAction};
use bloom_core::render::RenderFrame;
use bloom_core::BloomEditor;
use bloom_md::theme::{ThemePalette, BLOOM_DARK};
use crossbeam::channel::{Receiver, Sender};
use iced::widget::canvas::{Cache, Canvas};
use iced::{keyboard, window, Element, Length, Size, Subscription, Task};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::canvas::{AnimationState, EditorCanvas};
use crate::keys::convert_key;

pub(crate) const FONT_SIZE: f32 = 13.0;
pub(crate) const LINE_HEIGHT: f32 = FONT_SIZE * 1.4;
/// Vertical offset to center text within a LINE_HEIGHT row.
pub(crate) const TEXT_Y_OFFSET: f32 = (LINE_HEIGHT - FONT_SIZE) / 2.0;
/// Status bar is taller than content lines for visual prominence.
pub(crate) const STATUS_BAR_HEIGHT: f32 = LINE_HEIGHT * 1.35;
pub(crate) const CHAR_WIDTH: f32 = FONT_SIZE * 0.6;
pub(crate) const GUTTER_CHARS: usize = 5;
pub(crate) const GUTTER_WIDTH: f32 = GUTTER_CHARS as f32 * CHAR_WIDTH;

fn main() -> iced::Result {
    iced::application(boot, update, view)
        .title("Bloom")
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
    canvas_cache: Cache,
    quit_flag: Arc<AtomicBool>,
    anim: AnimationState,
    /// True while animation is in flight — drives high-frequency ticks.
    animating: bool,
}

#[derive(Debug, Clone)]
enum Message {
    KeyboardEvent(keyboard::Event),
    /// Animation tick (~120Hz, only while animating).
    AnimTick,
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
    let initial_rows = (800.0 / LINE_HEIGHT) as u16;
    let _ = frontend_tx.send(FrontendEvent::Resize {
        cols: initial_cols,
        rows: initial_rows,
    });

    (
        BloomApp {
            frontend_tx,
            frame_rx,
            frame: None,
            theme: &BLOOM_DARK,
            canvas_cache: Cache::default(),
            quit_flag,
            anim: AnimationState::default(),
            // Start animating so the tick subscription fires and picks up
            // the initial render frame from the editor thread.
            animating: true,
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
            let still_moving = if let Some(frame) = &state.frame {
                if let Some(pane) = frame.panes.iter().find(|p| p.is_active) {
                    let cursor_row = pane.cursor.line.saturating_sub(pane.scroll_offset);
                    let target_cursor_y =
                        pane.rect.y as f32 * LINE_HEIGHT + cursor_row as f32 * LINE_HEIGHT;
                    let target_scroll_y = pane.scroll_offset as f32 * LINE_HEIGHT;
                    state.anim.advance(target_cursor_y, target_scroll_y)
                } else {
                    false
                }
            } else {
                false
            };
            state.animating = still_moving;
            state.canvas_cache.clear();
        }
        Message::KeyboardEvent(event) => {
            if let keyboard::Event::KeyPressed {
                modified_key,
                modifiers,
                ..
            } = event
            {
                if let Some(key_event) = convert_key(modified_key, modifiers) {
                    let _ = state.frontend_tx.send(FrontendEvent::Key(key_event));
                }
            }
            // Key sent — bump to animation speed so response frame is picked up
            // on the next tick (8ms), and start the animation subscription.
            state.animating = true;
            state.canvas_cache.clear();
        }
    }

    Task::none()
}

fn view(state: &BloomApp) -> Element<'_, Message> {
    Canvas::new(EditorCanvas {
        frame: state.frame.as_deref(),
        theme: state.theme,
        cache: &state.canvas_cache,
        anim: &state.anim,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn subscription(state: &BloomApp) -> Subscription<Message> {
    let mut subs = vec![
        // Keyboard — always active, event-driven (zero cost when idle).
        keyboard::listen().map(Message::KeyboardEvent),
    ];

    // Animation tick — only while animating (~120Hz). Absent when idle = zero CPU.
    if state.animating {
        subs.push(
            iced::time::every(std::time::Duration::from_millis(8)).map(|_| Message::AnimTick),
        );
    }

    Subscription::batch(subs)
}

impl BloomApp {
    /// Drain any pending frames from the editor thread (non-blocking).
    /// Called on every update to catch frames promptly.
    fn drain_frames(&mut self) {
        while let Ok(frame) = self.frame_rx.try_recv() {
            if let Some(palette) = bloom_md::theme::palette_by_name(&frame.theme_name) {
                self.theme = palette;
            }
            self.frame = Some(frame);
            self.animating = true;
        }
    }
}
