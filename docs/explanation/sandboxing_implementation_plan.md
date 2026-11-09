# Sandboxing Implementation Plan

## Overview

This plan introduces kernel-enforced, capability-based sandboxing into XZatoma
using the `nono` crate, which provides a unified Rust API over Landlock (Linux
5.13+) and Seatbelt/`sandbox_init` (macOS). Five phases deliver incrementally
testable value: config schema, sandbox engine, terminal integration, network
hardening, and file-operation guards with documentation.

The existing application-level controls (denylist patterns, path walk, SSRF
checks) are complemented -- not replaced -- by kernel enforcement. Once Landlock
or Seatbelt is applied in a child process it is irreversible for that process
regardless of UID, which is the security property application logic alone cannot
provide.

`nono` is a **hard dependency**. The build fails intentionally on platforms
other than Linux and macOS. Do not add a cfg-gate or no-op fallback anywhere in
this plan.

---

## Current State Analysis

### Existing Infrastructure

| File                                     | Relevant symbols                                      | Current sandbox posture                                                                              |
| ---------------------------------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `src/tools/terminal.rs` L88-994          | `CommandValidator`, `TerminalTool`, `execute_command` | Allowlist/denylist and path-walk at the Rust layer only; `pre_exec` at L630-639 calls `setpgid` only |
| `src/tools/fetch.rs` L194-556            | `SsrfValidator`, `FetchTool`                          | Blocks private IPs and loopback in Rust; no kernel-level network restriction                         |
| `src/tools/registry_builder.rs` L121-437 | `ToolRegistryBuilder`, `build_for_write`              | Constructs all tools without any sandbox config parameter                                            |
| `src/config.rs` L1810-1849               | `TerminalConfig`                                      | Mode, timeout, output limits; no `SandboxConfig` field                                               |
| `src/error.rs` L27-350                   | `XzatomaError`                                        | No sandbox-specific error variant                                                                    |
| `src/lib.rs` L38-53                      | module list                                           | No `pub mod sandbox;` declaration                                                                    |
| `src/security.rs` L20-425                | URL validation, secret redaction                      | Does not touch process spawning or filesystem access control                                         |

File-operation tools (`WriteFileTool`, `EditFileTool`, `CreateDirectoryTool`,
`DeletePathTool`, `CopyPathTool`, `MovePathTool`) rely entirely on process
filesystem permissions for access control.

### Identified Issues

1. All sandboxing is application-level and bypassed by any syscall the
   application layer did not anticipate.
2. `CommandValidator::validate_paths` (L305-371) performs a lexical walk but
   cannot intercept writes the spawned process makes after starting.
3. `.git` write protection is not OS-enforced; the denylist only matches
   explicit `.git` tokens in the argument string.
4. The fetch tool has no kernel-level network restriction.
5. No per-tool sandbox granularity exists in config.
6. No config-defined filesystem root; agent scope equals process working
   directory only.
7. No HTTP/HTTPS proxy path for host-specific network grants.
8. `SandboxError` type does not exist; no sandbox error variant in
   `XzatomaError`.
9. `src/sandbox/` module does not exist; `src/lib.rs` has no declaration for it.
10. `CommandValidator::from_config` does not exist; `TerminalTool::from_config`
    does not exist.
11. `SandboxPolicy` needs `allow_execute` for subprocesses to exec any binary;
    system library paths and `$TMPDIR` are not in any allowed path list.
12. `ToolRegistryBuilder::build_for_write` (L367-437) creates `TerminalTool` via
    `CommandValidator::new` without sandbox wiring and creates `FetchTool` (if
    present) without sandbox config.

---

## Implementation Phases

### Phase 1: Error Types, Config Schema, and Module Registration

Establish all scaffolding consumed by later phases: the `SandboxError` error
type, the `SandboxConfig` configuration struct, the module declaration in
`lib.rs`, environment variable overrides, CLI flags, and config validation. **No
behavioral change is introduced.** The module exists but is empty. The config is
parsed and validated but not acted upon.

#### Task 1.1 Add `SandboxError` to `src/error.rs` and `XzatomaError`

**File**: `src/error.rs`

Add a new `SandboxError` enum in `src/error.rs` **before**
`pub enum XzatomaError` (insert before L27):

```rust
/// Errors produced by the sandbox engine.
#[derive(thiserror::Error, Debug)]
pub enum SandboxError {
    /// The kernel Landlock ABI version is below the minimum required.
    #[error("Landlock ABI version {found} is below minimum required version 1")]
    KernelVersionTooOld { found: u32 },

    /// The Seatbelt / sandbox_init call failed.
    #[error("Seatbelt sandbox_init failed: {message}")]
    SeatbeltApplyFailed { message: String },

    /// A path in the sandbox policy could not be resolved.
    #[error("Sandbox policy path error: {path}: {reason}")]
    PolicyPathError { path: String, reason: String },

    /// A filesystem access was denied by the sandbox guard.
    #[error("Sandbox denied access to {path}: {reason}")]
    PathDenied { path: String, reason: String },

    /// Generic sandbox engine error.
    #[error("Sandbox error: {0}")]
    Engine(String),
}
```

Add a `Sandbox` variant to `XzatomaError` (insert after the `Acp` variant at
L345):

```rust
    /// Sandbox enforcement error
    #[error("Sandbox error: {0}")]
    Sandbox(String),
```

Add a `From` conversion (append after the last `impl From<...> for XzatomaError`
block, after L536):

```rust
impl From<crate::sandbox::SandboxError> for XzatomaError {
    fn from(e: crate::sandbox::SandboxError) -> Self {
        XzatomaError::Sandbox(e.to_string())
    }
}
```

#### Task 1.2 Add `SandboxConfig` to `src/config.rs`

**File**: `src/config.rs`

Add the following struct after `TerminalConfig` (after L1849). Use the exact
attribute set shown:

```rust
/// Kernel-enforced sandbox policy configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", default)]
pub struct SandboxConfig {
    /// Master switch. When `false`, no sandbox layer is applied to any tool.
    pub enabled: bool,

    /// Per-tool sandbox opt-out map.
    ///
    /// Valid keys: `"terminal"`, `"fetch"`, `"file_ops"`.
    /// A key absent from the map means the tool IS sandboxed when
    /// `enabled = true`. Set a key to `false` to opt that tool out.
    pub sandboxed_tools: std::collections::HashMap<String, bool>,

    /// Root directory the agent may read and write.
    ///
    /// When `None`, the working directory at runtime is used.
    /// Supports `~/` prefix expansion via `resolve_config_like_path`.
    pub filesystem_root: Option<String>,

    /// Additional paths outside `filesystem_root` granted read access.
    pub extra_read_paths: Vec<String>,

    /// Additional paths outside `filesystem_root` granted write access.
    pub extra_write_paths: Vec<String>,

    /// When `true`, deny writes to any `.git` directory even inside
    /// `filesystem_root`.
    pub protect_git: bool,

    /// When `true`, terminal child processes may make network calls through
    /// the host-filtering HTTP proxy. When `false`, all outbound network is
    /// denied at the kernel level and at the application validator level.
    pub network_enabled: bool,

    /// Hostnames permitted to be reached when `network_enabled = true`.
    ///
    /// Entries must be bare hostnames without a protocol prefix.
    /// Valid: `"api.example.com"`. Invalid: `"https://api.example.com"`.
    pub network_allowed_hosts: Vec<String>,

    /// Local TCP port for the host-filtering HTTP CONNECT proxy.
    ///
    /// Must be in the range 1024..=65535. Default: 8877.
    pub proxy_port: u16,
}

fn default_sandbox_enabled() -> bool { true }
fn default_protect_git() -> bool { true }
fn default_proxy_port() -> u16 { 8877 }

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            sandboxed_tools: std::collections::HashMap::new(),
            filesystem_root: None,
            extra_read_paths: Vec::new(),
            extra_write_paths: Vec::new(),
            protect_git: true,
            network_enabled: false,
            network_allowed_hosts: Vec::new(),
            proxy_port: 8877,
        }
    }
}
```

Add the field `pub sandbox: SandboxConfig,` to `pub struct Config` (L29-49)
after the existing `pub log: LogConfig` field.

**YAML configuration example** (for use in documentation and tests):

```yaml
sandbox:
  enabled: true
  sandboxed_tools:
    fetch: false
  filesystem_root: "~/projects/myproject"
  extra_read_paths:
    - "/usr/share/doc"
  extra_write_paths:
    - "/tmp/agent_output"
  protect_git: true
  network_enabled: false
  network_allowed_hosts: []
  proxy_port: 8877
```

**Breaking change**: `sandbox.enabled` defaults to `true`. Workflows that write
outside the project directory, issue network calls from terminal commands, or
write into `.git` will fail. Escape hatches:

- `sandbox.enabled: false` in the config file.
- `XZATOMA_SANDBOX_ENABLED=false` in the environment.
- `sandbox.extra_write_paths: ["/the/needed/path"]` for targeted access.

Document this change in `CHANGELOG.md` (create the file if it does not exist).

#### Task 1.3 Register `src/sandbox/` Module in `src/lib.rs` and Create Stub Files

**File**: `src/lib.rs`

Add `pub mod sandbox;` after `pub mod security;` at L49:

```rust
pub mod sandbox;
```

**Files to create** (stub content only; implementations come in Phase 2):

`src/sandbox/mod.rs`:

```rust
//! Kernel-enforced sandboxing for XZatoma tool execution.
//!
//! This module provides OS-level process isolation using Landlock (Linux) and
//! Seatbelt (macOS) via the `nono` crate.

pub mod engine;
pub mod guard;
pub mod policy;
pub mod proxy;

pub use engine::apply_sandbox;
pub use guard::SandboxGuard;
pub use policy::{SandboxContext, SandboxPolicy};

pub use crate::error::SandboxError;
```

`src/sandbox/policy.rs` -- empty except for `// TODO: Phase 2`

`src/sandbox/engine.rs` -- empty except for `// TODO: Phase 2`

`src/sandbox/guard.rs` -- empty except for `// TODO: Phase 5`

`src/sandbox/proxy.rs` -- empty except for `// TODO: Phase 4`

#### Task 1.4 Add Environment Variable Overrides and CLI Flags

**File**: `src/config.rs`, inside `fn apply_env_vars` (L2110-3134)

Add the following block at the end of `apply_env_vars`, reading sandbox
environment variables in the pattern established by existing fields:

| Environment variable                    | Field                                | Type             | Parsing function                |
| --------------------------------------- | ------------------------------------ | ---------------- | ------------------------------- |
| `XZATOMA_SANDBOX_ENABLED`               | `self.sandbox.enabled`               | `bool`           | `parse_env_bool`                |
| `XZATOMA_SANDBOX_FILESYSTEM_ROOT`       | `self.sandbox.filesystem_root`       | `Option<String>` | `resolve_config_like_path`      |
| `XZATOMA_SANDBOX_PROTECT_GIT`           | `self.sandbox.protect_git`           | `bool`           | `parse_env_bool`                |
| `XZATOMA_SANDBOX_NETWORK_ENABLED`       | `self.sandbox.network_enabled`       | `bool`           | `parse_env_bool`                |
| `XZATOMA_SANDBOX_NETWORK_ALLOWED_HOSTS` | `self.sandbox.network_allowed_hosts` | `Vec<String>`    | split on `','`, trim each entry |
| `XZATOMA_SANDBOX_PROXY_PORT`            | `self.sandbox.proxy_port`            | `u16`            | `str::parse::<u16>()`           |

**File**: `src/cli.rs`

Add to the CLI argument struct, following the existing pattern for other config
overrides:

```rust
/// Disable all sandboxing (equivalent to sandbox.enabled = false in config)
#[arg(long, default_value_t = false)]
pub no_sandbox: bool,

/// Override the sandbox filesystem root directory
#[arg(long)]
pub sandbox_root: Option<String>,
```

**File**: `src/config.rs`, inside `fn apply_cli_overrides` (L3136-3149)

Add:

```rust
if cli.no_sandbox {
    self.sandbox.enabled = false;
}
if let Some(root) = &cli.sandbox_root {
    self.sandbox.filesystem_root = Some(root.clone());
}
```

#### Task 1.5 Add `validate_sandbox_config` to `Config::validate`

**File**: `src/config.rs`

Add the following private method to `impl Config` (L1989-3828):

```rust
fn validate_sandbox_config(&self) -> Result<(), XzatomaError> {
    let s = &self.sandbox;
    if s.proxy_port < 1024 {
        return Err(XzatomaError::Config(format!(
            "sandbox.proxy_port must be >= 1024, got {}",
            s.proxy_port
        )));
    }
    for host in &s.network_allowed_hosts {
        if host.is_empty() {
            return Err(XzatomaError::Config(
                "sandbox.network_allowed_hosts entries must not be empty".to_string(),
            ));
        }
        if host.contains("://") {
            return Err(XzatomaError::Config(format!(
                "sandbox.network_allowed_hosts entry '{}' must not contain a protocol prefix",
                host
            )));
        }
    }
    if !s.network_enabled && !s.network_allowed_hosts.is_empty() {
        tracing::warn!(
            "sandbox.network_allowed_hosts is non-empty but sandbox.network_enabled is false; \
             the allowed hosts list will be ignored at runtime"
        );
    }
    for path in s.extra_read_paths.iter().chain(s.extra_write_paths.iter()) {
        if path.is_empty() {
            return Err(XzatomaError::Config(
                "sandbox extra_read_paths and extra_write_paths entries must not be empty"
                    .to_string(),
            ));
        }
    }
    if let Some(root) = &s.filesystem_root {
        if root.is_empty() {
            return Err(XzatomaError::Config(
                "sandbox.filesystem_root must not be an empty string".to_string(),
            ));
        }
    }
    let valid_tool_keys = ["terminal", "fetch", "file_ops"];
    for key in s.sandboxed_tools.keys() {
        if !valid_tool_keys.contains(&key.as_str()) {
            return Err(XzatomaError::Config(format!(
                "sandbox.sandboxed_tools key '{}' is not valid; valid keys: {:?}",
                key, valid_tool_keys
            )));
        }
    }
    Ok(())
}
```

Call `self.validate_sandbox_config()?;` inside `pub fn validate` (L3163-3412)
immediately after the existing `validate_skills_config` call.

#### Task 1.6 Testing Requirements

**File**: `src/config.rs`, inside `mod tests` (after the last existing test)

All test functions must follow the naming convention
`test_<struct>_<condition>_<expected>`.

| Test name                                                                          | What it verifies                                               |
| ---------------------------------------------------------------------------------- | -------------------------------------------------------------- |
| `test_sandbox_config_defaults`                                                     | All fields match the Default impl table in Task 1.2            |
| `test_sandbox_config_deserialize_all_fields`                                       | YAML snippet with every field populates every field correctly  |
| `test_sandbox_config_deserialize_omitted_fields_use_defaults`                      | Partial YAML yields default values for omitted fields          |
| `test_sandbox_config_validation_rejects_proxy_port_below_1024`                     | port 0 and port 1023 each return `Config` error                |
| `test_sandbox_config_validation_rejects_protocol_prefix_in_allowed_host`           | `"https://example.com"` returns `Config` error                 |
| `test_sandbox_config_validation_rejects_empty_allowed_host`                        | `""` in allowed hosts returns `Config` error                   |
| `test_sandbox_config_validation_rejects_invalid_tool_key`                          | `sandboxed_tools: {"unknown": true}` returns `Config` error    |
| `test_sandbox_config_validation_warns_when_allowed_hosts_set_but_network_disabled` | Passes without error; emits no hard failure                    |
| `test_apply_env_vars_overrides_sandbox_enabled`                                    | `XZATOMA_SANDBOX_ENABLED=false` sets `sandbox.enabled = false` |
| `test_apply_env_vars_overrides_sandbox_filesystem_root`                            | Env var sets `sandbox.filesystem_root`                         |
| `test_apply_env_vars_overrides_sandbox_proxy_port`                                 | Env var sets `sandbox.proxy_port`                              |
| `test_apply_env_vars_overrides_sandbox_network_allowed_hosts`                      | Comma-separated env var populates the Vec                      |

#### Task 1.7 Deliverables

- [ ] `SandboxError` enum in `src/error.rs` with variants as listed in Task 1.1.
- [ ] `XzatomaError::Sandbox(String)` variant added to `src/error.rs`.
- [ ] `From<SandboxError> for XzatomaError` impl added to `src/error.rs`.
- [ ] `SandboxConfig` struct in `src/config.rs` with exact field names, types,
      and serde attributes from Task 1.2.
- [ ] `sandbox: SandboxConfig` field added to `Config` struct.
- [ ] `CHANGELOG.md` updated with breaking-change notice.
- [ ] Stub files `src/sandbox/{mod,policy,engine,guard,proxy}.rs` created.
- [ ] `pub mod sandbox;` added to `src/lib.rs` after `pub mod security;`.
- [ ] All environment variable overrides implemented in `apply_env_vars`.
- [ ] `--no-sandbox` and `--sandbox-root` CLI flags implemented.
- [ ] `validate_sandbox_config` implemented and called from `Config::validate`.
- [ ] All tests from Task 1.6 passing.

#### Task 1.8 Success Criteria

Run the following commands in order; all must exit with code 0:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

The default `SandboxConfig` round-trips through serde YAML without data loss.
`cargo check` succeeds on both Linux and macOS (expected build failure on
Windows is not tested in CI).

---

### Phase 2: Core Sandbox Engine

Implement the platform-specific sandbox engine. This phase adds `nono`, creates
`src/sandbox/policy.rs` and `src/sandbox/engine.rs`, and delivers a tested
engine that applies Landlock or Seatbelt policies. No tool is wired to the
sandbox yet.

#### Task 2.1 Add `nono` Dependency

**File**: `Cargo.toml`

Add to the `[dependencies]` table (not platform-gated):

```toml
nono = "0.1"  # verify exact version against https://docs.rs/nono before pinning
```

After adding the crate, run:

```text
cargo audit --deny warnings
```

If any advisory exists for `nono` or its transitive deps, do not proceed until
the advisory is resolved or suppressed with justification.

#### Task 2.2 Define `SandboxPolicy` and `SandboxContext` in `policy.rs`

**File**: `src/sandbox/policy.rs` (replace stub)

`SandboxPolicy` is a builder-pattern struct. It must be `Send + Sync`.

```rust
/// Builder that accumulates filesystem and network access rules before
/// applying them to a child process via [`apply_sandbox`].
///
/// # Send + Sync
///
/// `SandboxPolicy` is `Send + Sync` so it can be wrapped in `Arc<>` and
/// referenced from a `pre_exec` closure.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
    pub(crate) allow_read_paths: Vec<PathBuf>,
    pub(crate) allow_write_paths: Vec<PathBuf>,
    pub(crate) deny_write_paths: Vec<PathBuf>,
    pub(crate) allow_execute_paths: Vec<PathBuf>,
    pub(crate) allow_network: bool,
}
```

Builder methods (all return `Self` for chaining):

| Method signature                                                   | Effect                                                           |
| ------------------------------------------------------------------ | ---------------------------------------------------------------- |
| `pub fn new() -> Self`                                             | Empty policy; deny all writes, deny all network                  |
| `pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self`    | Grant read access to path and subtree                            |
| `pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self`   | Grant write access to path and subtree                           |
| `pub fn deny_write(mut self, path: impl Into<PathBuf>) -> Self`    | Narrow-deny writes to path; overrides a broader allow_write      |
| `pub fn allow_execute(mut self, path: impl Into<PathBuf>) -> Self` | Grant execute rights to path and subtree (Landlock `FS_EXECUTE`) |
| `pub fn with_network(mut self, allow: bool) -> Self`               | Set network access; `true` = allow, `false` = deny               |

`SandboxContext` is the zero-sized proof-of-application type:

```rust
/// Zero-sized type returned by a successful [`apply_sandbox`] call.
///
/// Holding a `SandboxContext` provides evidence that the sandbox was
/// applied in the current process; it carries no runtime data.
#[derive(Debug)]
pub struct SandboxContext {
    _private: (),
}

impl SandboxContext {
    pub(crate) fn applied() -> Self {
        Self { _private: () }
    }
}
```

Add the `from_config` constructor:

```rust
/// Build a `SandboxPolicy` from a [`SandboxConfig`] and a project root.
///
/// Paths are resolved in this order:
/// 1. `filesystem_root` (or `project_root` when `None`) -- read + write.
/// 2. Each entry in `extra_read_paths` -- read only.
/// 3. Each entry in `extra_write_paths` -- write (implies read).
/// 4. `.git` subdirectories under `filesystem_root` when `protect_git = true` -- deny write.
/// 5. `std::env::temp_dir()` -- read + write (always added).
/// 6. Platform system paths required for dynamic linking and binary execution
///    (see table below) -- read + execute.
///
/// # System paths added unconditionally
///
/// Linux:
///   `/bin`, `/usr/bin`, `/usr/local/bin`, `/lib`, `/lib64`,
///   `/usr/lib`, `/usr/local/lib`, `/etc/ld.so.cache`,
///   `/etc/ld.so.conf`, `/etc/ld.so.conf.d`, `/proc/self`,
///   `/dev/null`, `/dev/urandom`
///
/// macOS:
///   `/bin`, `/usr/bin`, `/usr/local/bin`, `/usr/lib`,
///   `/System/Library`, `/Library/Frameworks`,
///   `/private/var/db/dyld`, `/usr/share`
///
/// # Errors
///
/// Returns `SandboxError::PolicyPathError` if a path in the config
/// cannot be represented as a `PathBuf`.
pub fn from_config(
    cfg: &SandboxConfig,
    project_root: &Path,
) -> Result<SandboxPolicy, SandboxError> { ... }
```

Inside `from_config`:

1. Determine `root: PathBuf` =
   `cfg.filesystem_root.as_deref().map(PathBuf::from).unwrap_or_else(|| project_root.to_path_buf())`.
2. Add `.allow_read(root.clone()).allow_write(root.clone())`.
3. For each entry in `cfg.extra_read_paths`:
   `.allow_read(PathBuf::from(entry))`.
4. For each entry in `cfg.extra_write_paths`:
   `.allow_read(PathBuf::from(entry)).allow_write(PathBuf::from(entry))`.
5. If `cfg.protect_git`: use `walkdir::WalkDir` to find all `.git` directories
   directly under `root` (depth limit 4 to avoid expensive traversal); call
   `.deny_write(git_dir)` for each found.
6. Add `.allow_read(std::env::temp_dir()).allow_write(std::env::temp_dir())`.
7. Add platform system paths (read + execute) as listed in the doc comment.
8. If `cfg.network_enabled`: `.with_network(true)`.

#### Task 2.3 Implement `apply_sandbox` for Linux (Landlock) in `engine.rs`

**File**: `src/sandbox/engine.rs` (replace stub)

Full function signature:

```rust
/// Apply the sandbox policy to the **current process**.
///
/// This function MUST be called from a `pre_exec` closure (inside the child
/// process after `fork`, before `exec`). It must not allocate on the heap
/// beyond what `nono` performs internally; see the `// SAFETY:` comment in
/// `TerminalTool::execute` (Phase 3).
///
/// # Errors
///
/// - `SandboxError::KernelVersionTooOld` if the Landlock ABI version < 1.
/// - `SandboxError::SeatbeltApplyFailed` if the macOS sandbox_init call fails.
/// - `SandboxError::Engine` for other internal errors.
pub fn apply_sandbox(policy: &SandboxPolicy) -> Result<SandboxContext, SandboxError>
```

**Linux implementation** (inside `#[cfg(target_os = "linux")]`):

1. Call the `nono` Landlock API to determine the kernel ABI version. If the
   version is `< 1`, return
   `Err(SandboxError::KernelVersionTooOld { found: version })`. Do **not**
   return `Ok`; do **not** call `SandboxContext::applied()`. The spawn must fail
   with a clear error message.
2. Build a Landlock ruleset. Grant the following access rights for each path
   category:
   - `allow_read_paths`: `FS_READ_FILE | FS_READ_DIR`
   - `allow_write_paths`:
     `FS_WRITE_FILE | FS_REMOVE_FILE | FS_MAKE_REG | FS_MAKE_DIR | FS_READ_FILE | FS_READ_DIR`
   - `deny_write_paths`: Apply as a narrowing rule that restricts write bits
     within a previously-granted write subtree (consult the `nono` Landlock docs
     for the exact API to add a narrowing rule).
   - `allow_execute_paths`: `FS_EXECUTE`
3. If `!policy.allow_network`: add network restrictions. Landlock ABI v4+
   provides `LANDLOCK_ACCESS_NET_CONNECT_TCP` and
   `LANDLOCK_ACCESS_NET_BIND_TCP`. Use the highest ABI version reported by the
   kernel; gracefully skip network rules on kernels that report ABI < 4.
4. Call `set_no_new_privs()` (sets the `PR_SET_NO_NEW_PRIVS` flag).
5. Restrict the ruleset. Both calls are irreversible for this process.
6. Return `Ok(SandboxContext::applied())`.

#### Task 2.4 Implement `apply_sandbox` for macOS (Seatbelt) in `engine.rs`

**File**: `src/sandbox/engine.rs`

**macOS implementation** (inside `#[cfg(target_os = "macos")]`):

1. Build an SBPL (Seatbelt Policy Language) profile string:
   - Start: `(version 1)\n(deny default)\n`
   - For each path in `allow_read_paths`:
     `(allow file-read* (subpath "/absolute/path"))\n`
   - For each path in `allow_write_paths`:
     `(allow file-write* (subpath "/absolute/path"))\n`
     `(allow file-read* (subpath "/absolute/path"))\n`
   - For each path in `deny_write_paths`:
     `(deny file-write* (subpath "/absolute/path"))\n` (More-specific deny
     overrides broader allow in SBPL.)
   - For each path in `allow_execute_paths`:
     `(allow file-execute (subpath "/absolute/path"))\n`
   - If `allow_network`: `(allow network*)\n`
   - Else: `(deny network*)\n`
2. Call `nono`'s Seatbelt wrapper (verify exact function name against
   `docs.rs/nono`). On failure, map the error string to
   `SandboxError::SeatbeltApplyFailed { message: err_string }`.
3. Return `Ok(SandboxContext::applied())`.

All paths in the profile must be absolute. Call `.canonicalize()` on each
`PathBuf` before inserting; if `canonicalize` fails (path does not exist yet),
use the lexically normalized absolute path from
`CommandValidator::lexical_absolute_normalize` (reuse existing code at
`src/tools/terminal.rs` L263-283).

#### Task 2.5 Testing Requirements

**File**: `src/sandbox/policy.rs`, `mod tests`

| Test name                                              | What it verifies                                                                                 |
| ------------------------------------------------------ | ------------------------------------------------------------------------------------------------ |
| `test_policy_new_has_no_paths_and_no_network`          | `SandboxPolicy::new()` has empty path lists and `allow_network = false`                          |
| `test_policy_from_config_adds_root_read_and_write`     | `from_config` with default config adds `root` to both `allow_read_paths` and `allow_write_paths` |
| `test_policy_from_config_adds_tmpdir`                  | `from_config` adds `std::env::temp_dir()` to read and write lists                                |
| `test_policy_from_config_protect_git_adds_deny_write`  | When `protect_git = true` and a `.git` dir exists under root, it appears in `deny_write_paths`   |
| `test_policy_from_config_extra_read_paths_added`       | Extra read paths appear in `allow_read_paths`                                                    |
| `test_policy_from_config_extra_write_paths_added`      | Extra write paths appear in both `allow_read_paths` and `allow_write_paths`                      |
| `test_policy_from_config_network_enabled_sets_flag`    | `network_enabled = true` sets `allow_network = true`                                             |
| `test_policy_from_config_network_disabled_clears_flag` | `network_enabled = false` (default) sets `allow_network = false`                                 |
| `test_policy_from_config_adds_linux_system_paths`      | On Linux, system paths `/bin`, `/usr/lib`, etc. appear in `allow_read_paths`                     |
| `test_policy_from_config_adds_macos_system_paths`      | On macOS, system paths `/System/Library`, etc. appear in `allow_read_paths`                      |
| `test_sandbox_context_applied_is_debug`                | `SandboxContext::applied()` can be formatted with `{:?}`                                         |

#### Task 2.6 Deliverables

- [ ] `nono` in `Cargo.toml` `[dependencies]` as an unconditional hard
      dependency; `cargo audit --deny warnings` passes.
- [ ] `src/sandbox/policy.rs` with `SandboxPolicy`, `SandboxContext`, and
      `SandboxPolicy::from_config` as specified in Task 2.2.
- [ ] `src/sandbox/engine.rs` with `apply_sandbox` implementing both the Linux
      (Landlock) and macOS (Seatbelt) backends as specified in Tasks 2.3 and
      2.4.
- [ ] All tests from Task 2.5 passing.

#### Task 2.7 Success Criteria

Run in order; all must exit with code 0:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
cargo audit --deny warnings
```

`SandboxPolicy` is `Send + Sync` (verified by a
`fn assert_send_sync<T: Send + Sync>()` call in a test). `apply_sandbox`
compiles inside a `#[cfg(target_os = "linux")]` block and a
`#[cfg(target_os = "macos")]` block without unused-import warnings.

---

### Phase 3: Terminal Tool and `CommandValidator` Integration

Wire the sandbox engine into `TerminalTool`. This phase also adds
`CommandValidator::from_config` (needed here before Phase 4's network blocking)
and the combined `pre_exec` hook.

#### Task 3.1 Extend `CommandValidator` in `src/tools/terminal.rs`

**File**: `src/tools/terminal.rs`

**3.1a Add `network_blocked_programs` field**

Add to `CommandValidator` struct (L121-130) after the `denylist` field:

```rust
/// Programs blocked when sandbox.network_enabled = false.
///
/// Populated by `CommandValidator::from_config`; empty when sandbox is
/// disabled or network is permitted.
pub network_blocked_programs: Vec<String>,
```

**3.1b Add network-blocking check to `CommandValidator::validate`**

Inside `validate` (L197-241), insert the following as the **first** check,
before the denylist loop:

```rust
let program_basename = std::path::Path::new(&parsed.program)
    .file_name()
    .and_then(|n| n.to_str())
    .unwrap_or(&parsed.program);
if self.network_blocked_programs.contains(&program_basename.to_string()) {
    return Err(XzatomaError::DangerousCommand(format!(
        "program '{}' is blocked: sandbox network is disabled; \
         set sandbox.network_enabled = true to permit network tools",
        program_basename
    )));
}
```

**3.1c Extend `CommandValidator::new` denylist**

In `CommandValidator::new` (L134-188), add the following entries to the
`denylist_patterns` Vec:

```rust
r"\bnc\s+-[el]",        // netcat listen or execute
r"\bsocat\s+.*EXEC",   // socat execute relay
```

**3.1d Add `CommandValidator::from_config` constructor**

```rust
/// Construct a `CommandValidator` from configuration.
///
/// Sets `network_blocked_programs` to the fixed list of programs that open
/// raw TCP/UDP sockets and do not honour proxy environment variables when
/// `cfg.sandbox.enabled = true` and `cfg.sandbox.network_enabled = false`.
///
/// # Arguments
///
/// * `cfg` -- application config
/// * `working_dir` -- directory commands are constrained to
pub fn from_config(cfg: &crate::config::Config, working_dir: PathBuf) -> Self {
    let mut validator = Self::new(cfg.agent.terminal.default_mode, working_dir);
    if cfg.sandbox.enabled
        && !cfg.sandbox.network_enabled
        && cfg.sandbox.sandboxed_tools.get("terminal") != Some(&false)
    {
        validator.network_blocked_programs = vec![
            "ssh".to_string(),
            "scp".to_string(),
            "sftp".to_string(),
            "ftp".to_string(),
            "nc".to_string(),
            "ncat".to_string(),
            "netcat".to_string(),
            "telnet".to_string(),
            "socat".to_string(),
            "nmap".to_string(),
        ];
    }
    validator
}
```

**3.1e Add `.git` path guard to `CommandValidator::validate_paths`**

In `validate_paths` (L305-371), after the existing loop that checks paths
against the working directory, add:

```rust
// Belt-and-suspenders: reject explicit writes to .git regardless of
// kernel sandbox state so the error message is actionable.
if path.components().any(|c| c.as_os_str() == ".git") {
    // Only reject if the argument appears in a write position.
    // Heuristic: if parsed.program is "rm", "mv", "cp" (as destination),
    // or any argument to the FIRST token is a path-like argument.
    // For safety, reject .git paths in ALL argument positions.
    return Err(XzatomaError::PathOutsideWorkingDirectory(format!(
        "writes to .git directories are forbidden: {}",
        path.display()
    )));
}
```

#### Task 3.2 Add Sandbox Support to `TerminalTool`

**File**: `src/tools/terminal.rs`

**3.2a Add field to `TerminalTool` struct** (L528-532):

```rust
pub struct TerminalTool {
    pub validator: CommandValidator,
    pub config: TerminalConfig,
    pub safety_mode: SafetyMode,
    /// Kernel sandbox policy applied to each child process.
    pub sandbox_policy: Option<std::sync::Arc<crate::sandbox::SandboxPolicy>>,
    /// Address of the running host-filtering HTTP proxy, if any.
    pub proxy_addr: Option<std::net::SocketAddr>,
}
```

**3.2b Add builder methods** (after `with_safety_mode` at L553-556):

```rust
pub fn with_sandbox_policy(
    mut self,
    policy: crate::sandbox::SandboxPolicy,
) -> Self {
    self.sandbox_policy = Some(std::sync::Arc::new(policy));
    self
}

pub fn with_proxy_addr(mut self, addr: std::net::SocketAddr) -> Self {
    self.proxy_addr = Some(addr);
    self
}
```

**3.2c Add `TerminalTool::from_config` constructor**:

```rust
/// Construct a `TerminalTool` from the full application config.
///
/// Applies sandbox policy when `cfg.sandbox.enabled = true` and the
/// `"terminal"` key is not `false` in `cfg.sandbox.sandboxed_tools`.
///
/// # Errors
///
/// Returns `XzatomaError` if `SandboxPolicy::from_config` fails to resolve
/// configured paths.
pub fn from_config(
    cfg: &crate::config::Config,
    working_dir: PathBuf,
) -> crate::error::Result<Self> {
    let validator = CommandValidator::from_config(cfg, working_dir.clone());
    let mut tool = Self {
        validator,
        config: cfg.agent.terminal.clone(),
        safety_mode: SafetyMode::AlwaysConfirm,
        sandbox_policy: None,
        proxy_addr: None,
    };
    let terminal_sandboxed =
        cfg.sandbox.sandboxed_tools.get("terminal") != Some(&false);
    if cfg.sandbox.enabled && terminal_sandboxed {
        let policy =
            crate::sandbox::SandboxPolicy::from_config(&cfg.sandbox, &working_dir)?;
        tool.sandbox_policy = Some(std::sync::Arc::new(policy));
    }
    Ok(tool)
}
```

**3.2d Update `execute_command` convenience function** (L88-117):

Add a `sandbox_config: Option<&crate::config::SandboxConfig>` parameter. When
`Some(cfg)` is provided, build a `SandboxPolicy` via
`SandboxPolicy::from_config(cfg, &working_dir)` and set it on the tool. Update
all callers of `execute_command` across the codebase to pass `None` until Phase
5 wires the config through.

#### Task 3.3 Combine `setpgid` and Sandbox in a Single `pre_exec` Closure

**File**: `src/tools/terminal.rs`, inside `TerminalTool::execute` (L579-745)

`tokio::process::Command::pre_exec` may only be set **once** per command;
calling it a second time overwrites the first closure. The existing `setpgid`
call at L630-639 must be **merged** with the sandbox application into a single
`unsafe` block:

Replace L630-639 with:

```rust
// SAFETY: This closure runs in the child process after `fork` and before
// `exec`. It must be async-signal-safe (no heap allocation, no Rust
// runtime, no locking). `setpgid` is async-signal-safe. The Landlock
// ruleset application via `nono` must perform no heap allocation beyond
// what the kernel requires directly. The Seatbelt `sandbox_init` wrapper
// in `nono` is documented as signal-safe; verify against the nono
// release notes when updating the dependency version.
let policy_for_exec = self.sandbox_policy.clone();
unsafe {
    cmd.pre_exec(move || {
        // 1. Place child in its own process group.
        if libc::setpgid(0, 0) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        // 2. Apply kernel sandbox, if configured.
        if let Some(policy) = &policy_for_exec {
            crate::sandbox::apply_sandbox(policy).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::PermissionDenied, e.to_string())
            })?;
        }
        Ok(())
    });
}
```

After the `pre_exec` block and before `cmd.spawn()`, inject proxy environment
variables when the proxy is active:

```rust
if let Some(proxy_addr) = self.proxy_addr {
    let proxy_url = format!("http://{}", proxy_addr);
    cmd.env("HTTP_PROXY",  &proxy_url)
       .env("HTTPS_PROXY", &proxy_url)
       .env("http_proxy",  &proxy_url)
       .env("https_proxy", &proxy_url)
       .env("NO_PROXY",    "127.0.0.1,localhost");
}
```

#### Task 3.4 Testing Requirements

**File**: `src/tools/terminal.rs`, inside `mod tests`

| Test name                                                                     | What it verifies                                                                                     |
| ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `test_command_validator_from_config_blocks_ssh_when_network_disabled`         | `validate("ssh user@host")` returns `DangerousCommand` when `sandbox.network_enabled = false`        |
| `test_command_validator_from_config_allows_ssh_when_network_enabled`          | `validate("ssh user@host")` does not return `DangerousCommand` when `sandbox.network_enabled = true` |
| `test_command_validator_from_config_empty_blocked_list_when_sandbox_disabled` | `sandbox.enabled = false` yields empty `network_blocked_programs`                                    |
| `test_command_validator_from_config_empty_blocked_list_when_terminal_opt_out` | `sandboxed_tools: {"terminal": false}` yields empty `network_blocked_programs`                       |
| `test_command_validator_blocks_nc_listen`                                     | `nc -l 4444` matches unconditional denylist regex                                                    |
| `test_command_validator_blocks_socat_exec`                                    | `socat - EXEC:/bin/sh` matches unconditional denylist regex                                          |
| `test_command_validator_git_path_blocked_in_write_position`                   | Argument containing `.git/` triggers `PathOutsideWorkingDirectory` error                             |
| `test_terminal_tool_from_config_sandbox_policy_present_when_enabled`          | `from_config` with `sandbox.enabled = true` sets `sandbox_policy = Some(...)`                        |
| `test_terminal_tool_from_config_sandbox_policy_absent_when_disabled`          | `from_config` with `sandbox.enabled = false` sets `sandbox_policy = None`                            |
| `test_terminal_tool_from_config_sandbox_policy_absent_when_tool_opt_out`      | `sandboxed_tools: {"terminal": false}` sets `sandbox_policy = None`                                  |
| `test_terminal_sandbox_allows_write_inside_root`                              | Spawning `touch` inside `working_dir` succeeds with sandbox enabled                                  |
| `test_terminal_sandbox_blocks_write_outside_root` (gated)                     | `touch /tmp/escape` fails with permission error; `#[ignore = "requires kernel sandbox"]`             |
| `test_terminal_sandbox_blocks_git_write` (gated)                              | `touch .git/ORIG_HEAD` fails; `#[ignore = "requires kernel sandbox"]`                                |

#### Task 3.5 Deliverables

- [ ] `network_blocked_programs: Vec<String>` field on `CommandValidator`.
- [ ] Network-blocking check at the top of `CommandValidator::validate`.
- [ ] New denylist regex entries for `nc -l`/`-e` and `socat EXEC`.
- [ ] `CommandValidator::from_config` constructor.
- [ ] `.git` path guard added to `CommandValidator::validate_paths`.
- [ ] `sandbox_policy` and `proxy_addr` fields on `TerminalTool`.
- [ ] `TerminalTool::with_sandbox_policy`, `with_proxy_addr` builder methods.
- [ ] `TerminalTool::from_config` constructor.
- [ ] `execute_command` updated with `Option<&SandboxConfig>` parameter.
- [ ] Combined `setpgid` + sandbox `pre_exec` closure (single call to
      `pre_exec`; old standalone `setpgid` block removed).
- [ ] All tests from Task 3.4 passing.

#### Task 3.6 Success Criteria

Run in order; all must exit with code 0:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

`grep -r "pre_exec" src/tools/terminal.rs | wc -l` must output `1` (single
`pre_exec` call). Spawning a command that writes outside `working_dir` returns a
permission-denied error on a kernel with Landlock support when the sandbox is
enabled.

---

### Phase 4: Network Sandbox and Fetch Tool Hardening

Implement the host-filtering HTTP CONNECT proxy, wire `network_allowed_hosts`
into `SsrfValidator`, and inject proxy environment variables into terminal child
processes.

#### Task 4.1 Implement the Host-Filtering HTTP CONNECT Proxy

**File**: `src/sandbox/proxy.rs` (replace stub)

Implement `SandboxProxy` using `tokio::net::TcpListener` and
`tokio::net::TcpStream`. The proxy:

1. Binds to `127.0.0.1:<config.sandbox.proxy_port>` (default `127.0.0.1:8877`).
2. For each inbound `CONNECT host:port HTTP/1.x` request: a. Parses the `host`
   component (strip the port number). b. If `host` is not in
   `network_allowed_hosts`, respond:
   `HTTP/1.1 407 Proxy Authentication Required\r\nContent-Length: 0\r\n\r\n` and
   close the connection. c. If `host` is in the allowlist, validate the resolved
   IP address using the existing `SsrfValidator` from `src/tools/fetch.rs`. If
   validation fails, respond with `407` and close. d. On success, open a TCP
   connection to the target, respond
   `HTTP/1.1 200 Connection established\r\n\r\n`, and relay bytes
   bidirectionally using `tokio::io::copy_bidirectional`.
3. The proxy is started as a `tokio::spawn` task. It holds a
   `tokio_util::sync::CancellationToken`; on cancellation it stops accepting new
   connections.

Public API:

```rust
pub struct SandboxProxy {
    addr: std::net::SocketAddr,
    cancel: tokio_util::sync::CancellationToken,
}

impl SandboxProxy {
    /// Start the proxy on the configured port.
    ///
    /// Returns the bound `SocketAddr` for use by `TerminalTool::with_proxy_addr`.
    pub async fn start(cfg: &SandboxConfig) -> Result<Self, SandboxError> { ... }

    /// Return the address the proxy is listening on.
    pub fn addr(&self) -> std::net::SocketAddr { self.addr }

    /// Shut down the proxy gracefully.
    pub fn shutdown(&self) { self.cancel.cancel(); }
}
```

The proxy is started in `AgentRuntime` (or the ACP runtime) when
`cfg.sandbox.enabled && cfg.sandbox.network_enabled`. Locate the runtime
initialization path (search for `ToolRegistryBuilder::new` across the codebase
to find all entry points) and add proxy startup there. Store the resulting
`SandboxProxy` in the runtime struct so it is shut down on `Drop`.

#### Task 4.2 Extend `SsrfValidator` and `FetchTool`

**File**: `src/tools/fetch.rs`

**4.2a Add `allowed_hosts` field to `SsrfValidator`** (L194-197):

```rust
pub struct SsrfValidator {
    allow_private_ips: bool,
    /// When `Some`, only listed hostnames are permitted.
    /// When `None`, no additional host restriction is applied.
    allowed_hosts: Option<Vec<String>>,
}
```

**4.2b Add builder method**:

```rust
pub fn with_allowed_hosts(mut self, hosts: Vec<String>) -> Self {
    self.allowed_hosts = Some(hosts);
    self
}
```

**4.2c Add check in `SsrfValidator::validate`** (L231-247):

After the existing scheme and private-IP checks, add:

```rust
if let Some(hosts) = &self.allowed_hosts {
    let host = url
        .host_str()
        .ok_or_else(|| /* existing SSRF error pattern */ ...)?;
    if !hosts.iter().any(|h| h == host) {
        return Err(/* error message: "Host '{host}' is not in network_allowed_hosts" */);
    }
}
```

**4.2d Update `FetchTool::new`** (L641-658):

Add `sandbox_cfg: Option<&SandboxConfig>` as the last parameter. When
`Some(cfg)` and `cfg.enabled` and
`cfg.sandboxed_tools.get("fetch") != Some(&false)` and
`!cfg.network_allowed_hosts.is_empty()`, apply
`.with_allowed_hosts(cfg.network_allowed_hosts.clone())` to the `SsrfValidator`.

**Callsites of `FetchTool::new` to update** (search with
`grep -rn "FetchTool::new" src/`):

- `src/tools/registry_builder.rs` `build_for_write` (if FetchTool is registered
  there; confirm by searching the file).
- Any other call in `src/agent/`, `src/acp/`, or `src/commands/`. Pass `None`
  for `sandbox_cfg` at all callsites until Phase 5 threads the full
  `SandboxConfig` through `ToolRegistryBuilder`.

#### Task 4.3 Wire Proxy Address into `ToolRegistryBuilder`

**File**: `src/tools/registry_builder.rs`

Add a `sandbox_config: Option<SandboxConfig>` field to `ToolRegistryBuilder`
(L121-139) and a corresponding builder method:

```rust
pub fn with_sandbox_config(mut self, cfg: SandboxConfig) -> Self {
    self.sandbox_config = Some(cfg);
    self
}
```

In `build_for_write` (L367-437), replace the `CommandValidator::new` call at
L426-427 with
`CommandValidator::from_config(full_cfg, self.working_dir.clone())` and replace
`TerminalTool::new` with
`TerminalTool::from_config(full_cfg, self.working_dir.clone())?` when
`self.sandbox_config` is `Some`.

**Note**: To call `TerminalTool::from_config` the builder needs access to the
full `Config` reference, not just `SandboxConfig`. Change the builder field to
`config: Option<Arc<Config>>` and add a
`pub fn with_config(mut self, cfg: Arc<Config>) -> Self` method. Update existing
callers of `with_terminal_config` and `with_tools_config` to populate the
individual sub-configs from the `Config` reference they already hold.

**Callers of `ToolRegistryBuilder::new` to update** (run
`grep -rn "ToolRegistryBuilder::new" src/`):

Identify every callsite; pass `with_config(Arc::clone(&cfg))` at each one.

#### Task 4.4 Testing Requirements

| Test name                                                    | File                             | What it verifies                                                                              |
| ------------------------------------------------------------ | -------------------------------- | --------------------------------------------------------------------------------------------- |
| `test_proxy_allows_listed_host`                              | `src/sandbox/proxy.rs mod tests` | CONNECT to listed host is forwarded (use local TCP echo)                                      |
| `test_proxy_blocks_unlisted_host`                            | `src/sandbox/proxy.rs mod tests` | CONNECT to unlisted host returns `407`                                                        |
| `test_proxy_blocks_private_ip_even_if_listed`                | `src/sandbox/proxy.rs mod tests` | CONNECT to listed host resolving to `127.0.0.1` returns `407`                                 |
| `test_ssrf_validator_with_allowed_hosts_blocks_unlisted`     | `src/tools/fetch.rs mod tests`   | `validate("https://evil.internal")` fails when host not in list                               |
| `test_ssrf_validator_with_allowed_hosts_permits_listed`      | `src/tools/fetch.rs mod tests`   | `validate("https://example.com")` passes when `example.com` is listed                         |
| `test_ssrf_validator_no_allowed_hosts_no_restriction`        | `src/tools/fetch.rs mod tests`   | `allowed_hosts = None` does not add restriction (existing behavior preserved)                 |
| `test_fetch_tool_new_with_sandbox_config_sets_allowed_hosts` | `src/tools/fetch.rs mod tests`   | `FetchTool::new(sandbox_cfg)` with non-empty `network_allowed_hosts` configures the validator |
| `test_fetch_tool_new_without_sandbox_config_no_restriction`  | `src/tools/fetch.rs mod tests`   | `FetchTool::new(None)` does not restrict hosts                                                |

#### Task 4.5 Deliverables

- [ ] `src/sandbox/proxy.rs` with `SandboxProxy::start` and
      `SandboxProxy::shutdown`.
- [ ] `SandboxProxy` started and stored in runtime struct(s) when
      `sandbox.enabled && sandbox.network_enabled`.
- [ ] `SsrfValidator::allowed_hosts` field and `with_allowed_hosts` builder.
- [ ] Allowlist check added to `SsrfValidator::validate`.
- [ ] `FetchTool::new` updated with `Option<&SandboxConfig>` parameter; all
      callsites updated.
- [ ] `ToolRegistryBuilder::with_config` method; all callers updated.
- [ ] Proxy env vars (`HTTP_PROXY`, `HTTPS_PROXY`, etc.) injected in
      `TerminalTool::execute` when `proxy_addr` is set.
- [ ] All tests from Task 4.4 passing.

#### Task 4.6 Success Criteria

Run in order; all must exit with code 0:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
```

`FetchTool` rejects requests to hosts not in `network_allowed_hosts` when the
list is non-empty. Terminal subprocesses spawned with `proxy_addr = Some(...)`
have `HTTP_PROXY` and `HTTPS_PROXY` set. The proxy returns `407` for unlisted
hosts. `grep -c "FetchTool::new" src/` reflects the updated signature across all
callsites.

---

### Phase 5: File Operation Guards, Integration Tests, and Documentation

Apply application-level `SandboxGuard` path checks to all file-operation tools,
thread `SandboxConfig` through the full `ToolRegistryBuilder` call chain,
deliver integration tests that verify escape attempts are blocked, and create
all documentation.

#### Task 5.1 Create `SandboxGuard` in `src/sandbox/guard.rs`

**File**: `src/sandbox/guard.rs` (replace stub)

```rust
/// Application-level path guard enforcing the sandbox policy for file tools.
///
/// This is a belt-and-suspenders layer: the kernel sandbox (Landlock or
/// Seatbelt) provides the hard OS-level barrier; `SandboxGuard` provides a
/// clear, early error message before any syscall is attempted.
///
/// When `sandbox.enabled = false`, both `check_read` and `check_write`
/// return `Ok(())` unconditionally.
#[derive(Debug, Clone)]
pub struct SandboxGuard {
    cfg: crate::config::SandboxConfig,
    /// Resolved absolute filesystem root (cfg.filesystem_root or cwd).
    root: std::path::PathBuf,
}

impl SandboxGuard {
    /// Construct a guard from config and the runtime project root.
    pub fn new(cfg: crate::config::SandboxConfig, project_root: std::path::PathBuf) -> Self {
        let root = cfg
            .filesystem_root
            .as_deref()
            .map(std::path::PathBuf::from)
            .unwrap_or(project_root);
        Self { cfg, root }
    }

    /// Verify that `path` is within an allowed read location.
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::PathDenied` if `path` is outside all allowed
    /// read paths and sandboxing is enabled.
    pub fn check_read(&self, path: &std::path::Path) -> Result<(), SandboxError> { ... }

    /// Verify that `path` is within an allowed write location and not inside
    /// a protected `.git` directory.
    ///
    /// # Errors
    ///
    /// Returns `SandboxError::PathDenied` if `path` is outside the allowed
    /// write subtree, inside `.git` when `protect_git = true`, or
    /// sandboxing is enabled and the path fails any configured rule.
    pub fn check_write(&self, path: &std::path::Path) -> Result<(), SandboxError> { ... }
}
```

Implementation rules:

- Both methods return `Ok(())` immediately when `self.cfg.enabled = false`.
- `check_read`: canonicalize `path`; accept if it starts with `self.root` or any
  entry in `cfg.extra_read_paths` (after canonicalization).
- `check_write`: canonicalize `path`; reject if it does not start with
  `self.root` or any `cfg.extra_write_paths` entry; reject if any component
  equals `.git` and `cfg.protect_git = true`.
- When canonicalization fails (file does not yet exist), use lexical absolute
  normalization (same logic as `CommandValidator::lexical_absolute_normalize` at
  `src/tools/terminal.rs` L263-283).

#### Task 5.2 Apply `SandboxGuard` to File Operation Tools

For each tool listed below, add `sandbox_guard: Option<Arc<SandboxGuard>>` field
and call the guard at the start of `execute`:

| Tool struct           | File                            | Guard call                                         |
| --------------------- | ------------------------------- | -------------------------------------------------- |
| `WriteFileTool`       | `src/tools/write_file.rs`       | `check_write(target_path)?`                        |
| `EditFileTool`        | `src/tools/edit_file.rs`        | `check_write(target_path)?`                        |
| `CreateDirectoryTool` | `src/tools/create_directory.rs` | `check_write(target_path)?`                        |
| `DeletePathTool`      | `src/tools/delete_path.rs`      | `check_write(target_path)?`                        |
| `CopyPathTool`        | `src/tools/copy_path.rs`        | `check_write(dest_path)?`; `check_read(src_path)?` |
| `MovePathTool`        | `src/tools/move_path.rs`        | `check_write(dest_path)?`; `check_read(src_path)?` |
| `ReadFileTool`        | `src/tools/read_file.rs`        | `check_read(target_path)?`                         |
| `ListDirectoryTool`   | `src/tools/list_directory.rs`   | `check_read(target_path)?`                         |
| `FindPathTool`        | `src/tools/find_path.rs`        | `check_read(root_path)?`                           |
| `GrepTool`            | `src/tools/grep.rs`             | `check_read(root_path)?`                           |

Each tool gains a builder method
`fn with_sandbox_guard(mut self, guard: Arc<SandboxGuard>) -> Self`. When
`sandbox_guard` is `None`, the guard call is a no-op.

#### Task 5.3 Thread `SandboxGuard` through `ToolRegistryBuilder`

**File**: `src/tools/registry_builder.rs`

After the `with_config` method added in Phase 4:

1. In `build_for_write` (L367-437): construct `Arc<SandboxGuard>` from the
   stored `config` and `working_dir`; pass it to every tool constructor via
   `.with_sandbox_guard(Arc::clone(&guard))`.
2. In `build_for_planning` (L315-340): do the same for read-only tools.
3. All existing tests in `mod tests` (L490-784) must still pass; update the test
   builders to pass a default `SandboxConfig` where needed.

#### Task 5.4 Integration Tests

**File**: `tests/integration/sandbox_integration.rs` (create new file)

```text
tests/
  integration/
    sandbox_integration.rs
```

Add `[[test]]` or `mod` declaration so `cargo test` discovers the file.

| Test name                                    | Description                                                                                      | Kernel required |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------ | --------------- |
| `test_terminal_cannot_write_outside_project` | `touch /tmp/escape_test` fails with permission error                                             | Yes             |
| `test_terminal_can_write_inside_project`     | `touch` inside project dir succeeds                                                              | No              |
| `test_terminal_git_write_blocked_by_guard`   | `touch .git/ORIG_HEAD` blocked by application guard                                              | No              |
| `test_write_file_tool_blocked_outside_root`  | `WriteFileTool::execute` with path outside root returns `SandboxError::PathDenied`               | No              |
| `test_write_file_tool_blocked_git_dir`       | `WriteFileTool::execute` targeting `.git/ORIG_HEAD` returns `SandboxError::PathDenied`           | No              |
| `test_read_file_tool_blocked_outside_root`   | `ReadFileTool::execute` with path outside root returns `SandboxError::PathDenied`                | No              |
| `test_fetch_tool_blocked_by_allowed_hosts`   | `FetchTool::fetch("https://evil.internal")` with non-empty `network_allowed_hosts` returns error | No              |
| `test_sandbox_disabled_bypasses_guard`       | `sandbox.enabled = false`; `SandboxGuard::check_write` returns `Ok(())` for any path             | No              |
| `test_sandbox_per_tool_opt_out_file_ops`     | `sandboxed_tools: {"file_ops": false}`; guard returns `Ok(())` for write tools                   | No              |

Tests requiring kernel sandbox are gated with
`#[ignore = "requires kernel sandbox"]`. Run them with:

```text
cargo test -p xzatoma --test sandbox_integration -- --include-ignored
```

#### Task 5.5 Update `Makefile` Quality Gates

**File**: `Makefile`

Add the following target after the `audit` target (L62-63):

```makefile
# Run kernel-enforced sandbox integration tests (requires Linux 5.13+ or macOS)
sandbox-test: ; $(info $(M) running sandbox integration tests...) @ ## Runs sandbox integration tests with kernel enforcement
 $Q $(CARGO) test -p xzatoma --test sandbox_integration -- --include-ignored
```

Add `sandbox-test` to the `.PHONY` line (L92).

Document the new target in `README.md` under the "Development" section,
immediately after the existing `audit` entry.

#### Task 5.6 Documentation

Create the following documents. Each must pass
`markdownlint --fix --config .markdownlint.json <file>` and
`prettier --write --parser markdown --prose-wrap always <file>` with exit code 0
before the task is considered complete.

**Reference** (`docs/reference/sandbox_configuration.md`):

- H2 section: "Configuration Reference" -- table with all `SandboxConfig`
  fields, types, defaults, environment variable names, and valid ranges.
- H2 section: "Valid `sandboxed_tools` Keys" -- table listing `terminal`,
  `fetch`, `file_ops` and what each controls.
- H2 section: "Platform Requirements" -- Landlock kernel version (5.13+), macOS
  Seatbelt availability, intended build failure on Windows.
- H2 section: "Example Configuration" -- the YAML example from Task 1.2.

**How-to** (`docs/how-to/configure_sandboxing.md`):

- Task: "Restrict the agent to a single project directory."
- Task: "Permit network access to specific hosts."
- Task: "Disable sandboxing for a specific tool."
- Task: "Disable sandboxing for a single run using an environment variable."
- Task: "Verify that the sandbox is active" (what log lines to look for).

**Tutorial** (`docs/tutorials/sandboxing_quickstart.md`):

- Learning-oriented walkthrough: start from default config, observe the sandbox
  in action, configure an `extra_write_path`, verify it works.
- Should be completable in under 15 minutes.

**This document** (`docs/explanation/sandboxing_implementation_plan.md`) serves
as the implementation explanation per AGENTS.md Rule 5.

#### Task 5.7 Testing Requirements

**File**: `src/sandbox/guard.rs`, `mod tests`

| Test name                                                      | What it verifies                                                                        |
| -------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| `test_sandbox_guard_check_write_allows_path_inside_root`       | Path inside `root` passes `check_write`                                                 |
| `test_sandbox_guard_check_write_blocks_path_outside_root`      | Path outside `root` returns `SandboxError::PathDenied`                                  |
| `test_sandbox_guard_check_write_blocks_git_dir`                | Path with `.git` component returns `SandboxError::PathDenied` when `protect_git = true` |
| `test_sandbox_guard_check_write_allows_git_when_protect_false` | Path with `.git` passes when `protect_git = false`                                      |
| `test_sandbox_guard_check_read_allows_extra_read_path`         | Path in `extra_read_paths` passes `check_read`                                          |
| `test_sandbox_guard_check_read_blocks_outside_all_paths`       | Path not in root or extra paths returns `SandboxError::PathDenied`                      |
| `test_sandbox_guard_disabled_allows_any_write`                 | `sandbox.enabled = false`; `check_write("/etc/passwd")` returns `Ok(())`                |
| `test_sandbox_guard_disabled_allows_any_read`                  | `sandbox.enabled = false`; `check_read("/etc/shadow")` returns `Ok(())`                 |

#### Task 5.8 Deliverables

- [ ] `src/sandbox/guard.rs` with `SandboxGuard::new`, `check_read`,
      `check_write`, and all doc comments.
- [ ] All ten file-operation tools updated with `sandbox_guard` field and guard
      calls (see table in Task 5.2).
- [ ] `ToolRegistryBuilder::build_for_write` and `build_for_planning` construct
      and distribute `Arc<SandboxGuard>`.
- [ ] All existing `ToolRegistryBuilder` tests still pass.
- [ ] `tests/integration/sandbox_integration.rs` with all tests from Task 5.4.
- [ ] `sandbox-test` Makefile target added and `README.md` updated.
- [ ] `docs/reference/sandbox_configuration.md` created and lint-clean.
- [ ] `docs/how-to/configure_sandboxing.md` created and lint-clean.
- [ ] `docs/tutorials/sandboxing_quickstart.md` created and lint-clean.
- [ ] All tests from Task 5.7 passing.

#### Task 5.9 Success Criteria

Run in order; all must exit with code 0:

```text
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
cargo test -p xzatoma --test sandbox_integration
cargo audit --deny warnings
markdownlint --config .markdownlint.json docs/reference/sandbox_configuration.md
markdownlint --config .markdownlint.json docs/how-to/configure_sandboxing.md
markdownlint --config .markdownlint.json docs/tutorials/sandboxing_quickstart.md
```

All new public structs, enums, functions, and methods carry `///` doc comments
with `# Arguments`, `# Returns`, `# Errors`, and `# Examples` sections as
applicable. `cargo doc --no-deps 2>&1 | grep -c "warning"` outputs `0`.

---

## Dependency Summary

| Phase | Dependency                   | Location in `Cargo.toml` | Justification                                                                    |
| ----- | ---------------------------- | ------------------------ | -------------------------------------------------------------------------------- |
| 2     | `nono` (hard, unconditional) | `[dependencies]`         | Landlock and Seatbelt backends; build fails intentionally on non-Linux/non-macOS |
| 4     | none                         | --                       | Proxy reuses `tokio`, `tokio-util` already in the dependency graph               |

---

## Risk and Mitigation

| Risk                                                    | Impact                                                    | Mitigation                                                                                                                                      |
| ------------------------------------------------------- | --------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Build fails on non-Linux/non-macOS                      | Build error on Windows                                    | Intended; document in `README.md` "Platform Support"; remove any Windows build targets from CI                                                  |
| Landlock ABI < 1 on target kernel                       | `apply_sandbox` returns `Err`; process spawn fails        | Log `SandboxError::KernelVersionTooOld` at ERROR level; require minimum kernel 5.13 in deployment docs                                          |
| `pre_exec` closure must be async-signal-safe            | Deadlock or corruption if non-signal-safe code runs       | Single closure with `setpgid` + nono call; `// SAFETY:` comment; verify nono release notes on each upgrade                                      |
| Seatbelt `sandbox_init` fails silently on newer macOS   | Policy not applied; false sense of security               | Map all `sandbox_init` return codes to `SandboxError::SeatbeltApplyFailed`; fail the spawn on any non-zero return                               |
| System paths missing from `SandboxPolicy`               | Subprocess cannot load dynamic libraries or exec binaries | `from_config` adds the full platform system path list unconditionally; add an integration test that spawns `echo hello` with the sandbox active |
| `proxy_port` already in use                             | `SandboxProxy::start` fails                               | Return `SandboxError::Engine("port already in use")` from `start`; surface as a startup error                                                   |
| `ToolRegistryBuilder` callsite audit incomplete         | Some runtimes bypass sandbox                              | Run `grep -rn "ToolRegistryBuilder::new" src/` and update every result before marking Phase 4 complete                                          |
| `execute_command` convenience function bypasses sandbox | Sandbox not applied in code paths using the shortcut      | Update `execute_command` signature in Phase 3; run `grep -rn "execute_command" src/` and confirm callers pass config                            |

---

## Recommended Implementation Order

1. **Phase 1** -- Error types, config schema, module stubs; no behavioral
   change; safe to merge independently.
2. **Phase 2** -- Sandbox engine; tested in isolation; not yet wired to tools.
3. **Phase 3** -- Terminal tool and `CommandValidator`; highest security value;
   includes the raw-socket program denylist.
4. **Phase 4** -- Network proxy and fetch hardening; extends Phase 2 engine.
5. **Phase 5** -- File operation guards, integration tests, and documentation;
   ties all phases together.
