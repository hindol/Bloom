mod input;

use std::io;

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::BloomEditor;
use crossbeam::channel;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use bloom_tui::theme::TuiTheme;

fn main() -> io::Result<()> {
    // Initialize tracing subscriber (file logging) before anything else
    let vault_path = default_vault_path();
    init_logging(&vault_path);

    // Terminal setup
    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal);

    // Terminal teardown
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, cursor::Show)?;
    terminal.show_cursor()?;

    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    let vault_path = default_vault_path();
    let config_path = std::path::Path::new(&vault_path).join("config.toml");
    let config = if config_path.exists() {
        Config::load(&config_path).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "config parse failed, using defaults");
            Config::defaults()
        })
    } else {
        Config::defaults()
    };
    let config_ref = config.clone();
    let mut editor = BloomEditor::new(config).map_err(|e| io::Error::other(format!("{e:?}")))?;

    // First-run detection: show setup wizard if no vault exists
    if editor.needs_setup() {
        editor.start_wizard();
    } else {
        // Existing vault — initialize and spawn background indexer
        let vault_path = default_vault_path();
        if let Err(e) = editor.init_vault(std::path::Path::new(&vault_path)) {
            return Err(io::Error::other(format!("{e}")));
        }
        editor.startup();
    }

    // Update viewport to terminal size
    let size = terminal.size()?;
    editor.resize(size.height as usize, size.width as usize);

    // Spawn dedicated input reader → converts crossterm events to FrontendEvents
    let (frontend_tx, frontend_rx) = channel::unbounded();
    let tx = frontend_tx.clone();
    std::thread::Builder::new()
        .name("bloom-input".into())
        .spawn(move || {
            while let Ok(ev) = event::read() {
                let msg = match ev {
                    Event::Key(key_event) => {
                        if key_event.kind != KeyEventKind::Press {
                            continue;
                        }
                        if let Some(bloom_key) = input::convert_key(key_event) {
                            bloom_core::event_loop::FrontendEvent::Key(bloom_key)
                        } else {
                            continue;
                        }
                    }
                    Event::Resize(w, h) => {
                        bloom_core::event_loop::FrontendEvent::Resize { cols: w, rows: h }
                    }
                    _ => continue,
                };
                if tx.send(msg).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn input thread");

    // Run the shared event loop — same code path as the GUI
    let mut render_error: Option<io::Error> = None;

    // Fallback theme in case palette_by_name fails (shouldn't happen)
    let fallback_theme = TuiTheme::new(editor.theme());

    bloom_core::event_loop::run_event_loop(&mut editor, &frontend_rx, |action| match action {
        bloom_core::event_loop::LoopAction::Render(frame) => {
            // Resolve theme fresh each frame for live preview support
            let theme = bloom_md::theme::palette_by_name(&frame.theme_name)
                .map(TuiTheme::new)
                .unwrap_or_else(|| fallback_theme.clone());

            let result = terminal.draw(|f| {
                if let Some(pane) = frame.panes.iter().find(|p| p.is_active) {
                    let cursor_style = match pane.cursor.shape {
                        bloom_core::render::CursorShape::Block => {
                            cursor::SetCursorStyle::SteadyBlock
                        }
                        bloom_core::render::CursorShape::Bar => cursor::SetCursorStyle::SteadyBar,
                        bloom_core::render::CursorShape::Underline => {
                            cursor::SetCursorStyle::SteadyUnderScore
                        }
                    };
                    let _ = execute!(std::io::stdout(), cursor_style);
                }

                bloom_tui::render::draw(f, &frame, &theme, &config_ref);
            });
            if let Err(e) = result {
                render_error = Some(e);
                return false;
            }
            true
        }
        bloom_core::event_loop::LoopAction::Quit => false,
    });

    render_error.map_or(Ok(()), Err)
}

fn init_logging(vault_path: &str) {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let log_dir = std::path::Path::new(vault_path).join(".bloom").join("logs");
    let _ = std::fs::create_dir_all(&log_dir);

    // Rotate on startup: if bloom.log > 5MB, rotate existing files
    rotate_logs(&log_dir);

    let log_file = log_dir.join("bloom.log");
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file)
    {
        Ok(f) => f,
        Err(_) => return, // can't log — continue without crashing
    };

    let file_layer = fmt::layer()
        .json()
        .with_writer(std::sync::Mutex::new(file))
        .with_target(true)
        .with_span_events(fmt::format::FmtSpan::CLOSE);

    let filter = EnvFilter::try_from_env("BLOOM_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(file_layer)
        .init();

    tracing::info!("bloom started");
}

fn rotate_logs(log_dir: &std::path::Path) {
    let current = log_dir.join("bloom.log");
    let max_size = 5 * 1024 * 1024; // 5 MB
    let max_files = 3;

    let needs_rotate = current
        .metadata()
        .map(|m| m.len() > max_size)
        .unwrap_or(false);

    if !needs_rotate {
        return;
    }

    // Delete oldest, shift others down
    let oldest = log_dir.join(format!("bloom.{max_files}.log"));
    let _ = std::fs::remove_file(&oldest);

    for i in (1..max_files).rev() {
        let from = log_dir.join(format!("bloom.{i}.log"));
        let to = log_dir.join(format!("bloom.{}.log", i + 1));
        let _ = std::fs::rename(&from, &to);
    }

    let _ = std::fs::rename(&current, log_dir.join("bloom.1.log"));
}
