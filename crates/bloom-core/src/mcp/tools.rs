use serde_json::{json, Value};

use crate::agenda;
use crate::document::BloomId;
use crate::index::SqliteIndex;
use crate::journal::JournalService;
use crate::refactor;
use crate::store::{sanitize_filename, LocalFileStore, NoteStore};

use super::security::SecurityPolicy;
use super::types::*;

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

pub fn tool_definitions() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "search_notes".into(),
            description: "Full-text search across all notes. Returns titles and snippets.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" }
                },
                "required": ["query"]
            }),
        },
        ToolDefinition {
            name: "read_note".into(),
            description: "Read the full content of a note by title or ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" }
                }
            }),
        },
        ToolDefinition {
            name: "create_note".into(),
            description: "Create a new page with a title and body content.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "content": { "type": "string", "description": "Markdown body content" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags" }
                },
                "required": ["title", "content"]
            }),
        },
        ToolDefinition {
            name: "edit_note".into(),
            description: "Replace the body content of an existing page (frontmatter is preserved).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" },
                    "content": { "type": "string", "description": "New markdown body content" }
                },
                "required": ["content"]
            }),
        },
        ToolDefinition {
            name: "delete_note".into(),
            description: "Delete a page by title or ID.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" }
                }
            }),
        },
        ToolDefinition {
            name: "list_notes".into(),
            description: "List all pages with their title, ID, and path.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "add_to_journal".into(),
            description: "Append text to today's journal entry.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Text to append" }
                },
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "add_task".into(),
            description: "Append a task checkbox to today's journal entry.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "Task description" }
                },
                "required": ["text"]
            }),
        },
        ToolDefinition {
            name: "list_tags".into(),
            description: "List all tags with their usage counts.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "search_by_tag".into(),
            description: "Find all pages that have a specific tag.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "tag": { "type": "string", "description": "Tag to search for" }
                },
                "required": ["tag"]
            }),
        },
        ToolDefinition {
            name: "get_backlinks".into(),
            description: "Get all pages that link to the specified page.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" }
                }
            }),
        },
        ToolDefinition {
            name: "get_unlinked_mentions".into(),
            description: "Find pages that mention a page's title without a wiki-link.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" }
                }
            }),
        },
        ToolDefinition {
            name: "get_page_links".into(),
            description: "Get all outbound links and embeds from a page.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Page title" },
                    "id": { "type": "string", "description": "Page ID" }
                }
            }),
        },
        ToolDefinition {
            name: "rename_page".into(),
            description: "Rename a page's title (updates frontmatter).".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": { "type": "string", "description": "Page ID" },
                    "new_title": { "type": "string", "description": "New title" }
                },
                "required": ["id", "new_title"]
            }),
        },
        ToolDefinition {
            name: "rename_tag".into(),
            description: "Rename a tag across the entire vault.".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "old_tag": { "type": "string", "description": "Current tag name" },
                    "new_tag": { "type": "string", "description": "New tag name" }
                },
                "required": ["old_tag", "new_tag"]
            }),
        },
        ToolDefinition {
            name: "get_agenda".into(),
            description: "Return agenda items grouped as overdue, today, and upcoming.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
    ]
}

// ---------------------------------------------------------------------------
// Dispatch
// ---------------------------------------------------------------------------

const WRITE_TOOLS: &[&str] = &[
    "create_note",
    "edit_note",
    "delete_note",
    "add_to_journal",
    "add_task",
    "rename_page",
    "rename_tag",
];

const PAGE_TOOLS: &[&str] = &[
    "read_note",
    "edit_note",
    "delete_note",
    "get_backlinks",
    "get_unlinked_mentions",
    "get_page_links",
    "rename_page",
];

pub fn dispatch_tool(
    name: &str,
    params: &Value,
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    policy: &SecurityPolicy,
) -> Result<ToolCallResult, JsonRpcError> {
    // Write-access gate
    if WRITE_TOOLS.contains(&name) && !policy.allows_write() {
        return Err(JsonRpcError {
            code: INVALID_REQUEST,
            message: "write operations not allowed in read-only mode".into(),
        });
    }

    // Path-exclusion gate for page-targeted tools
    if PAGE_TOOLS.contains(&name) {
        if let Ok(page) = resolve_page(index, params) {
            let path_str = page.path.to_string_lossy();
            if !policy.is_path_allowed(&path_str) {
                return Err(JsonRpcError {
                    code: INVALID_REQUEST,
                    message: format!("path excluded by policy: {}", path_str),
                });
            }
        }
    }

    match name {
        "search_notes" => search_notes(index, store, params),
        "read_note" => read_note(index, store, params),
        "create_note" => create_note(index, store, params),
        "edit_note" => edit_note(index, store, params),
        "delete_note" => delete_note(index, store, params),
        "list_notes" => list_notes(index, store, params),
        "add_to_journal" => add_to_journal(index, store, params),
        "add_task" => add_task(index, store, params),
        "list_tags" => list_tags_tool(index, store, params),
        "search_by_tag" => search_by_tag(index, store, params),
        "get_backlinks" => get_backlinks(index, store, params),
        "get_unlinked_mentions" => get_unlinked_mentions(index, store, params),
        "get_page_links" => get_page_links(index, store, params),
        "rename_page" => rename_page(index, store, params),
        "rename_tag" => rename_tag_tool(index, store, params),
        "get_agenda" => get_agenda(index, store, params),
        _ => Err(JsonRpcError {
            code: METHOD_NOT_FOUND,
            message: format!("unknown tool: {}", name),
        }),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, JsonRpcError> {
    params
        .get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("missing '{}' parameter", key),
        })
}

fn internal_error(e: impl std::fmt::Display) -> JsonRpcError {
    JsonRpcError {
        code: INTERNAL_ERROR,
        message: e.to_string(),
    }
}

fn text_result(text: &str) -> ToolCallResult {
    ToolCallResult {
        content: vec![ContentBlock {
            content_type: "text".into(),
            text: text.to_string(),
        }],
    }
}

fn resolve_page(
    index: &SqliteIndex,
    params: &Value,
) -> Result<crate::index::IndexedPage, JsonRpcError> {
    if let Some(id) = params.get("id").and_then(|v| v.as_str()) {
        index
            .page_for_id(id)
            .map_err(internal_error)?
            .ok_or_else(|| JsonRpcError {
                code: INVALID_PARAMS,
                message: format!("page not found: {}", id),
            })
    } else if let Some(title) = params.get("title").and_then(|v| v.as_str()) {
        index
            .page_for_title(title)
            .map_err(internal_error)?
            .ok_or_else(|| JsonRpcError {
                code: INVALID_PARAMS,
                message: format!("page not found: {}", title),
            })
    } else {
        Err(JsonRpcError {
            code: INVALID_PARAMS,
            message: "provide 'id' or 'title'".into(),
        })
    }
}

/// Split raw markdown into `(frontmatter_block, body)`.
fn split_frontmatter(content: &str) -> (&str, &str) {
    if !content.starts_with("---") {
        return ("", content);
    }
    if let Some(end) = content[3..].find("\n---") {
        let fm_end = 3 + end + 4; // past "\n---"
        let body_start = if fm_end < content.len() && content.as_bytes()[fm_end] == b'\n' {
            fm_end + 1
        } else {
            fm_end
        };
        (&content[..body_start], &content[body_start..])
    } else {
        ("", content)
    }
}

/// Check whether `title` appears in `content` outside of `[[…]]` link syntax.
fn has_unlinked_mention(content: &str, title: &str) -> bool {
    let title_lower = title.to_lowercase();
    let content_lower = content.to_lowercase();
    let mut search_from = 0;

    while let Some(pos) = content_lower[search_from..].find(&title_lower) {
        let abs_pos = search_from + pos;
        let before = &content[..abs_pos];
        let last_open = before.rfind("[[");
        let last_close = before.rfind("]]");

        let inside_link = match (last_open, last_close) {
            (Some(open), Some(close)) => open > close,
            (Some(_), None) => true,
            _ => false,
        };

        if !inside_link {
            return true;
        }
        search_from = abs_pos + title.len();
    }
    false
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

fn search_notes(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let query = require_str(params, "query")?;
    let hits = index.search(query).map_err(internal_error)?;
    let results: Vec<Value> = hits
        .iter()
        .map(|h| {
            json!({
                "page_id": h.page_id,
                "title": h.title,
                "path": h.path.to_string_lossy(),
                "snippet": h.snippet,
            })
        })
        .collect();
    Ok(text_result(
        &serde_json::to_string_pretty(&results).unwrap_or_default(),
    ))
}

fn read_note(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    let content = store.read(&page.path).map_err(internal_error)?;
    Ok(text_result(&content))
}

fn create_note(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let title = require_str(params, "title")?;
    let content = require_str(params, "content")?;
    let tags: Vec<String> = params
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let id = BloomId::new();
    let tags_str = tags
        .iter()
        .map(|t| format!("\"{}\"", t))
        .collect::<Vec<_>>()
        .join(", ");
    let body = if content.is_empty() {
        String::new()
    } else {
        format!("\n{}\n", content)
    };
    let full = format!(
        "---\nid: {}\ntitle: \"{}\"\ntags: [{}]\n---\n{}",
        id, title, tags_str, body
    );

    let filename = sanitize_filename(title);
    let path = store.pages_dir().join(format!("{}.md", filename));

    if store.exists(&path) {
        return Err(JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("page already exists at {}", path.display()),
        });
    }

    store.write(&path, &full).map_err(internal_error)?;
    let doc = crate::parser::parse(&full).map_err(internal_error)?;
    index.index_document(&path, &doc).map_err(internal_error)?;

    Ok(text_result(&format!(
        "Created page '{}' at {}",
        title,
        path.display()
    )))
}

fn edit_note(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    let new_body = require_str(params, "content")?;

    let existing = store.read(&page.path).map_err(internal_error)?;
    let (frontmatter, _) = split_frontmatter(&existing);

    let full = if frontmatter.is_empty() {
        new_body.to_string()
    } else {
        format!("{}\n{}\n", frontmatter.trim_end(), new_body)
    };

    store.write(&page.path, &full).map_err(internal_error)?;
    let doc = crate::parser::parse(&full).map_err(internal_error)?;
    index.index_document(&page.path, &doc).map_err(internal_error)?;

    Ok(text_result(&format!("Updated page '{}'", page.title)))
}

fn delete_note(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    store.delete(&page.path).map_err(internal_error)?;
    index.remove_document(&page.path).map_err(internal_error)?;
    Ok(text_result(&format!("Deleted page '{}'", page.title)))
}

fn list_notes(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    _params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let pages = index.list_pages().map_err(internal_error)?;
    let results: Vec<Value> = pages
        .iter()
        .map(|p| {
            json!({
                "page_id": p.page_id,
                "title": p.title,
                "path": p.path.to_string_lossy(),
            })
        })
        .collect();
    Ok(text_result(
        &serde_json::to_string_pretty(&results).unwrap_or_default(),
    ))
}

fn add_to_journal(
    _index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let text = require_str(params, "text")?;
    let service = JournalService::new(store, store.journal_dir());
    let path = service.quick_append_text(text).map_err(internal_error)?;
    Ok(text_result(&format!(
        "Appended to journal: {}",
        path.display()
    )))
}

fn add_task(
    _index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let text = require_str(params, "text")?;
    let service = JournalService::new(store, store.journal_dir());
    let path = service.quick_append_task(text).map_err(internal_error)?;
    Ok(text_result(&format!(
        "Added task to journal: {}",
        path.display()
    )))
}

fn list_tags_tool(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    _params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let tags = index.list_tags().map_err(internal_error)?;
    let results: Vec<Value> = tags
        .iter()
        .map(|t| json!({ "tag": t.tag, "count": t.count }))
        .collect();
    Ok(text_result(
        &serde_json::to_string_pretty(&results).unwrap_or_default(),
    ))
}

fn search_by_tag(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let tag = require_str(params, "tag")?;
    let paths = index.paths_for_tag(tag).map_err(internal_error)?;
    let results: Vec<Value> = paths
        .iter()
        .map(|p| json!({ "path": p.to_string_lossy() }))
        .collect();
    Ok(text_result(
        &serde_json::to_string_pretty(&results).unwrap_or_default(),
    ))
}

fn get_backlinks(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    let backlinks = index.backlinks_for(&page.page_id).map_err(internal_error)?;
    let results: Vec<Value> = backlinks
        .iter()
        .map(|bl| {
            json!({
                "source_page_id": bl.source_page_id,
                "source_path": bl.source_path.to_string_lossy(),
            })
        })
        .collect();
    Ok(text_result(
        &serde_json::to_string_pretty(&results).unwrap_or_default(),
    ))
}

fn get_unlinked_mentions(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    let all_pages = index.list_pages().map_err(internal_error)?;
    let mut mentions = Vec::new();

    for p in &all_pages {
        if p.page_id == page.page_id {
            continue;
        }
        let content = match index.content_for_page_id(&p.page_id).map_err(internal_error)? {
            Some(c) => c,
            None => continue,
        };
        if has_unlinked_mention(&content, &page.title) {
            mentions.push(json!({
                "page_id": p.page_id,
                "title": p.title,
                "path": p.path.to_string_lossy(),
            }));
        }
    }

    Ok(text_result(
        &serde_json::to_string_pretty(&mentions).unwrap_or_default(),
    ))
}

fn get_page_links(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let page = resolve_page(index, params)?;
    let content = store.read(&page.path).map_err(internal_error)?;
    let doc = crate::parser::parse(&content).map_err(internal_error)?;

    let mut links = Vec::new();
    for block in &doc.blocks {
        for link in &block.links {
            links.push(json!({
                "page_id": link.page_id,
                "display": link.display,
            }));
        }
        for embed in &block.embeds {
            links.push(json!({
                "page_id": embed.page_id,
                "display": embed.display,
                "embed": true,
            }));
        }
    }

    Ok(text_result(
        &serde_json::to_string_pretty(&links).unwrap_or_default(),
    ))
}

fn rename_page(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let id = require_str(params, "id")?;
    let new_title = require_str(params, "new_title")?;

    let page = index
        .page_for_id(id)
        .map_err(internal_error)?
        .ok_or_else(|| JsonRpcError {
            code: INVALID_PARAMS,
            message: format!("page not found: {}", id),
        })?;

    let content = store.read(&page.path).map_err(internal_error)?;
    let old_pattern = format!("title: \"{}\"", page.title);
    let new_pattern = format!("title: \"{}\"", new_title);
    let new_content = content.replacen(&old_pattern, &new_pattern, 1);

    store.write(&page.path, &new_content).map_err(internal_error)?;
    let doc = crate::parser::parse(&new_content).map_err(internal_error)?;
    index
        .index_document(&page.path, &doc)
        .map_err(internal_error)?;

    Ok(text_result(&format!(
        "Renamed page '{}' to '{}'",
        page.title, new_title
    )))
}

fn rename_tag_tool(
    index: &mut SqliteIndex,
    store: &LocalFileStore,
    params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let old_tag = require_str(params, "old_tag")?;
    let new_tag = require_str(params, "new_tag")?;

    let report =
        refactor::rename_tag(store, index, old_tag, new_tag).map_err(internal_error)?;

    Ok(text_result(&format!(
        "Renamed tag '{}' to '{}': {} files modified, {} occurrences",
        old_tag, new_tag, report.files_modified, report.occurrences
    )))
}

fn get_agenda(
    index: &mut SqliteIndex,
    _store: &LocalFileStore,
    _params: &Value,
) -> Result<ToolCallResult, JsonRpcError> {
    let today = chrono::Local::now().date_naive();
    let view = agenda::scan_vault(index, today).map_err(internal_error)?;

    let format_items = |items: &[agenda::AgendaItem]| -> Vec<Value> {
        items
            .iter()
            .map(|i| {
                json!({
                    "page_id": i.page_id,
                    "page_title": i.page_title,
                    "text": i.text,
                    "date": i.timestamp.date.to_string(),
                    "completed": i.completed,
                })
            })
            .collect()
    };

    let result = json!({
        "overdue": format_items(&view.overdue),
        "today": format_items(&view.today),
        "upcoming": format_items(&view.upcoming),
    });

    Ok(text_result(
        &serde_json::to_string_pretty(&result).unwrap_or_default(),
    ))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::SqliteIndex;
    use crate::parser;
    use crate::store::{LocalFileStore, NoteStore};
    use std::path::PathBuf;
    use tempfile::TempDir;

    use super::super::security::AccessMode;

    fn setup() -> (TempDir, SqliteIndex, LocalFileStore) {
        let tmp = TempDir::new().unwrap();
        let store = LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
        let db_path = tmp.path().join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path).unwrap();
        (tmp, index, store)
    }

    fn rw_policy() -> SecurityPolicy {
        SecurityPolicy {
            mode: AccessMode::ReadWrite,
            exclude_patterns: vec![],
        }
    }

    fn write_page(
        store: &LocalFileStore,
        index: &mut SqliteIndex,
        name: &str,
        content: &str,
    ) -> PathBuf {
        let path = store.pages_dir().join(name);
        store.write(&path, content).unwrap();
        let doc = parser::parse(content).unwrap();
        index.index_document(&path, &doc).unwrap();
        path
    }

    #[test]
    fn search_notes_returns_results() {
        let (_tmp, mut index, store) = setup();
        write_page(
            &store,
            &mut index,
            "alpha.md",
            "---\nid: aaa00001\ntitle: \"Alpha\"\ntags: []\n---\n\nSQLite indexing is fast.\n",
        );

        let params = json!({ "query": "sqlite" });
        let result =
            dispatch_tool("search_notes", &params, &mut index, &store, &rw_policy()).unwrap();
        let text = &result.content[0].text;
        assert!(text.contains("aaa00001"), "got: {}", text);
        assert!(text.contains("Alpha"), "got: {}", text);
    }

    #[test]
    fn read_note_by_title() {
        let (_tmp, mut index, store) = setup();
        write_page(
            &store,
            &mut index,
            "beta.md",
            "---\nid: bbb00001\ntitle: \"Beta\"\ntags: []\n---\n\nBeta body content.\n",
        );

        let params = json!({ "title": "Beta" });
        let result =
            dispatch_tool("read_note", &params, &mut index, &store, &rw_policy()).unwrap();
        assert!(result.content[0].text.contains("Beta body content"));
    }

    #[test]
    fn create_note_writes_file() {
        let (_tmp, mut index, store) = setup();

        let params = json!({ "title": "New Page", "content": "Hello world" });
        let result =
            dispatch_tool("create_note", &params, &mut index, &store, &rw_policy()).unwrap();
        assert!(result.content[0].text.contains("Created page"));

        let path = store.pages_dir().join("New Page.md");
        assert!(store.exists(&path));
        let content = store.read(&path).unwrap();
        assert!(content.contains("Hello world"));
        assert!(content.contains("title: \"New Page\""));
    }

    #[test]
    fn edit_note_updates_content() {
        let (_tmp, mut index, store) = setup();
        write_page(
            &store,
            &mut index,
            "editable.md",
            "---\nid: eee00001\ntitle: \"Editable\"\ntags: []\n---\n\nOld content.\n",
        );

        let params = json!({ "title": "Editable", "content": "New content here." });
        let result =
            dispatch_tool("edit_note", &params, &mut index, &store, &rw_policy()).unwrap();
        assert!(result.content[0].text.contains("Updated page"));

        let path = store.pages_dir().join("editable.md");
        let content = store.read(&path).unwrap();
        assert!(content.contains("New content here."));
        assert!(!content.contains("Old content."));
        assert!(content.contains("title: \"Editable\""));
    }

    #[test]
    fn security_blocks_write_in_readonly() {
        let (_tmp, mut index, store) = setup();
        let policy = SecurityPolicy {
            mode: AccessMode::ReadOnly,
            exclude_patterns: vec![],
        };

        let params = json!({ "title": "Test", "content": "Hello" });
        let result = dispatch_tool("create_note", &params, &mut index, &store, &policy);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, INVALID_REQUEST);
    }

    #[test]
    fn security_blocks_excluded_path() {
        let (_tmp, mut index, store) = setup();
        write_page(
            &store,
            &mut index,
            "secret.md",
            "---\nid: sec00001\ntitle: \"Secret\"\ntags: []\n---\n\nSecret content.\n",
        );

        let policy = SecurityPolicy {
            mode: AccessMode::ReadWrite,
            exclude_patterns: vec!["*secret*".into()],
        };

        let params = json!({ "title": "Secret" });
        let result = dispatch_tool("read_note", &params, &mut index, &store, &policy);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, INVALID_REQUEST);
        assert!(err.message.contains("excluded"));
    }

    #[test]
    fn add_to_journal_appends() {
        let (_tmp, mut index, store) = setup();

        let params = json!({ "text": "Morning standup notes" });
        let result =
            dispatch_tool("add_to_journal", &params, &mut index, &store, &rw_policy()).unwrap();
        assert!(result.content[0].text.contains("Appended to journal"));

        let today_path = store.today_journal_path();
        let content = store.read(&today_path).unwrap();
        assert!(content.contains("Morning standup notes"));
    }

    #[test]
    fn get_agenda_returns_items() {
        let (_tmp, mut index, store) = setup();
        let today = chrono::Local::now().format("%Y-%m-%d");
        let body = format!("- [ ] today task @due({})", today);
        write_page(
            &store,
            &mut index,
            "tasks.md",
            &format!(
                "---\nid: age00001\ntitle: \"Tasks\"\ntags: []\n---\n\n{}\n",
                body
            ),
        );

        let params = json!({});
        let result =
            dispatch_tool("get_agenda", &params, &mut index, &store, &rw_policy()).unwrap();
        let text = &result.content[0].text;
        assert!(text.contains("today task"), "got: {}", text);
    }
}
