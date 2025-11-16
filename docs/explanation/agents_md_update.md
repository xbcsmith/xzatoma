# AGENTS.md Update Implementation

## Overview

Updated the AGENTS.md file to reflect the current XZatoma architecture as documented in docs/reference/architecture.md. This ensures AI agents working on the codebase have accurate information about the project structure, modules, and design principles.

## Components Delivered

- `AGENTS.md` (1,290 lines) - Updated project identity, architecture, and examples

Total: ~1,290 lines updated

## Implementation Details

### Changes Made

#### 1. Project Identity Update

**Before:**
- Name: XZepr-MCP
- Type: MCP (Model Context Protocol) server
- Purpose: Protocol adapter for XZepr Event Tracking System

**After:**
- Name: XZatoma
- Type: Autonomous AI agent CLI
- Purpose: Execute tasks through conversation with AI providers using basic file system and terminal tools

#### 2. Architecture Section Rewrite

Updated the module structure from MCP server architecture to agent CLI architecture:

**Old Structure:**
```
src/
├── config/
├── client/
├── mcp/
├── models/
└── error.rs
```

**New Structure:**
```
src/
├── cli.rs
├── config.rs
├── agent/
├── providers/
├── tools/
└── error.rs
```

#### 3. Architecture Principles Update

Changed focus from "protocol translation layer" to "agent CLI with basic tools":

- Removed references to MCP protocol and XZepr API
- Added guidance on agent/provider/tool separation
- Updated warnings about over-engineering to reflect agent context
- Added note about keeping tools generic

#### 4. Module Responsibilities Table

Updated to reflect new component structure:

| Module       | Responsibility                 | Dependencies         |
| ------------ | ------------------------------ | -------------------- |
| `cli.rs`     | CLI parsing and user interface | clap                 |
| `config.rs`  | Configuration management       | serde                |
| `agent/`     | Autonomous execution loop      | providers, tools     |
| `providers/` | AI provider abstraction        | reqwest, async-trait |
| `tools/`     | File and terminal operations   | walkdir, similar     |
| `error.rs`   | Error types and conversions    | thiserror, anyhow    |

#### 5. Data Flow Diagram

Replaced MCP client/server flow with agent execution loop:

```
User Input → Agent → AI Provider (with tools) → Tool Call
                ↑                                    ↓
                └────────── Tool Result ─────────────┘
```

#### 6. Component Boundaries

Updated module dependency rules:

**New Rules:**
- `agent/` can call `providers/` and `tools/`
- `providers/` and `tools/` are independent
- No circular dependencies between layers

**Old Rules (removed):**
- `mcp/` can call `client/`
- MCP-specific boundary rules

#### 7. Code Examples Update

Changed all package references from `xzepr-mcp` to `xzatoma`:

```rust
// Before
use xzepr-mcp::math::factorial;
use xzepr-mcp::module::function;

// After
use xzatoma::math::factorial;
use xzatoma::module::function;
```

#### 8. Git Convention Examples

Updated branch names and commit message examples to use XZatoma context:

**Branch Names:**
```
pr-xzatoma-1234
pr-feature-5678
```

**Commit Messages:**
```
feat(tools): add file diff tool support (XZATOMA-4567)
feat(agent): simplify provider abstraction (XZATOMA-3456)
fix(api): handle edge case in event validation (XZATOMA-5678)
```

Changed commit message body examples to reflect agent features:
```
Implements diff generation using the similar crate.
Adds file comparison capabilities for plan execution.
```

## Testing

No code changes were made, only documentation updates. Verification performed by:

- Reviewing updated sections for accuracy against architecture.md
- Ensuring all XZepr-MCP references were updated to XZatoma
- Confirming module structure matches current codebase design
- Checking that examples use correct package names

## Validation Results

- ✅ All project identity references updated
- ✅ Architecture section matches docs/reference/architecture.md
- ✅ Module structure reflects CLI agent design
- ✅ Data flow diagrams updated for agent execution loop
- ✅ Code examples use `xzatoma` package name
- ✅ Git convention examples updated with relevant contexts
- ✅ Component boundaries reflect new architecture
- ✅ No XZepr-MCP references remaining in documentation

## Key Changes Summary

1. **Project Identity**: XZepr-MCP → XZatoma
2. **Architecture Type**: MCP server → Agent CLI
3. **Module Structure**: config/client/mcp/models → cli/agent/providers/tools
4. **Data Flow**: MCP protocol translation → Agent execution loop
5. **Dependencies**: mcp-sdk/reqwest → clap/tokio/async-trait
6. **Focus**: Protocol adapter → Autonomous agent with basic tools

## References

- Architecture: `docs/reference/architecture.md`
- Original file: `AGENTS.md` (lines 1-1290)
