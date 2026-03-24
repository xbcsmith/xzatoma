# Agent Skills Support Implementation Plan

## Overview

This plan adds Agent Skills support to XZatoma in a phased, implementation-ready
way that follows the Agent Skills specification and the client implementation
guidance while fitting the current XZatoma Rust CLI architecture.

The implementation uses the Agent Skills progressive disclosure model:

1. discover valid skills and load only metadata at session start
2. disclose the catalog to the model
3. activate a skill only through a dedicated `activate_skill` tool
4. load referenced skill resources only when needed
5. inject active skill content at prompt-construction time from a separate
   active-skill registry

This plan is written for AI-agent execution. It uses explicit file targets,
symbol targets, configuration keys, command names, precedence rules, and
machine-verifiable success criteria.

## Explicit First-Release Decisions

The following decisions are locked for the first release and MUST NOT be changed
during implementation:

| Decision Area                                    | Decision                                                            | Status       |
| ------------------------------------------------ | ------------------------------------------------------------------- | ------------ |
| Primary activation mechanism                     | Dedicated synthetic tool named `activate_skill`                     | REQUIRED     |
| Direct file-read activation                      | Not part of the supported activation contract                       | DEFERRED     |
| Invalid skills                                   | Invalid skills are never loaded and never disclosed                 | REQUIRED     |
| Invalid skill visibility                         | Invalid skills appear only in validation output and logs            | REQUIRED     |
| Built-in first-party skills                      | Not included in first release                                       | OUT OF SCOPE |
| Active skill persistence across resumed sessions | Not included in first release                                       | OUT OF SCOPE |
| `allowed-tools` behavior                         | Parse and surface as advisory metadata only                         | REQUIRED     |
| Active skill runtime model                       | Prompt-layer injected context from a separate active-skill registry | REQUIRED     |
| Trust model                                      | Include dedicated `skills trust` CLI subcommands                    | REQUIRED     |
| Discovery sources                                | Project-level, user-level, and config-defined custom paths only     | REQUIRED     |

## Recommended Implementation Order

1. Skill discovery and parsing foundation
2. Skill catalog disclosure and prompt integration
3. Skill activation and active-skill registry
4. Skill resource access, trust enforcement, and lifecycle management
5. Configuration, CLI, documentation, and hardening

---

## Current State Analysis

### Existing Infrastructure

| Area                 | File                                          | Symbol or Responsibility                     | Relevance                                                                   |
| -------------------- | --------------------------------------------- | -------------------------------------------- | --------------------------------------------------------------------------- |
| Tool registry        | `xzatoma/src/tools/mod.rs`                    | `ToolRegistry`, `ToolExecutor`, `ToolResult` | Needed to register `activate_skill`                                         |
| Mode-aware tools     | `xzatoma/src/tools/registry_builder.rs`       | `ToolRegistryBuilder`                        | Needed for mode-specific registration behavior                              |
| Chat entrypoint      | `xzatoma/src/commands/mod.rs`                 | `chat::run_chat`                             | Needed for session startup discovery, catalog disclosure, activation wiring |
| Chat tool builder    | `xzatoma/src/commands/mod.rs`                 | `chat::build_tools_for_mode`                 | Needed to inject skill-aware tool registration                              |
| Run entrypoint       | `xzatoma/src/commands/mod.rs`                 | `run::run_plan_with_options`                 | Needed for non-chat disclosure and activation support                       |
| Conversation state   | `xzatoma/src/agent/conversation.rs`           | `Conversation`                               | Relevant for pruning interactions and non-persistent session guidance       |
| Mention augmentation | `xzatoma/src/mention_parser.rs`               | `augment_prompt_with_mentions`               | Reference pattern for prompt augmentation                                   |
| Config root          | `xzatoma/src/config.rs`                       | `Config` and nested config structs           | Needed for `SkillsConfig`                                                   |
| Library wiring       | `xzatoma/src/lib.rs`                          | module exports and re-exports                | Needed for `pub mod skills;`                                                |
| CLI wiring           | `xzatoma/src/cli.rs`                          | `Commands` enum and subcommands              | Needed for `skills` commands                                                |
| Persistent storage   | `xzatoma/src/storage/mod.rs`                  | `SqliteStorage`                              | Explicitly NOT used for active-skill persistence in v1                      |
| Existing docs index  | `xzatoma/docs/explanation/implementations.md` | implementation tracking                      | Must be updated                                                             |
| Example config       | `xzatoma/config/config.yaml`                  | commented config examples                    | Must be updated                                                             |

### Identified Issues

| ID  | Issue                                                           | Impact                                                                    |
| --- | --------------------------------------------------------------- | ------------------------------------------------------------------------- |
| 1   | There is no `skills` module in the crate                        | No place to implement discovery, parsing, disclosure, or activation       |
| 2   | There is no `activate_skill` tool                               | The model has no supported activation path                                |
| 3   | There is no `SkillsConfig` in `Config`                          | No way to enable, tune, or trust-gate skills                              |
| 4   | There is no discovery implementation for `SKILL.md` directories | Skills cannot be found                                                    |
| 5   | There is no parser for Agent Skills frontmatter and body        | Skills cannot be validated or cataloged                                   |
| 6   | There is no active-skill registry                               | Activated guidance cannot be managed separately from conversation history |
| 7   | There is no prompt-layer skill injection                        | Activated skills cannot influence provider input reliably                 |
| 8   | There is no trust-state mechanism or trust CLI                  | Untrusted project skills cannot be controlled safely                      |
| 9   | There is no invalid-skill diagnostics surface                   | Broken skills cannot be validated cleanly                                 |
| 10  | There are no skills-specific tests or docs                      | Feature cannot be maintained safely                                       |

---

## Specification Alignment

### Supported Specification Behaviors

| Behavior                                             | First Release Support |
| ---------------------------------------------------- | --------------------- |
| Discover skill directories containing `SKILL.md`     | Yes                   |
| Parse YAML frontmatter and Markdown body             | Yes                   |
| Load only `name` and `description` at startup        | Yes                   |
| Progressive disclosure                               | Yes                   |
| Project-level skill scope                            | Yes                   |
| User-level skill scope                               | Yes                   |
| Custom configured paths                              | Yes                   |
| Deterministic collision handling                     | Yes                   |
| Skill activation tool                                | Yes                   |
| Lazy resource loading                                | Yes                   |
| Lenient parsing where safe                           | Yes                   |
| `allowed-tools` parsing                              | Yes                   |
| `allowed-tools` enforcement                          | No                    |
| Built-in bundled skills                              | No                    |
| Persistence of active skills across resumed sessions | No                    |

### Invalid Skill Policy

| Condition                                     | Behavior                                                                                                      |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| Missing `SKILL.md`                            | Ignore directory                                                                                              |
| Unparseable frontmatter with no safe fallback | Mark invalid, log diagnostic, do not load                                                                     |
| Missing `description`                         | Mark invalid, log diagnostic, do not load                                                                     |
| Empty `description`                           | Mark invalid, log diagnostic, do not load                                                                     |
| Name does not match directory                 | Mark invalid, log diagnostic, do not load                                                                     |
| Constraint violation on `name`                | Mark invalid, log diagnostic, do not load                                                                     |
| Constraint violation on optional fields       | Mark invalid if spec-invalid and required for parse correctness; otherwise log warning only during validation |

First release policy is intentionally strict on skill loading:

- **invalid skills are never loaded**
- **invalid skills are never disclosed**
- **invalid skills are only shown by validation workflows and logs**

---

## Explicit Runtime Contracts

### Discovery Paths

The implementation MUST support the following discovery roots in this order:

| Precedence Rank | Scope                       | Path Pattern                     | Enabled By                |
| --------------- | --------------------------- | -------------------------------- | ------------------------- |
| 1               | Project client-specific     | `<working_dir>/.xzatoma/skills/` | `skills.project_enabled`  |
| 2               | Project shared convention   | `<working_dir>/.agents/skills/`  | `skills.project_enabled`  |
| 3               | User client-specific        | `~/.xzatoma/skills/`             | `skills.user_enabled`     |
| 4               | User shared convention      | `~/.agents/skills/`              | `skills.user_enabled`     |
| 5               | Custom configured path 1..N | exact configured path order      | `skills.additional_paths` |

### Collision Precedence

When multiple valid skills share the same `name`, the winner MUST be selected by
the following deterministic rules:

1. lower precedence rank number wins
2. if same rank, earlier configured path wins
3. if same directory scope, lexicographically smaller absolute `SKILL.md` path wins
4. every shadowed skill is recorded as a diagnostic entry
5. only the winning skill is loaded into the catalog

### `SkillsConfig` Schema

Add `#[serde(default)] pub skills: SkillsConfig` to `Config` in
`xzatoma/src/config.rs`.

The first-release config schema MUST be:

| Field                              | Type             | Default | Description                                                        |
| ---------------------------------- | ---------------- | ------- | ------------------------------------------------------------------ |
| `enabled`                          | `bool`           | `true`  | Global skills feature flag                                         |
| `project_enabled`                  | `bool`           | `true`  | Enable project-level discovery                                     |
| `user_enabled`                     | `bool`           | `true`  | Enable user-level discovery                                        |
| `additional_paths`                 | `Vec<String>`    | `[]`    | Additional absolute or config-resolved paths                       |
| `max_discovered_skills`            | `usize`          | `256`   | Hard cap on valid loaded skills                                    |
| `max_scan_directories`             | `usize`          | `2000`  | Hard cap on directories visited                                    |
| `max_scan_depth`                   | `usize`          | `6`     | Max traversal depth per root                                       |
| `catalog_max_entries`              | `usize`          | `128`   | Max disclosed catalog entries                                      |
| `activation_tool_enabled`          | `bool`           | `true`  | Register `activate_skill` when skills are available                |
| `project_trust_required`           | `bool`           | `true`  | Require explicit trust for project-level skills                    |
| `trust_store_path`                 | `Option<String>` | `None`  | Optional override for trust-state file path                        |
| `allow_custom_paths_without_trust` | `bool`           | `false` | Whether custom paths bypass trust checks                           |
| `strict_frontmatter`               | `bool`           | `true`  | If true, invalid skills are rejected without lenient load fallback |

### Environment Variable Overrides

The plan MUST include env-var handling for the following keys:

| Environment Variable                     | Maps To                          |
| ---------------------------------------- | -------------------------------- |
| `XZATOMA_SKILLS_ENABLED`                 | `skills.enabled`                 |
| `XZATOMA_SKILLS_PROJECT_ENABLED`         | `skills.project_enabled`         |
| `XZATOMA_SKILLS_USER_ENABLED`            | `skills.user_enabled`            |
| `XZATOMA_SKILLS_ACTIVATION_TOOL_ENABLED` | `skills.activation_tool_enabled` |
| `XZATOMA_SKILLS_PROJECT_TRUST_REQUIRED`  | `skills.project_trust_required`  |
| `XZATOMA_SKILLS_TRUST_STORE_PATH`        | `skills.trust_store_path`        |

### CLI Command Tree

Add a new top-level command group in `xzatoma/src/cli.rs`:

| Command                              | Purpose                                                                 |
| ------------------------------------ | ----------------------------------------------------------------------- |
| `xzatoma skills list`                | List valid loaded skills only                                           |
| `xzatoma skills validate`            | Validate configured skill roots and show invalid skills and diagnostics |
| `xzatoma skills show <name>`         | Show metadata for one valid loaded skill                                |
| `xzatoma skills paths`               | Show effective discovery paths and trust state                          |
| `xzatoma skills trust show`          | Show trust configuration and trusted paths                              |
| `xzatoma skills trust add <path>`    | Mark a project path trusted                                             |
| `xzatoma skills trust remove <path>` | Remove trust for a project path                                         |

### Activation Tool Contract

The tool name MUST be `activate_skill`.

The tool MUST:

- only be registered when `skills.enabled == true`
- only be registered when `skills.activation_tool_enabled == true`
- only be registered when at least one valid skill exists
- expose only valid skill names as selectable inputs
- fail cleanly if asked to activate a missing or filtered skill
- deduplicate activation within the current session
- return body content plus metadata wrapper, not raw file contents

Tool behavior MUST be identical in chat mode and run mode, subject to skill
visibility rules.

### Active-Skill Runtime Model

First release MUST use a separate active-skill registry and MUST NOT store active
skill content in `Conversation.messages`.

The runtime model MUST be:

| Component              | Responsibility                                                    |
| ---------------------- | ----------------------------------------------------------------- |
| Skill catalog          | Map valid skill names to parsed metadata and file locations       |
| Active-skill registry  | Track which skills are active in the current process/session      |
| Prompt injection layer | Inject active skills into model input before provider completion  |
| Conversation history   | Remain unchanged except for ordinary user/assistant/tool messages |
| Persistent storage     | Not used for active-skill persistence in v1                       |

### Invalid Skill Visibility

| Surface                    | Show Invalid Skills? |
| -------------------------- | -------------------- |
| `skills list`              | No                   |
| `skills show <name>`       | No                   |
| startup catalog disclosure | No                   |
| `activate_skill` tool      | No                   |
| `skills validate`          | Yes                  |
| log output                 | Yes                  |

---

## Concrete Integration Targets

### Files to Create

| File                                  | Purpose                                    |
| ------------------------------------- | ------------------------------------------ |
| `xzatoma/src/skills/mod.rs`           | module wiring                              |
| `xzatoma/src/skills/types.rs`         | core data structures                       |
| `xzatoma/src/skills/discovery.rs`     | filesystem scan and precedence resolution  |
| `xzatoma/src/skills/parser.rs`        | `SKILL.md` parsing                         |
| `xzatoma/src/skills/validation.rs`    | validation and diagnostics                 |
| `xzatoma/src/skills/catalog.rs`       | valid skill catalog                        |
| `xzatoma/src/skills/disclosure.rs`    | startup disclosure rendering               |
| `xzatoma/src/skills/activation.rs`    | active-skill registry and activation logic |
| `xzatoma/src/skills/trust.rs`         | trust-state read/write and checks          |
| `xzatoma/src/tools/activate_skill.rs` | synthetic activation tool                  |
| `xzatoma/src/commands/skills.rs`      | CLI command handlers                       |

### Files to Edit

| File                                          | Required Changes                                                                                                                          |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `xzatoma/src/lib.rs`                          | add `pub mod skills;` and any required re-exports                                                                                         |
| `xzatoma/src/config.rs`                       | add `SkillsConfig`, defaults, validation, env overrides                                                                                   |
| `xzatoma/src/cli.rs`                          | add `Skills` top-level command and trust subcommands                                                                                      |
| `xzatoma/src/commands/mod.rs`                 | add `pub mod skills;`, wire command dispatch, integrate skill discovery/disclosure into `chat::run_chat` and `run::run_plan_with_options` |
| `xzatoma/src/tools/mod.rs`                    | add `pub mod activate_skill;` and any tool exports                                                                                        |
| `xzatoma/src/tools/registry_builder.rs`       | define where activation tool registration occurs or explicitly defer to command-layer registration                                        |
| `xzatoma/src/agent/conversation.rs`           | no active-skill persistence; only adjust comments or interfaces if prompt assembly needs session-aware hooks                              |
| `xzatoma/src/mention_parser.rs`               | no direct skills parsing; may add comments or helper reuse only if needed                                                                 |
| `xzatoma/config/config.yaml`                  | add commented skills config examples                                                                                                      |
| `xzatoma/docs/explanation/implementations.md` | append implementation tracking entry                                                                                                      |

### Symbol-Level Targets

| File                                  | Symbol                          | Action                                                             |
| ------------------------------------- | ------------------------------- | ------------------------------------------------------------------ |
| `xzatoma/src/config.rs`               | `Config`                        | add `skills: SkillsConfig`                                         |
| `xzatoma/src/config.rs`               | `SkillsConfig`                  | create                                                             |
| `xzatoma/src/cli.rs`                  | `Commands`                      | add `Skills` variant                                               |
| `xzatoma/src/commands/mod.rs`         | `chat::run_chat`                | add startup skill discovery, disclosure, activation runtime wiring |
| `xzatoma/src/commands/mod.rs`         | `run::run_plan_with_options`    | add startup skill discovery, disclosure, activation runtime wiring |
| `xzatoma/src/tools/mod.rs`            | `ToolRegistry` usage sites      | register `activate_skill` when enabled                             |
| `xzatoma/src/tools/activate_skill.rs` | `ActivateSkillTool`             | create                                                             |
| `xzatoma/src/skills/activation.rs`    | `ActiveSkillRegistry`           | create                                                             |
| `xzatoma/src/skills/catalog.rs`       | `SkillCatalog`                  | create                                                             |
| `xzatoma/src/skills/trust.rs`         | trust store structs and helpers | create                                                             |
| `xzatoma/src/commands/skills.rs`      | CLI handlers                    | create                                                             |

---

## Implementation Phases

### Phase 1: Skill Discovery and Parsing Foundation

#### Task 1.1 Foundation Work

Create the `skills` module tree and wire it into the crate.

**Create files:**

- `xzatoma/src/skills/mod.rs`
- `xzatoma/src/skills/types.rs`
- `xzatoma/src/skills/discovery.rs`
- `xzatoma/src/skills/parser.rs`
- `xzatoma/src/skills/validation.rs`
- `xzatoma/src/skills/catalog.rs`

**Edit files:**

- `xzatoma/src/lib.rs`

**Required symbols:**

- `pub mod skills;`
- `SkillSourceScope`
- `SkillMetadata`
- `SkillRecord`
- `SkillDiagnostic`
- `SkillCatalog`

**Required type definitions:**

- distinguish valid loaded skills from invalid discovered skills
- separate startup metadata from full activation content
- include absolute paths and source scope explicitly

#### Task 1.2 Add Foundation Functionality

Implement deterministic discovery for valid and invalid skills.

**Required file target:**

- `xzatoma/src/skills/discovery.rs`

**Discovery requirements:**

- scan only configured and supported roots
- visit at most `max_scan_directories`
- recurse at most `max_scan_depth`
- ignore directories without `SKILL.md`
- record every invalid skill candidate as a diagnostic
- stop loading additional valid skills after `max_discovered_skills`
- continue collecting invalid diagnostics after valid-skill cap only if cheap and bounded

**Required outputs:**

- collection of valid skills
- collection of invalid diagnostics
- collection of shadowed-skill diagnostics

#### Task 1.3 Integrate Foundation Work

Implement parsing and validation for `SKILL.md`.

**Required file targets:**

- `xzatoma/src/skills/parser.rs`
- `xzatoma/src/skills/validation.rs`

**Required parse fields:**

- `name`
- `description`
- `license`
- `compatibility`
- `metadata`
- `allowed-tools`
- Markdown body

**Validation requirements:**

- reject missing `description`
- reject empty `description`
- reject name mismatch with parent directory
- reject name values violating spec format
- parse `allowed-tools` as a raw string plus normalized vector
- never promote invalid skills into the loaded catalog

#### Task 1.4 Testing Requirements

Create test files:

- `xzatoma/tests/skills_discovery.rs`
- `xzatoma/tests/skills_parser.rs`

Add tests:

- valid project skill discovery
- valid user skill discovery
- custom path discovery
- collision precedence
- invalid `name`
- invalid `description`
- missing frontmatter
- malformed frontmatter
- shadowed valid skill diagnostics

#### Task 1.5 Deliverables

| Deliverable                                  | Verification                |
| -------------------------------------------- | --------------------------- |
| `xzatoma/src/skills/` module exists          | file exists                 |
| `xzatoma/src/lib.rs` exports `skills` module | symbol present              |
| valid skill discovery implemented            | discovery tests pass        |
| invalid skills excluded from loaded catalog  | parser/discovery tests pass |
| diagnostics model implemented                | validation tests pass       |

#### Task 1.6 Success Criteria

The phase is complete only if all of the following are true:

1. `xzatoma/src/skills/mod.rs` exists and is wired in `xzatoma/src/lib.rs`
2. valid skills can be discovered from project, user, and custom paths
3. invalid skills are diagnosed and excluded from the loaded catalog
4. collision precedence is deterministic and test-covered
5. `cargo test --all-features` passes with the new skills discovery/parser tests

---

### Phase 2: Skill Catalog Disclosure and Prompt Integration

#### Task 2.1 Feature Work

Create disclosure rendering and startup catalog generation.

**Create file:**

- `xzatoma/src/skills/disclosure.rs`

**Required symbols:**

- `render_skill_catalog`
- `build_skill_disclosure_section`

**Catalog content requirements:**

- include `name`
- include `description`
- include skill directory or `SKILL.md` location if needed for runtime messaging
- do not include invalid skills
- do not include full instruction bodies

#### Task 2.2 Integrate Feature

Integrate skill catalog disclosure into prompt construction paths.

**Edit file:**

- `xzatoma/src/commands/mod.rs`

**Required symbol targets:**

- `chat::run_chat`
- `run::run_plan_with_options`

**Required runtime behavior:**

- discover skills at session startup
- filter by trust and config
- build catalog disclosure
- inject disclosure into the prompt context before the first provider call
- omit disclosure completely if no valid visible skills exist

#### Task 2.3 Configuration Updates

Define visibility rules and trust gating for disclosure.

**Edit files:**

- `xzatoma/src/config.rs`
- `xzatoma/src/skills/trust.rs` if already created early, otherwise Phase 4

**Disclosure visibility rules:**

- project skills require trust when `skills.project_trust_required == true`
- user skills do not require project trust
- custom paths require trust unless `skills.allow_custom_paths_without_trust == true`
- invalid skills are never visible
- catalog entry count must not exceed `skills.catalog_max_entries`

#### Task 2.4 Testing Requirements

Create test file:

- `xzatoma/tests/skills_disclosure.rs`

Add tests:

- disclosure contains only valid visible skills
- untrusted project skills are omitted
- invalid skills are omitted
- empty catalog yields no disclosure block
- disclosed catalog ordering is deterministic
- catalog entry count obeys `catalog_max_entries`

#### Task 2.5 Deliverables

| Deliverable                     | Verification                                        |
| ------------------------------- | --------------------------------------------------- |
| disclosure renderer implemented | test file passes                                    |
| chat startup integrates catalog | integration test or prompt-construction test passes |
| run startup integrates catalog  | integration test or prompt-construction test passes |
| trust-gated omission works      | disclosure tests pass                               |

#### Task 2.6 Success Criteria

1. both chat and run flows disclose only valid visible skills
2. no full `SKILL.md` bodies are loaded at startup
3. invalid skills never appear in disclosed output
4. trust rules are enforced at disclosure time
5. disclosure tests pass in CI-quality test runs

---

### Phase 3: Skill Activation and Active-Skill Registry

#### Task 3.1 Foundation Work

Create activation runtime and dedicated tool.

**Create files:**

- `xzatoma/src/skills/activation.rs`
- `xzatoma/src/tools/activate_skill.rs`

**Required symbols:**

- `ActiveSkillRegistry`
- `ActiveSkill`
- `ActivateSkillTool`

**Registry requirements:**

- track active skills by name
- deduplicate re-activation
- resolve skill body on activation
- store skill directory and advisory metadata
- produce prompt-injection-ready content

#### Task 3.2 Add Foundation Functionality

Implement `activate_skill` as the only supported activation path.

**Tool contract requirements:**

- tool name is exactly `activate_skill`
- parameter schema contains exactly one required skill name input
- skill name input is restricted to valid loaded skill names
- activation returns structured wrapped content suitable for prompt-layer injection
- activation fails on invalid, hidden, or missing skills
- activation never reads arbitrary file paths from model input

**Structured activation output MUST include:**

- skill name
- skill directory path
- normalized advisory `allowed-tools`
- body content
- optional enumerated resource list

#### Task 3.3 Integrate Foundation Work

Register and use `activate_skill` in runtime flows.

**Edit files:**

- `xzatoma/src/tools/mod.rs`
- `xzatoma/src/commands/mod.rs`
- optionally `xzatoma/src/tools/registry_builder.rs` if the registration is centralized there

**Required runtime behavior:**

- register `activate_skill` only if valid visible skills exist
- register it only when `skills.activation_tool_enabled == true`
- share the active-skill registry across the session lifetime
- do not store active skills in `Conversation.messages`
- update prompt assembly to inject currently active skills before provider completion

#### Task 3.4 Testing Requirements

Create test files:

- `xzatoma/tests/skills_activation.rs`
- `xzatoma/tests/skills_tool.rs`

Add tests:

- activation succeeds for valid visible skill
- duplicate activation does not duplicate registry state
- activation fails for missing skill
- activation fails for invalid skill
- activation fails for untrusted project skill
- tool not registered when there are no valid visible skills
- tool schema only lists valid visible skill names
- prompt-layer injection includes active skill content after activation

#### Task 3.5 Deliverables

| Deliverable                                 | Verification            |
| ------------------------------------------- | ----------------------- |
| `activate_skill` tool implemented           | tool tests pass         |
| active-skill registry implemented           | activation tests pass   |
| activation deduplication implemented        | activation tests pass   |
| prompt-layer injection uses active registry | integration test passes |

#### Task 3.6 Success Criteria

1. the model can activate a valid skill only through `activate_skill`
2. duplicate activation does not create duplicate active-skill entries
3. active skill content is injected during prompt assembly
4. `Conversation.messages` remains free of synthetic skill content
5. activation and tool registration tests pass

---

### Phase 4: Skill Resource Access, Trust Enforcement, and Lifecycle Management

#### Task 4.1 Foundation Work

Create trust-state and resource-resolution support.

**Create file:**

- `xzatoma/src/skills/trust.rs`

**Required trust features:**

- persistent trusted-path store
- add/remove/show operations
- canonicalized path handling
- path-prefix trust checks for project-level skills

**Required resource features:**

- resolve relative paths from skill root only
- enumerate files under `scripts/`, `references/`, and `assets/`
- prevent path traversal outside skill root

#### Task 4.2 Add Foundation Functionality

Implement trust enforcement and advisory `allowed-tools` handling.

**Required behavior:**

- project skills are visible only when trusted if `project_trust_required == true`
- custom paths are trusted only if explicitly allowed or trusted
- user-level skill roots are considered trusted by default for this release
- parse `allowed-tools` and surface it in activation metadata
- do not enforce `allowed-tools` against the tool registry in v1

#### Task 4.3 Integrate Foundation Work

Integrate trust operations and active-skill prompt injection lifecycle.

**Edit files:**

- `xzatoma/src/cli.rs`
- `xzatoma/src/commands/skills.rs`
- `xzatoma/src/commands/mod.rs`

**Required lifecycle behavior:**

- active skills exist only for the current process/session
- active skills are lost on process exit by design in v1
- resumed conversations do not restore active skills
- prompt assembly must inject active skills on every provider call during the session
- resource access remains lazy and outside activation unless the model explicitly reads referenced files later

#### Task 4.4 Testing Requirements

Create test files:

- `xzatoma/tests/skills_trust.rs`
- `xzatoma/tests/skills_resources.rs`
- `xzatoma/tests/skills_lifecycle.rs`

Add tests:

- trust add/show/remove
- untrusted project path omission
- trusted project path inclusion
- custom path trust behavior
- path traversal rejection
- resource enumeration stays within skill root
- active skills do not persist across restarted runtime instances
- prompt injection repeats correctly across multiple turns in one session

#### Task 4.5 Deliverables

| Deliverable                      | Verification         |
| -------------------------------- | -------------------- |
| trust store implemented          | trust tests pass     |
| trust CLI backend implemented    | command tests pass   |
| resource resolution implemented  | resource tests pass  |
| no persistence for active skills | lifecycle tests pass |

#### Task 4.6 Success Criteria

1. project-level trust is enforced consistently
2. trust CLI operations work against a persistent store
3. skill resources cannot escape the skill root
4. active skills remain session-local only
5. trust, resource, and lifecycle tests pass

---

### Phase 5: Configuration, CLI, Documentation, and Hardening

#### Task 5.1 Feature Work

Add full configuration and CLI command surface.

**Create file:**

- `xzatoma/src/commands/skills.rs`

**Edit files:**

- `xzatoma/src/config.rs`
- `xzatoma/src/cli.rs`
- `xzatoma/src/commands/mod.rs`
- `xzatoma/config/config.yaml`

**Required CLI commands:**

- `skills list`
- `skills validate`
- `skills show <name>`
- `skills paths`
- `skills trust show`
- `skills trust add <path>`
- `skills trust remove <path>`

**Required config documentation additions:**

- all `SkillsConfig` fields in commented YAML examples
- trust-store configuration example
- custom-path configuration example

#### Task 5.2 Integrate Feature

Add operational and validation behavior.

**Required command behavior:**

- `skills list` shows only valid visible skills
- `skills validate` shows valid skills, invalid skills, and diagnostics
- `skills show <name>` errors on missing or hidden skill
- `skills paths` prints effective roots and trust status
- `skills trust show` prints trusted paths
- `skills trust add/remove` updates trust store deterministically

#### Task 5.3 Configuration Updates

Add validation, doc comments, and implementation tracking.

**Required edits:**

- add `///` doc comments to every new public module, struct, enum, and function
- update `xzatoma/docs/explanation/implementations.md`
- create documentation files:
  - `xzatoma/docs/explanation/agent_skills_implementation.md`
  - `xzatoma/docs/how-to/use_agent_skills.md`
  - `xzatoma/docs/how-to/create_project_skills_for_xzatoma.md`
  - `xzatoma/docs/reference/agent_skills_configuration.md`
  - `xzatoma/docs/reference/agent_skills_cli.md`
  - `xzatoma/docs/reference/agent_skills_behavior.md`

#### Task 5.4 Testing Requirements

Create test files:

- `xzatoma/tests/skills_cli.rs`
- `xzatoma/tests/skills_config.rs`

Add tests:

- config defaulting
- config deserialization
- env var override behavior
- `skills list` output
- `skills validate` output
- `skills show` output
- `skills paths` output
- `skills trust show/add/remove` output

Run required quality gates in this exact order:

1. `cargo fmt --all`
2. `cargo check --all-targets --all-features`
3. `cargo clippy --all-targets --all-features -- -D warnings`
4. `cargo test --all-features`

Run Markdown quality steps for each new Markdown file:

1. `markdownlint --fix --config .markdownlint.json "${FILE}"`
2. `prettier --write --parser markdown --prose-wrap always "${FILE}"`

#### Task 5.5 Deliverables

| Deliverable                                   | Verification                            |
| --------------------------------------------- | --------------------------------------- |
| `SkillsConfig` implemented and documented     | config tests pass                       |
| CLI command surface implemented               | CLI tests pass                          |
| trust subcommands implemented                 | CLI tests pass                          |
| docs created in Diataxis locations            | files exist                             |
| `config/config.yaml` updated                  | file contains commented skills examples |
| `docs/explanation/implementations.md` updated | file contains Agent Skills entry        |
| all quality gates pass                        | commands succeed                        |

#### Task 5.6 Success Criteria

The full feature is complete only if all of the following are true:

1. valid skills can be discovered, disclosed, activated, and used in both chat and run flows
2. invalid skills are never loaded or disclosed
3. activation is only available through `activate_skill`
4. active skill content is injected from a separate session-local registry
5. project-level trust is enforced and controllable via CLI
6. `allowed-tools` is parsed and surfaced as advisory metadata
7. CLI commands behave as specified
8. required docs exist and pass formatting/linting
9. all Rust quality gates pass

---

## Required Test Inventory

| Test File                            | Coverage                                          |
| ------------------------------------ | ------------------------------------------------- |
| `xzatoma/tests/skills_discovery.rs`  | discovery roots, precedence, caps                 |
| `xzatoma/tests/skills_parser.rs`     | frontmatter/body parsing, invalid-skill rejection |
| `xzatoma/tests/skills_disclosure.rs` | startup catalog rendering and filtering           |
| `xzatoma/tests/skills_activation.rs` | registry activation and deduplication             |
| `xzatoma/tests/skills_tool.rs`       | `activate_skill` schema and runtime behavior      |
| `xzatoma/tests/skills_trust.rs`      | trust store and trust gating                      |
| `xzatoma/tests/skills_resources.rs`  | root-safe resource resolution                     |
| `xzatoma/tests/skills_lifecycle.rs`  | session-local active skill behavior               |
| `xzatoma/tests/skills_cli.rs`        | top-level skills CLI commands                     |
| `xzatoma/tests/skills_config.rs`     | config defaults, parsing, env overrides           |

---

## Required Documentation Outputs

| File                                                       | Purpose                            |
| ---------------------------------------------------------- | ---------------------------------- |
| `xzatoma/docs/explanation/agent_skills_implementation.md`  | mandatory implementation summary   |
| `xzatoma/docs/how-to/use_agent_skills.md`                  | end-user workflow                  |
| `xzatoma/docs/how-to/create_project_skills_for_xzatoma.md` | repository/project authoring guide |
| `xzatoma/docs/reference/agent_skills_configuration.md`     | configuration reference            |
| `xzatoma/docs/reference/agent_skills_cli.md`               | CLI reference                      |
| `xzatoma/docs/reference/agent_skills_behavior.md`          | runtime behavior reference         |
| `xzatoma/docs/explanation/implementations.md`              | implementation tracking update     |

---

## Deferred Work

The following items are explicitly deferred and MUST NOT be included in the
first implementation unless a follow-up plan is created:

| Deferred Item                                                | Reason                       |
| ------------------------------------------------------------ | ---------------------------- |
| Built-in bundled first-party skills                          | Out of scope by decision     |
| Persistent active-skill restoration on resumed sessions      | Out of scope by decision     |
| File-read-based activation semantics as a supported contract | Out of scope by decision     |
| Enforcement of `allowed-tools` against tool execution        | Deferred to later phase/plan |
| Remote registry or cloud skill sources                       | Out of scope by decision     |
| Automatic activation heuristics outside model tool use       | Deferred                     |

---

## Risks and Mitigations

| Risk                                          | Mitigation                                                  |
| --------------------------------------------- | ----------------------------------------------------------- |
| Prompt bloat from too many valid skills       | Cap loaded and disclosed skills with explicit config values |
| Untrusted project skills influence the model  | Require trust and expose trust CLI                          |
| Duplicate activation causes repeated guidance | Deduplicate in `ActiveSkillRegistry`                        |
| Invalid skills leak into model context        | Never load invalid skills into the catalog                  |
| Resource references escape the skill root     | Canonicalize and reject out-of-root paths                   |
| Session behavior becomes confusing on resume  | Explicitly document non-persistence for v1                  |

---

## Final Recommended Implementation Order

1. Phase 1: Skill discovery and parsing foundation
2. Phase 2: Skill catalog disclosure and prompt integration
3. Phase 3: Skill activation and active-skill registry
4. Phase 4: Skill resource access, trust enforcement, and lifecycle management
5. Phase 5: Configuration, CLI, documentation, and hardening
