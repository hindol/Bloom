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
            // Keep pane viewport geometry and scroll state in sync before
            // producing the read-only render frame.
            editor.update_layout(viewport.0, viewport.1);
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
            recv(channels.write_result_rx.as_ref().unwrap_or(&wc_never)) -> msg => {
                if let Ok(result) = msg {
                    if editor.handle_write_result(result) {
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
        if editor.flush_autosave_debounce() {
            needs_render = true;
        }
        if editor.tick(Instant::now()) {
            needs_render = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam::channel;
    use std::path::Path;

    fn render_long_file(height: usize, keys: &[crate::types::KeyEvent]) -> Box<RenderFrame> {
        let mut editor = crate::BloomEditor::new(crate::config::Config::defaults()).unwrap();
        let id = crate::uuid::generate_hex_id();
        let content = (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        editor.open_page_with_content(&id, "Long", Path::new("[scratch]"), &content);
        editor.resize(height, 80);

        let (tx, rx) = channel::unbounded();
        for key in keys {
            tx.send(FrontendEvent::Key(key.clone())).unwrap();
        }
        tx.send(FrontendEvent::Quit).unwrap();

        let mut last_frame: Option<Box<RenderFrame>> = None;
        run_event_loop(&mut editor, &rx, |action| {
            match action {
                LoopAction::Render(frame) => last_frame = Some(frame),
                LoopAction::Quit => {}
            }
            true
        });

        last_frame.expect("expected at least one rendered frame")
    }

    #[test]
    fn event_loop_uses_current_pane_height_for_visible_lines() {
        let frame = render_long_file(40, &[]);
        let pane = frame
            .panes
            .iter()
            .find(|pane| pane.is_active)
            .expect("expected an active pane");

        assert_eq!(pane.rect.content_height, 39);
        assert_eq!(pane.visible_lines.len(), 39);
    }

    #[test]
    fn event_loop_keeps_cursor_in_visible_range() {
        let frame = render_long_file(24, &vec![crate::types::KeyEvent::char('j'); 30]);
        let pane = frame
            .panes
            .iter()
            .find(|pane| pane.is_active)
            .expect("expected an active pane");
        let visible_start = pane.scroll_offset;
        let visible_end = visible_start + pane.visible_lines.len();

        assert!(
            visible_start > 0,
            "expected the viewport to scroll once the cursor moved below the first screenful"
        );
        assert_eq!(pane.cursor.line, 30);
        assert!(
            pane.cursor.line >= visible_start && pane.cursor.line < visible_end,
            "cursor line {} should stay within visible range {}..{}",
            pane.cursor.line,
            visible_start,
            visible_end
        );
    }
}
