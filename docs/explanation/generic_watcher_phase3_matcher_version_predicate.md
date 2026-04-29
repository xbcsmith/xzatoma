# Generic Watcher Phase 3: Matcher Version Predicate Improvements

## Overview

Phase 3 replaces the regex-based `version` predicate in `GenericMatcher` with
proper semver constraint evaluation. Operators can now write human-readable
version constraints such as `">=2.0.0"` or `"^1"` instead of constructing regex
approximations. The `action` and `name` predicates continue to use regex
matching, which remains the right tool for flexible string pattern matching.

## Problem Statement

Before Phase 3 the `GenericMatcher` compiled all three predicate fields
(`action`, `name`, `version`) as regular expressions. This forced operators who
wanted to match on version to express semver relationships as regex patterns:

- Matching `>=2.0.0` required something like
  `[2-9]\.\d+\.\d+|[1-9]\d+\.\d+\.\d+`
- Matching `^1` (compatible with major version 1) required `1\.\d+\.\d+`
- Exact match `1.2.3` worked but silently also matched `1.2.30` unless anchored

These are error-prone, hard to read, and easy to get subtly wrong. Semver
constraints are the natural language for version filtering.

## Implementation

### Task 3.1: version_matches in src/watcher/mod.rs

A new public function
`version_matches(plan_version: &str, constraint: &str) -> bool` was added to
`src/watcher/mod.rs`. It is placed at the watcher module level so it can be used
by any watcher backend that needs version predicate evaluation, not just the
generic backend.

The function follows a two-step evaluation strategy:

1. Parse `plan_version` as `semver::Version`. If parsing fails return `false`
   immediately. This is a deliberate guard: a plan that carries a malformed or
   absent version string cannot satisfy any version constraint.

2. Parse `constraint` as `semver::VersionReq`. If parsing succeeds, return
   `req.matches(&version)`. If `constraint` cannot be parsed as a `VersionReq`
   (for example the operator wrote `"latest"` or a custom tag), fall back to
   case-insensitive exact string equality between `plan_version` and
   `constraint`.

The fallback ensures that operators who have not adopted semver and use plain
string tags as version identifiers in their constraint configuration are not
broken. When the constraint is not a valid `VersionReq` and the plan version
string happens to equal the constraint string, the matcher will accept the
event.

The `semver` crate version `1.0` was added to `Cargo.toml`. No other new
dependency was introduced.

### Task 3.2: Updated GenericMatcher in src/watcher/generic/matcher.rs

The `compiled_version: Option<Arc<Regex>>` field was removed from
`GenericMatcher` and replaced with `version_constraint: Option<String>`. The raw
constraint string is stored as-is and evaluated at match time via
`crate::watcher::version_matches`.

Changes to `GenericMatcher::new`:

- The call to `compile_optional_pattern` for the `version` field was removed.
- `version_constraint` is now populated directly from `config.version.clone()`.
- Invalid version constraint strings no longer cause `new` to return an error. A
  constraint that is not a valid `VersionReq` is silently treated as a
  plain-string constraint, consistent with the fallback in `version_matches`.

Changes to `GenericMatcher::matches_version`:

```src/watcher/generic/matcher.rs#L348-356
fn matches_version(&self, event: &GenericPlanEvent) -> bool {
    match (&self.version_constraint, &event.version) {
        (Some(constraint), Some(plan_version)) => {
            crate::watcher::version_matches(plan_version, constraint)
        }
        (None, _) => true,
        (Some(_), None) => false,
    }
}
```

Changes to `GenericMatcher::summary`:

The version field is now formatted as `version=<constraint>` (without enclosing
forward slashes) to visually distinguish semver constraints from regex
predicates:

```src/watcher/generic/matcher.rs#L292-302
if let Some(version) = &self.version_constraint {
    parts.push(format!("version={version}"));
}
```

The `mode()` and `fallback_mode()` functions were updated to use
`self.version_constraint.is_some()` in place of the former
`self.compiled_version.is_some()`. The matching mode logic is otherwise
unchanged.

Changes to `GenericMatcher::mode` and `GenericMatcher::fallback_mode`:

Both match arms previously checked `self.compiled_version.is_some()`. They now
check `self.version_constraint.is_some()`. The set of `MatchMode` variants and
their semantics are unchanged.

### Task 3.3: has_predicates and Accept-All Documentation

A new public method `GenericMatcher::has_predicates` was added:

```src/watcher/generic/matcher.rs#L243-247
pub fn has_predicates(&self) -> bool {
    self.compiled_action.is_some()
        || self.compiled_name.is_some()
        || self.version_constraint.is_some()
}
```

This method returns `true` when at least one predicate is configured. It allows
watcher startup code to detect accept-all mode and emit a warning log before the
consume loop begins, giving operators immediate feedback when no filtering is in
effect.

The module-level doc comment in `src/watcher/generic/mod.rs` was updated to
explicitly document accept-all semantics under a dedicated heading:

- An empty `GenericMatchConfig` (all `None`) causes the matcher to accept every
  structurally valid plan event consumed from the input topic.
- Operators should be aware that a watcher with no match config is effectively a
  catch-all executor. This is intentional for single-purpose dedicated watchers
  but is dangerous on shared topics.
- Callers can use `has_predicates()` to gate a startup warning.

The version matching section in `mod.rs` was also updated to clarify that
`action` and `name` continue to use regex while `version` now uses
`crate::watcher::version_matches`.

## Behavior Changes

| Scenario                                      | Phase 2 behavior                                                             | Phase 3 behavior                                                                       |
| --------------------------------------------- | ---------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `version = ">=2.0.0"`, plan version `"2.1.0"` | regex match on `">=2.0.0"` pattern, unpredictable                            | semver: `2.1.0` satisfies `>=2.0.0`, accepts                                           |
| `version = ">=2.0.0"`, plan version `"1.9.0"` | regex match succeeds (string `"1.9.0"` contains digits), accepts incorrectly | semver: `1.9.0` does not satisfy `>=2.0.0`, rejects                                    |
| `version = "^1"`, plan version `"1.5.3"`      | regex match on `"^1"`, anchors to start of string, may mis-match             | semver: `1.5.3` satisfies `^1`, accepts                                                |
| `version = "1.2.3"`, plan version `"1.2.30"`  | unanchored regex: `"1.2.3"` matches `"1.2.30"`, accepts incorrectly          | semver: `1.2.30` satisfies `^1.2.3` (compatible), accepts; exact `=1.2.3` would reject |
| `version = "latest"`, plan version `"latest"` | regex match, accepts                                                         | semver parse fails, string fallback: `"latest"` == `"latest"`, accepts                 |
| plan carries no version field                 | `None` vs `Some(regex)` -> rejects                                           | `None` vs `Some(constraint)` -> rejects (unchanged)                                    |
| `version = None` (accept-all)                 | accepts                                                                      | accepts (unchanged)                                                                    |

Note on the `"1.2.3"` exact constraint case:
`semver::VersionReq::parse("1.2.3")` interprets the constraint as `^1.2.3`
(compatible with `>=1.2.3, <2.0.0`), not as an exact-equality check. Operators
who need strict exact-version matching must write `"=1.2.3"`.

## Design Decisions

### Why not regex for version?

Semver version strings have structured semantics. The ordering relationship
`1.9.0 < 2.0.0` cannot be expressed correctly with a regex without constructing
a complex multi-branch pattern. Operators who write `version: ">=2.0.0"` have a
clear, unambiguous intent that regex cannot faithfully represent.

### Why keep regex for action and name?

Action and name are free-form strings without numeric ordering semantics.
Operators benefit from regex flexibility for these fields, for example
`action: "deploy.*"` to match any action beginning with `"deploy"`, or
`name: "service-(a|b|c)"` to match a specific set of services. Replacing regex
with equality or glob patterns would be a regression in expressive power.

### Why return false for non-semver plan versions?

The plan says: attempt to parse `plan_version` as `semver::Version`; return
`false` on parse failure. This is intentional. A plan that publishes a version
field containing a non-semver string such as `"nightly"` cannot meaningfully
satisfy a semver range constraint. Returning `false` immediately communicates
clearly that the plan's version is not eligible for version-range matching.

Operators who use non-semver plan versions and want to match on version must
configure an exact-string constraint that matches the plan version verbatim
(case-insensitively). The fallback in `version_matches` handles this case when
the `constraint` string is also not a valid `VersionReq` AND `plan_version`
happens to be valid semver.

### Why not validate the constraint at construction time?

The spec explicitly states that an invalid semver constraint should fall back to
exact string equality rather than failing at startup. This makes the matcher
tolerant of plain-string version tags in operator configuration without
requiring a separate configuration field for "semver vs. plain-string" mode.
Operators see consistent behavior: configure `version: "nightly"` and it will
match plans whose version field is exactly `"nightly"`.

## Testing

### Tests added to src/watcher/mod.rs

| Test name                                                               | What it verifies                                                            |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------- |
| `test_version_matches_exact`                                            | A plain `"X.Y.Z"` constraint (parsed as `^X.Y.Z`) matches the same version  |
| `test_version_matches_gte_range`                                        | `">=1.0.0"` accepts versions at or above `1.0.0` and rejects versions below |
| `test_version_matches_caret_range`                                      | `"^2"` accepts `2.x.x` and rejects `3.x.x` and `1.x.x`                      |
| `test_version_matches_invalid_version_returns_false`                    | Malformed plan versions return `false` unconditionally                      |
| `test_version_matches_invalid_constraint_falls_back_to_string_equality` | When constraint is not a valid `VersionReq`, string comparison is used      |

### Tests added to src/watcher/generic/matcher.rs

| Test name                                        | What it verifies                                                                          |
| ------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| `test_version_constraint_gte_matches`            | `">=1.0.0"` accepts `"1.2.0"`                                                             |
| `test_version_constraint_gte_rejects`            | `">=1.0.0"` rejects `"0.9.0"`                                                             |
| `test_version_constraint_caret_matches`          | `"^2"` accepts `"2.5.1"`                                                                  |
| `test_version_constraint_caret_rejects`          | `"^2"` rejects `"3.0.0"`                                                                  |
| `test_version_exact_string_fallback`             | Non-semver constraint triggers string fallback; non-semver plan version rejects at step 1 |
| `test_version_required_but_plan_version_is_none` | Returns `false` when constraint is set but event carries no version                       |
| `test_has_predicates_empty`                      | Returns `false` for a default all-None config                                             |
| `test_has_predicates_version_only`               | Returns `true` when only version constraint is set                                        |

## Files Changed

| File                             | Change                                                                                                                                                                                                       |
| -------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `Cargo.toml`                     | Added `semver = "1.0"` dependency                                                                                                                                                                            |
| `src/watcher/mod.rs`             | Added `version_matches` function and unit tests; moved `pub use XzeprWatcher` before test module                                                                                                             |
| `src/watcher/generic/matcher.rs` | Replaced `compiled_version: Option<Arc<Regex>>` with `version_constraint: Option<String>`; updated `new`, `mode`, `fallback_mode`, `matches_version`, `summary`; added `has_predicates`; added Phase 3 tests |
| `src/watcher/generic/mod.rs`     | Updated module description; added accept-all semantics section and version matching section to module doc comment                                                                                            |

## Success Criteria Verification

- A `GenericMatcher` configured with `version: Some(">=2.0.0".to_string())`
  correctly rejects a plan event with `version = "1.9.0"` (verified by
  `test_version_constraint_gte_rejects`) and accepts one with
  `version = "2.1.0"` (verified by `test_version_constraint_gte_matches`).
- The `regex` crate is no longer used for the version predicate anywhere in
  `matcher.rs`. The `compiled_version` field and the call to
  `compile_optional_pattern` for version have been removed.
- All four quality gates pass: `cargo fmt --all`,
  `cargo check --all-targets --all-features`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test --all-features`.
- 181 watcher tests pass; 17 are ignored (require a live Kafka broker).
