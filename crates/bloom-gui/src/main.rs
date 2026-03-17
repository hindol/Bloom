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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use iced::widget::canvas::{Cache, Canvas};
use iced::{keyboard, window, Element, Length, Size, Subscription, Task};

use crate::canvas::EditorCanvas;
use crate::keys::convert_key;

pub(crate) const FONT_SIZE: f32 = 14.0;
pub(crate) const LINE_HEIGHT: f32 = FONT_SIZE * 1.6;
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
    /// Set to true when the editor thread exits (on :q / :qa).
    quit_flag: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
enum Message {
    KeyboardEvent(keyboard::Event),
    Tick,
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
        },
        Task::none(),
    )
}

fn update(state: &mut BloomApp, message: Message) -> Task<Message> {
    match message {
        Message::Tick => {
            if state.quit_flag.load(Ordering::SeqCst) {
                return iced::exit();
            }
            while let Ok(frame) = state.frame_rx.try_recv() {
                if let Some(palette) = bloom_md::theme::palette_by_name(&frame.theme_name) {
                    state.theme = palette;
                }
                state.frame = Some(frame);
            }
            state.canvas_cache.clear();
        }
        Message::KeyboardEvent(event) => {
            if let keyboard::Event::KeyPressed { modified_key, modifiers, .. } = event {
                if let Some(key_event) = convert_key(modified_key, modifiers) {
                    let _ = state.frontend_tx.send(FrontendEvent::Key(key_event));
                }
            }
        }
    }

    Task::none()
}

fn view(state: &BloomApp) -> Element<'_, Message> {
    Canvas::new(EditorCanvas {
        frame: state.frame.as_deref(),
        theme: state.theme,
        cache: &state.canvas_cache,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn subscription(_state: &BloomApp) -> Subscription<Message> {
    Subscription::batch([
        iced::time::every(std::time::Duration::from_millis(16)).map(|_| Message::Tick),
        keyboard::listen().map(Message::KeyboardEvent),
    ])
}
