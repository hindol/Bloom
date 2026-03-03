mod input;
mod render;
mod theme;

use std::io;
use std::time::{Duration, Instant};

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::keymap::dispatch::Action;
use bloom_core::BloomEditor;
use crossterm::event::{self, Event, KeyEventKind};
use crossterm::terminal::{
    self, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{cursor, execute};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use theme::ThemePalette;

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
    let config = Config::defaults();
    let mut editor = BloomEditor::new(config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e:?}")))?;

    // First-run detection: show setup wizard if no vault exists
    if editor.needs_setup() {
        editor.start_wizard();
    } else {
        // Existing vault — initialize and startup normally
        let vault_path = default_vault_path();
        let _ = editor.init_vault(std::path::Path::new(&vault_path));
        editor.startup();
    }

    // Update viewport to terminal size
    let size = terminal.size()?;
    editor.resize(size.height as usize, size.width as usize);

    let theme = ThemePalette::bloom_dark();
    let tick_rate = Duration::from_millis(100);

    loop {
        // Render
        let frame = editor.render();

        // Apply cursor shape from active pane
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

        // Event handling with tick timeout
        if event::poll(tick_rate)? {
            match event::read()? {
                Event::Key(key_event) => {
                    // Only handle key press events (not release/repeat)
                    if key_event.kind != KeyEventKind::Press {
                        continue;
                    }
                    if let Some(bloom_key) = input::convert_key(key_event) {
                        let actions = editor.handle_key(bloom_key);
                        for action in actions {
                            match action {
                                Action::Quit => return Ok(()),
                                _ => {} // Other actions are handled internally by BloomEditor
                            }
                        }
                    }
                }
                Event::Resize(w, h) => {
                    editor.resize(h as usize, w as usize);
                }
                _ => {}
            }
        }

        // Tick for notifications/timers
        editor.tick(Instant::now());
    }
}
