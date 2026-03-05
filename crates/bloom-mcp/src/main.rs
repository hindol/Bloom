use bloom_core::config::Config;
use bloom_core::index::Index;
use bloom_core::parser::{BloomMarkdownParser, traits::DocumentParser};
use bloom_core::default_vault_path;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

fn main() {
    let vault_path = default_vault_path();
    let root = PathBuf::from(&vault_path);
    let config_path = root.join("config.toml");
    let config = if config_path.exists() {
        Config::load(&config_path).unwrap_or_else(|_| Config::defaults())
    } else {
        Config::defaults()
    };

    if !config.mcp.enabled {
        eprintln!("MCP server is disabled in config.toml. Set [mcp] enabled = true.");
        std::process::exit(1);
    }

    let index_path = root.join(".index.db");
    let index = Index::open(&index_path).expect("Failed to open index");
    let parser = BloomMarkdownParser::new();
    let read_only = matches!(config.mcp.mode, bloom_core::config::McpMode::ReadOnly);
    let exclude_paths: Vec<String> = config.mcp.exclude_paths.clone();

    let server = McpServer {
        root,
        index,
        parser,
        read_only,
        exclude_paths,
    };

    // JSON-RPC over stdin/stdout
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let response = server.handle_request(&line);
        let _ = writeln!(stdout, "{}", response);
        let _ = stdout.flush();
    }
}

struct McpServer {
    root: PathBuf,
    index: Index,
    parser: BloomMarkdownParser,
    read_only: bool,
    exclude_paths: Vec<String>,
}

impl McpServer {
    fn handle_request(&self, request: &str) -> String {
        let Ok(req) = serde_json::from_str::<serde_json::Value>(request) else {
            return self.error_response(None, -32700, "Parse error");
        };

        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(serde_json::Value::Null);

        match method {
            "initialize" => self.handle_initialize(id),
            "tools/list" => self.handle_tools_list(id),
            "tools/call" => self.handle_tools_call(id, &params),
            _ => self.error_response(id, -32601, &format!("Unknown method: {method}")),
        }
    }

    fn handle_initialize(&self, id: Option<serde_json::Value>) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "bloom-mcp", "version": "0.1.0" },
                "capabilities": { "tools": {} }
            }
        }).to_string()
    }

    fn handle_tools_list(&self, id: Option<serde_json::Value>) -> String {
        let mut tools = vec![
            tool_def("search_notes", "Full-text search across all notes", &[
                ("query", "string", "Search query text"),
            ]),
            tool_def("read_note", "Read a note by title (fuzzy matched)", &[
                ("title", "string", "Page title to find"),
            ]),
        ];

        if !self.read_only {
            tools.extend(vec![
                tool_def("create_note", "Create a new note", &[
                    ("title", "string", "Page title"),
                    ("content", "string", "Markdown content"),
                ]),
                tool_def("edit_note", "Edit a note by replacing text", &[
                    ("title", "string", "Page title"),
                    ("old_text", "string", "Text to find"),
                    ("new_text", "string", "Replacement text"),
                ]),
                tool_def("add_to_journal", "Append a line to a journal entry", &[
                    ("content", "string", "Text to append"),
                ]),
                tool_def("toggle_task", "Toggle a task checkbox in a page", &[
                    ("title", "string", "Page title"),
                    ("task_text", "string", "Task text to find"),
                ]),
            ]);
        }

        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "tools": tools }
        }).to_string()
    }

    fn handle_tools_call(&self, id: Option<serde_json::Value>, params: &serde_json::Value) -> String {
        let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(serde_json::Value::Object(Default::default()));

        let result = match tool_name {
            "search_notes" => self.tool_search(&args),
            "read_note" => self.tool_read(&args),
            "create_note" => {
                if self.read_only {
                    Err("MCP server is in read-only mode".into())
                } else {
                    self.tool_create(&args)
                }
            }
            "edit_note" => {
                if self.read_only {
                    Err("MCP server is in read-only mode".into())
                } else {
                    self.tool_edit(&args)
                }
            }
            "add_to_journal" => {
                if self.read_only {
                    Err("MCP server is in read-only mode".into())
                } else {
                    self.tool_add_journal(&args)
                }
            }
            "toggle_task" => {
                if self.read_only {
                    Err("MCP server is in read-only mode".into())
                } else {
                    self.tool_toggle_task(&args)
                }
            }
            _ => Err(format!("Unknown tool: {tool_name}")),
        };

        match result {
            Ok(content) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": content }]
                }
            }).to_string(),
            Err(e) => serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": e }],
                    "isError": true
                }
            }).to_string(),
        }
    }

    fn is_excluded(&self, path: &Path) -> bool {
        let rel = path.strip_prefix(&self.root).unwrap_or(path);
        let rel_str = rel.to_string_lossy();
        self.exclude_paths.iter().any(|pattern| {
            let pat = pattern.trim_end_matches('*');
            rel_str.starts_with(pat)
        })
    }

    fn tool_search(&self, args: &serde_json::Value) -> Result<String, String> {
        let query = args.get("query").and_then(|q| q.as_str())
            .ok_or("Missing 'query' parameter")?;
        let pages_dir = self.root.join("pages");
        let mut results = Vec::new();

        if let Ok(entries) = std::fs::read_dir(&pages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if self.is_excluded(&path) { continue; }
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    for (i, line) in content.lines().enumerate() {
                        if line.to_lowercase().contains(&query.to_lowercase()) {
                            let title = self.parser.parse_frontmatter(&content)
                                .and_then(|fm| fm.title)
                                .unwrap_or_default();
                            results.push(format!("{}:{}: {}", title, i + 1, line.trim()));
                        }
                    }
                }
            }
        }

        if results.is_empty() {
            Ok("No matches found.".into())
        } else {
            Ok(results.join("\n"))
        }
    }

    fn tool_read(&self, args: &serde_json::Value) -> Result<String, String> {
        let title = args.get("title").and_then(|t| t.as_str())
            .ok_or("Missing 'title' parameter")?;
        let pages_dir = self.root.join("pages");

        if let Ok(entries) = std::fs::read_dir(&pages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if self.is_excluded(&path) { continue; }
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(fm) = self.parser.parse_frontmatter(&content) {
                        if let Some(ref t) = fm.title {
                            if t.to_lowercase().contains(&title.to_lowercase()) {
                                return Ok(content);
                            }
                        }
                    }
                }
            }
        }

        Err(format!("Page not found: '{title}'"))
    }

    fn tool_create(&self, args: &serde_json::Value) -> Result<String, String> {
        let title = args.get("title").and_then(|t| t.as_str())
            .ok_or("Missing 'title' parameter")?;
        let content = args.get("content").and_then(|c| c.as_str())
            .ok_or("Missing 'content' parameter")?;

        let id = bloom_core::uuid::generate_hex_id();
        let fm = format!("---\nid: {}\ntitle: \"{}\"\ncreated: {}\ntags: []\n---\n\n{}",
            id.to_hex(), title, chrono::Local::now().format("%Y-%m-%d"), content);

        let filename = format!("{}-{}.md",
            title.to_lowercase().replace(' ', "-"),
            id.to_hex());
        let path = self.root.join("pages").join(&filename);
        std::fs::write(&path, &fm).map_err(|e| format!("Write failed: {e}"))?;

        Ok(format!("Created page '{}' at {}", title, path.display()))
    }

    fn tool_edit(&self, args: &serde_json::Value) -> Result<String, String> {
        let title = args.get("title").and_then(|t| t.as_str())
            .ok_or("Missing 'title' parameter")?;
        let old_text = args.get("old_text").and_then(|t| t.as_str())
            .ok_or("Missing 'old_text' parameter")?;
        let new_text = args.get("new_text").and_then(|t| t.as_str())
            .ok_or("Missing 'new_text' parameter")?;

        let pages_dir = self.root.join("pages");
        if let Ok(entries) = std::fs::read_dir(&pages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if self.is_excluded(&path) { continue; }
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(fm) = self.parser.parse_frontmatter(&content) {
                        if let Some(ref t) = fm.title {
                            if t.to_lowercase().contains(&title.to_lowercase()) {
                                let count = content.matches(old_text).count();
                                if count == 0 {
                                    return Err("Text not found in page. Re-read and try again.".into());
                                }
                                if count > 1 {
                                    return Err(format!("Ambiguous match ({count} occurrences). Include more context."));
                                }
                                let new_content = content.replacen(old_text, new_text, 1);
                                std::fs::write(&path, &new_content)
                                    .map_err(|e| format!("Write failed: {e}"))?;
                                return Ok(format!("Edited page '{}'", t));
                            }
                        }
                    }
                }
            }
        }

        Err(format!("Page not found: '{title}'"))
    }

    fn tool_add_journal(&self, args: &serde_json::Value) -> Result<String, String> {
        let content = args.get("content").and_then(|c| c.as_str())
            .ok_or("Missing 'content' parameter")?;
        let date = args.get("date").and_then(|d| d.as_str());

        let date_str = date.unwrap_or(&chrono::Local::now().format("%Y-%m-%d").to_string()).to_string();
        let path = self.root.join("journal").join(format!("{date_str}.md"));

        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let new_content = if existing.is_empty() {
            format!("---\ntitle: \"{date_str}\"\ncreated: {date_str}\ntags: [journal]\n---\n\n{content}\n")
        } else {
            format!("{existing}\n{content}\n")
        };

        std::fs::write(&path, &new_content).map_err(|e| format!("Write failed: {e}"))?;
        Ok(format!("Appended to journal {date_str}"))
    }

    fn tool_toggle_task(&self, args: &serde_json::Value) -> Result<String, String> {
        let title = args.get("title").and_then(|t| t.as_str())
            .ok_or("Missing 'title' parameter")?;
        let task_text = args.get("task_text").and_then(|t| t.as_str())
            .ok_or("Missing 'task_text' parameter")?;

        let pages_dir = self.root.join("pages");
        if let Ok(entries) = std::fs::read_dir(&pages_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if self.is_excluded(&path) { continue; }
                if path.extension().and_then(|e| e.to_str()) != Some("md") { continue; }
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(fm) = self.parser.parse_frontmatter(&content) {
                        if let Some(ref t) = fm.title {
                            if t.to_lowercase().contains(&title.to_lowercase()) {
                                let new_content: String = content.lines().map(|line| {
                                    let trimmed = line.trim();
                                    if trimmed.contains(task_text) {
                                        if trimmed.starts_with("- [ ] ") {
                                            line.replacen("- [ ] ", "- [x] ", 1)
                                        } else if trimmed.starts_with("- [x] ") {
                                            line.replacen("- [x] ", "- [ ] ", 1)
                                        } else {
                                            line.to_string()
                                        }
                                    } else {
                                        line.to_string()
                                    }
                                }).collect::<Vec<_>>().join("\n");
                                std::fs::write(&path, &new_content)
                                    .map_err(|e| format!("Write failed: {e}"))?;
                                return Ok(format!("Toggled task in '{}'", t));
                            }
                        }
                    }
                }
            }
        }

        Err(format!("Page not found: '{title}'"))
    }

    fn error_response(&self, id: Option<serde_json::Value>, code: i32, message: &str) -> String {
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": code, "message": message }
        }).to_string()
    }
}

fn tool_def(name: &str, description: &str, params: &[(&str, &str, &str)]) -> serde_json::Value {
    let properties: serde_json::Map<String, serde_json::Value> = params.iter().map(|(n, t, d)| {
        (n.to_string(), serde_json::json!({ "type": t, "description": d }))
    }).collect();
    let required: Vec<&str> = params.iter().map(|(n, _, _)| *n).collect();

    serde_json::json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required
        }
    })
}