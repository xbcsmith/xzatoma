# Agent Skills Phase 4 Implementation

## Overview

This document describes the Phase 4 implementation for agent skills: trust
enforcement, skill resource access, and session-local lifecycle management.

Phase 1 established:

- deterministic discovery
- `SKILL.md` parsing
- validation and diagnostics
- valid-only catalog construction

Phase 2 added:

- skill catalog disclosure
- visibility filtering
- prompt disclosure integration

Phase 3 added:

- skill activation
- active-skill registry
- transient prompt injection for active skill content

Phase 4 builds on that foundation by introducing:

- a persistent trust store for project and custom skill paths
- safe resource resolution relative to the skill root
- bounded resource enumeration under approved directories
- explicit lifecycle guarantees for active skills

The key objective of Phase 4 is to make trust and resource behavior safe,
predictable, and testable without changing the core design decision that active
skills remain session-local only.

## Goals

Phase 4 has four primary goals:

1. enforce project and custom-path trust consistently
2. implement a persistent trust store with add/show/remove operations
3. safely resolve and enumerate skill resources without path escape
4. preserve session-local active-skill lifecycle semantics

These goals correspond directly to the implementation plan and complete the
first trustworthy runtime model for skills.

## Scope

Phase 4 covers the following planned work:

- create `src/skills/trust.rs`
- implement persistent trust store operations
- implement canonicalized path handling
- implement path-prefix trust checks
- implement safe resource resolution relative to a skill root
- implement resource enumeration under:
  - `scripts/`
  - `references/`
  - `assets/`
- integrate trust-backed visibility in runtime flows
- integrate trust-related CLI/backend support
- preserve session-local active-skill lifecycle
- document the implementation

Phase 4 does not yet include:

- full hardening and release polish
- final CLI/documentation sweep for all skill commands
- long-term active-skill persistence
- activation-time resource loading
- strict enforcement of `allowed-tools` against the tool registry

Those remain Phase 5 or deferred work.

## Files Added and Updated

### New file

Phase 4 introduces:

- `src/skills/trust.rs`

### Existing files updated

Phase 4 requires updates to:

- `src/skills/mod.rs`
- `src/commands/mod.rs`
- `src/cli.rs`
- `src/main.rs`

In the planned full Phase 4 implementation, a dedicated command handler file is
also expected:

- `src/commands/skills.rs`

### Tests added

Phase 4 requires:

- `tests/skills_trust.rs`
- `tests/skills_resources.rs`
- `tests/skills_lifecycle.rs`

### Documentation added

Phase 4 requires:

- `docs/explanation/agent_skills_phase4_implementation.md`

## Trust Model

Phase 4 formalizes the trust model for visible and activatable skills.

### Trust rules

The runtime trust rules are:

- project skills are visible only when trusted if
  `skills.project_trust_required == true`
- custom paths are trusted only if explicitly allowed or trusted
- user-level skill roots are considered trusted by default for this release
- invalid skills are never visible regardless of trust
- `allowed-tools` remains advisory in v1

These rules apply consistently at disclosure time, activation time, and any
other point where visible skills are derived from the valid catalog.

### Project roots

Project roots include:

- `<working_dir>/.xzatoma/skills/`
- `<working_dir>/.agents/skills/`

If `skills.project_trust_required == true`, these roots must be trusted through
the trust store before their valid skills may become visible.

### User roots

User roots include:

- `~/.xzatoma/skills/`
- `~/.agents/skills/`

For this release, user roots are treated as trusted by default. This avoids
adding unnecessary friction for user-owned skill libraries while still keeping
project and custom paths gated.

### Custom roots

Custom roots are configured through:

- `skills.additional_paths`

Custom paths are visible only when one of the following is true:

- the path is explicitly trusted
- `skills.allow_custom_paths_without_trust == true`

This keeps arbitrary configured paths from silently becoming visible to the
model without an explicit trust decision.

## Trust Store

Phase 4 introduces a persistent trust store implemented in
`src/skills/trust.rs`.

### Responsibilities

The trust store is responsible for:

- persistent trusted-path storage
- canonicalized path handling
- add/remove/show operations
- prefix-based trust checks for descendants

### Persistence

The store persists trusted paths to disk in a YAML file. A typical default path
is:

```/dev/null/skills_trust.yaml#L1-1
~/.xzatoma/skills_trust.yaml
```

If `skills.trust_store_path` is configured, that path overrides the default.

### Canonicalization

All trusted paths are canonicalized before storage. This is essential for:

- stable equality checks
- robust descendant-prefix trust checks
- avoiding duplicate entries caused by path spelling differences
- preventing misleading trust decisions caused by symlinked or normalized paths

### Prefix trust checks

Trust checks are prefix-based. If a path is trusted, then descendants of that
path are also trusted.

Representative behavior:

```/dev/null/trust_examples.txt#L1-6
Trusted root: /workspace/project

Trusted:
- /workspace/project
- /workspace/project/.xzatoma/skills/demo
- /workspace/project/subdir/file.txt
```

This behavior matches the requirement for project-level trust checks and
supports project-root trust rather than forcing per-skill trust operations.

## Trust Store API

A practical Phase 4 implementation exposes a small API surface.

### `SkillTrustStore`

`SkillTrustStore` represents the persistent store plus in-memory data needed to
manage it safely.

Representative responsibilities:

- load existing trust state
- create empty trust state if no file exists
- save trust state
- add a path
- remove a path
- list trusted paths
- test whether a path is trusted

### `SkillTrustStoreData`

Separating serializable data from runtime behavior keeps persistence simple and
testable.

Representative shape:

```/dev/null/trust_store_data.txt#L1-5
SkillTrustStoreData {
  trusted_paths: {
    "/workspace/project",
    "/opt/xzatoma/custom_skills"
  }
}
```

### Typical operations

Representative trust operations:

```/dev/null/trust_flow.txt#L1-8
1. Resolve trust store path
2. Load existing store or create empty one
3. Canonicalize requested path
4. Add/remove canonical path
5. Save updated store
6. Re-load or inspect trusted paths as needed
```

This simple flow is sufficient for Phase 4 and aligns with the CLI trust
commands planned in later integration work.

## Resource Access Model

Phase 4 introduces safe skill resource resolution and enumeration.

### Supported resource directories

Skill resources are limited to files under:

- `scripts/`
- `references/`
- `assets/`

These directories are resolved relative to the skill root.

### Why resource access is constrained

The implementation plan explicitly requires that resource resolution:

- remains inside the skill root
- prevents path traversal
- stays lazy until explicitly needed

This protects against accidental or malicious resource access outside the skill
boundary.

### Lazy access rule

Resources are not automatically loaded during activation.

Instead:

- activation may surface an advisory resource list
- actual access happens later only if the model explicitly chooses to read
  referenced files through ordinary file tools

This keeps activation lightweight and preserves the existing generic tool model.

## Resource Resolution

### Relative-only resolution

Resource paths must be resolved only from the skill root. Absolute paths are
rejected.

Valid example:

```/dev/null/resource_paths.txt#L1-3
references/guide.md
scripts/build.sh
assets/logo.png
```

Invalid example:

```/dev/null/resource_paths.txt#L1-3
/etc/passwd
../../outside.txt
/absolute/path/file.md
```

### Path traversal rejection

Any resource path containing parent traversal that would escape the skill root
must be rejected.

Representative rejection cases:

```/dev/null/path_traversal.txt#L1-4
../outside.txt
references/../../escape.md
scripts/../../../tmp/file.sh
```

These must fail cleanly rather than normalize into an escaped path.

### Canonicalized validation

A safe implementation should:

1. canonicalize the skill root
2. join the relative resource path
3. canonicalize or normalize the resolved path
4. verify the resolved path still starts with the canonical skill root

If not, the operation fails.

This protects against both direct traversal and more subtle path escape cases.

## Resource Enumeration

### Enumeration rules

Phase 4 resource enumeration:

- starts at the skill root
- visits only supported directories
- recurses within them
- returns deterministic results
- rejects escaped paths
- ignores unsupported sibling directories

### Deterministic output

Resource enumeration should return results in deterministic order. This
supports:

- stable tests
- predictable activation metadata
- clearer debugging

### Example enumerated result

Representative result shape:

```/dev/null/skill_resources.txt#L1-12
SkillResources {
  scripts: [
    /workspace/project/.xzatoma/skills/demo/scripts/build.sh
  ],
  references: [
    /workspace/project/.xzatoma/skills/demo/references/guide.md
  ],
  assets: [
    /workspace/project/.xzatoma/skills/demo/assets/logo.png
  ]
}
```

### Unsupported directories

Files outside the supported directories are not enumerated as resources in v1.

For example:

```/dev/null/ignored_resources.txt#L1-4
ignored:
- notes/
- tmp/
- random.txt
```

These remain accessible only if the model explicitly reaches them through
ordinary file operations and if such access is otherwise permitted.

## `allowed-tools` in Phase 4

Phase 4 completes the advisory handling of `allowed-tools`.

### Required behavior

The implementation must:

- parse `allowed-tools`
- surface it in activation metadata
- not enforce it against the live tool registry in v1

### Why it remains advisory

This project intentionally keeps the first release simple. Enforcing
`allowed-tools` dynamically against the registry would add policy complexity
that is deferred for later hardening.

So in Phase 4:

- the metadata is preserved
- the metadata is disclosed in activation output
- the metadata is injected into active-skill prompt content
- the runtime does not block tool execution based on it

## Lifecycle Semantics

Phase 4 formalizes lifecycle behavior for active skills.

### Session-local only

Active skills exist only for the current process/session.

This means:

- they are created during the current runtime
- they survive only as long as that process lives
- they are lost on process exit by design

### No restoration on resume

Resumed conversations must not restore active skills.

This is an explicit design choice in v1. Conversation history and active-skill
state are intentionally separate.

So:

- resumed chat history may be restored
- active skills are not restored
- a resumed session starts with no active skills
- the model must activate skills again through `activate_skill` if needed

### Prompt injection per provider call

Prompt assembly must inject active skills on every provider call during the same
session.

This ensures:

- active guidance remains available throughout the session
- active skills do not depend on a one-time startup injection
- transient injection remains consistent across turns

This also matches the cleanup pass that moved active skill content out of
`Conversation.messages` and into transient provider-call assembly.

## Integration Responsibilities

### `src/commands/mod.rs`

Phase 4 integration in command flow is responsible for:

- loading trusted paths from the trust store
- applying trust filtering when building visible catalogs
- ensuring prompt injection remains session-local
- ensuring resumed sessions do not restore active skills

### `src/cli.rs`

Phase 4 introduces the command surface for trust-oriented operations. The
implementation plan expects:

- `skills paths`
- `skills trust show`
- `skills trust add <path>`
- `skills trust remove <path>`

This phase lays the backend and lifecycle groundwork for those commands.

### `src/commands/skills.rs`

The intended dedicated command backend is responsible for:

- listing valid skills
- validating configured roots
- showing skill metadata
- showing effective paths and trust state
- adding/removing/showing trust entries

A clean Phase 4 implementation keeps the actual trust logic in `skills/trust.rs`
and lets the command layer remain thin.

## Representative CLI Semantics

Although command formatting can vary, the expected semantics are:

### Show trust state

```/dev/null/skills_cli.txt#L1-6
xzatoma skills trust show

Trust store: ~/.xzatoma/skills_trust.yaml
Trusted paths:
- /workspace/project
- /opt/xzatoma/custom_skills
```

### Add trust

```/dev/null/skills_cli.txt#L1-3
xzatoma skills trust add /workspace/project
Added trusted path: /workspace/project
```

### Remove trust

```/dev/null/skills_cli.txt#L1-3
xzatoma skills trust remove /workspace/project
Removed trusted path: /workspace/project
```

### Show effective paths and trust

```/dev/null/skills_cli.txt#L1-8
xzatoma skills paths

Discovery paths:
- /workspace/project/.xzatoma/skills
- /workspace/project/.agents/skills
- ~/.xzatoma/skills
- ~/.agents/skills

Project trust required: true
```

These commands provide the human-visible operational surface for the trust
model.

## Prompt Injection Lifecycle

Phase 4 also tightens the lifecycle expectation for prompt injection.

### Startup disclosure vs active skill injection

There are now two distinct prompt-layer mechanisms:

1. startup disclosure of visible valid skills
2. transient injection of active skill content

These must remain separate.

### Repeated per-turn injection

Active skill content must be injected for every provider call while the skill is
active in the current session.

That ensures the model has access to active skill instructions even when the
transient provider input is rebuilt each turn.

### No persistence in history

The cleanup pass completed before Phase 4 is important here:

- active skill content is transient
- it is not stored in `Conversation.messages`
- it is not replayed as synthetic conversation history
- it is not restored on resume

This is exactly the intended lifecycle shape for v1.

## Testing Requirements

Phase 4 requires three dedicated test files:

- `tests/skills_trust.rs`
- `tests/skills_resources.rs`
- `tests/skills_lifecycle.rs`

### Required trust tests

#### Trust add/show/remove

Verify:

- trusted paths can be added
- stored trust can be listed/shown
- trusted paths can be removed
- persistence survives reload

#### Untrusted project path omission

Verify:

- a valid project skill is omitted when the project root is not trusted and
  `project_trust_required == true`

#### Trusted project path inclusion

Verify:

- a trusted project root allows valid project skills to become visible

#### Custom path trust behavior

Verify:

- custom skills are omitted when trust is required and the path is not trusted
- custom skills are visible when explicitly trusted
- custom skills are visible when `allow_custom_paths_without_trust == true`

### Required resource tests

#### Path traversal rejection

Verify:

- traversal attempts are rejected
- absolute paths are rejected
- escaped canonicalization is rejected

#### Resource enumeration stays within skill root

Verify:

- enumeration returns only files under supported directories
- all returned paths remain under the skill root
- unsupported directories are ignored

### Required lifecycle tests

#### Active skills do not persist across restarted runtime instances

Verify:

- activate a skill in one runtime instance
- construct a new runtime instance
- confirm no active skills are present

#### Prompt injection repeats correctly across multiple turns in one session

Verify:

- active skill content is injected on each provider call in one session
- active skill content is not added to `Conversation.messages`
- resumed sessions do not restore prior active skills

## Representative Test Matrix

A practical test matrix for Phase 4 looks like this:

```/dev/null/test_matrix.txt#L1-14
skills_trust.rs
- test_trust_store_add_show_remove
- test_untrusted_project_path_omission
- test_trusted_project_path_inclusion
- test_custom_path_trust_behavior

skills_resources.rs
- test_path_traversal_rejection
- test_resource_enumeration_stays_within_skill_root
- test_only_supported_directories_are_enumerated

skills_lifecycle.rs
- test_active_skills_are_session_local_only
- test_active_skills_not_restored_on_resume
- test_prompt_injection_repeats_across_turns
```

## Deliverables Checklist

Phase 4 is complete only when all of the following are true:

- `src/skills/trust.rs` exists
- persistent trust store is implemented
- trust add/show/remove operations work
- canonicalized path handling is implemented
- prefix-based trust checks are implemented
- relative resource resolution is implemented
- resource enumeration for `scripts/`, `references/`, and `assets/` is
  implemented
- path traversal outside skill root is rejected
- active skills remain session-local only
- resumed conversations do not restore active skills
- prompt injection repeats per provider call during a session
- trust, resource, and lifecycle tests pass
- documentation is written in `docs/explanation/`

## Quality Gates

Per project rules, the following commands must pass in order:

```/dev/null/commands.sh#L1-4
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

This document must also pass Markdown linting and formatting:

```/dev/null/commands.sh#L1-2
markdownlint --fix --config .markdownlint.json "docs/explanation/agent_skills_phase4_implementation.md"
prettier --write --parser markdown --prose-wrap always "docs/explanation/agent_skills_phase4_implementation.md"
```

## Risks and Mitigations

### Risk: trust checks become inconsistent across flows

If disclosure, activation, and CLI trust operations use different trust logic,
the system becomes unpredictable.

Mitigation:

- centralize trust logic in `skills/trust.rs`
- keep command layer thin
- test visibility decisions directly

### Risk: canonicalization differences create duplicate trust entries

If paths are stored without canonicalization, logically identical paths may be
treated as distinct.

Mitigation:

- canonicalize before storing
- canonicalize before checking
- test reload behavior

### Risk: resource traversal escapes skill boundaries

If relative resource paths are joined without validation, a malicious or
accidental traversal could escape the skill root.

Mitigation:

- reject absolute resource paths
- reject parent traversal attempts
- verify resolved paths remain within the canonical root
- test escape cases explicitly

### Risk: active skills accidentally persist

If active skill state is mixed with conversation history or persistence layers,
session-local semantics will be broken.

Mitigation:

- keep active-skill registry separate
- inject active content transiently only
- never restore active skills on resume
- test fresh-runtime behavior explicitly

### Risk: resource enumeration grows too broad

If enumeration is not constrained to approved directories, the runtime may
accidentally expose too much filesystem state.

Mitigation:

- enumerate only `scripts/`, `references/`, and `assets/`
- ignore unsupported siblings
- keep actual resource reads lazy

## Summary

Phase 4 completes the first trustworthy operational model for agent skills.

The key outcomes are:

- trust is persistent and path-based
- project and custom paths are gated correctly
- user roots remain trusted by default in v1
- skill resource access is safely constrained to the skill root
- supported resources are enumerable but lazily accessed
- active skills remain session-local only
- prompt injection repeats during a session without polluting conversation
  history
- resumed conversations do not restore active skills

This phase is the bridge between initial skill capability and production-grade
behavior. It keeps the implementation simple where the plan calls for simplicity
while enforcing the critical boundaries required for safe skill operation.
