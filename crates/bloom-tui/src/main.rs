mod input;
mod render;
mod theme;

use std::io;
use std::time::{Duration, Instant};

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::keymap::dispatch::Action;
use bloom_core::BloomEditor;
use crossbeam::channel;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use theme::TuiTheme;

fn main() -> io::Result<()> {
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
    let mut editor = BloomEditor::new(config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e:?}")))?;

    // First-run detection: show setup wizard if no vault exists
    if editor.needs_setup() {
        editor.start_wizard();
    } else {
        // Existing vault — initialize and spawn background indexer
        let vault_path = default_vault_path();
        let _ = editor.init_vault(std::path::Path::new(&vault_path));
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
            loop {
                match event::read() {
                    Ok(ev) => {
                        if input_tx.send(ev).is_err() {
                            break; // receiver dropped, editor shutting down
                        }
                    }
                    Err(_) => break,
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
            let theme = TuiTheme::new(editor.theme());
            let size = terminal.size()?;
            let frame = editor.render(size.width, size.height);

            if let Some(pane) = frame.panes.iter().find(|p| p.is_active) {
                let cursor_style = match pane.cursor.shape {
                    bloom_core::render::CursorShape::Block => {
                        cursor::SetCursorStyle::SteadyBlock
                    }
                    bloom_core::render::CursorShape::Bar => {
                        cursor::SetCursorStyle::SteadyBar
                    }
                    bloom_core::render::CursorShape::Underline => {
                        cursor::SetCursorStyle::SteadyUnderScore
                    }
                };
                execute!(terminal.backend_mut(), cursor_style)?;
            }

            terminal.draw(|f| {
                render::draw(f, &frame, &theme);
            })?;

            needs_render = false;
        }

        // Compute timeout from editor timers (debounce, notifications, which-key)
        let timeout = editor.next_deadline()
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
                    editor.handle_write_complete(wc);
                    needs_render = true;
                }
            }
            recv(channels.watcher_rx.as_ref().unwrap_or(&channel::never())) -> msg => {
                if let Ok(ev) = msg {
                    editor.handle_file_event(ev);
                    needs_render = true;
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
