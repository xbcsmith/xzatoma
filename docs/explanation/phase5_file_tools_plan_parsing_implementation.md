# Phase 5: File Tools & Plan Parsing Implementation

## Overview

This document summarizes the implementation work completed for Phase 5 — File Tools and Plan Parsing — which provides safe, configurable file operations for the agent and a robust plan parser that supports YAML, JSON, and Markdown.

Goals:
- Provide a `FileOpsTool` tool that can be registered with the agent's `ToolRegistry` and executed safely by name (`file_ops`) from the provider's tool call.
- Provide a `PlanParser` capable of parsing structured execution plans expressed as YAML, JSON, and simple Markdown.
- Add comprehensive tests and validation to ensure security, reliability, and correctness.

This phase focuses on correctness and safety: all file operations are confined to a configured working directory, path validation prevents directory traversal and symlink escapes, and destructive operations require explicit confirmation.

## Components Delivered

- `src/tools/file_ops.rs` — File operations tool and convenience functions
  - Provides `FileOpsTool` with methods: `new`, `validate_path`, `list_files`, `read_file`, `write_file`, `delete_file`, `file_diff`.
  - Provides convenience async helper functions: `read_file`, `write_file`, `list_files`, `search_files`.
  - Uses `walkdir`, `regex`, `similar`, `tokio::fs`, and `rust std::fs` for file ops, patterns and diffs.
- `src/tools/plan.rs` — Plan parsing utilities
  - Provides the `Plan` and `PlanStep` data structures.
  - Provides `PlanParser` with `from_file`, `from_yaml`, `from_json`, `from_markdown`, and `validate` methods.
- `src/tools/mod.rs` — Re-exports convenience and ensures the tools are available to registries and the Agent.
  - Re-exports: `FileOpsTool`, `PlanParser`, `Plan`, `PlanStep`, `generate_diff`, and the convenience `read_file`, `write_file`, `list_files`, etc.
- Tests were added to both modules to validate behavior and edge cases (`#[cfg(test)]` unit tests in the same files).

## Implementation Details

### FileOpsTool

The `FileOpsTool` is a `ToolExecutor` with its `tool_definition` returning a JSON schema describing the supported `operation` and optional parameters:

- Supported operations: `list`, `read`, `write`, `delete`, `diff`.
- Input parameters: `path`, `path2` (for `diff`), `content` (for `write`), `pattern` (for `list`), `recursive` (boolean for `list`), `confirm` (boolean required for `delete`).

Key design decisions:

- Working Directory Confinement
  - All paths are treated as relative to the tool's configured `working_dir`. The `validate_path` function performs:
    - Reject absolute paths and `~` (home) references.
    - Reject `..` path components (directory traversal).
    - If a target exists, canonicalize and ensure it is under the canonicalized `working_dir`.
    - For non-existent target (e.g., writing a new file), ensure the parent directory (canonicalized if possible) is within the `working_dir`.
  - This approach prevents escapes via symlinks and ensures files must remain in the allowed working directory.

- Read File:
  - `read_file` enforces `config.max_file_read_size` (from `ToolsConfig`) and returns an error `ToolResult` if exceeded.
  - Reads are async via `tokio::fs`.

- Write File:
  - Creates parent directories as necessary (`fs::create_dir_all`), with async write performed using `tokio::fs::write`.
  - Returns a `ToolResult` with a success message on success.

- Delete File:
  - The `execute` entrypoint only performs delete when `confirm` is `true`.
  - `delete_file` checks existence and then removes the file with `tokio::fs::remove_file`.

- Listing Files:
  - `list_files` uses `walkdir` and can filter using either a regex (if it compiles) or a simple substring match.
  - Supports non-recursive (`max_depth = 1`) and recursive walks.

- File Diff:
  - `file_diff` reads both files and uses `similar::TextDiff` to create a line-based unified output.

- `ToolResult` semantics:
  - A `ToolResult` contains `success`, `output`, `error`, `truncated`, and `metadata`.
  - `success` indicates the operation outcome; `error` contains failure messages.
  - `truncate_if_needed` is available on `ToolResult` for agent level truncation based on `max_output_size`.

Security: The path validation is deliberately conservative. It uses canonicalization for existing targets to avoid symlink escapes and lexically validates non-existing paths. It rejects absolute and home path usage and uses `..` checks. `delete` requires `confirm=true` to prevent accidental destruction.

Example (Tool registration flow):

```/dev/null/example.rs#L1-20
// Example: Register the file_ops tool with the tool registry
use xzatoma::tools::{FileOpsTool, ToolRegistry};
use std::sync::Arc;
use std::path::PathBuf;

let mut tools = ToolRegistry::new();
let file_tool = Arc::new(FileOpsTool::new(PathBuf::from("."), ToolsConfig::default()));
tools.register("file_ops", file_tool);
```

### Plan Parser

The `Plan` model:
- `Plan`:
  - `name: String` — Plan title (top-level H1 in Markdown).
  - `description: Option<String>` — optional plan description.
  - `steps: Vec<PlanStep>` — sequence of steps describing the plan.
- `PlanStep`:
  - `name: String` — step title (H2 in Markdown).
  - `action: String` — textual action to perform (used as command or description).
  - `context: Option<String>` — multiline context (e.g., a code block or command).

Parsing:
- `from_yaml` and `from_json` use `serde_yaml::from_str` and `serde_json::from_str`, returning `Plan`.
- `from_markdown` uses a simple, robust parser:
  - First `#` heading => plan `name`.
  - The first paragraph after the title => `description` (optional).
  - Each `##` heading => new `PlanStep`.
  - The first non-empty line under the `##` heading => step `action`.
  - Code block content between triple backticks (```) is aggregated and stored as `context`.
  - Lines with `action:`, `command:`, or `context:` under a step are parsed as structured fields.
- `from_file` uses the file extension to dispatch to the appropriate parser (`.yaml/.yml`, `.json`, `.md`).

Verification:
- `PlanParser::validate` ensures:
  - `name` is non-empty.
  - at least one `step` exists.
  - each step has a non-empty `name` and a non-empty `action`.

This parser translates human-authored Markdown plans into a structured `Plan` and is resilient to minor formatting variations.

Example YAML plan:

```/dev/null/example.yaml#L1-40
name: Setup Project
description: Initialize a new Rust project

steps:
  - name: Create project
    action: Run cargo init command
    context: cargo init --bin my-project

  - name: Add dependencies
    action: Update Cargo.toml with required dependencies
    context: |
      [dependencies]
      tokio = { version = "1", features = ["full"] }
      serde = { version = "1", features = ["derive"] }
```

Markdown plan example (convention supported by `PlanParser::from_markdown`):

```/dev/null/example.md#L1-40
# Setup Project
Initialize a new Rust project

## Create project
Run `cargo init --bin my-project`
```bash
cargo init --bin my-project
```

## Add dependencies
Add the following to Cargo.toml:

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
```
```

## Tests & Validation

The following unit tests were added (and are located inline in the respective modules):

- `src/tools/file_ops.rs` tests:
  - `test_read_file_success` — ensures reading contents works.
  - `test_write_file_success` — ensures writing creates directories and writes bytes.
  - `test_list_files_recursive_and_non_recursive` — confirms list behavior and recursion.
  - `test_search_files_regex_and_substring` — confirms regex and substring search.
  - `test_generate_diff_basic` — ensures diff output shows + and - lines correctly.
  - `test_fileops_tool_read_write_delete_and_diff` — end-to-end test for write/read/diff/delete.
  - `test_fileops_tool_list_and_pattern` — verifies regex-based list filtering.
  - `test_validate_path_outside` — ensures path validation rejects escapes.

- `src/tools/plan.rs` tests:
  - `test_from_yaml` — parse YAML and assert fields.
  - `test_from_json` — parse JSON and assert fields.
  - `test_from_markdown` — parse Markdown and assert names, actions, contexts.
  - `test_from_file_yaml` — parse plan from a file and assert correctness.
  - `test_validate_errors` — ensure validation errors for missing fields.
  - `test_parse_plan_and_load_plan` — ensures `parse_plan` and `load_plan` behave as expected.

Validation results observed (local test environment):
- `cargo fmt --all` — passed and code formatted
- `cargo check --all-targets --all-features` — passed with no errors
- `cargo clippy --all-targets --all-features -- -D warnings` — passed, zero warnings
- `cargo test --all-features` — passed; unit test totals in the repo were recorded as: all tests passed.

Note: coverage measurement (e.g., `cargo tarpaulin`) is recommended in CI to ensure coverage >80% as required by our process. The tests above add coverage for the new modules.

## Usage Examples

Registering the file ops tool and calling it as a `ToolExecutor` (via `ToolRegistry`):

```/dev/null/example.rs#L1-40
use std::sync::Arc;
use std::path::PathBuf;

use xzatoma::tools::{FileOpsTool, ToolRegistry, ToolExecutor, ToolsConfig};

let mut registry = ToolRegistry::new();
let file_tool = Arc::new(FileOpsTool::new(PathBuf::from("/tmp/workdir"), ToolsConfig::default()));
registry.register("file_ops", file_tool);

// Execute from an agent-like context (simplified)
let executor = registry.get("file_ops").unwrap();
let params = serde_json::json!({
    "operation": "read",
    "path": "README.md"
});
let result = executor.execute(params).await.unwrap();
println!("Tool output: {}", result.to_message());
```

Using plan parser:

```/dev/null/example.rs#L1-40
use xzatoma::tools::PlanParser;

let yaml = r#"
name: Build Project
steps:
  - name: Build
    action: cargo build
"#;

let plan = PlanParser::from_yaml(yaml).unwrap();
PlanParser::validate(&plan).unwrap();
```

## Security Considerations & Caveats

- All file operations are strictly confined to the tool's `working_dir`. Attempts to use absolute paths, `~`, `..`, or escape the working directory are rejected.
- We canonicalize existing targets to prevent symlink-based escapes. For non-existent targets, we validate their parent directory.
- `delete` is guarded by a `confirm=true` parameter — the tool will decline to delete files without explicit confirmation.
- The `read_file` function enforces the configured `max_file_read_size` (from `ToolsConfig`), returning an informative error `ToolResult` if exceeded.
- `list_files` performs regex matching or substring checks. Regular expressions are validated via `regex::Regex::new` and fall back to substring if invalid.
- `write_file` uses `tokio::fs::write` and ensures directory creation succeeds before writing. Atomicity and concurrency considerations should be addressed by callers if necessary.

Limitations & Future work:
- Race conditions are possible between validation and file operations (TOCTOU). For highly-sensitive operations a stronger locking or atomic API is required.
- The `delete` semantics expect the caller (agent/invoker) to manage confirmation; the agent needs a higher-level confirmation flow for user-permission scenarios.
- The current `PlanParser::from_markdown` supports a very pragmatic subset of Markdown. For heavyweight plan descriptions or advanced frontmatter, consider integrating a formal Markdown AST parser or frontmatters (YAML frontmatter).
- Consider adding streaming read for large files (chunked reads) and `tail -f` style options if instrumenting real-time monitoring.

## Integration & Example Workflow

- Register `FileOpsTool` in your agent's tool registry at startup:
  - The tool should be provided with a configured working directory (the agent's workspace).
  - The `Agent` will send tool calls as JSON to the tools via `ToolRegistry.get(...)`, where the tool performs the required operation and returns a `ToolResult`.

- Plan execution:
  - `PlanParser::from_file` is used to load a plan from YAML/JSON/Markdown.
  - `PlanParser::validate` ensures the plan is well-formed before execution.
  - The Agent or a `run_plan` command translates each step into tool calls: read a file, run commands using terminal tool, write files etc.
  - For a small example, a plan step that uses `context` could be translated into a `terminal` or `file_ops` call depending on the action.

## References

- Code:
  - `src/tools/file_ops.rs` – FileOps tool implementation (primary)
  - `src/tools/plan.rs` – Plan parser and associated tests
  - `src/tools/mod.rs` – Re-exports and ToolRegistry integration
- Tests:
  - Unit tests included inline inside the modules.
- Tools & Libraries:
  - `walkdir` for directory traversal
  - `regex` for pattern matching
  - `similar` for diffs
  - `tokio::fs` for async file operations

---

## Next Steps

- Phase 6 (CLI Integration and Plan Runner):
  - Integrate `PlanParser` with the CLI and implement `run_plan` to translate plan steps into a series of provider and tool calls.
  - Wire confirmation flows to fully support `confirm=true` requirements and user approval in `Interactive` mode.
- Run a targeted `tarpaulin` coverage report in CI to validate test coverage >80%.
- Add integration tests that exercise the full end-to-end flow: parse plan → register tools → agent executes steps using `file_ops` and `terminal`.

If you want, I can continue by:
- Adding `run_plan` CLI wiring to execute a parsed plan using `Agent` and `ToolRegistry`.
- Adding integration or end-to-end tests to exercise the plan execution path.
