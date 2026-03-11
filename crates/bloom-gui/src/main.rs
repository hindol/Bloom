//! Bloom GUI — Tauri frontend over bloom-core.
//!
//! The editor runs on a dedicated thread using the shared event loop.
//! Tauri commands send events via channels — no Mutex on the editor.

use std::sync::{Arc, Mutex};

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::event_loop::{FrontendEvent, LoopAction};
use bloom_core::BloomEditor;
use crossbeam::channel::Sender;
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

/// Key event from the frontend.
#[derive(Debug, Deserialize)]
struct KeyInput {
    code: String,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
}

/// Channel sender for frontend events, shared via Tauri state.
struct FrontendTx(Sender<FrontendEvent>);

fn main() {
    let config_path_str = default_vault_path();
    let config_path = std::path::Path::new(&config_path_str).join("config.toml");
    let config = if config_path.exists() {
        Config::load(&config_path).unwrap_or_else(|_| Config::defaults())
    } else {
        Config::defaults()
    };

    let mut editor = BloomEditor::new(config).expect("failed to create editor");

    // Initialize vault if it exists
    let vault_path = default_vault_path();
    let vault_root = std::path::Path::new(&vault_path);
    if vault_root.join("config.toml").exists() {
        let _ = editor.init_vault(vault_root);
        editor.startup();
    }

    let (frontend_tx, frontend_rx) = crossbeam::channel::unbounded();
    let app_handle: Arc<Mutex<Option<AppHandle>>> = Arc::new(Mutex::new(None));
    let app_handle_for_loop = app_handle.clone();

    // Editor loop runs on a dedicated thread — no Mutex on editor
    std::thread::Builder::new()
        .name("bloom-editor".into())
        .spawn(move || {
            bloom_core::event_loop::run_event_loop(&mut editor, &frontend_rx, |action| {
                match action {
                    LoopAction::Render(frame) => {
                        if let Some(app) = app_handle_for_loop.lock().unwrap().as_ref() {
                            let _ = app.emit("render", &frame);
                        }
                        true
                    }
                    LoopAction::Quit => {
                        if let Some(app) = app_handle_for_loop.lock().unwrap().as_ref() {
                            let _ = app.emit("quit", ());
                        }
                        false
                    }
                }
            });
        })
        .expect("failed to spawn editor thread");

    let tx_for_state = frontend_tx.clone();

    tauri::Builder::default()
        .manage(FrontendTx(tx_for_state))
        .setup(move |app| {
            *app_handle.lock().unwrap() = Some(app.handle().clone());
            // Trigger initial render by sending a resize
            let _ = frontend_tx.send(FrontendEvent::Resize { cols: 120, rows: 40 });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![key_event, resize])
        .run(tauri::generate_context!())
        .expect("failed to run Bloom GUI");
}

/// Handle a key event from the frontend — sends to the editor loop channel.
#[tauri::command]
fn key_event(key: KeyInput, state: tauri::State<FrontendTx>) {
    if let Some(bloom_key) = convert_key(&key) {
        let _ = state.0.send(FrontendEvent::Key(bloom_key));
    }
}

/// Handle window resize from the frontend.
#[tauri::command]
fn resize(cols: u16, rows: u16, state: tauri::State<FrontendTx>) {
    let _ = state.0.send(FrontendEvent::Resize { cols, rows });
}

/// Convert a frontend KeyInput to a bloom-core KeyEvent.
fn convert_key(key: &KeyInput) -> Option<bloom_core::types::KeyEvent> {
    use bloom_core::types::{KeyCode, Modifiers};

    let code = match key.code.as_str() {
        "Escape" => KeyCode::Esc,
        "Enter" => KeyCode::Enter,
        "Backspace" => KeyCode::Backspace,
        "Delete" => KeyCode::Delete,
        "Tab" => KeyCode::Tab,
        "ArrowUp" => KeyCode::Up,
        "ArrowDown" => KeyCode::Down,
        "ArrowLeft" => KeyCode::Left,
        "ArrowRight" => KeyCode::Right,
        "Home" => KeyCode::Home,
        "End" => KeyCode::End,
        "PageUp" => KeyCode::PageUp,
        "PageDown" => KeyCode::PageDown,
        " " => KeyCode::Char(' '),
        s if s.len() == 1 => {
            let ch = s.chars().next()?;
            KeyCode::Char(ch)
        }
        _ => return None,
    };

    let modifiers = Modifiers {
        ctrl: key.ctrl,
        alt: key.alt,
        shift: key.shift,
        meta: key.meta,
    };

    Some(bloom_core::types::KeyEvent { code, modifiers })
}
