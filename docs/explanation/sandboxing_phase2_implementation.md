# Sandboxing Phase 2: Core Sandbox Engine

## Overview

Phase 2 implements the sandbox policy builder and the platform-specific engine
that applies Landlock (Linux) or Seatbelt (macOS) restrictions to a child
process. No tool is wired to the sandbox in this phase; that wiring is the
subject of Phases 3-5.

---

## Components

### `src/sandbox/policy.rs`

Defines two public types:

- **`SandboxPolicy`** -- a builder that accumulates filesystem and network
  access rules. It is `Send + Sync` so it can be wrapped in `Arc` and referenced
  from a `pre_exec` closure.
- **`SandboxContext`** -- a zero-sized proof type. Holding a `SandboxContext`
  provides evidence that `apply_sandbox` completed successfully in the current
  process.

#### `SandboxPolicy` builder methods

| Method                | Effect                                                      |
| --------------------- | ----------------------------------------------------------- |
| `new()`               | Empty policy; deny all writes, deny network                 |
| `allow_read(path)`    | Grant read access to path and subtree                       |
| `allow_write(path)`   | Grant write (and read) access to path and subtree           |
| `deny_write(path)`    | Narrow-deny writes; overrides a broader `allow_write`       |
| `allow_execute(path)` | Grant execute rights (mapped to `AccessMode::Read` in nono) |
| `with_network(allow)` | `true` = allow network, `false` = deny                      |

#### `SandboxPolicy::from_config`

Derives a `SandboxPolicy` from a `SandboxConfig` and a project root. Paths are
resolved in this order:

1. `filesystem_root` (or `project_root` when `None`) -- read and write.
2. Each `extra_read_paths` entry -- read only.
3. Each `extra_write_paths` entry -- read and write.
4. `.git` subdirectories under `filesystem_root` when `protect_git = true` --
   deny write (walkdir depth-4 scan).
5. `std::env::temp_dir()` -- read and write (always added).
6. Platform system paths required for dynamic linking and binary execution --
   read and execute.

System paths added unconditionally on Linux:

```text
/bin, /usr/bin, /usr/local/bin, /lib, /lib64,
/usr/lib, /usr/local/lib, /etc/ld.so.cache,
/etc/ld.so.conf, /etc/ld.so.conf.d, /proc/self,
/dev/null, /dev/urandom
```

System paths added unconditionally on macOS:

```text
/bin, /usr/bin, /usr/local/bin, /usr/lib,
/System/Library, /Library/Frameworks,
/private/var/db/dyld, /usr/share
```

### `src/sandbox/engine.rs`

Defines
`apply_sandbox(policy: &SandboxPolicy) -> Result<SandboxContext, SandboxError>`.

The function converts the `SandboxPolicy` into a `nono::CapabilitySet`, then
calls `Sandbox::apply_auto` which dispatches to the correct OS backend.

#### Linux path

1. Call `nono::detect_abi()`. If Landlock ABI v1 is not available, return
   `SandboxError::KernelVersionTooOld`.
2. Build a `CapabilitySet` from the policy via `try_allow_any` (skip
   non-existent paths gracefully).
3. On macOS (unreachable on this path), inject SBPL deny rules via
   `CapabilitySet::platform_rule`.
4. Block network via `CapabilitySet::block_network()` when
   `policy.allow_network = false`.
5. Apply via `Sandbox::apply_auto(&caps)`.

#### macOS path

The same `apply_sandbox` function is used. `nono` internally calls
`sandbox_init` (Seatbelt) when running on macOS. The `block_network()` call
generates an SBPL `(deny network*)` rule.

Deny write rules for `deny_write_paths` are injected via
`CapabilitySet::platform_rule` (macOS-only; Linux defers to `SandboxGuard` in
Phase 5).

#### `try_allow_any` helper

Paths that do not exist at call time are silently skipped (trace log). For
existing paths, `allow_path` is used for directories and `allow_file` for
regular files. A clone of the `CapabilitySet` is taken before each call so a
failure to grant one path does not discard previous grants.

---

## nono 0.81 API

Phase 2 targets `nono = "0.81"`. Key API points:

- `CapabilitySet::new()` -- create an empty capability set.
- `.allow_path(path, AccessMode)` -- grant directory access.
- `.allow_file(path, AccessMode)` -- grant file access.
- `.block_network(self) -> Self` -- infallible; returns `Self`.
- `.platform_rule(rule)` -- inject raw SBPL on macOS.
- `Sandbox::apply_auto(&caps)` -- apply to current process (irreversible).
- `nono::detect_abi()` -- check Landlock ABI version on Linux.
- `AccessMode::{Read, Write, ReadWrite}` -- no `Execute` variant in 0.81.

---

## Testing

Eleven unit tests in `src/sandbox/policy.rs`:

| Test                                                   | Verifies                                   |
| ------------------------------------------------------ | ------------------------------------------ |
| `test_policy_new_has_no_paths_and_no_network`          | Empty policy state                         |
| `test_policy_from_config_adds_root_read_and_write`     | Root path in read and write lists          |
| `test_policy_from_config_adds_tmpdir`                  | temp dir always added                      |
| `test_policy_from_config_protect_git_adds_deny_write`  | `.git` dirs in deny list                   |
| `test_policy_from_config_extra_read_paths_added`       | Extra read paths                           |
| `test_policy_from_config_extra_write_paths_added`      | Extra write paths in both lists            |
| `test_policy_from_config_network_enabled_sets_flag`    | `allow_network = true`                     |
| `test_policy_from_config_network_disabled_clears_flag` | `allow_network = false`                    |
| `test_policy_from_config_adds_linux_system_paths`      | Linux system paths (cfg-gated)             |
| `test_policy_from_config_adds_macos_system_paths`      | macOS system paths (cfg-gated)             |
| `test_sandbox_context_applied_is_debug`                | `SandboxContext` implements `Debug`        |
| `test_policy_send_sync`                                | `SandboxPolicy: Send + Sync` compile check |

`apply_sandbox` is not tested in unit tests because calling it would apply an
irreversible OS-level sandbox to the test process. Integration tests gated with
`#[ignore = "requires kernel sandbox"]` are added in Phase 3 and Phase 5.

---

## Dependency

```toml
# Cargo.toml [dependencies]
nono = "0.81"
```

`cargo audit --deny warnings` passes with this dependency.

---

## Files Modified

| File                    | Change                                            |
| ----------------------- | ------------------------------------------------- |
| `Cargo.toml`            | Added `nono = "0.81"` to `[dependencies]`         |
| `src/sandbox/policy.rs` | Full implementation replacing Phase 1 stub        |
| `src/sandbox/engine.rs` | Full implementation replacing Phase 1 stub        |
| `src/sandbox/mod.rs`    | Updated doc comment to reflect Phase 2 completion |
