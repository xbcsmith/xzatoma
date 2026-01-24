# API Reference

## Overview

This document describes how to consume XZatoma as a library, how to generate and view the Rust API documentation locally, and summarizes the primary public modules and examples that are useful when embedding the crate into other tooling or tests.

The repository provides both a binary (`xzatoma`) and a library crate that exposes building blocks (CLI parser, configuration loader, providers, plan parsing, and command handlers) suitable for programmatic usage.

## Generating library documentation

To generate the crate API docs locally, use `cargo doc`. Common useful invocations:

- Generate and open the docs for the current crate (without dependencies):

```bash
# Generate docs and open in the browser (recommended during development)
cargo doc --no-deps --open
```

- Generate docs including dependencies (slower):

```bash
cargo doc --open
```

- Generate docs including private items (useful for internal reviews):

```bash
RUSTDOCFLAGS="--document-private-items" cargo doc --no-deps --open
```

Notes:
- Doc examples embedded in `///` doc comments are executed as part of `cargo test` (they are doc tests). You can run `cargo test` to validate doc examples.
- When updating public API or doc comments, run `cargo test` and `cargo doc --no-deps --open` to validate both examples and generated documentation.

## Using XZatoma as a dependency

If you want to use XZatoma programmatically in another Rust project, add it as a dependency. During local development, using a path dependency is convenient:

```toml
# In your Cargo.toml
[dependencies]
xzatoma = { path = "../xzatoma" } # adjust the path as needed
```

Alternatively, if the crate is published, use a standard versioned dependency:

```toml
[dependencies]
xzatoma = "0.1"
```

After adding the dependency, you can import and use library types and helpers directly from your code.

## Primary modules & public surface (summary)

This is a concise guide to the main modules and important public types you will likely interact with. For full details and function signatures, refer to the generated `cargo doc` output.

- `xzatoma::cli`
 - `Cli` — Clap-based CLI argument structure.
 - `Cli::parse_args()` — Parse command-line arguments programmatically.

- `xzatoma::config`
 - `Config` — The central configuration type.
 - `Config::load(path, &Cli)` — Load configuration from file with environment and CLI overrides.
 - `Config::validate()` — Validate the configuration.

- `xzatoma::tools::plan`
 - `Plan` / `PlanStep` — The plan data model.
 - `PlanParser` — Parse plans from YAML, JSON, and Markdown.
 - `PlanParser::from_file(path)` / `from_yaml` / `from_json` / `from_markdown` — Parsing helpers.
 - `PlanParser::validate(&plan)` — Structural validation.

- `xzatoma::providers`
 - `Provider` (trait) — Abstraction for AI providers (methods for completion and metadata).
 - Implementations:
  - `CopilotProvider` — GitHub Copilot integration (includes device-flow auth).
  - `OllamaProvider` — Ollama local/remote server support.
 - Provider helpers include model listing, info, and authentication flows.

- `xzatoma::commands`
 - `run::run_plan(...)` — Programmatic entry for executing a plan or prompt.
 - `auth::authenticate(...)` — Programmatic entry for provider authentication.
 - These wrappers mirror the CLI behavior and are useful for integration tests or embedding.

- `xzatoma::agent`
 - `Agent` — The high-level agent orchestration (creates providers and tools, coordinates execution).

- `xzatoma::tools` (various)
 - `terminal::TerminalTool` — Executes shell commands with validation.
 - `file_ops::FileOpsTool` — File read/write helpers used by the agent.
 - Tool executors implement a `ToolExecutor` trait to allow the agent to execute them.

- `xzatoma::error`
 - Central error types and conversions (e.g., `XzatomaError`).

Note: See `docs/reference/quick_reference.md` for overview examples and `cargo doc` for detailed API docs.

## Example: Programmatic plan execution

Below is a minimal example that demonstrates loading configuration, and executing a plan via the library API. This mirrors what the CLI does when you run `xzatoma run`.

```rust
use xzatoma::cli::Cli;
use xzatoma::config::Config;
use xzatoma::commands::r#run;

// This example is intentionally concise. In production code handle errors accordingly.
#[tokio::main]
async fn main() -> xzatoma::error::Result<()> {
  // Parse CLI args if desired, or construct a `Cli` instance manually
  let cli = Cli::parse_args();

  // Load configuration (path is typically provided by `--config` or default)
  let config = Config::load("config/config.yaml", &cli)?;

  // Execute a plan file (or pass a prompt via the third argument)
  r#run::run_plan(config, Some("plans/generate_docs.yaml".to_string()), None).await?;

  Ok(())
}
```

## Example: Calling a provider directly

If you need to call providers programmatically (for example, to build tests or custom flows), here's a small example using the Copilot provider API:

```rust
use xzatoma::config::CopilotConfig;
use xzatoma::providers::{CopilotProvider, Provider, Message};

#[tokio::main]
async fn example() -> xzatoma::error::Result<()> {
  let config = CopilotConfig {
    model: "gpt-5-mini".to_string(),
    ..Default::default()
  };

  let provider = CopilotProvider::new(config)?;
  let messages = vec![Message::user("Hello!")];

  let completion = provider.complete(&messages, &[]).await?;
  let message = completion.message;
  println!("Provider responded: {}", message.content);

  Ok(())
}
```

Refer to the provider module docs (`cargo doc`) for precise types and return structures.

## Writing & testing API docs and examples

- Doc comments (`///`) placed on public functions, enums, structs are automatically included in the generated docs.
- Inline Rust code examples in doc comments are executed as doc tests by `cargo test`. Use annotations as needed:
 - `/// ```rust,no_run` to avoid executing long-running examples.
 - `/// ```rust,ignore` if the example cannot be executed in CI.
- Run `cargo test` to validate doc examples and unit tests.
- Keep examples small, deterministic, and with clear `use` statements to increase likelihood doc tests pass.

Example doc comment style:

```rust
/// Calculates the factorial of a number.
///
/// # Examples
///
/// ```
/// use xzatoma::math::factorial;
/// assert_eq!(factorial(5).unwrap(), 120);
/// ```
pub fn factorial(n: u64) -> Result<u64, MathError> { /* ... */ }
```

## Publishing docs (brief)

If you publish the crate, `docs.rs` will generate and host API documentation automatically for released crate versions. For project-hosted documentation (GitHub Pages) you can publish the output of `cargo doc` (for example using CI to push `target/doc` to a `gh-pages` branch). Typical steps:

1. Generate docs locally: `cargo doc --no-deps`.
2. Use a GitHub Action to run `cargo doc` and deploy `target/doc` to GitHub Pages, or use `ghp-import` locally to push the generated docs.

## Troubleshooting & tips

- If doc examples fail during `cargo test`, prefer adding `no_run` or `ignore` to the example while you adjust the sample to be deterministic for CI.
- Use `cargo doc --no-deps` when you only care about your crate's documentation (faster).
- Keep `///` comments and public API stable to avoid churn in published doc pages.
- When adding a new public module, include at least one example usage in the module-level docs so consumers have a starting point.

## See also

- Tutorials and how-to docs:
 - `../tutorials/quickstart.md`
 - `../how-to/create_workflows.md`
 - `../how-to/configure_providers.md`
- CLI reference: `../reference/cli.md`
- Workflow format: `../reference/workflow_format.md`
- Plan parsing & examples: `src/tools/plan.rs` (implementation)

---

Last updated: 2026-01-24
