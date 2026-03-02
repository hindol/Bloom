use std::sync::Mutex;

use bloom_core::editor::{EditorState, Key, KeyResult};
use bloom_core::render::{self as bloom_render, Rgb, Theme};
use serde::Serialize;
use tauri::State;

// ---------------------------------------------------------------------------
// Serializable RenderFrame mirror (for JSON transport to webview)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct GuiStyledSpan {
    pub start: usize,
    pub end: usize,
    pub style: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiLine {
    pub text: String,
    pub line_number: Option<usize>,
    pub spans: Vec<GuiStyledSpan>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiStatusBar {
    pub mode: String,
    pub filename: String,
    pub dirty: bool,
    pub position: String,
    pub pending_keys: String,
    pub filetype: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiPane {
    pub lines: Vec<GuiLine>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub cursor_shape: String,
    pub status_bar: GuiStatusBar,
    pub focused: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiFilterPill {
    pub label: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiPickerItem {
    pub text: String,
    pub match_indices: Vec<usize>,
    pub marginalia: String,
    pub selected: bool,
    pub marked: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiPickerFrame {
    pub title: String,
    pub query: String,
    pub filters: Vec<GuiFilterPill>,
    pub items: Vec<GuiPickerItem>,
    pub result_count: String,
    pub preview: Option<Vec<GuiLine>>,
    pub inline: bool,
    pub action_menu: Option<Vec<String>>,
    pub action_menu_selected: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiWhichKeyEntry {
    pub key: String,
    pub description: String,
    pub is_group: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiWhichKeyFrame {
    pub prefix: String,
    pub entries: Vec<GuiWhichKeyEntry>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiUndoTreeEntry {
    pub branch_index: usize,
    pub current: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiUndoTreeFrame {
    pub title: String,
    pub entries: Vec<GuiUndoTreeEntry>,
    pub selected: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiAgendaItem {
    pub text: String,
    pub page_title: String,
    pub date: String,
    pub completed: bool,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiAgendaSection {
    pub label: String,
    pub items: Vec<GuiAgendaItem>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiAgendaFrame {
    pub title: String,
    pub sections: Vec<GuiAgendaSection>,
    pub selected: usize,
    pub total_items: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiDiagnostic {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub kind: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiThemePalette {
    pub foreground: String,
    pub background: String,
    pub modeline: String,
    pub highlight: String,
    pub critical: String,
    pub popout: String,
    pub strong: String,
    pub salient: String,
    pub faded: String,
    pub subtle: String,
    pub mild: String,
    pub ultralight: String,
    pub accent_red: String,
    pub accent_green: String,
    pub accent_blue: String,
    pub accent_yellow: String,
    // Derived UI chrome colors
    pub status_normal: String,
    pub status_insert: String,
    pub status_visual: String,
    pub status_command: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuiRenderFrame {
    pub panes: Vec<GuiPane>,
    pub picker: Option<GuiPickerFrame>,
    pub which_key: Option<GuiWhichKeyFrame>,
    pub diagnostics: Vec<GuiDiagnostic>,
    pub command_line: Option<String>,
    pub capture_bar: Option<String>,
    pub undo_tree: Option<GuiUndoTreeFrame>,
    pub agenda: Option<GuiAgendaFrame>,
    pub theme: GuiThemePalette,
}

fn rgb_to_hex(rgb: &Rgb) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}

fn style_to_string(style: &bloom_render::Style) -> String {
    match style {
        bloom_render::Style::Normal => "normal".into(),
        bloom_render::Style::Heading { level } => format!("h{}", level),
        bloom_render::Style::Bold => "bold".into(),
        bloom_render::Style::Italic => "italic".into(),
        bloom_render::Style::Code => "code".into(),
        bloom_render::Style::CodeBlock => "code-block".into(),
        bloom_render::Style::Link => "link".into(),
        bloom_render::Style::Embed => "embed".into(),
        bloom_render::Style::Tag => "tag".into(),
        bloom_render::Style::Timestamp => "timestamp".into(),
        bloom_render::Style::BlockId => "block-id".into(),
        bloom_render::Style::ListMarker => "list-marker".into(),
        bloom_render::Style::CheckboxUnchecked => "checkbox-unchecked".into(),
        bloom_render::Style::CheckboxChecked => "checkbox-checked".into(),
        bloom_render::Style::Frontmatter => "frontmatter".into(),
        bloom_render::Style::BrokenLink => "broken-link".into(),
    }
}

fn convert_line(l: &bloom_render::RenderedLine) -> GuiLine {
    GuiLine {
        text: l.text.clone(),
        line_number: l.line_number,
        spans: l
            .spans
            .iter()
            .map(|s| GuiStyledSpan {
                start: s.start,
                end: s.end,
                style: style_to_string(&s.style),
            })
            .collect(),
    }
}

fn convert_theme(theme: &Theme) -> GuiThemePalette {
    let p = &theme.palette;
    GuiThemePalette {
        foreground: rgb_to_hex(&p.foreground),
        background: rgb_to_hex(&p.background),
        modeline: rgb_to_hex(&p.modeline),
        highlight: rgb_to_hex(&p.highlight),
        critical: rgb_to_hex(&p.critical),
        popout: rgb_to_hex(&p.popout),
        strong: rgb_to_hex(&p.strong),
        salient: rgb_to_hex(&p.salient),
        faded: rgb_to_hex(&p.faded),
        subtle: rgb_to_hex(&p.subtle),
        mild: rgb_to_hex(&p.mild),
        ultralight: rgb_to_hex(&p.ultralight),
        accent_red: rgb_to_hex(&p.accent_red),
        accent_green: rgb_to_hex(&p.accent_green),
        accent_blue: rgb_to_hex(&p.accent_blue),
        accent_yellow: rgb_to_hex(&p.accent_yellow),
        status_normal: rgb_to_hex(&theme.status_normal),
        status_insert: rgb_to_hex(&theme.status_insert),
        status_visual: rgb_to_hex(&theme.status_visual),
        status_command: rgb_to_hex(&theme.status_command),
    }
}

fn render_to_gui(state: &EditorState) -> GuiRenderFrame {
    let frame = state.render();
    let theme = &state.theme;

    GuiRenderFrame {
        panes: frame
            .panes
            .iter()
            .map(|pane| GuiPane {
                lines: pane.lines.iter().map(convert_line).collect(),
                cursor_row: pane.cursor.row,
                cursor_col: pane.cursor.col,
                cursor_shape: format!("{:?}", pane.cursor.shape),
                status_bar: GuiStatusBar {
                    mode: pane.status_bar.mode.clone(),
                    filename: pane.status_bar.filename.clone(),
                    dirty: pane.status_bar.dirty,
                    position: pane.status_bar.position.clone(),
                    pending_keys: pane.status_bar.pending_keys.clone(),
                    filetype: pane.status_bar.filetype.clone(),
                },
                focused: pane.focused,
            })
            .collect(),
        picker: frame.picker.as_ref().map(|p| GuiPickerFrame {
            title: p.title.clone(),
            query: p.query.clone(),
            filters: p
                .filters
                .iter()
                .map(|f| GuiFilterPill {
                    label: f.label.clone(),
                    kind: f.kind.clone(),
                })
                .collect(),
            items: p
                .items
                .iter()
                .map(|i| GuiPickerItem {
                    text: i.text.clone(),
                    match_indices: i.match_indices.clone(),
                    marginalia: i.marginalia.clone(),
                    selected: i.selected,
                    marked: i.marked,
                })
                .collect(),
            result_count: p.result_count.clone(),
            preview: p
                .preview
                .as_ref()
                .map(|lines| lines.iter().map(convert_line).collect()),
            inline: p.inline,
            action_menu: p.action_menu.clone(),
            action_menu_selected: p.action_menu_selected,
        }),
        which_key: frame.which_key.as_ref().map(|wk| GuiWhichKeyFrame {
            prefix: wk.prefix.clone(),
            entries: wk
                .entries
                .iter()
                .map(|e| GuiWhichKeyEntry {
                    key: e.key.clone(),
                    description: e.description.clone(),
                    is_group: e.is_group,
                })
                .collect(),
        }),
        diagnostics: frame
            .diagnostics
            .iter()
            .map(|d| GuiDiagnostic {
                line: d.line,
                start: d.start,
                end: d.end,
                kind: format!("{:?}", d.kind),
                message: d.message.clone(),
            })
            .collect(),
        command_line: frame.command_line.clone(),
        capture_bar: frame.capture_bar.clone(),
        undo_tree: frame.undo_tree.as_ref().map(|ut| GuiUndoTreeFrame {
            title: ut.title.clone(),
            entries: ut
                .entries
                .iter()
                .map(|e| GuiUndoTreeEntry {
                    branch_index: e.branch_index,
                    current: e.current,
                })
                .collect(),
            selected: ut.selected,
        }),
        agenda: frame.agenda.as_ref().map(|a| GuiAgendaFrame {
            title: a.title.clone(),
            sections: a
                .sections
                .iter()
                .map(|s| GuiAgendaSection {
                    label: s.label.clone(),
                    items: s
                        .items
                        .iter()
                        .map(|i| GuiAgendaItem {
                            text: i.text.clone(),
                            page_title: i.page_title.clone(),
                            date: i.date.clone(),
                            completed: i.completed,
                            tags: i.tags.clone(),
                        })
                        .collect(),
                })
                .collect(),
            selected: a.selected,
            total_items: a.total_items,
        }),
        theme: convert_theme(theme),
    }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------

pub struct AppState {
    pub editor: Mutex<EditorState>,
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
fn get_frame(state: State<'_, AppState>) -> GuiRenderFrame {
    let mut editor = state.editor.lock().unwrap();
    editor.poll_fts_results();
    render_to_gui(&editor)
}

#[tauri::command]
fn send_key(state: State<'_, AppState>, key: String) -> (String, GuiRenderFrame) {
    let mut editor = state.editor.lock().unwrap();
    let parsed_key = parse_key(&key);
    let result = if let Some(k) = parsed_key {
        editor.handle_key(k)
    } else {
        KeyResult::Handled
    };

    let result_str = match result {
        KeyResult::Quit => "quit",
        KeyResult::SaveAndQuit => "save_and_quit",
        KeyResult::Save => "save",
        KeyResult::Handled => "handled",
        KeyResult::Pending => "pending",
    };

    (result_str.to_string(), render_to_gui(&editor))
}

#[tauri::command]
fn resize_viewport(state: State<'_, AppState>, height: usize) {
    let mut editor = state.editor.lock().unwrap();
    editor.viewport_height = height;
}

fn parse_key(key: &str) -> Option<Key> {
    match key {
        "Escape" => Some(Key::Escape),
        "Enter" => Some(Key::Enter),
        "AltEnter" => Some(Key::AltEnter),
        "Backspace" => Some(Key::Backspace),
        "Tab" => Some(Key::Tab),
        "ArrowLeft" => Some(Key::Left),
        "ArrowRight" => Some(Key::Right),
        "ArrowUp" => Some(Key::Up),
        "ArrowDown" => Some(Key::Down),
        s if s.starts_with("Ctrl+") => {
            let c = s.chars().last()?;
            Some(Key::Ctrl(c))
        }
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            Some(Key::Char(c))
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let editor = EditorState::new("# Welcome to Bloom 🌱\n\nStart typing...\n");

    tauri::Builder::default()
        .manage(AppState {
            editor: Mutex::new(editor),
        })
        .invoke_handler(tauri::generate_handler![get_frame, send_key, resize_viewport])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Bloom GUI");
}
