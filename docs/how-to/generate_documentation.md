# Generate Documentation

## Overview

This how-to describes how to generate documentation for a repository using XZatoma's plan-based workflow model. The recommended approach today is to author a small plan that includes a `generate_documentation` step and then run it with the `run` subcommand. This guide shows example plans, a suggested `context` schema, run commands, and troubleshooting tips.

## When to use

- You want to generate docs for a repo programmatically using the agent.
- You want to integrate documentation generation into a repeatable workflow (CI, scheduled runs, local development).
- You want to iterate on documentation generation by changing plan content and re-running the agent.

## Prerequisites

- Rust toolchain (rustup + cargo) if running from source
- `xzatoma` binary available on PATH (via `cargo install --path .` or a packaged binary)
- Optional: configured AI provider (Copilot, Ollama) and valid authentication if you plan to use provider-backed plan interpretation or content generation. Use `xzatoma auth --provider copilot` to authenticate Copilot.

---

## Quick start

1. Create a plan file that contains a `generate_documentation` step (YAML or Markdown).
2. Run the plan:

```bash
# Run a plan file
xzatoma run --plan plans/generate_docs.yaml
```

3. Inspect the produced artifacts in the configured output directory (e.g., `docs/generated`).

---

## Example plans

### YAML example

Save as `plans/generate_docs.yaml`:

```yaml
name: Generate Documentation
description: Scan the repository, analyze the codebase, and generate documentation artifacts.

steps:
 - name: Scan repository
  action: Scan repository to collect files and symbols
  context: |
   repository: .
   depth: 2

 - name: Analyze code
  action: Analyze code and extract doc hints
  context: |
   analysis:
    include_tests: false

 - name: Generate documentation
  action: Generate documentation
  context: |
   repository: .
   output_dir: docs/generated
   categories:
    - tutorials
    - how_to
    - reference
   deliverables:
    - docs/reference/api.md
    - docs/tutorials/quickstart.md
```

### Markdown example

A compact Markdown plan (saved as `plans/generate_docs.md`) is convenient for authoring:

```markdown
# Generate Documentation
Scan the repository and produce documentation artifacts.

## Scan repository
Collect file and symbol metadata

## Analyze code
Extract documentation hints and API surface

## Generate documentation
Write docs to docs/generated/

```yaml
repository: .
output_dir: docs/generated
categories: [reference, how_to]
```
```

Notes:
- Markdown plan parsing rules: H1 → plan name; H2 → steps; first non-empty line under a step is the `action`; fenced code blocks under a step become `context`.
- The `context` field is free-form text that the agent or tooling can interpret — YAML or JSON are common choices.

---

## Suggested `context` schema for `generate_documentation`

The agent/router that performs `generate_documentation` should expect a small set of conventional parameters encoded in the `context` field. Use YAML or JSON inside the `context` block for clarity:

- `repository` (string) — path or URL of the repository (default: `.`)
- `output_dir` (string) — where to write generated docs (relative to repo root)
- `categories` (list[string]) — which Diataxis categories to generate (e.g., `tutorials`, `how_to`, `reference`)
- `deliverables` (list[string]) — files to produce (optional)
- `include_tests` (bool) — whether to parse/test files in tests/ (optional)
- `ignore_patterns` (list[string]) — globs to exclude (optional)

Example context snippet:

```yaml
context: |
 repository: .
 output_dir: docs/generated
 categories:
  - reference
  - tutorials
 deliverables:
  - docs/reference/api.md
```

Implementation note: PlanParser treats `context` as an opaque string; if you need structured parameters, encode them as YAML/JSON text in `context` and document the expected schema for your consumers.

---

## Running and validating

- Execute a plan (the PlanParser will validate structure before execution):

```bash
xzatoma run --plan plans/generate_docs.yaml
```

- Run a direct prompt (useful for quick ad-hoc generation):

```bash
xzatoma run --prompt "Analyze the repository at . and generate a short reference and tutorial outline. Write files to docs/generated/"
```

- For debugging, enable verbose logging:

```bash
RUST_LOG=debug xzatoma run --plan plans/generate_docs.yaml
```

- If a plan fails validation you'll see clear errors produced by the PlanParser, e.g.:
 - "Plan name cannot be empty"
 - "Plan must have at least one step"
 - "Step 'X' has no action"

---

## Programmatic usage (Rust example)

You can invoke plan execution programmatically from a Rust binary if you want fine-grained control or integration:

```rust
use xzatoma::config::Config;
use xzatoma::cli::Cli;
use xzatoma::commands::r#run;

#[tokio::main]
async fn main() -> xzatoma::error::Result<()> {
  // Load config from a file (path) and CLI overrides (if any)
  let cli = Cli::parse_args();
  let cfg = Config::load("config/config.yaml", &cli)?;

  // Execute a plan file with the same behavior as the CLI
  r#run::run_plan(cfg, Some("plans/generate_docs.yaml".to_string()), None).await?;

  Ok(())
}
```

Note: handle errors and configuration according to your environment and use cases.

---

## Testing & verification

- Unit tests:
 - Add tests for parsing and interpreting `context` payloads if you implement a `docgen` module.
 - Test plan parsing for YAML, JSON, and Markdown cases (edge cases: empty name, empty steps).
- Integration tests:
 - Use temporary directories and assertions to validate files were created in `output_dir`.
 - Mock provider completions if your generation relies on external models.
- Local validation:
 - Run `xzatoma run --plan plans/generate_docs.yaml` and inspect `docs/generated/` for expected artifacts.

---

## Troubleshooting

- No artifacts created:
 - Check the `output_dir` in `context`.
 - Re-run with `RUST_LOG=debug` and inspect logs.
- Provider authentication errors:
 - Re-run `xzatoma auth --provider copilot` or verify Ollama host settings in `config/config.yaml`.
- Plan validation errors:
 - Ensure `name` is non-empty and each step has `name` and `action`.
- Step didn't run as expected:
 - Verify the `context` contains the right structured data the `docgen` expects (YAML/JSON).
 - Simplify the step (smaller scope) and iterate.

---

## Next steps & references

- Quickstart: `../tutorials/quickstart.md` — walk-through with an example plan
- Authoring plans: `../how-to/create_workflows.md`
- Workflow format: `../reference/workflow_format.md`
- CLI reference: `../reference/cli.md`
- Provider configuration & auth: `configure_providers.md`

---

Last updated: 2026-01-24
