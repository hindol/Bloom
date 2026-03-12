//! Shared editor event loop for TUI and GUI frontends.
//!
//! A single `crossbeam::select!` loop that processes all events: frontend
//! input, background channel completions, and timer ticks. The frontend
//! sends [`FrontendEvent`]s via a channel; the callback receives
//! [`LoopAction`]s (render frames, quit signal).
//!
//! Both frontends use this — the TUI passes terminal input, the GUI passes
//! Tauri IPC messages. The editor is owned by the loop, no Mutex needed.

use crossbeam::channel::Receiver;
use std::time::{Duration, Instant};

use crate::keymap::dispatch::Action;
use crate::render::RenderFrame;

/// Events sent from the frontend to the editor loop.
pub enum FrontendEvent {
    /// A key press.
    Key(crate::types::KeyEvent),
    /// Window/terminal resized.
    Resize { cols: u16, rows: u16 },
    /// Frontend is shutting down.
    Quit,
}

/// Actions the loop sends back to the frontend callback.
pub enum LoopAction {
    /// A new frame to render.
    Render(Box<RenderFrame>),
    /// The editor is quitting.
    Quit,
}

/// Run the editor event loop. Blocks the calling thread.
///
/// Processes frontend input, background channels (write_complete, file
/// watcher, indexer), and timer ticks. Calls `on_action` with render
/// frames and quit signals. Returns when `on_action` returns `false`
/// or the frontend channel closes.
pub fn run_event_loop(
    editor: &mut crate::BloomEditor,
    frontend_rx: &Receiver<FrontendEvent>,
    mut on_action: impl FnMut(LoopAction) -> bool,
) {
    let mut needs_render = true;
    let mut viewport: (u16, u16) = (editor.terminal_width.max(1), editor.terminal_height.max(1));

    loop {
        // Render if state changed
        if needs_render {
            let frame = editor.render(viewport.0, viewport.1);
            if !on_action(LoopAction::Render(Box::new(frame))) {
                return;
            }
            needs_render = false;
        }

        // Compute timeout from editor timers
        let timeout = editor
            .next_deadline()
            .map(|d| d.saturating_duration_since(Instant::now()))
            .unwrap_or(Duration::from_millis(500));

        // Grab channel refs for select
        let channels = editor.channels();
        let wc_never = crossbeam::channel::never();
        let watch_never = crossbeam::channel::never();
        let idx_never = crossbeam::channel::never();
        let hist_never = crossbeam::channel::never();

        crossbeam::channel::select! {
            recv(frontend_rx) -> msg => {
                match msg {
                    Ok(FrontendEvent::Key(key)) => {
                        let actions = editor.handle_key(key);
                        for action in &actions {
                            match action {
                                Action::Quit => {
                                    let _ = editor.save_session();
                                    on_action(LoopAction::Quit);
                                    return;
                                }
                                Action::Save => {
                                    let _ = editor.save_current();
                                }
                                _ => {}
                            }
                        }
                        needs_render = true;
                    }
                    Ok(FrontendEvent::Resize { cols, rows }) => {
                        viewport = (cols, rows);
                        editor.resize(rows as usize, cols as usize);
                        needs_render = true;
                    }
                    Ok(FrontendEvent::Quit) | Err(_) => {
                        let _ = editor.save_session();
                        on_action(LoopAction::Quit);
                        return;
                    }
                }
            }
            recv(channels.write_complete_rx.as_ref().unwrap_or(&wc_never)) -> msg => {
                if let Ok(wc) = msg {
                    if editor.handle_write_complete(wc) {
                        needs_render = true;
                    }
                }
            }
            recv(channels.watcher_rx.as_ref().unwrap_or(&watch_never)) -> msg => {
                if let Ok(ev) = msg {
                    if editor.handle_file_event(ev) {
                        needs_render = true;
                    }
                }
            }
            recv(channels.indexer_rx.as_ref().unwrap_or(&idx_never)) -> msg => {
                if let Ok(complete) = msg {
                    editor.handle_index_complete(complete);
                    needs_render = true;
                }
            }
            recv(channels.history_rx.as_ref().unwrap_or(&hist_never)) -> msg => {
                if let Ok(complete) = msg {
                    editor.handle_history_complete(complete);
                    needs_render = true;
                }
            }
            default(timeout) => {
                // Timer fired
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
