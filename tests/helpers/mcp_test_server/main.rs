//! MCP test server binary for integration tests
//!
//! This binary implements a minimal MCP server that communicates over
//! stdin/stdout using newline-delimited JSON (the stdio transport protocol).
//! It is used exclusively by integration tests to exercise the
//! [`StdioTransport`] without requiring a real external MCP server.
//!
//! # Handled Methods
//!
//! - `initialize` -- responds with a valid `InitializeResponse` using
//!   `protocol_version: "2025-11-25"` and `ServerCapabilities` with `tools`
//!   set.
//! - `notifications/initialized` -- acknowledged silently (no response).
//! - `tools/list` -- returns one tool: `"echo"` with a string `message`
//!   parameter.
//! - `tools/call` with `name: "echo"` -- echoes back the `message` argument.
//! - All other methods -- returns a JSON-RPC `-32601 Method not found` error.
//!
//! # Usage
//!
//! The binary reads from stdin and writes to stdout. Each line of stdin is
//! treated as one JSON-RPC message. Each response is written as a single
//! line of JSON followed by `\n`.

use std::io::{self, BufRead, Write};

fn main() {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut out = stdout.lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => {
                // Malformed JSON: send a parse error and continue.
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": null,
                    "error": {
                        "code": -32700,
                        "message": "Parse error"
                    }
                });
                let _ = writeln!(out, "{}", serde_json::to_string(&response).unwrap());
                let _ = out.flush();
                continue;
            }
        };

        let method = request.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let id = request
            .get("id")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Notifications (no id or id is null AND no expected response).
        // For `notifications/initialized` we silently swallow the message.
        if method == "notifications/initialized" {
            continue;
        }

        let response = match method {
            "initialize" => handle_initialize(&id),
            "tools/list" => handle_tools_list(&id),
            "tools/call" => handle_tools_call(&id, &request),
            "ping" => handle_ping(&id),
            _ => make_error(&id, -32601, &format!("Method not found: {}", method)),
        };

        let serialized = match serde_json::to_string(&response) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("mcp_test_server: failed to serialize response: {}", e);
                continue;
            }
        };

        if writeln!(out, "{}", serialized).is_err() {
            break;
        }
        if out.flush().is_err() {
            break;
        }
    }
}

/// Handle the `initialize` request.
///
/// Returns a valid `InitializeResponse` with protocol version `2025-11-25`
/// and `ServerCapabilities` that advertise `tools`.
fn handle_initialize(id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "protocolVersion": "2025-11-25",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "mcp-test-server",
                "version": "0.1.0"
            }
        }
    })
}

/// Handle the `tools/list` request.
///
/// Returns a single tool named `"echo"` with a `message` string parameter.
fn handle_tools_list(id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "echo",
                    "description": "Echoes input",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "message": {
                                "type": "string"
                            }
                        }
                    }
                }
            ]
        }
    })
}

/// Handle the `tools/call` request.
///
/// If `name` is `"echo"`, returns the value of `arguments.message` as a
/// `Text` content item. For any other tool name, returns a JSON-RPC error.
fn handle_tools_call(id: &serde_json::Value, request: &serde_json::Value) -> serde_json::Value {
    let params = request.get("params").unwrap_or(&serde_json::Value::Null);

    let tool_name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");

    if tool_name != "echo" {
        return make_error(id, -32602, &format!("Unknown tool: {}", tool_name));
    }

    let message = params
        .get("arguments")
        .and_then(|a| a.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");

    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": message
                }
            ],
            "isError": false
        }
    })
}

/// Handle the `ping` request.
///
/// Returns an empty result object per the MCP specification.
fn handle_ping(id: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {}
    })
}

/// Build a JSON-RPC error response.
///
/// # Arguments
///
/// * `id` - The request ID (echoed back).
/// * `code` - The JSON-RPC error code.
/// * `message` - Human-readable error message.
fn make_error(id: &serde_json::Value, code: i32, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    })
}
