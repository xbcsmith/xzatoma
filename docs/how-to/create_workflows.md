# Create Workflows

## Overview

This how-to explains how to author and validate execution plans (workflows) for XZatoma.
Workflows are simple, human-friendly plans that describe a sequence of steps the agent
should perform. Plans can be written in YAML, JSON, or Markdown and are parsed by the
built-in PlanParser.

This guide covers:
- the canonical plan structure,
- authoring tips and best practices,
- examples (YAML and Markdown),
- validation and troubleshooting steps.

For the formal specification and parser details, see the workflow format reference:
`../reference/workflow_format.md`.

---

## Plan basics

A plan describes:
- `name` (string): Plan title (required).
- `description` (string): Optional short description.
- `steps` (array): Ordered steps to execute. Each step must include:
 - `name` (string): Human readable step name (required).
 - `action` (string): Brief description of the step's intent (required).
 - `context` (string, optional): Additional data or command snippets (free-form).

Plan file extensions supported:
- `.yaml` / `.yml` (YAML)
- `.json` (JSON)
- `.md` (Markdown; see "Markdown plan authoring" below)

The PlanParser validates the following rules before execution:
- Plan name must be non-empty.
- The plan must have at least one step.
- Each step must have a non-empty `name` and `action`.

---

## Step-by-step: Authoring a YAML plan

1. Create a file with the `.yaml` extension, e.g., `plans/my_workflow.yaml`.
2. Add a top-level `name` and optional `description`.
3. Add one or more `steps`, each with `name` and `action`.
4. Use the optional `context` field to include command snippets, config fragments,
  or small YAML/structured metadata the agent can use.

Example (YAML):

```yaml
# plans/generate_docs.yaml
name: Generate Documentation
description: Scan repository, analyze code, and generate documentation artifacts.

steps:
 - name: Scan repository
  action: Scan the repository to collect file and symbol metadata
  context: |
   # You can include short command snippets or configuration here
   repository: .

 - name: Analyze code
  action: Analyze code and extract documentation hints
  context: |
   analysis:
    depth: 2

 - name: Generate documentation
  action: Generate documentation files based on analysis
  context: |
   output_dir: docs/generated
   categories:
    - tutorials
    - how_to
    - reference
```

Run the plan:

```bash
xzatoma run --plan plans/generate_docs.yaml
```

---

## Markdown plan authoring

Markdown plans are convenient for authoring in editors and for including rich
documentation inside the plan. PlanParser uses these rules when parsing Markdown:

- The first H1 header (`#`) becomes the plan `name`.
- The first paragraph after the H1 becomes the plan `description` (optional).
- Each H2 header (`##`) defines a step. The first non-empty line after the H2
 becomes the step `action`.
- Code fences (```lang ... ```) that appear under a step are captured as the
 step `context` (multi-line string).

Example (Markdown):

```md
# Quick Setup Plan
Initialize docs for a repository with a minimal workflow.

## Create project
Run cargo init to bootstrap a new project.

```bash
cargo init --bin my-project
```

## Verify setup
Ensure the project builds and tests pass.

```bash
cargo build
cargo test
```
```

---

## Best practices

- Keep steps small and focused: prefer many small steps over one gigantic step.
- Use descriptive step names for readability (e.g., "Scan repository", "Generate API docs").
- Use `context` to include additional data or small command snippets. Treat `context` as
 a structured hint, not executable secrets.
- Avoid embedding secrets (API keys, tokens) directly in plan files. Use provider
 configuration and environment variables instead.
- Validate plans frequently: run them locally and check parser errors early.
- Prefer explicit, deterministic actions so the agent can provide reproducible outcomes.

---

## Testing & validation

- Parse/validate a plan by running it:
 - Valid: `xzatoma run --plan path/to/plan.yaml`
 - If the parser rejects a plan, you'll typically see errors like:
  - `Plan name cannot be empty`
  - `Plan must have at least one step`
  - `Step 1 has no name`
  - `Step 'Foo' has no action`

Example of an invalid plan and expected error:

```yaml
# plans/invalid.yaml
name: ""
steps: []
```

Running the invalid plan:

```bash
xzatoma run --plan plans/invalid.yaml
# Expected feedback: Plan name cannot be empty
# or: Plan must have at least one step
```

Unit tests (developer guidance):
- Test plan parsing using the PlanParser helpers (see `src/tools/plan.rs`).
- Add tests covering YAML, JSON, and Markdown cases and edge conditions (empty name, missing actions).

---

## Advanced notes

- The current Plan model is intentionally simple (`name`, `description`, `steps.name`, `steps.action`, `steps.context`).
 If you need richer semantics (explicit dependency graphs, deliverables metadata), include structured data
 in `context` (e.g., small YAML/JSON snippets) and document the expectations for your workflow runner or tooling.
- Future versions may add explicit `id`, `dependencies`, or structured `params`. Check
 `../reference/workflow_format.md` for updates.

---

## Examples & troubleshooting

- Quick validation (parse and run a plan):
 ```bash
 xzatoma run --plan examples/quickstart_plan.yaml
 ```

- Want to author incrementally? Use simple steps first (e.g., "List files") and
 progressively add analysis/generation actions as you confirm behavior.

- If a step doesn't behave as you expect, inspect the step `context` and consider
 whether it contains the correct information for the agent/tooling to act upon.

---

## See also

- Workflow format reference: `../reference/workflow_format.md`
- Quickstart tutorial (example plan + run): `../tutorials/quickstart.md`
- How to generate documentation using plans: `../how-to/generate_documentation.md`
- CLI reference (for `run` and other subcommands): `../reference/cli.md`

---

Last updated: 2026-01-24
Maintained by: XZatoma Development Team
