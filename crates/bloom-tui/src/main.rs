mod input;

use std::io;
use std::time::{Duration, Instant};

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::keymap::dispatch::Action;
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
        Config::load(&config_path).unwrap_or_else(|_| Config::defaults())
    } else {
        Config::defaults()
    };
    let mut editor = BloomEditor::new(config).map_err(|e| io::Error::other(format!("{e:?}")))?;

    // First-run detection: show setup wizard if no vault exists
    if editor.needs_setup() {
        editor.start_wizard();
    } else {
        // Existing vault — initialize and spawn background indexer
        let vault_path = default_vault_path();
        if let Err(e) = editor.init_vault(std::path::Path::new(&vault_path)) {
            // Teardown happens in the caller; print the friendly error to stderr.
            return Err(io::Error::other(format!("{e}")));
        }
        editor.startup();
    }

    // Update viewport to terminal size
    let size = terminal.size()?;
    editor.resize(size.height as usize, size.width as usize);

    // Spawn dedicated input reader thread → crossbeam channel
    let (input_tx, input_rx) = channel::unbounded();
    std::thread::Builder::new()
        .name("bloom-input".into())
        .spawn(move || {
            while let Ok(ev) = event::read() {
                if input_tx.send(ev).is_err() {
                    break; // receiver dropped, editor shutting down
                }
            }
        })
        .expect("failed to spawn input thread");

    // Grab channel receivers for select!
    let channels = editor.channels();
    let mut needs_render = true;

    loop {
        // Render immediately when state has changed
        if needs_render {
            let t_render = Instant::now();
            let theme = TuiTheme::new(editor.theme());

            terminal.draw(|f| {
                let size = f.area();
                let frame = editor.render(size.width, size.height);

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

                bloom_tui::render::draw(f, &frame, &theme, &editor.config);
            })?;

            let render_ms = t_render.elapsed().as_secs_f64() * 1000.0;
            if render_ms > 16.0 {
                tracing::warn!(render_ms = format!("{render_ms:.1}"), "slow TUI render");
            }

            needs_render = false;
        }

        // Compute timeout from editor timers (debounce, notifications, which-key)
        let timeout = editor
            .next_deadline()
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(500));

        // Block until any channel fires or timeout expires
        crossbeam::channel::select! {
            recv(input_rx) -> msg => {
                if let Ok(ev) = msg {
                    match ev {
                        Event::Key(key_event) => {
                            if key_event.kind != KeyEventKind::Press {
                                continue;
                            }
                            if let Some(bloom_key) = input::convert_key(key_event) {
                                let actions = editor.handle_key(bloom_key);
                                for action in actions {
                                    match action {
                                        Action::Quit => {
                                            let _ = editor.save_session();
                                            return Ok(());
                                        }
                                        Action::Save => {
                                            let _ = editor.save_current();
                                        }
                                        _ => {}
                                    }
                                }
                                needs_render = true;
                            }
                        }
                        Event::Resize(w, h) => {
                            editor.resize(h as usize, w as usize);
                            needs_render = true;
                        }
                        _ => {}
                    }
                }
            }
            recv(channels.write_complete_rx.as_ref().unwrap_or(&channel::never())) -> msg => {
                if let Ok(wc) = msg {
                    if editor.handle_write_complete(wc) {
                        needs_render = true;
                    }
                }
            }
            recv(channels.watcher_rx.as_ref().unwrap_or(&channel::never())) -> msg => {
                if let Ok(ev) = msg {
                    if editor.handle_file_event(ev) {
                        needs_render = true;
                    }
                }
            }
            recv(channels.indexer_rx.as_ref().unwrap_or(&channel::never())) -> msg => {
                if let Ok(complete) = msg {
                    editor.handle_index_complete(complete);
                    needs_render = true;
                }
            }
            default(timeout) => {
                // Timer fired — tick handles notification expiry, which-key popup
            }
        }

        // After any wake: flush debounced file events, tick timers
        if editor.flush_file_event_debounce() {
            needs_render = true;
        }
        if editor.tick(Instant::now()) {
            needs_render = true;
        }
    }
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
