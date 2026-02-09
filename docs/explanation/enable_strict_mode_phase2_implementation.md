# Enable Strict Mode (Phase 2) Implementation

## Overview

Phase 2 moves the file-editing safety work from permissive (Phase 1) into strict enforcement. The goal is to remove the legacy fallback behavior that allowed `edit` requests without an explicit anchor (`old_text`) to implicitly replace whole files, and to make `old_text` a required parameter for `edit` mode. This is a breaking change for any integrations or agents that relied on the old fallback behavior, so the rollout must be staged and monitored.

Key outcomes:
- Remove fallback (no silent overwrite when `old_text` is omitted)
- Make `old_text` required at the schema level for `mode == "edit"`
- Update agent prompts and guidance to communicate the breaking change and correct workflows
- Add tests that validate the stricter contract and helpful error messaging
- Provide a clear migration and monitoring plan

## Components Delivered

- Code
  - `src/tools/edit_file.rs` — enforce strict mode and update tool schema and error messages
  - `src/prompts/write_prompt.rs` — strengthen agent guidance (explicit strict-mode guidance)
- Tests
  - Updated unit tests in `src/tools/edit_file.rs`:
    - `test_edit_without_old_text_returns_error` (validates strict error messaging)
    - `test_tool_definition_includes_append_and_old_text_description` (validates JSON schema conditional `old_text` requirement)
  - Added test in `src/prompts/write_prompt.rs`:
    - `test_write_prompt_mentions_strict_mode`
- Documentation
  - `docs/explanation/enable_strict_mode_phase2_implementation.md` (this file)
  - Cross-references to Phase 1 docs: `docs/explanation/add_safety_features_phase1_implementation.md`

## Implementation Details

1. Remove fallback behavior
   - Previously, some callers that issued `edit` without `old_text` could cause the tool to effectively overwrite the file (unsafe fallback).
   - We explicitly return an error if `mode == "edit"` and `old_text` is not provided. The error message now clearly references the strict mode behavior and suggests alternatives (`append` or `overwrite`).

Example (excerpt from `EditMode::Edit` handling — see source for exact context):
```src/tools/edit_file.rs#L196-216
let old_text = match params.old_text.as_ref() {
    Some(t) => t,
    None => {
        return Ok(ToolResult::error(
            "edit mode requires old_text parameter.\n\
             To append to file, use 'append' mode.\n\
             To replace entire file, use 'overwrite' mode explicitly.\n\
             NOTE: Strict mode is enabled; edit will not fall back to overwriting the file.".to_string(),
        ));
    }
};
```

2. Schema-level enforcement (breaking)
   - Add a conditional JSON Schema clause to the tool definition so that when `mode == "edit"` the `old_text` parameter is required at the schema level. This helps tool consumers and contract validation to detect missing parameters early (before execution).

Key fragment from the tool definition:
```src/tools/edit_file.rs#L100-122
"parameters": {
  "type": "object",
  "properties": { ... },
  "required": ["path", "mode", "content"],
  "allOf": [
    {
      "if": { "properties": { "mode": { "const": "edit" } } },
      "then": { "required": ["old_text"] }
    }
  ]
}
```

3. Agent prompt updates
   - The system prompt (write-mode) was updated to explicitly state the strict-mode behavior, the expectation to use `old_text` for targeted edits, and to prevent agents from relying on fallback semantics.

Prompt snippet:
```src/prompts/write_prompt.rs#L17-28
- **edit**: For targeted modifications to existing files
  - STRICT MODE: REQUIRES `old_text` parameter with a unique snippet from the file. Calls to `edit` without `old_text` will be rejected — the tool no longer falls back to overwriting the file.
  - Use `read_file` first to find a good anchor point
  - Make `old_text` specific enough to match only once
```

4. Error messages and guidance
   - When validation fails, errors are intentionally helpful: they state what is missing, provide suggested modes to use (`append`, `overwrite`), and mention strict mode so agents can adapt.

## Testing

Tests were added and/or updated to exercise strict-mode behavior and to serve as safety nets for regressions.

- `src/tools/edit_file.rs`
  - `test_edit_without_old_text_returns_error`
    - Verifies the tool rejects `edit` without `old_text` and that the error message contains actionable guidance and mentions strict mode.
  - `test_tool_definition_includes_append_and_old_text_description`
    - Verifies `mode` enum includes `append` and that the `old_text` description indicates it's required for `edit`.
    - Verifies the presence of the conditional schema (`allOf` with `if`/`then`) requiring `old_text` when `mode == 'edit'`.
  - Existing safety tests (dramatic reduction detection, append behavior) remain in place.

- `src/prompts/write_prompt.rs`
  - `test_write_prompt_mentions_strict_mode`
    - Ensures the system prompt communicates the strict-mode change to agents (contains the words `strict`, `rejected`, or similar).

- Example expected behavior
  - Call without `old_text` (previously dangerous):
```/dev/null/before_example.json#L1-6
{
  "path": "Makefile",
  "mode": "edit",
  "content": "release-linux:\n\t@cargo build --release\n"
}
```

  - Expected error output:
```/dev/null/expected_error.txt#L1-4
edit mode requires old_text parameter.
To append to file, use 'append' mode.
To replace entire file, use 'overwrite' mode explicitly.
NOTE: Strict mode is enabled; edit will not fall back to overwriting the file.
```

  - Correct ways (examples):
```/dev/null/after_append.json#L1-6
{
  "path": "Makefile",
  "mode": "append",
  "content": "\nrelease-linux:\n\t@cargo build --release\n"
}
```

```/dev/null/after_targeted_edit.json#L1-8
{
  "path": "Makefile",
  "mode": "edit",
  "old_text": ".PHONY: all build run",
  "content": ".PHONY: all build run release-linux\n\nrelease-linux: ..."
}
```

- How to validate locally
  Run the project's quality gates in this exact order and verify each completes successfully with zero warnings/errors:
  - `cargo fmt --all`
  - `cargo check --all-targets --all-features`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test --all-features`

If any failures or warnings appear, fix them and re-run gates. If you want, I can run these checks and fix issues iteratively.

## Migration Path & Rollout Plan

1. Create a feature branch (e.g., `pr-feat-enable-strict-edit`) and land the changes there.
2. Merge into a staging environment and run end-to-end tests that simulate agent behavior against sample repositories.
3. Deploy to a limited set of agents (canaries) and monitor metrics:
   - Count of `edit` calls without `old_text` (should rise initially)
   - Rate of agent errors due to missing `old_text` (should decrease as agents are adapted)
   - Incidents of unintended full-file overwrites (should go to zero)
4. If agent authors require it, provide:
   - A migration guide and examples (this doc)
   - `confirm_overwrite` or `force` flags (future work) to allow explicit destructive operations
5. After a monitoring window with acceptable metrics, roll out to all agents.

Rollback strategy:
- If strict mode causes unacceptable disruption, revert the change in the staging branch and notify teams; however, the preferred approach is to temporarily add a compatibility layer (agent-side) that translates prior behavior into explicit `overwrite` calls and provides a feedback channel to help agents migrate.

## Risk Mitigation

- Risk: Agents or integrations break due to missing `old_text`. Mitigation: Clear, explicit error messages and updated prompts with examples; provide a migration guide and monitor failures.
- Risk: False positives from schema-level validation. Mitigation: Use schema conditional to require `old_text` only when `mode == 'edit'`, preserving non-edit workflows.
- Risk: Agents ignore prompts. Mitigation: Strengthen system prompts and add telemetry to identify non-compliant agents so maintainers can update them.

## Success Metrics

- Zero accidental whole-file overwrites triggered by `edit` mode.
- Decrease in incidents where users report data loss after edits.
- Low and declining rate of `edit` calls without `old_text` (agents updated).
- All CI gates (format, build, clippy, tests) pass with zero warnings.

## References

- Phase 1 implementation: `docs/explanation/add_safety_features_phase1_implementation.md`
- Source files changed:
  - `src/tools/edit_file.rs` (tool schema, error messages, tests)
  - `src/prompts/write_prompt.rs` (system prompt guidance and tests)

---

If you'd like, I can now:
- Run the full validation suite and fix any diagnostics found,
- Prepare a tidy commit and suggested PR description,
- Help draft a short announcement for agent integrators describing the breaking change and migration steps.

Which of these would you like me to do next?
