# Phase 1 Security Refactor Implementation

## Overview

This phase hardens terminal tool execution by removing shell invocation and adding safe parsing, and strengthens SSRF validation by resolving hostnames to IPs before allowing outbound fetches. The goal is to reduce command injection surfaces and DNS rebinding risks while maintaining existing safety modes and tool behavior.

## Components Delivered

- `src/tools/terminal.rs` (updated) - Added structured command parsing, disallowed shell operators, and executed commands without shell wrappers.
- `src/tools/fetch.rs` (updated) - Added hostname resolution for SSRF checks and updated tests to avoid DNS dependency.
- `docs/explanation/phase1_security_refactor_implementation.md` (this file) - Implementation summary and usage examples.

## Implementation Details

### Terminal Command Parsing And Execution

The terminal tool now parses commands into a program and arguments using a simple, quote-aware parser. It rejects shell operators (`|`, `;`, `<`, `>`, `&`) and command substitution (`$(`, backticks) to prevent shell expansion. The execution path now invokes the program directly via `tokio::process::Command` rather than `sh -c` or `cmd /C`.

```rust
use xzatoma::tools::terminal::parse_command_line;

let parsed = parse_command_line("echo \"hello world\"").unwrap();
assert_eq!(parsed.program, "echo");
assert_eq!(parsed.args, vec!["hello world".to_string()]);
```

### SSRF Hostname Resolution

The SSRF validator now resolves hostnames to IP addresses and validates each resolved IP against private and link-local ranges. This closes the DNS rebinding gap by enforcing IP-based checks even for hostname inputs.

```rust
use xzatoma::tools::fetch::SsrfValidator;

let validator = SsrfValidator::new();
assert!(validator.validate("https://93.184.216.34").is_ok());
```

## Testing

- Unit tests were updated to reflect the new terminal parsing rules and SSRF validation behavior.
- Manual test execution was not run in this environment.

## Usage Examples

```rust
use xzatoma::tools::terminal::parse_command_line;
use xzatoma::tools::fetch::SsrfValidator;

let parsed = parse_command_line("echo hello").unwrap();
assert_eq!(parsed.program, "echo");

let validator = SsrfValidator::new();
assert!(validator.validate("http://1.1.1.1").is_ok());
```

## References

- `docs/explanation/security_refactor_plan.md`
- `docs/reference/architecture.md`
