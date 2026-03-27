# Agent Skills Phase 3 Implementation

## Overview

This document describes the intended Phase 3 implementation for agent skills:
skill activation and the active-skill registry.

Phase 1 established deterministic discovery, parsing, validation, diagnostics,
and a valid-only skill catalog.

Phase 2 added startup catalog disclosure and prompt integration for visible
skills without exposing full `SKILL.md` bodies.

Phase 3 builds on that foundation by introducing the first supported activation
path. It allows the model to explicitly activate one valid visible skill through
a synthetic tool and stores the resulting activated skill state in a dedicated
runtime registry. That registry is then used by prompt assembly to inject active
skill content before provider completion without storing synthetic skill content
in `Conversation.messages`.

## Goals

Phase 3 has four primary goals:

1. create a dedicated runtime registry for active skills
2. implement `activate_skill` as the only supported activation path
3. register the activation tool only when valid visible skills exist
4. inject active skill content into prompt assembly without polluting
   conversation history

These goals enforce the runtime contract defined in the implementation plan and
prepare the system for later lifecycle, resource, and trust work.

## Scope

Phase 3 covers the following planned work:

- create `src/skills/activation.rs`
- create `src/tools/activate_skill.rs`
- define:
  - `ActiveSkillRegistry`
  - `ActiveSkill`
  - `ActivateSkillTool`
- register `activate_skill` only when appropriate
- deduplicate repeated activation attempts
- ensure activation is driven only through the tool
- update prompt assembly to inject active skills before provider completion
- keep active skill content out of `Conversation.messages`
- add activation and tool tests
- add implementation documentation

Phase 3 does not yet include:

- trust store persistence and lifecycle management
- skill resource loading beyond placeholder resource enumeration
- CLI activation management commands
- long-term session persistence of active skills
- persistent storage of activation state
- direct activation outside the synthetic tool path

Those responsibilities are deferred to later phases.

## Runtime Model

Phase 3 introduces a separate active-skill runtime layer.

The runtime model now has four distinct skill-related concepts:

1. discovered valid skills
2. visible disclosed skills
3. active runtime skills
4. prompt-injected active skill content

This separation is intentional and important.

### Valid skill catalog

The valid skill catalog remains the source of truth for all parsed,
precedence-resolved, valid skills discovered from configured roots.

The catalog:

- contains only valid winning skills
- excludes invalid skills
- excludes shadowed skills
- is used as the source for activation

### Visible skill set

The visible skill set is a filtered view of the valid catalog based on trust and
configuration rules.

The visible set is used for:

- startup catalog disclosure
- activation tool registration
- activation tool input enumeration

This means a skill may be valid but not visible, and therefore not activatable.

### Active-skill registry

The active-skill registry is the new Phase 3 runtime structure.

It is responsible for:

- tracking active skills by canonical name
- deduplicating repeat activation
- storing prompt-injection-ready skill content
- exposing deterministic active-skill ordering
- keeping active-skill state out of conversation history

### Prompt injection layer

The prompt injection layer reads from the active-skill registry and appends
active skill content before provider completion.

This is the key Phase 3 behavior change:

- active skill content influences the model
- active skill content is not stored as ordinary conversation messages

That separation preserves cleaner conversation history and avoids synthetic
skill content becoming part of persisted or replayed user/assistant history.

## Files Added and Updated

### New files

Phase 3 introduces:

- `src/skills/activation.rs`
- `src/tools/activate_skill.rs`

### Existing files updated

Phase 3 requires updates to:

- `src/tools/mod.rs`
- `src/commands/mod.rs`
- optionally `src/tools/registry_builder.rs`
- `src/prompts/mod.rs`
- `docs/explanation/implementations.md`

### Tests added

Phase 3 should add:

- `tests/skills_activation.rs`
- `tests/skills_tool.rs`

### Documentation added

Phase 3 requires:

- `docs/explanation/agent_skills_phase3_implementation.md`

## Active Skill Data Model

### `ActiveSkill`

`ActiveSkill` represents one activated runtime skill.

It should contain prompt-injection-ready information including:

- `skill_name`
- skill directory path
- `SKILL.md` file path
- description
- normalized advisory `allowed-tools`
- full body content
- optional enumerated resources list

Representative shape:

```/dev/null/active_skill.txt#L1-9
ActiveSkill {
  skill_name: "rust_refactor",
  skill_directory: "/workspace/.xzatoma/skills/rust_refactor",
  skill_file: "/workspace/.xzatoma/skills/rust_refactor/SKILL.md",
  description: "Assist with safe Rust refactors",
  allowed_tools: ["read_file", "grep", "edit_file"],
  body_content: "...full skill body...",
  resources: []
}
```

Important constraints:

- the skill is created only from a valid loaded catalog entry
- the model never provides arbitrary file paths
- activation resolves the body only from the catalog-backed skill record
- the structure is ready for prompt-layer injection

### `ActiveSkillRegistry`

`ActiveSkillRegistry` is the session-scoped store for activated skills.

It should provide:

- activation by skill name via a valid visible catalog
- lookup by skill name
- deterministic iteration order
- deduplication
- optional removal/clear helpers
- prompt rendering for all active skills

The registry must track active skills by canonical skill name.

A suitable internal representation is a deterministic map keyed by skill name.

Representative behavior:

- activating a new skill inserts it
- activating the same skill again does not duplicate it
- rendering active skills returns prompt-ready content in deterministic order

### `ActivationStatus`

A useful implementation detail is to model activation outcomes explicitly.

For example:

- `Activated(ActiveSkill)`
- `AlreadyActive(ActiveSkill)`

This makes deduplication observable and testable without complicating registry
state.

## Activation Tool Contract

The only supported activation path in Phase 3 is the synthetic tool named:

- `activate_skill`

This is a runtime contract and must remain exact.

### Registration rules

The tool must be registered only when all of the following are true:

- `skills.enabled == true`
- `skills.activation_tool_enabled == true`
- at least one valid visible skill exists

If there are no valid visible skills, the tool must not be registered.

### Input contract

The tool parameter schema must contain exactly one required input:

- `skill_name`

The skill name input must be restricted to valid visible skill names only.

Representative schema:

```/dev/null/activate_skill_schema.json#L1-17
{
  "name": "activate_skill",
  "description": "Activate one valid visible skill by name for the current session.",
  "parameters": {
    "type": "object",
    "properties": {
      "skill_name": {
        "type": "string",
        "enum": ["git_review", "rust_refactor", "test_debugging"]
      }
    },
    "required": ["skill_name"],
    "additionalProperties": false
  }
}
```

Important constraints:

- exactly one required input
- no arbitrary path input
- no raw body input
- no multi-skill activation in a single call
- enum restricted to valid visible names only

### Output contract

Activation output must be structured and wrapped for prompt-layer use.

Required output content:

- skill name
- skill directory path
- normalized advisory `allowed-tools`
- full body content
- optional enumerated resource list

Representative output shape:

```/dev/null/activate_skill_output.json#L1-11
{
  "activated": true,
  "deduplicated": false,
  "skill_name": "rust_refactor",
  "skill_dir": "/workspace/.xzatoma/skills/rust_refactor",
  "allowed_tools": ["read_file", "grep", "edit_file"],
  "body": "Full skill body content here",
  "resources": []
}
```

On repeat activation:

```/dev/null/activate_skill_output.json#L1-11
{
  "activated": true,
  "deduplicated": true,
  "skill_name": "rust_refactor",
  "skill_dir": "/workspace/.xzatoma/skills/rust_refactor",
  "allowed_tools": ["read_file", "grep", "edit_file"],
  "body": "Full skill body content here",
  "resources": []
}
```

The wrapped output is intended for runtime processing and prompt-layer
injection, not for exposing direct filesystem access.

### Failure conditions

Activation must fail cleanly when:

- the requested skill name is missing
- the requested skill is not visible
- the requested skill is invalid
- the requested skill is filtered by trust rules
- registry access fails

The tool must never read arbitrary file paths from model input.

## Activation Flow

The intended Phase 3 activation flow is:

1. startup discovers valid skills
2. startup filters visible skills
3. startup registers `activate_skill` if allowed
4. model sees visible skill names through tool schema
5. model calls `activate_skill` with one valid visible skill name
6. tool validates the requested name against visible catalog
7. tool activates the skill through the active registry
8. tool returns structured activation output
9. prompt assembly injects active skills before provider completion

This flow enforces the design requirement that skill activation occurs only
through the dedicated tool path.

## Registry Requirements

### Track active skills by name

The registry must store active skills by canonical skill name.

This ensures:

- deterministic lookup
- stable deduplication
- easy prompt ordering
- simple future lifecycle operations

### Deduplicate repeat activation

If a skill is already active, activating it again must not create a duplicate
entry.

Instead:

- registry state remains unchanged
- the activation call returns a deduplicated result
- prompt injection remains stable

### Resolve skill body on activation

Activation must resolve the body from a valid catalog-backed skill record.

The model must never supply a path or body payload.

This ensures:

- no arbitrary path reading from model input
- activation remains catalog-bounded
- prompt injection is always sourced from validated records

### Store skill directory and advisory metadata

Activation records must preserve enough metadata to support future lifecycle and
resource features.

That includes at minimum:

- canonical skill name
- skill directory path
- `SKILL.md` path
- description
- advisory `allowed-tools`

### Produce prompt-injection-ready content

The registry should expose either:

- rendered active-skill injection blocks
- or data structures that can be rendered deterministically by prompt assembly

The simplest implementation is to let the registry provide a
`render_for_prompt_injection` helper.

## Prompt Injection Behavior

Phase 3 requires prompt-layer injection of currently active skills.

### Required behavior

The runtime must inject active skill content before provider completion.

This means the model sees:

- base system prompt
- optional startup skill disclosure
- active skill content

in that general order.

### Ordering rationale

A practical and clear prompt assembly order is:

1. base mode prompt
2. disclosure section for visible skills
3. active skill content section

This keeps the distinction between:

- what is available
- what is currently active

### Conversation history rule

A critical runtime rule is:

- active skills must not be stored in `Conversation.messages`

This means the system should avoid inserting active skill bodies as synthetic
messages into ordinary history.

Instead, prompt assembly should append active skill content immediately before
provider completion.

This preserves:

- cleaner history
- better replay semantics
- lower persistence noise
- clearer conceptual boundaries

## Integration in Runtime Flows

### `commands::chat::run_chat`

Chat startup must:

- build the visible catalog
- construct or share the active skill registry for the session
- register `activate_skill` when allowed
- keep the registry alive across the session lifetime
- ensure prompt assembly consults the active registry before provider calls

This registry must persist for the duration of the chat session.

### `commands::run::run_plan_with_options`

Run startup must do the same:

- build visible skill catalog
- create active registry
- register `activate_skill` when appropriate
- ensure prompt assembly uses active registry before provider completion

This keeps activation semantics aligned between chat mode and run mode.

### `tools/mod.rs`

The tool module must export the activation tool so it can be registered by the
command or registry-builder layer.

### `tools/registry_builder.rs`

If registration is centralized here, the builder may optionally support adding
`activate_skill` after skill visibility filtering has already occurred.

The key design rule is:

- do not register activation blindly
- register only after visible valid skills are known

## Suggested Prompt Shape for Active Skills

A reasonable prompt injection block for active skills might look like this:

```/dev/null/active_skills_prompt.txt#L1-13
## Active Skills

The following skills have been explicitly activated for this session.
Follow their guidance when relevant.

### Active Skill: rust_refactor
- Description: Assist with safe Rust refactors
- Directory: /workspace/.xzatoma/skills/rust_refactor
- Allowed Tools: read_file, grep, edit_file
- Resources: none

<full skill body content here>
```

Important properties:

- deterministic ordering
- clear headings
- includes advisory metadata
- includes full body content only after activation
- not stored in ordinary conversation history

## Testing Requirements

Phase 3 requires two new test files:

- `tests/skills_activation.rs`
- `tests/skills_tool.rs`

### Required tests

#### Activation succeeds for valid visible skill

Verify that:

- a valid visible skill can be activated
- activation returns the expected wrapped output
- registry now contains the skill

#### Duplicate activation does not duplicate registry state

Verify that:

- repeated activation of the same skill does not increase registry size
- second activation returns a deduplicated outcome

#### Activation fails for missing skill

Verify that:

- activating a non-existent skill fails
- registry remains unchanged

#### Activation fails for invalid skill

Verify that:

- invalid skills never enter the valid visible catalog
- they cannot be activated through the tool

#### Activation fails for untrusted project skill

Verify that:

- a valid but hidden untrusted project skill is not activatable
- tool rejects the request cleanly

#### Tool not registered when there are no valid visible skills

Verify that:

- empty visible catalog yields no `activate_skill` registration

#### Tool schema only lists valid visible skill names

Verify that:

- enum values in the tool schema contain only visible valid skill names
- invalid, hidden, and shadowed skills are absent

#### Prompt-layer injection includes active skill content after activation

Verify that:

- after activation, prompt assembly contains active skill content
- before activation, it does not
- active content is injected from registry state

### Strongly recommended additional tests

#### Conversation messages remain clean

Verify that:

- active skill content is not inserted into `Conversation.messages`
- synthetic activation state is external to conversation history

#### Deterministic active skill ordering

Verify that:

- multiple active skills render in deterministic canonical order

#### Wrapped output includes all required fields

Verify that activation output includes:

- `skill_name`
- `skill_dir`
- `allowed_tools`
- `body`
- `resources`

## Deliverables Checklist

Phase 3 is complete only when all of the following are true:

- `src/skills/activation.rs` exists
- `src/tools/activate_skill.rs` exists
- `ActiveSkillRegistry` is implemented
- `ActiveSkill` is implemented
- `ActivateSkillTool` is implemented
- activation deduplication works
- prompt-layer injection uses the active registry
- `Conversation.messages` remains free of synthetic active skill content
- activation and tool tests pass
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
markdownlint --fix --config .markdownlint.json "docs/explanation/agent_skills_phase3_implementation.md"
prettier --write --parser markdown --prose-wrap always "docs/explanation/agent_skills_phase3_implementation.md"
```

## Risks and Mitigations

### Risk: activation bypasses visibility rules

If the tool can activate any catalog skill instead of only visible skills,
hidden or untrusted skills could leak into runtime behavior.

Mitigation:

- build the tool from the visible catalog only
- restrict schema enum to visible valid names
- validate requested names against visible names at execution time

### Risk: duplicate activation bloats prompt context

If repeated activation inserts duplicate active entries, prompt size and runtime
state will drift.

Mitigation:

- registry keyed by canonical skill name
- explicit deduplication semantics
- dedicated tests for repeat activation

### Risk: synthetic skill content leaks into conversation history

If active skills are stored as ordinary messages, persistence and replay become
noisy and confusing.

Mitigation:

- keep active skill registry separate from `Conversation.messages`
- inject active content only during prompt assembly
- test for history cleanliness directly

### Risk: activation reads arbitrary model-provided paths

If activation accepts a path or body from the model, it could bypass catalog
validation entirely.

Mitigation:

- schema exposes only `skill_name`
- activation resolves content only from valid loaded catalog entries
- no arbitrary path input is accepted

### Risk: chat and run flows diverge

If activation registration and prompt injection are implemented differently in
chat and run flows, runtime behavior becomes inconsistent.

Mitigation:

- centralize helper functions for visible catalog building
- centralize helper functions for tool registration
- centralize helper functions for active prompt injection
- test both flows

## Summary

Phase 3 introduces the first true runtime skill behavior: activation.

The core architectural result is a clean separation between:

- valid discovered skills
- visible disclosed skills
- active runtime skills
- prompt-injected active skill content

The model can activate only valid visible skills and only through the
`activate_skill` tool. Activated skill content is stored in a dedicated runtime
registry, deduplicated by name, and injected into prompt assembly before
provider completion.

At the same time, `Conversation.messages` remains free of synthetic active skill
content, preserving cleaner history and setting up later phases for lifecycle
management, trust persistence, and resource-aware activation.
