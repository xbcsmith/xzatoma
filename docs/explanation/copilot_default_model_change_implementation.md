xzatoma/docs/explanation/copilot_default_model_change_implementation.md
# Change: Copilot default model → gpt-5-mini

## Overview
This change updates the GitHub Copilot default model from `gpt-4o` to `gpt-5-mini`. The default is used when the user or configuration does **not** explicitly specify a Copilot model. Explicitly-configured models are unaffected.

Why:
- `gpt-5-mini` is the new recommended default for typical interactive and CLI-centered workloads (better latency/cost profile).
- Keep explicit model selection unchanged so existing user configurations remain stable.

## Components delivered
- Code: change default provider behavior so `CopilotConfig::default()` returns `gpt-5-mini`.
  - `xzatoma/src/config.rs` — default value change
  - `xzatoma/src/providers/copilot.rs` — doc examples and added unit test
  - `xzatoma/src/test_utils.rs` — test fixture updated
  - `config/config.yaml` — example/default config updated
- Docs: update references that stated the old default (explanations + reference docs)
  - `docs/explanation/*` and `docs/reference/*` — relevant occurrences updated
- Tests: new unit test asserting the new default

## Implementation details
- Source of truth for the default model is `CopilotConfig::default()` (implemented via `default_copilot_model()`).
- Change is non-breaking for users who explicitly set `copilot.model` or `XZATOMA_COPILOT_MODEL`.
- Added a focused unit test to prevent regressions.

Critical code changes (high-level):

- default provider model
```xzatoma/src/config.rs#L36-44
fn default_copilot_model() -> String {
    "gpt-5-mini".to_string()
}
```

- updated example/default config
```xzatoma/config/config.yaml#L9-15
copilot:
  # Model to use (e.g., gpt-5-mini, gpt-4-turbo)
  model: gpt-5-mini
```

- new unit test (verifies default)
```xzatoma/src/providers/copilot.rs#L520-528
#[test]
fn test_copilot_config_default_model() {
    let cfg = CopilotConfig::default();
    assert_eq!(cfg.model, "gpt-5-mini");
}
```

- updated public documentation examples and doc-comments (examples now show `gpt-5-mini` where relevant)
```xzatoma/src/providers/copilot.rs#L220-236
/// let config = CopilotConfig {
///     model: "gpt-5-mini".to_string(),
/// };
/// assert_eq!(provider.model(), "gpt-5-mini");
```

Notes:
- Tests that explicitly set `model: "gpt-4o"` are intentionally unchanged (they validate behavior when a user selects that model).
- All changes respect module boundaries and only update configuration/defaults and documentation.

## Testing & validation
What I ran locally (summary):
```/dev/null/validation_output.txt#L1-10
cargo fmt --all                     -> no formatting changes
cargo check --all-targets --all-features -> finished (0 errors)
cargo clippy --all-targets --all-features -- -D warnings -> finished (0 warnings)
cargo test --all-features           -> test result: ok. 138 passed; 0 failed
```

Targeted validations you can run locally:
- Unit test that proves the default:
  - `cargo test test_copilot_config_default_model -- --nocapture`
- Full validation (must pass before merging):
  - `cargo fmt --all`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`

What the new test asserts (important regression guard):
```xzatoma/src/providers/copilot.rs#L520-528
// Ensures CopilotConfig::default() uses the new model
let cfg = CopilotConfig::default();
assert_eq!(cfg.model, "gpt-5-mini");
```

## Backwards compatibility & migration
- Backward-compatible: existing user configs that explicitly set `copilot.model` continue to work unchanged.
- To keep the old default behavior, users may:
  - Set `copilot.model: gpt-4o` in their config file, or
  - Export env var: `export XZATOMA_COPILOT_MODEL=gpt-4o`
- No migration of persisted data is required — this is a configuration-default change only.

## Rollback plan
- Revert the `default_copilot_model()` string to `"gpt-4o"` (single-line change in `src/config.rs`) and run the full validation checklist above.
- Tests and docs have been written/updated to make this safe to revert.

## QA / Acceptance criteria
- [x] `CopilotConfig::default().model == "gpt-5-mini"` (unit test added)
- [x] All existing tests continue to pass (no regressions)
- [x] Documentation and example configs updated to avoid confusion
- [x] All quality gates green:
  - `cargo fmt --all`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`

## Usage examples
Default behavior (programmatic):
```xzatoma/src/providers/copilot.rs#L220-228
use xzatoma::config::CopilotConfig;

let cfg = CopilotConfig::default();
assert_eq!(cfg.model, "gpt-5-mini"); // new default
```

Override in config file:
```xzatoma/config/config.yaml#L9-15
copilot:
  model: gpt-4o   # explicit override to retain previous model
```

Override via environment variable:
```/dev/null/env_example.sh#L1-2
export XZATOMA_COPILOT_MODEL=gpt-4o
xzatoma run --plan my_plan.yaml
```

## Follow-ups (recommended)
- Announce change in CHANGELOG / release notes and migration notes for users.
- Search & update any external documentation, READMEs, or blog posts that reference the old default.
- Optional: add a short note in the CLI `--help` or README explaining the default model and how to override it.
- Monitor downstream integrations for any unexpected changes in cost/latency.

## Files changed (summary)
- Primary behavior
  - `xzatoma/src/config.rs` — default value changed to `gpt-5-mini`
  - `xzatoma/src/providers/copilot.rs` — examples updated, unit test added
  - `xzatoma/src/test_utils.rs` — test fixture updated
- Configuration example
  - `xzatoma/config/config.yaml` — example/default updated
- Documentation (explanation + reference) updated where they described the previous default

## References
- Implementation: see `xzatoma/src/config.rs` (default source of truth)
- Verification: see `xzatoma/src/providers/copilot.rs` unit tests

---
If you'd like, I can:
- Open a draft release note entry you can paste into the CHANGELOG
- Prepare a short user-facing announcement blurb (email/Slack/PR description)
- Run additional searches to update any remaining occurrences in external docs or examples
