# Quick Reference: Implementation Planning Session

## Status: Ready to Build 

**Architecture**: APPROVED (9/10 score)
**Validation**: COMPLETE
**Next Step**: Create phased implementation plan

## Essential Reading (In Order)

1. **`docs/reference/architecture.md`** (1,114 lines) - THE SOURCE OF TRUTH
2. **`AGENTS.md`** - Development rules (MUST FOLLOW)
3. **`PLAN.md`** - Planning methodology (USE THIS FORMAT)
4. **`docs/explanation/notes_for_implementation_planning.md`** - Detailed context

## Key Facts

**Target**: Simplest autonomous AI agent for CLI automation
**Code Size**: 3,000-5,000 lines total (NOT including tests)
**Test Coverage**: >80% mandatory
**Timeline**: 10-12 weeks realistic estimate
**Niche**: DevOps, CI/CD, server environments

## Architecture at a Glance

```
CLI Layer → Agent Core → Provider Abstraction → Basic Tools
        ↓
     Conversation Management (token pruning)
        ↓
     Command Validation (security)
```

## Critical Components (MUST IMPLEMENT)

1. **Iteration Limits**: `max_iterations` enforced in Agent loop
2. **Terminal Security**: Command denylist, path validation, execution modes
3. **Token Management**: Automatic pruning when approaching limits
4. **Structured Results**: `ToolResult` struct (not plain strings)
5. **Configuration Precedence**: CLI > Env > File > Defaults

## Module Structure

```
src/
├── error.rs      # ~100 lines - XzatomaError enum
├── config.rs     # ~200 lines - Settings with precedence
├── cli.rs       # ~150 lines - clap parser
├── agent/
│  ├── agent.rs    # ~300 lines - Execution loop
│  ├── conversation.rs # ~300 lines - Token management
│  └── executor.rs  # ~200 lines - Tool registry
├── providers/
│  ├── base.rs    # ~100 lines - Provider trait
│  ├── copilot.rs   # ~250 lines - Copilot client
│  └── ollama.rs   # ~250 lines - Ollama client
└── tools/
  ├── file_ops.rs  # ~300 lines - File operations
  ├── terminal.rs  # ~250 lines - Command execution
  └── plan.rs    # ~150 lines - Plan parsing
```

**Total**: ~2,500 lines core + ~1,500 lines tests = ~4,000 lines

## Recommended Phases

**Phase 1**: Foundation (error types, config, CLI skeleton)
**Phase 2**: Core Agent (execution loop, conversation, tool executor)
**Phase 3**: Providers (Copilot, Ollama, auth)
**Phase 4**: Tools (file ops, terminal, plan parser)
**Phase 5**: Integration (E2E tests, docs, examples)
**Phase 6**: Polish (security audit, release prep)

## Security Requirements (NON-NEGOTIABLE)

**Command Denylist**:
- `rm -rf /`, `dd if=/dev/zero`, `mkfs.*`
- Fork bombs, `sudo`, `curl | sh`

**Path Validation**:
- Working directory only
- No `..` traversal beyond root
- Validate symlink targets

**Execution Modes**:
- Interactive (confirm each)
- Restricted Autonomous (allowlist only)
- Full Autonomous (requires `--allow-dangerous`)

## Design Principles (DO NOT VIOLATE)

1. Keep it simple (<5k lines)
2. Security first (validate everything)
3. Generic tools (no specialized features)
4. CLI native (no GUI)
5. Local LLM friendly (Ollama first-class)
6. Autonomous ready (unattended operation)

## Success Criteria for Implementation Plan

- [ ] Every phase has clear, actionable tasks
- [ ] Dependencies explicit between phases
- [ ] Testing defined at every step
- [ ] Code size estimates total <5k lines
- [ ] Security addressed in relevant phases
- [ ] MVP achievable in Phases 1-3
- [ ] Each phase produces working deliverables
- [ ] Documentation requirements specified

## File to Create

**Path**: `docs/explanation/implementation_plan.md`

**Format**: Follow PLAN.md template exactly:
- Overview section
- Current State Analysis
- Implementation Phases (1-6)
 - Each phase: Overview, Tasks, Testing, Deliverables, Success Criteria

## Dependencies (Estimated)

```toml
clap = "4.5"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
reqwest = { version = "0.11", features = ["json"] }
async-trait = "0.1"
keyring = "2"
walkdir = "2"
similar = "2"
```

## What NOT to Do

- Don't add features because "Goose has them"
- Don't over-engineer (simple > clever)
- Don't skip security validation
- Don't ignore token limits
- Don't defer tests to later
- Don't write docs after code (write together)

## Positioning

**Not competing with**: Goose (feature-rich), Zed Agent (editor-integrated)
**Competing on**: Simplicity, CLI-native, local LLM, auditability
**Target users**: DevOps, CI/CD, server admins, privacy-conscious

## Remember

You're building the **simplest** autonomous agent that's **secure** and **effective**.

The architecture is solid. Now make it real.

**Start with**: Read architecture.md completely, then create the plan.

---

**Created**: 2025-01-15
**Status**: Ready for implementation planning
**Next Session**: Create phased implementation plan
