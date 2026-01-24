# Workflow Format

## Overview

This reference documents the workflow (plan) file format supported by XZatoma, and the parsing and validation rules used by the built-in `PlanParser`. Plans are a simple, human-friendly description of sequential steps the agent should reason about and (where appropriate) execute. Supported file formats are YAML, JSON, and a lightweight Markdown representation.

Key properties:
- Plan files are intentionally simple: a `name`, optional `description`, and a list of `steps`.
- Each step contains a `name` and an `action`. An optional `context` field holds additional information (free-form text, YAML, or JSON).
- Supported file extensions: `.yaml`, `.yml`, `.json`, `.md` (Markdown).
- The parser validates structure and basic invariants before plan execution.

---

## Plan data model

Canonical fields (models in `src/tools/plan.rs`):

- Plan
 - `name: String` (required) — Plan title.
 - `description: Option<String>` (optional) — Short description of the plan.
 - `steps: Vec<PlanStep>` (required, non-empty) — Ordered list of steps.

- PlanStep
 - `name: String` (required) — Human-readable step name.
 - `action: String` (required) — Short description of the action to perform.
 - `context: Option<String>` (optional) — Additional information or small configuration block; often used to pass parameters to agent/tooling.

Notes:
- `context` is treated as an opaque multiline string by the PlanParser. If you need structured parameters, encode them as YAML or JSON inside the `context` block.
- The implementation is intentionally minimal so authors can extend behavior by agreement with tooling or the agent (e.g., structured `context` payloads).

---

## Supported file formats

The parser supports three plan file formats:

1. YAML (`.yaml`, `.yml`) — standard, recommended for most use cases.
2. JSON (`.json`) — if you prefer JSON over YAML.
3. Markdown (`.md`) — concise authoring that uses headings for plan and steps.

The parser selects the loader by file extension:

- `yaml` or `yml` → `PlanParser::from_yaml`
- `json` → `PlanParser::from_json`
- `md` → `PlanParser::from_markdown`

If a file has no recognized extension, `PlanParser::from_file` returns an error.

---

## YAML example

```yaml
name: Generate Documentation
description: Analyze the repository and generate reference and tutorial docs.

steps:
 - name: Scan repository
  action: Collect file metadata for the repository
  context: |
   repository: .
   depth: 2

 - name: Analyze code
  action: Extract API surface and doc comments

 - name: Generate docs
  action: Generate documentation files
  context: |
   output_dir: docs/generated
   categories:
    - reference
    - tutorials
```

---

## JSON example

```json
{
 "name": "Generate Documentation",
 "description": "Analyze the repository and generate reference and tutorial docs.",
 "steps": [
  {
   "name": "Scan repository",
   "action": "Collect file metadata for the repository",
   "context": "repository: .\ndepth: 2\n"
  },
  {
   "name": "Analyze code",
   "action": "Extract API surface and doc comments"
  },
  {
   "name": "Generate docs",
   "action": "Generate documentation files",
   "context": "output_dir: docs/generated\ncategories:\n - reference\n - tutorials\n"
  }
 ]
}
```

---

## Markdown plans (parsable rules)

Markdown is a convenient authoring format. The Markdown parser uses straightforward rules:

- The first H1 (`# Title`) becomes the plan `name`.
- The first paragraph after the H1 becomes the plan `description` (optional).
- Each H2 (`## Step Name`) defines a step. The first non-empty line under an H2 becomes the step `action`.
- A code fence (triple-backticks) under a step becomes the step `context`. The parser preserves code fence contents verbatim, including language tags if present.

Example:

```markdown
# Quick Setup
A short description of the plan.

## Initialize
Run `cargo init` to create a new project.

```bash
cargo init --bin my-project
```

## Verify
Ensure the project compiles and tests pass.

```bash
cargo build
cargo test
```
```

Implementation notes:
- Code fence toggles are recognized using the standard triple-backtick convention; the parser collects code block contents and stores them as step `context`.
- The Markdown parser is forgiving of whitespace and blank lines between sections.

---

## Validation rules

`PlanParser::validate(&plan)` enforces basic invariants:

- Plan `name` must be non-empty (error: "Plan name cannot be empty").
- Plan must have at least one step (error: "Plan must have at least one step").
- Each step must have a non-empty `name` (error: "Step N has no name").
- Each step must have a non-empty `action` (error: "Step '<name>' has no action").

Validation errors are returned with descriptive messages to help authors fix issues before execution.

---

## Programmatic usage (Rust)

The parser exposes a small, synchronous API you can call from Rust:

```rust
use std::path::Path;
use xzatoma::tools::plan::PlanParser;

let plan = PlanParser::from_file(Path::new("plans/generate_docs.yaml"))?;
PlanParser::validate(&plan)?;
println!("Parsed plan: {}", plan.name);

let yaml_plan = r#"
name: Quick Plan
steps:
 - name: Step 1
  action: echo hi
"#;
let plan2 = PlanParser::from_yaml(yaml_plan)?;
```

Convenience helpers:
- `PlanParser::from_file(path: &Path)` — choose parser by extension.
- `PlanParser::from_yaml(content: &str)` / `from_json(content: &str)` / `from_markdown(content: &str)` — parse directly from text.
- `parse_plan(yaml: &str)` — convenience wrapper to parse YAML text.

---

## Testing recommendations

When adding tests for plans or plan-driven features:

- Unit tests: validate parsing and validation logic for YAML, JSON, and Markdown variants:
 - Valid plan parses and validates successfully.
 - Invalid plans trigger the expected validation error.
- Integration tests: prepare temporary plan files and run the `run` command handler (or `run_plan_with_options`) under test harnesses:
 - Use temporary directories to confirm steps that write files produce expected output.
- Use the existing PlanParser tests as templates in `src/tools/plan.rs` (the repository already contains unit tests for parsing YAML, JSON, and Markdown).

Example unit test sketch:

```rust
#[test]
fn test_from_yaml_valid() {
  let yaml = r#"
name: Test Plan
steps:
 - name: s1
  action: do something
"#;
  let plan = PlanParser::from_yaml(yaml).unwrap();
  assert_eq!(plan.name, "Test Plan");
}
```

---

## Best practices & authoring guidance

- Keep steps focused and small: short, descriptive `action` text helps the agent reason about intended behavior.
- Use `context` for structured parameters (YAML/JSON snippet inside `context`) rather than encoding secrets or large blobs.
- Prefer YAML for readability, Markdown for human-editable plans with narrative content.
- Validate plans locally before use: `xzatoma run --plan <file>` will parse and validate the plan before execution.

---

## Implementation notes and future directions

- Current `Plan` model purposefully minimal: `name`, `description`, `steps` (each step has `name`, `action`, `context`).
- If you need richer workflow semantics (IDs, step dependencies, parallelization, explicit `params`), prefer putting structured metadata in the `context` field for now, and coordinate with tooling that consumes the plan.
- Future improvements might add explicit step `id`, `dependencies`, or more structured plan schemas. When such extensions are added, the reference will be updated and converters for backward compatibility may be provided.

For details of the current implementation, see `src/tools/plan.rs`.

---

## References

- Quickstart: `../tutorials/quickstart.md`
- How-To: Create workflows: `../how-to/create_workflows.md`
- Implementation: `src/tools/plan.rs` (Plan and PlanParser implementation)

---
Last updated: 2026-01-24
