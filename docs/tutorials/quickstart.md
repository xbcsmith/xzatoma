# Quickstart Tutorial

## Overview

This quickstart walks you through installing (or running from source) and executing a minimal plan with XZatoma. The goal is to get a simple, repeatable example running so you understand the basic workflow: build → authenticate (optional) → run a plan.

This tutorial uses the example plan at `examples/quickstart_plan.yaml` that demonstrates plan parsing and a few simple steps that are safe to run locally.

## Prerequisites

- Rust toolchain (rustup + cargo)
- Git
- Network access (required for cloud provider usage, e.g., GitHub Copilot)
- Optional: Provider credentials (for Copilot, Ollama, or other providers)

If you only want to run the sample plan locally (no provider calls), you can proceed without provider credentials. For provider-driven behavior (e.g., model completions, tool calls), follow the provider configuration steps in `../how-to/configure_providers.md`.

## Step 1 — Get the code

```bash
git clone https://github.com/xbcsmith/xzatoma.git
cd xzatoma
```

## Step 2 — Build or run from source

Option A — Run directly from source (useful during development):

```bash
# Run the example plan using the local cargo-built binary
cargo run -- run --plan examples/quickstart_plan.yaml
```

Note: the `--` after `cargo run` stops cargo from interpreting the subsequent arguments and forwards them to the `xzatoma` binary.

Option B — Install the binary (makes it available on your PATH):

```bash
cargo install --path .
# then run:
xzatoma run --plan examples/quickstart_plan.yaml
```

## Step 3 — Authenticate providers (optional)

If your plan or workflow requires a provider (Copilot, Ollama), authenticate before running:

```bash
# Copilot (OAuth device flow - you will be asked to visit a URL and enter a code)
xzatoma auth --provider copilot

# Ollama (usually a local host; ensure config is set)
xzatoma auth --provider ollama
```

Alternatively, set the provider via environment variables (overrides config file for a single run):

```bash
# Use Ollama just for this run
XZATOMA_PROVIDER=ollama xzatoma run --plan examples/quickstart_plan.yaml
```

## Step 4 — Run a one-off prompt (alternative)

Instead of a plan file you can provide a direct prompt:

```bash
cargo run -- run --prompt "Analyze this repository and produce a short documentation outline"
```

## Example: Quickstart plan

The repository includes `examples/quickstart_plan.yaml`. Below is an illustrative example of a minimal YAML plan:

```yaml
name: Quickstart Tutorial
description: Minimal example plan used by the quickstart tutorial.

steps:
 - name: Print greeting
  action: Print a greeting to verify execution
  context: echo "Hello from XZatoma"

 - name: List top-level files
  action: Inspect repository contents
  context: |
   # Unix
   ls -la

 - name: Generate documentation outline
  action: Generate a short documentation outline and save it to docs/generated/outline.md
  context: |
   repository: .
   output_file: docs/generated/outline.md
   sections:
    - Overview
    - Installation
    - Usage
    - API
```

Notes:
- Plan files support YAML, JSON, and Markdown formats.
- Each step requires `name` and `action`. `context` is optional and is commonly used for supporting data or command snippets.

For the canonical example file, see: `examples/quickstart_plan.yaml`.

## What to expect

When you run a plan, XZatoma composes a task representation from the plan, sends it to the configured provider (if any), and prints the agent's response. You will typically see:

```
Executing task...

Result:
<agent-provided summary or step results>
```

If the agent executes actions that write files (for example, a `generate_documentation` step), check the specified output locations (e.g., `docs/generated/outline.md`) to see generated artifacts.

## Troubleshooting

- Plan parser errors
 - Error: "Plan must have at least one step" → Ensure `steps:` exists and contains at least one item.
 - Error: "Step has no name" or "Step has no action" → Make sure every step defines `name` and `action`.

- Authentication errors
 - Copilot errors usually indicate a device flow issue or expired token. Re-run `xzatoma auth --provider copilot`.
 - Ollama: ensure the host is reachable and `provider.ollama` config is set (see `../how-to/configure_providers.md`).

- Want to run with a different provider temporarily?
 - Use `XZATOMA_PROVIDER=<provider>` environment variable for a single invocation:
  ```bash
  XZATOMA_PROVIDER=ollama xzatoma run --plan examples/quickstart_plan.yaml
  ```

- For verbose output, use the top-level `--verbose` flag:
 ```bash
 cargo run -- --verbose run --plan examples/quickstart_plan.yaml
 ```

- Dangerous operations
 - The `run` subcommand supports `--allow-dangerous` to enable escalated execution (FullAutonomous). Use with caution and only when you understand the implications.

## Validation / Testing

- Parse/validate the plan using the built-in plan parser (happens automatically when you run a plan).
- Run unit and integration tests:
 ```bash
 cargo test -- --nocapture
 ```
- Format and lint before contributing:
 ```bash
 cargo fmt --all
 cargo clippy --all-targets --all-features -- -D warnings
 ```

## Next steps

- Configure providers (secrets, models): `../how-to/configure_providers.md`
- Learn how to author richer plans and workflows: `../how-to/create_workflows.md`
- Generate documentation from repository analysis: `../how-to/generate_documentation.md`
- Read the CLI reference for available flags and subcommands: `../reference/cli.md`
- Understand plan structure and supported formats: `../reference/workflow_format.md`

## Getting help

- Issues: https://github.com/xbcsmith/xzatoma/issues
- Discussions: https://github.com/xbcsmith/xzatoma/discussions

---

Last updated: 2026-01-24
