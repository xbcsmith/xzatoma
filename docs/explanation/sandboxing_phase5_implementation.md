# Sandboxing Phase 5: SandboxGuard Integration for File-Operation Tools

## Summary

Phase 5 integrates the `SandboxGuard` (implemented in Phase 5.1) into every
file-operation tool and wires it through `ToolRegistryBuilder` so that all
I/O-performing tools enforce the configured filesystem sandbox policy before
touching the filesystem.

## What Changed

### Task 5.2: SandboxGuard added to 10 file-operation tools

Each tool received three additions:

1. A `sandbox_guard: Option<Arc<SandboxGuard>>` field, defaulting to `None`.
2. A `with_sandbox_guard(guard: Arc<SandboxGuard>) -> Self` builder method for
   method-chaining construction.
3. A guard check at the start of `execute` (or inside the inner method for tools
   using the `Err(ToolResult)` return pattern) after path resolution but before
   any filesystem I/O.

| Tool                  | File                            | Guard call                                       | Method         |
| --------------------- | ------------------------------- | ------------------------------------------------ | -------------- |
| `WriteFileTool`       | `src/tools/write_file.rs`       | `check_write(path)?`                             | `execute`      |
| `EditFileTool`        | `src/tools/edit_file.rs`        | `check_write(full_path)?`                        | `execute`      |
| `CreateDirectoryTool` | `src/tools/create_directory.rs` | `check_write(dir_path)`                          | inner `create` |
| `DeletePathTool`      | `src/tools/delete_path.rs`      | `check_write(path)?`                             | `execute`      |
| `CopyPathTool`        | `src/tools/copy_path.rs`        | `check_read(source)`, `check_write(destination)` | inner `copy`   |
| `MovePathTool`        | `src/tools/move_path.rs`        | `check_read(source)`, `check_write(destination)` | inner `rename` |
| `ReadFileTool`        | `src/tools/read_file.rs`        | `check_read(path)?`                              | `execute`      |
| `ListDirectoryTool`   | `src/tools/list_directory.rs`   | `check_read(path)?`                              | `execute`      |
| `FindPathTool`        | `src/tools/find_path.rs`        | `check_read(working_dir)?`                       | `execute`      |
| `GrepTool`            | `src/tools/grep.rs`             | `check_read(working_dir)?`                       | `execute`      |

#### Error propagation pattern

Tools whose `execute` method returns `Result<ToolResult, XzatomaError>` use the
`?` operator directly. `SandboxError` converts to `XzatomaError` via the
`From<SandboxError> for XzatomaError` impl in `src/error.rs`, so propagation is
automatic.

Tools using the inner-method pattern (`CreateDirectoryTool`, `CopyPathTool`,
`MovePathTool`) return `std::result::Result<ToolResult, ToolResult>` from their
inner `create`/`copy`/`rename` methods. These cannot use `?` to propagate
`XzatomaError`, so they convert the denial to a `ToolResult::error(...)` value,
which is consistent with how other path errors are reported.

```rust
// Pattern used in inner methods (create / copy / rename):
if let Some(guard) = &self.sandbox_guard
    && let Err(e) = guard.check_write(&path)
{
    return Ok(ToolResult::error(e.to_string()));
}
```

#### GrepTool note

`GrepTool` is not registered through `ToolRegistryBuilder` (it is wired
separately). The `sandbox_guard` field and `with_sandbox_guard` builder were
added so that callers that construct `GrepTool` directly can opt in to sandbox
enforcement, but the registry does not set it.

### Task 5.3: ToolRegistryBuilder wired with optional SandboxGuard

`src/tools/registry_builder.rs` was updated in both `build_for_planning` and
`build_for_write`.

At the start of each function, an optional guard is constructed:

```rust
let guard: Option<Arc<SandboxGuard>> = if let Some(cfg) = &self.config {
    let file_ops_sandboxed = cfg
        .sandbox
        .sandboxed_tools
        .get("file_ops")
        .copied()
        .unwrap_or(true);
    if cfg.sandbox.enabled && file_ops_sandboxed {
        Some(Arc::new(SandboxGuard::new(
            cfg.sandbox.clone(),
            self.working_dir.clone(),
        )))
    } else {
        None
    }
} else {
    None
};
```

The guard is `None` when:

- No full `Config` was provided to the builder (`with_config` was not called).
- `cfg.sandbox.enabled` is `false`.
- `cfg.sandbox.sandboxed_tools["file_ops"]` is explicitly `false`.

When the guard is `Some`, each tool construction is followed by an optional
`.with_sandbox_guard(Arc::clone(&g))` call using the if-let pattern:

```rust
let read_tool = ReadFileTool::new(...);
let read_tool = if let Some(g) = &guard {
    read_tool.with_sandbox_guard(Arc::clone(g))
} else {
    read_tool
};
```

This keeps the existing test surface intact: tests that call `ToolXxx::new(...)`
without a guard continue to work because `sandbox_guard` defaults to `None`.

## Also Fixed: Pre-existing Clippy Warnings in guard.rs

While running quality gates the following pre-existing `clippy::collapsible_if`
and `clippy::field_reassign_with_default` warnings in `src/sandbox/guard.rs`
were surfaced by `--all-targets`. They were fixed as part of this task to allow
the gate to pass:

- Collapsed the nested `if let Component::Normal(name) = component` /
  `if name == ".git"` guard in `check_write`.
- Replaced `let mut cfg = SandboxConfig::default(); cfg.xxx = yyy;` patterns in
  unit tests with struct-literal initializers using
  `..SandboxConfig::default()`.

## Quality Gates

All four gates passed:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

Test result: **2688 passed; 0 failed; 40 ignored**.
