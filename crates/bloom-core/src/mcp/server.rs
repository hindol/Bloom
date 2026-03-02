use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::index::SqliteIndex;
use crate::store::LocalFileStore;

use super::config::McpConfig;
use super::security::{AccessMode, SecurityPolicy};
use super::tools;
use super::types::*;

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("store error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("index error: {0}")]
    Index(#[from] crate::index::IndexError),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

pub struct McpServer {
    index: RefCell<SqliteIndex>,
    store: LocalFileStore,
    policy: SecurityPolicy,
}

impl McpServer {
    pub fn new(vault_root: &Path, config: &McpConfig) -> Result<Self, McpError> {
        let store = LocalFileStore::new(vault_root.to_path_buf())?;
        let db_path = vault_root.join(".index").join("core.db");
        let index = SqliteIndex::open(&db_path)?;

        let mode = match config.mode.as_str() {
            "read-only" => AccessMode::ReadOnly,
            _ => AccessMode::ReadWrite,
        };

        let policy = SecurityPolicy {
            mode,
            exclude_patterns: config.exclude_paths.clone(),
        };

        Ok(Self {
            index: RefCell::new(index),
            store,
            policy,
        })
    }

    /// Process a single JSON-RPC request and return a response.
    pub fn handle_request(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id.clone()),
            "tools/list" => self.handle_tools_list(request.id.clone()),
            "tools/call" => self.handle_tools_call(request.id.clone(), &request.params),
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: METHOD_NOT_FOUND,
                    message: format!("unknown method: {}", request.method),
                }),
            },
        }
    }

    /// Run the stdio server loop (blocking).
    /// Reads newline-delimited JSON-RPC from stdin, writes responses to stdout.
    pub fn run_stdio(&self) -> Result<(), McpError> {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            let request: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(req) => req,
                Err(_) => {
                    let response = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: PARSE_ERROR,
                            message: "invalid JSON".into(),
                        }),
                    };
                    writeln!(writer, "{}", serde_json::to_string(&response)?)?;
                    writer.flush()?;
                    continue;
                }
            };

            let response = self.handle_request(&request);
            writeln!(writer, "{}", serde_json::to_string(&response)?)?;
            writer.flush()?;
        }

        Ok(())
    }

    /// Handle `initialize` — return server capabilities.
    fn handle_initialize(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        let result = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "bloom-mcp",
                "version": "0.1.0"
            }
        });

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Handle `tools/list` — return all tool definitions.
    fn handle_tools_list(&self, id: Option<serde_json::Value>) -> JsonRpcResponse {
        let defs = tools::tool_definitions();
        let result = serde_json::json!({ "tools": defs });

        JsonRpcResponse {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Handle `tools/call` — dispatch to the named tool.
    fn handle_tools_call(
        &self,
        id: Option<serde_json::Value>,
        params: &serde_json::Value,
    ) -> JsonRpcResponse {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(serde_json::Value::Object(Default::default()));

        let mut index = self.index.borrow_mut();
        match tools::dispatch_tool(name, &arguments, &mut index, &self.store, &self.policy) {
            Ok(tool_result) => {
                let result =
                    serde_json::to_value(tool_result).unwrap_or(serde_json::Value::Null);
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id,
                    result: Some(result),
                    error: None,
                }
            }
            Err(error) => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(error),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser;
    use crate::store::NoteStore;
    use tempfile::TempDir;

    fn make_server() -> (TempDir, McpServer) {
        let tmp = TempDir::new().unwrap();
        let config = McpConfig::default();
        let server = McpServer::new(tmp.path(), &config).unwrap();
        (tmp, server)
    }

    #[test]
    fn handle_initialize_returns_capabilities() {
        let (_tmp, server) = make_server();

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(1)),
            method: "initialize".into(),
            params: serde_json::Value::Null,
        };

        let response = server.handle_request(&request);
        assert!(response.error.is_none());
        let result = response.result.unwrap();
        assert_eq!(result["serverInfo"]["name"], "bloom-mcp");
        assert_eq!(result["protocolVersion"], "2024-11-05");
        assert!(result["capabilities"]["tools"].is_object());
    }

    #[test]
    fn handle_tools_call_dispatches_correctly() {
        let (tmp, server) = make_server();

        // Index a page so list_notes returns something
        {
            let store =
                LocalFileStore::new(tmp.path().to_path_buf()).unwrap();
            let path = store.pages_dir().join("test.md");
            let content =
                "---\nid: tst00001\ntitle: \"Test\"\ntags: []\n---\n\nTest body.\n";
            store.write(&path, content).unwrap();
            let doc = parser::parse(content).unwrap();
            server
                .index
                .borrow_mut()
                .index_document(&path, &doc)
                .unwrap();
        }

        let request = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(serde_json::json!(2)),
            method: "tools/call".into(),
            params: serde_json::json!({
                "name": "list_notes",
                "arguments": {}
            }),
        };

        let response = server.handle_request(&request);
        assert!(response.error.is_none(), "{:?}", response.error);
        let result = response.result.unwrap();
        let text = result["content"][0]["text"].as_str().unwrap();
        assert!(text.contains("tst00001"), "got: {}", text);
    }
}
