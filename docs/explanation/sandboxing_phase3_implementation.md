# Sandboxing Phase 3: Terminal Tool and CommandValidator Integration

## Overview

Phase 3 wires the sandbox engine (Phase 2) into `TerminalTool`. It adds
`CommandValidator::from_config`, a combined `setpgid` and `apply_sandbox`
`pre_exec` hook, proxy environment variable injection, and a `.git` path guard
at the application layer. No new crate dependencies are introduced.

---

## Changes

### `src/tools/terminal.rs`

#### `CommandValidator` struct

Added field:

```rust
pub network_blocked_programs: Vec<String>,
```

Initialized to `Vec::new()` in `CommandValidator::new`. Populated by
`CommandValidator::from_config` when `sandbox.enabled = true` and
`sandbox.network_enabled = false` and the terminal tool is not opted out.

#### `CommandValidator::new` -- new denylist patterns

Two patterns added to the unconditional denylist:

| Pattern            | Blocks                                       |
| ------------------ | -------------------------------------------- |
| `\bnc\s+-[el]`     | `nc -l` (listen) and `nc -e` (execute relay) |
| `\bsocat\s+.*EXEC` | `socat ... EXEC:` execute-relay invocations  |

#### `CommandValidator::validate` -- network-blocking check

A new check is inserted as the **first** validation step (before the denylist
loop). It compares the program basename against `network_blocked_programs`. If
matched, it returns `XzatomaError::DangerousCommand` with a message explaining
that sandbox network is disabled and which config to change.

Order of checks in `validate`:

1. Network-blocked programs (new -- actionable error message)
2. Denylist regex (existing)
3. Mode-specific allowlist / path validation (existing)

#### `CommandValidator::from_config`

New constructor:

```rust
pub fn from_config(cfg: &crate::config::Config, working_dir: PathBuf) -> Self
```

Sets `network_blocked_programs` to the fixed list: `ssh`, `scp`, `sftp`, `ftp`,
`nc`, `ncat`, `netcat`, `telnet`, `socat`, `nmap`

when all three conditions hold:

- `cfg.sandbox.enabled = true`
- `cfg.sandbox.network_enabled = false`
- `cfg.sandbox.sandboxed_tools.get("terminal") != Some(&false)`

#### `CommandValidator::validate_paths` -- `.git` guard

Added inside the per-token loop, after the `..` traversal check:

```rust
if candidate_path.components().any(|c| c.as_os_str() == ".git") {
    return Err(XzatomaError::PathOutsideWorkingDirectory(...));
}
```

This rejects any argument whose resolved path contains a `.git` component, in
ALL argument positions, regardless of whether the kernel sandbox is active. The
check fires in `FullAutonomous` and `RestrictedAutonomous` (when the program is
in the allowlist).

#### `TerminalTool` struct

Two new fields:

```rust
pub sandbox_policy: Option<std::sync::Arc<crate::sandbox::SandboxPolicy>>,
pub proxy_addr: Option<std::net::SocketAddr>,
```

Both default to `None` in `TerminalTool::new`.

#### `TerminalTool::with_sandbox_policy` and `with_proxy_addr`

Builder methods that set the respective fields (consume `self`, return `Self`
for chaining).

#### `TerminalTool::from_config`

New constructor:

```rust
pub fn from_config(cfg: &crate::config::Config, working_dir: PathBuf)
    -> crate::error::Result<Self>
```

Builds the tool from config. When `sandbox.enabled = true` and the terminal tool
is not opted out, it calls `SandboxPolicy::from_config` and stores the result in
`sandbox_policy`.

#### `execute_command` convenience function

Signature change:

```rust
pub async fn execute_command(
    command: &str,
    working_dir: PathBuf,
    mode: ExecutionMode,
    sandbox_config: Option<&crate::config::SandboxConfig>,
) -> Result<String>
```

When `sandbox_config = Some(cfg)`, builds a `SandboxPolicy` and attaches it to
the tool before executing. Callers that do not need sandboxing pass `None`.

#### Combined `pre_exec` closure

The previous standalone `setpgid` call has been replaced with a single
`pre_exec` closure that:

1. Calls `libc::setpgid(0, 0)` to place the child in its own process group.
2. Calls `crate::sandbox::apply_sandbox(policy)` if a policy is set, mapping
   errors to `std::io::ErrorKind::PermissionDenied`.

There is exactly **one** `cmd.pre_exec(...)` call in the file.

#### Proxy environment variable injection

After the `pre_exec` block, when `self.proxy_addr` is set:

```rust
cmd.env("HTTP_PROXY",  &proxy_url)
   .env("HTTPS_PROXY", &proxy_url)
   .env("http_proxy",  &proxy_url)
   .env("https_proxy", &proxy_url)
   .env("NO_PROXY",    "127.0.0.1,localhost");
```

This wires the Phase 4 proxy into child processes before Phase 4 is implemented.

---

## Testing

Thirteen tests added to `src/tools/terminal.rs`:

| Test                                                                          | Mode             | What it verifies                                         |
| ----------------------------------------------------------------------------- | ---------------- | -------------------------------------------------------- |
| `test_command_validator_from_config_blocks_ssh_when_network_disabled`         | unit             | `DangerousCommand` for `ssh` with network disabled       |
| `test_command_validator_from_config_allows_ssh_when_network_enabled`          | unit             | No network block when `network_enabled=true`             |
| `test_command_validator_from_config_empty_blocked_list_when_sandbox_disabled` | unit             | Empty `network_blocked_programs` when sandbox disabled   |
| `test_command_validator_from_config_empty_blocked_list_when_terminal_opt_out` | unit             | Empty `network_blocked_programs` when terminal opted out |
| `test_command_validator_blocks_nc_listen`                                     | unit             | `nc -l` matches denylist                                 |
| `test_command_validator_blocks_socat_exec`                                    | unit             | `socat EXEC` matches denylist                            |
| `test_command_validator_git_path_blocked_in_write_position`                   | unit             | `.git/config` argument rejected                          |
| `test_terminal_tool_from_config_sandbox_policy_present_when_enabled`          | unit             | `sandbox_policy = Some(...)` when enabled                |
| `test_terminal_tool_from_config_sandbox_policy_absent_when_disabled`          | unit             | `sandbox_policy = None` when disabled                    |
| `test_terminal_tool_from_config_sandbox_policy_absent_when_tool_opt_out`      | unit             | `sandbox_policy = None` when opted out                   |
| `test_terminal_sandbox_allows_write_inside_root`                              | unix integration | `touch` inside sandbox root succeeds                     |
| `test_terminal_sandbox_blocks_write_outside_root`                             | `#[ignore]`      | Absolute path outside root denied                        |
| `test_terminal_sandbox_blocks_git_write`                                      | `#[ignore]`      | `.git/ORIG_HEAD` write denied                            |

The two `#[ignore = "requires kernel sandbox"]` tests apply a real OS-level
sandbox to a child process. They are gated to avoid modifying the test
environment irreversibly.

---

## Verification

```text
cargo fmt --all                                              # ok
cargo check --all-targets --all-features                    # ok
cargo clippy --all-targets --all-features -- -D warnings    # ok
cargo test -p xzatoma --lib -- --skip providers::copilot --skip mcp::auth
# 2671 passed; 0 failed; 39 ignored
grep -c "pre_exec" src/tools/terminal.rs                    # 1
```

---

## Files Modified

| File                                                   | Change                                 |
| ------------------------------------------------------ | -------------------------------------- |
| `src/tools/terminal.rs`                                | All Phase 3 changes as described above |
| `src/sandbox/mod.rs`                                   | Doc comment updated to reflect Phase 3 |
| `CHANGELOG.md`                                         | Phase 2 and Phase 3 entries added      |
| `docs/explanation/sandboxing_phase2_implementation.md` | New (Phase 2 deliverable)              |
| `docs/explanation/sandboxing_phase3_implementation.md` | This file                              |
