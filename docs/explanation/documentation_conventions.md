# Documentation Conventions

## Overview

This document captures the repository-wide documentation conventions used by the XZatoma project.
It is a contributor-facing reference describing how to name, organize, and author markdown files so
they are discoverable, consistent, and compatible with automated checks.

This document complements `AGENTS.md` (the authoritative guidelines for automated checks and developer workflows) and the top-level `docs/README.md` (the user-facing documentation index).

## Components

- `docs/explanation/documentation_conventions.md` — This file (contributor-facing conventions).
- Reference pointers to helper validation scripts (if present): `scripts/doc_link_check.py`, `scripts/emoji_check.py`.
- Guidance for PR authors and reviewers (see "PR checklist" below).

## Implementation Details

### 1) File Naming & Extensions

- Filenames:
  - Always use `lowercase_with_underscores.md`.
  - Example good: `docs/how-to/create_workflows.md`
  - Examples bad: `CreateWorkflows.md`, `distributed-tracing.md`, `Docs.MD`

```bash
# Good
touch docs/explanation/documentation_conventions.md
touch docs/tutorials/quickstart.md

# Bad
touch docs/Explanation/DocumentationConventions.md
touch docs/how-to/Distributed-Tracing.md
```

- Markdown files must use the `.md` extension.
- YAML files must use the `.yaml` extension (NOT `.yml`).
  - Example to rename `.yml` → `.yaml`:

```bash
find . -name "*.yml" -exec sh -c 'mv "$0" "${0%.yml}.yaml"' {} \;
```

- Exception: `README.md` is the only allowed uppercase filename.

### 2) Directory Layout & Diataxis Mapping

Place new content according to the Diataxis intent:

- `docs/tutorials/` — Tutorials: learning-by-doing, step-by-step.
- `docs/how-to/` — How-to guides: task-oriented recipes.
- `docs/explanation/` — Explanations: conceptual/architectural discussions.
- `docs/reference/` — Reference material: schemas, CLI reference, API reference.

Decision rule: If it is step-by-step and learning oriented → `tutorials/`. If it's a focused task recipe → `how-to/`. If the content explains rationale or architecture → `explanation/`. If it documents interfaces/config → `reference/`.

### 3) Content Style & Format Requirements

- No emojis anywhere in docs or code comments.
  - To find emoji usage (example):

```bash
# May catch common emoji ranges
grep -R --line-number -P "[\x{1F600}-\x{1F64F}]" docs/ || true
```

- Fenced code blocks MUST include a language (or path) tag (e.g., `rust, `bash, ```json).
  - Good:

````markdown
```rust
fn main() { println!("hello"); }
```
````

````
  - Bad:
```markdown
````

fn main() { println!("hello"); }

```

```

- Prefer relative links for internal docs (e.g., `../reference/cli.md`) and ensure links are updated after moves/renames.

- Small, focused files are preferred. Avoid overly long developer logs in top-level docs — archive those under `docs/archive/implementation_summaries/`.

- For Rust code, public functions, types, and modules MUST have `///` doc comments including runnable examples when appropriate (these are tested by `cargo test`):

````rust
/// Computes factorial of `n`.
///
/// # Examples
///
/// ```rust
/// let f = xzatoma::math::factorial(5).unwrap();
/// assert_eq!(f, 120);
/// ```
pub fn factorial(n: u64) -> Result<u64, MathError> { /* ... */ }
````

### 4) Move / Reclassification Policy

- Prefer move-only PRs when reorganizing documentation (no content edits in that PR).
- Update all links when moving files.
- Archive developer-focused content to `docs/archive/implementation_summaries/` rather than deleting without signoff.
- Record move rationale in `docs/explanation/documentation_cleanup_summary.md` (or the PR description) so reviewers can validate the intent.

## Testing / Validation

Before submitting a docs PR:

1. Run doc-specific validation (if helper scripts exist):

   - `python3 scripts/doc_link_check.py` — validate internal links (or run a link checker you prefer).
   - `python3 scripts/emoji_check.py [--strict]` — scan for emoji characters. By default the checker uses a relaxed detection that avoids flagging box-drawing characters and simple dingbats (e.g., check marks); pass `--strict` to enable the legacy broader matching that will flag additional Unicode symbols.
   - `python3 scripts/code_fence_check.py` — ensure all fenced code blocks include a language/path tag.
   - `python3 scripts/docs_filename_check.py` — validate doc filenames and YAML extensions.
   - Alternatively, run the Makefile helper: `make docs-check` (recommended; runs the above checks).

2. Verify code fence language usage and fix missing language tags.

3. Run repository quality gates (these are mandatory for any change that touches source or docs per AGENTS.md):

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

4. Run a final manual pass:

   - Read the doc as a new user and confirm steps/examples are reproducible.
   - Confirm the doc is discoverable via `docs/README.md` (update the index if you add or move top-level pages).

5. CI & PR validation:
   - A GitHub Actions workflow at `.github/workflows/docs_ci.yaml` runs these docs checks automatically on docs-only PRs (and when `scripts/` is changed).
   - Ensure the CI job passes on your PR and address any failures before requesting review.

## PR Checklist (copy into your PR description)

- [ ] Filename follows `lowercase_with_underscores.md`
- [ ] Uses `.md` for markdown, `.yaml` for YAML files
- [ ] No emojis in the content (run emoji scan)
- [ ] All code fences include a language tag
- [ ] Internal links validated and updated (run link checker)
- [ ] Documentation placed in the correct Diataxis category
- [ ] `docs/README.md` updated to surface top-level changes
- [ ] Followed "move-only PR" pattern for reorganizations
- [ ] Ran repository quality gates (format, check, clippy, tests)

## Examples

Example template for a documentation page (recommended sections):

````markdown
# Feature Name Implementation

## Overview

Short description of purpose.

## Components Delivered

- `file.md` — One-line description

## Implementation Details

Technical explanation, design notes, and any important caveats.

## Testing

Commands you used to validate this (link checks, example runs).

## Usage Examples

```rust
// Complete example that can be copy/pasted by users
```
````

## References

- Link to related docs or issues

```

## References

- Project guidelines: `AGENTS.md`
- Diataxis Framework: https://diataxis.fr/
- Top-level docs index: `docs/README.md`
- Archive location for developer logs: `docs/archive/implementation_summaries/`

---

Last updated: 2026-01-24
Maintained by: XZatoma Documentation Team
```
