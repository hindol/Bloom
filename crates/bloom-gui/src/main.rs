//! Bloom GUI — Tauri frontend over bloom-core.
//!
//! The Rust backend owns BloomEditor and all state. The TypeScript frontend
//! is a pure render target — it receives RenderFrame as JSON and sends
//! key events back via Tauri commands.

use std::sync::Mutex;

use bloom_core::config::Config;
use bloom_core::default_vault_path;
use bloom_core::keymap::dispatch::Action;
use bloom_core::types::KeyEvent as BloomKey;
use bloom_core::BloomEditor;
use serde::{Deserialize, Serialize};
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

/// Shared editor state, protected by a Mutex for Tauri's thread model.
struct EditorState {
    editor: Mutex<BloomEditor>,
}

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

    tauri::Builder::default()
        .manage(EditorState {
            editor: Mutex::new(editor),
        })
        .invoke_handler(tauri::generate_handler![key_event, initial_render])
        .run(tauri::generate_context!())
        .expect("failed to run Bloom GUI");
}

/// Handle a key event from the frontend. Processes the key, then emits
/// a fresh RenderFrame back to the frontend.
#[tauri::command]
fn key_event(key: KeyInput, state: tauri::State<EditorState>, app: AppHandle) {
    let Some(bloom_key) = convert_key(&key) else {
        return;
    };

    let mut editor = state.editor.lock().unwrap();
    let actions = editor.handle_key(bloom_key);

    for action in &actions {
        match action {
            Action::Quit => {
                let _ = editor.save_session();
                std::process::exit(0);
            }
            Action::Save => {
                let _ = editor.save_current();
            }
            _ => {}
        }
    }

    emit_render(&mut editor, &app);
}

/// Initial render — called once when the frontend loads.
#[tauri::command]
fn initial_render(state: tauri::State<EditorState>, app: AppHandle) {
    let mut editor = state.editor.lock().unwrap();
    emit_render(&mut editor, &app);
}

/// Render the current editor state and emit it to the frontend.
fn emit_render(editor: &mut BloomEditor, app: &AppHandle) {
    // Use a reasonable default size — the frontend will send resize events later
    let frame = editor.render(120, 40);
    let _ = app.emit("render", &frame);
}

/// Convert a frontend KeyInput to a bloom-core KeyEvent.
fn convert_key(key: &KeyInput) -> Option<BloomKey> {
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

    Some(BloomKey { code, modifiers })
}
