# AGENTS.md - AI Agent Development Guidelines

**CRITICAL**: This file contains mandatory rules for AI agents working on XZatoma.
Non-compliance will result in rejected code.

---

## Quick Reference for AI Agents

### BEFORE YOU START ANY TASK

**YOU MUST verify these are installed:**

```bash
rustup component add clippy rustfmt
cargo install cargo-audit  # Optional but recommended
```

**YOU MUST run these commands and ALL MUST PASS:**

```bash
# 1. Format code
cargo fmt --all

# 2. Check compilation
cargo check --all-targets --all-features

# 3. Lint with zero warnings
cargo clippy --all-targets --all-features -- -D warnings

# 4. Run all tests (must achieve >80% coverage)
cargo test --all-features
```

**Expected Output**: All commands complete successfully with zero errors and
zero warnings.

### AFTER YOU COMPLETE ANY TASK

**YOU MUST verify:**

- [ ] `cargo fmt --all` applied successfully
- [ ] `cargo check --all-targets --all-features` passes with zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero
      warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] Documentation file created in `docs/explanation/` with
      lowercase_filename.md

**IF ANY CHECK FAILS, YOU MUST FIX IT BEFORE PROCEEDING.**

---

## CRITICAL RULES - NEVER VIOLATE

### Rule 1: File Extensions (MOST VIOLATED)

**YOU MUST:**

- Use `.yaml` extension for ALL YAML files
- Use `.md` extension for ALL Markdown files
- Use `.rs` extension for ALL Rust files

**NEVER:**

- ❌ Use `.yml` extension (even though common in industry)
- ❌ Use `.MD` or `.markdown` extensions

**Examples:**

```text
✅ CORRECT:
   config/production.yaml
   config/development.yaml
   docker-compose.yaml

❌ WRONG:
   config/production.yml
   config/development.yml
   docker-compose.yml
```

**Why This Matters**: CI/CD pipelines expect `.yaml`. Using `.yml` will cause
build failures.

### Rule 2: Markdown File Naming (SECOND MOST VIOLATED)

**YOU MUST:**

- Use lowercase letters ONLY
- Use underscores to separate words
- Exception: `README.md` is the ONLY uppercase filename allowed

**NEVER:**

- ❌ Use CamelCase (DistributedTracing.md)
- ❌ Use kebab-case (distributed-tracing.md)
- ❌ Use spaces (Distributed Tracing.md)
- ❌ Use uppercase (DISTRIBUTED_TRACING.md)

**Examples:**

```text
✅ CORRECT:
   docs/explanation/distributed_tracing_architecture.md
   docs/how-to/setup_monitoring.md
   docs/reference/api_specification.md
   README.md (ONLY exception)

❌ WRONG:
   docs/explanation/Distributed-Tracing-Architecture.md
   docs/explanation/DistributedTracingArchitecture.md
   docs/explanation/ARCHITECTURE.md
   docs/how-to/setup-monitoring.md
   docs/how-to/Setup Monitoring.md
```

**Why This Matters**: Inconsistent naming breaks documentation linking and makes
files hard to find.

### Rule 3: No Emojis Anywhere (THIRD MOST VIOLATED)

**YOU MUST:**

- Write ALL documentation without emojis
- Write ALL code comments without emojis
- Write ALL commit messages without emojis

**NEVER:**

- ❌ Use emojis in code: `// ✅ This function works`
- ❌ Use emojis in docs: `## Setup Guide 🚀`
- ❌ Use emojis in commits: `feat: add auth ✨`

**ONLY EXCEPTION**: This AGENTS.md file uses emojis for visual markers to help
you follow rules.

**Why This Matters**: Emojis cause encoding issues and make documentation
unprofessional.

### Rule 4: Code Quality Gates (MUST ALL PASS)

**YOU MUST ensure ALL of these pass before claiming task complete:**

```bash
# Run in this exact order:

# 1. Format (auto-fixes issues)
cargo fmt --all

# 2. Compile check (fast, no binary)
cargo check --all-targets --all-features

# 3. Lint (treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings

# 4. Tests (must have >80% coverage)
cargo test --all-features
```

**Expected Results:**

```text
✅ cargo fmt         → No output (all files formatted)
✅ cargo check       → "Finished" with 0 errors
✅ cargo clippy      → "Finished" with 0 warnings
✅ cargo test        → "test result: ok. X passed; 0 failed"
```

**IF ANY FAIL**: Stop immediately and fix before proceeding.

### Rule 5: Documentation is Mandatory

**YOU MUST:**

- Create documentation file in `docs/explanation/` for EVERY feature/task
- Use filename pattern: `{feature_name}_implementation.md` or
  `{phase}_summary.md`
- Include: Overview, Components, Implementation Details, Testing, Examples
- Add `///` doc comments to EVERY public function, struct, enum, module
- Include runnable examples in doc comments (tested by `cargo test`)

**NEVER:**

- ❌ Skip documentation because "code is self-documenting"
- ❌ Put documentation in wrong directory
- ❌ Forget to specify language in code blocks

**Examples:**

````rust
/// Calculates the factorial of a number
///
/// # Arguments
///
/// * `n` - The number to calculate factorial for (must be ≤ 20)
///
/// # Returns
///
/// Returns the factorial as u64
///
/// # Errors
///
/// Returns `MathError::Overflow` if n > 20
///
/// # Examples
///
/// ```
/// use xzatoma::math::factorial;
///
/// let result = factorial(5);
/// assert_eq!(result, 120);
/// ```
///
/// # Panics
///
/// Panics if n is negative (though type system prevents this)
pub fn factorial(n: u64) -> Result<u64, MathError> {
    // Implementation
}
````

**Documentation File Structure:**

`````markdown
# Feature Name Implementation

## Overview

Brief description of what was implemented

## Components Delivered

- File 1: Description (X lines)
- File 2: Description (Y lines)

## Implementation Details

Technical explanation with code examples

## Testing

Test coverage and validation results

## Usage Examples

```rust
// Complete, runnable examples
```

## References

- Link to architecture docs
- Link to related features

---

## Project Overview

### Identity

- **Name**: XZatoma
- **Type**: Autonomous AI agent CLI
- **Purpose**: Execute tasks through conversation with AI providers using basic file system and terminal tools
- **Language**: Rust (latest stable)
- **Key Features**: Interactive chat mode, structured plan execution, GitHub Copilot and Ollama provider support

### Architecture (Simple Modular Design)

**XZatoma is an autonomous agent CLI with a simple, focused architecture.**

**CRITICAL**: XZatoma is intentionally simple - an agent with conversation loop, AI provider abstraction, and basic tools. Do NOT over-engineer with unnecessary abstraction layers.

#### Module Structure

```text
src/
├── main.rs              # Entry point
├── lib.rs               # Library root
├── cli.rs               # CLI parser
├── config.rs            # Configuration
├── error.rs             # Error types
│
├── agent/               # Agent core
│   ├── mod.rs
│   ├── agent.rs         # Main agent logic
│   ├── conversation.rs  # Message history
│   └── executor.rs      # Tool execution
│
├── providers/           # AI providers
│   ├── mod.rs
│   ├── base.rs          # Provider trait
│   ├── copilot.rs       # GitHub Copilot
│   └── ollama.rs        # Ollama
│
└── tools/               # Basic tools
    ├── mod.rs
    ├── file_ops.rs      # File operations
    ├── terminal.rs      # Terminal execution
    └── plan.rs          # Plan parsing
```

#### Architecture Principles

**YOU MUST:**

- Keep it simple - this is an agent CLI with basic tools
- Separate concerns by technical responsibility (CLI, agent, providers, tools)
- Avoid unnecessary abstraction layers
- Make the structure obvious and easy to navigate

**NEVER:**

- ❌ Add domain/application/infrastructure layers (overkill for agent tools)
- ❌ Create complex inheritance hierarchies
- ❌ Abstract prematurely - wait until you have 3 examples before abstracting
- ❌ Mix agent logic with provider-specific code
- ❌ Build specialized tools - keep tools generic (file ops, terminal)

#### Module Responsibilities

| Module       | Responsibility                 | Dependencies         |
| ------------ | ------------------------------ | -------------------- |
| `cli.rs`     | CLI parsing and user interface | clap                 |
| `config.rs`  | Configuration management       | serde                |
| `agent/`     | Autonomous execution loop      | providers, tools     |
| `providers/` | AI provider abstraction        | reqwest, async-trait |
| `tools/`     | File and terminal operations   | walkdir, similar     |
| `error.rs`   | Error types and conversions    | thiserror, anyhow    |

#### Data Flow

```text
┌─────────────────────────────────────────────────────┐
│  User (CLI input: interactive or plan file)         │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────┐
│  Agent - Autonomous execution loop                  │
│  (conversation management + tool execution)         │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────┐
│  Provider - AI completion with tool calls           │
│  (Copilot or Ollama)                                │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼
┌─────────────────────────────────────────────────────┐
│  Tools - Execute requested operations               │
│  (file_ops, terminal, plan parsing)                 │
└─────────────────┬───────────────────────────────────┘
                  │
                  ▼ (Results flow back to Agent → loop)
```

#### Component Boundaries

**YOU MUST respect these boundaries:**

✅ `agent/` modules can call `providers/` modules
✅ `agent/` modules can call `tools/` modules
✅ `agent/` modules can call `config.rs`
✅ `providers/` modules can call `config.rs`
✅ `tools/` modules are independent (no cross-dependencies)
✅ All modules can use `error.rs`

❌ `providers/` NEVER imports from `agent/` or `tools/`
❌ `tools/` NEVER imports from `agent/` or `providers/`
❌ `config.rs` NEVER imports from `agent/`, `providers/`, or `tools/`
❌ No circular dependencies between modules

---

## Development Workflow

### Step-by-Step Process (FOLLOW EXACTLY)

#### Phase 1: Preparation

1. **Understand the Task**

   - Read requirements completely
   - Identify which architecture layers are affected
   - Check for existing similar code

2. **Search Existing Code**

   ```bash
   # Find relevant files
   grep -r "function_name" src/
   find src/ -name "*feature*.rs"
   ```

3. **Plan Changes**
   - List files to create/modify
   - Identify tests needed
   - Determine documentation category

#### Phase 2: Implementation

1. **Write Code**

   ````rust
   // Follow this pattern for ALL public items:

   /// One-line description
   ///
   /// Longer explanation of behavior and purpose.
   ///
   /// # Arguments
   ///
   /// * `param` - Description
   ///
   /// # Returns
   ///
   /// Description of return value
   ///
   /// # Errors
   ///
   /// Returns `ErrorType` if condition
   ///
   /// # Examples
   ///
   /// ```
   /// use xzatoma::module::function;
   ///
   /// let result = function(arg);
   /// assert_eq!(result, expected);
   /// ```
   pub fn function(param: Type) -> Result<ReturnType, Error> {
       // Implementation
   }
   ````
`````

2. **Write Tests (MANDATORY)**

   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_function_with_valid_input() {
           // Arrange
           let input = "test";

           // Act
           let result = function(input);

           // Assert
           assert!(result.is_ok());
           assert_eq!(result.unwrap(), expected);
       }

       #[test]
       fn test_function_with_invalid_input() {
           let result = function("");
           assert!(result.is_err());
       }

       #[test]
       fn test_function_edge_case() {
           // Test boundary conditions
       }
   }
   ```

3. **Run Quality Checks Incrementally**

   ```bash
   # After writing code
   cargo fmt --all
   cargo clippy --all-targets --all-features -- -D warnings

   # After writing tests
   cargo test --all-features

   # Before committing - verify all checks pass
   cargo fmt --all
   cargo check --all-targets --all-features
   cargo clippy --all-targets --all-features -- -D warnings
   cargo test --all-features
   ```

#### Phase 3: Documentation

**YOU MUST UPDATE** `docs/explanation/implementations.md`:

````markdown
# Feature Name Implementation

## Overview

Brief description of what was implemented and why.

## Components Delivered

- `src/path/file.rs` (XXX lines) - Description
- `src/path/tests.rs` (YYY lines) - Test coverage
- `docs/explanation/feature.md` (ZZZ lines) - This document

Total: ~N,NNN lines

## Implementation Details

### Component 1: Name

Description with code examples:

```rust
pub fn example() {
    // Code
}
```
````

### Component 2: Name

More details...

## Testing

Test coverage: XX% (must be >80%)

```text
test result: ok. X passed; 0 failed; Y ignored
```

## Usage Examples

Complete, runnable examples:

```rust
use xzatoma::module::Feature;

fn main() {
    let feature = Feature::new();
    feature.do_something();
}
```

## Validation Results

- ✅ `cargo fmt --all` passed
- ✅ `cargo check --all-targets --all-features` passed
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` shows zero
  warnings
- ✅ `cargo test --all-features` passed with >80% coverage
- ✅ Documentation complete

## References

- Architecture: `docs/explanation/architecture.md`
- API Reference: `docs/reference/api.md`

### Phase 4: Validation (CRITICAL)

**Run these commands and verify output:**

```bash
# 1. Format check
cargo fmt --all
# Expected: No output (all files formatted)

# 2. Compilation check
cargo check --all-targets --all-features
# Expected: "Finished" with 0 errors

# 3. Lint check (treats warnings as errors)
cargo clippy --all-targets --all-features -- -D warnings
# Expected: "Finished" with 0 warnings

# 4. Test check
cargo test --all-features
# Expected: "test result: ok. X passed; 0 failed" where X > previous count

# 5. Verify documentation created
ls -la docs/explanation/*{feature}*.md
# Expected: File exists with lowercase filename

# 6. Verify no emoji in docs
grep -r "[\x{1F600}-\x{1F64F}]" docs/
# Expected: No matches (except AGENTS.md)
```

**IF ANY VALIDATION FAILS: Stop and fix immediately.**

---

## Rust Coding Standards

### Error Handling (MANDATORY PATTERNS)

**YOU MUST:**

- Use `Result<T, E>` for ALL recoverable errors
- Use `?` operator for error propagation
- Use `thiserror` for custom error types
- Use descriptive error messages

**NEVER:**

- ❌ Use `unwrap()` without justification
- ❌ Use `expect()` without descriptive message
- ❌ Ignore errors with `let _ =`
- ❌ Return `panic!` for recoverable errors

**Correct Patterns:**

```rust
// ✅ GOOD - Proper error handling
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadError(String),

    #[error("Invalid YAML syntax: {0}")]
    ParseError(String),
}

pub fn load_config(path: &str) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| ConfigError::ReadError(e.to_string()))?;

    let config: Config = serde_yaml::from_str(&contents)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    config.validate()?;
    Ok(config)
}

// ❌ BAD - Using unwrap
pub fn load_config(path: &str) -> Config {
    let contents = std::fs::read_to_string(path).unwrap(); // NEVER
    serde_yaml::from_str(&contents).unwrap() // NEVER
}

// ⚠️ ACCEPTABLE - unwrap with justification
pub fn get_app_version() -> String {
    // SAFETY: This is set at compile time and cannot fail
    env!("CARGO_PKG_VERSION").to_string()
}
```

### Testing Standards (MANDATORY)

**YOU MUST:**

- Write tests for ALL public functions
- Test both success and failure cases
- Test edge cases and boundaries
- Achieve >80% code coverage
- Use descriptive test names: `test_{function}_{condition}_{expected}`

**Test Structure Template:**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Success case
    #[test]
    fn test_parse_config_with_valid_yaml() {
        let yaml = "key: value";
        let result = parse_config(yaml);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().key, "value");
    }

    // Failure case
    #[test]
    fn test_parse_config_with_invalid_yaml() {
        let yaml = "invalid: : yaml";
        let result = parse_config(yaml);
        assert!(result.is_err());
    }

    // Edge case
    #[test]
    fn test_parse_config_with_empty_string() {
        let result = parse_config("");
        assert!(result.is_err());
    }

    // Boundary condition
    #[test]
    fn test_parse_config_with_max_size() {
        let yaml = "x".repeat(MAX_CONFIG_SIZE);
        let result = parse_config(&yaml);
        assert!(result.is_ok());
    }

    // Error propagation
    #[test]
    fn test_parse_config_propagates_validation_error() {
        let yaml = "invalid_field: value";
        let result = parse_config(yaml);
        assert!(matches!(result, Err(ConfigError::ValidationError(_))));
    }
}
```

---

## Git Conventions

**DO NOT RUN GIT COMMANDS**

User will handle all git interactions



---

## Documentation Organization (Diataxis Framework)

**YOU MUST categorize documentation correctly:**

### Category 1: Tutorials (`docs/tutorials/`)

**Purpose**: Learning-oriented, step-by-step lessons

**Use for**:

- Getting started guides
- Learning path tutorials
- Hands-on examples

**Example**: `docs/tutorials/getting_started.md`

### Category 2: How-To Guides (`docs/how-to/`)

**Purpose**: Task-oriented, problem-solving recipes

**Use for**:

- Installation steps
- Configuration guides
- Troubleshooting procedures

**Example**: `docs/how-to/setup_monitoring.md`

### Category 3: Explanations (`docs/explanation/`) ← DEFAULT FOR YOUR SUMMARIES

**Purpose**: Understanding-oriented, conceptual discussion

**Use for**:

- Architecture explanations
- Design decisions
- Implementation summaries ← **YOU TYPICALLY CREATE THESE**
- Concept clarifications

**Example**: `docs/explanation/phase4_observability_implementation.md`

### Category 4: Reference (`docs/reference/`)

**Purpose**: Information-oriented, technical specifications

**Use for**:

- API documentation
- Configuration reference
- Command reference

**Example**: `docs/reference/api_specification.md`

### Decision Tree: Where to Put Documentation?

```text
Is it a step-by-step tutorial?
├─ YES → docs/tutorials/
└─ NO
   ├─ Is it solving a specific task?
   │  ├─ YES → docs/how-to/
   │  └─ NO
   │     ├─ Is it explaining concepts/architecture?
   │     │  ├─ YES → docs/explanation/  ← MOST COMMON FOR AI AGENTS
   │     │  └─ NO
   │     │     └─ Is it reference material?
   │     │        └─ YES → docs/reference/
```

---

## Common Pitfalls and How to Avoid Them

### Pitfall 1: Using `.yml` Instead of `.yaml`

**ISSUE**: `.yml` is common in industry, so agents default to it

**WHY IT FAILS**: Our CI/CD expects `.yaml` extension only

**PREVENTION**:

```bash
# ✅ Before creating any YAML file, use full extension
touch config/production.yaml

# ❌ Never use short extension
touch config/production.yml  # Will cause CI failure
```

**FIX IF YOU MADE THIS MISTAKE**:

```bash
# Rename all .yml to .yaml
find . -name "*.yml" -exec sh -c 'mv "$0" "${0%.yml}.yaml"' {} \;
```

### Pitfall 2: Uppercase or CamelCase in Documentation Filenames

**ISSUE**: Agents use CamelCase or capitalization for readability

**WHY IT FAILS**: Breaks documentation links, inconsistent naming

**PREVENTION**:

```bash
# ✅ Always use lowercase_with_underscores
touch docs/explanation/distributed_tracing_implementation.md

# ❌ Never use these patterns
touch docs/explanation/DistributedTracingImplementation.md  # CamelCase
touch docs/explanation/Distributed-Tracing-Implementation.md # Capitalized
touch docs/explanation/DISTRIBUTED_TRACING.md # Uppercase
```

**FIX IF YOU MADE THIS MISTAKE**:

```bash
# Rename to lowercase with underscores
mv docs/explanation/DistributedTracing.md \
   docs/explanation/distributed_tracing.md
```

### Pitfall 3: Forgetting to Run `cargo fmt`

**ISSUE**: Code works but fails CI due to formatting

**WHY IT FAILS**: CI runs `cargo fmt --check` which fails if code isn't
formatted

**PREVENTION**:

```bash
# ALWAYS run before committing
cargo fmt --all

# Verify it worked
cargo fmt --all -- --check
# Expected: no output = success
```

**FIX IF CI FAILS**:

```bash
cargo fmt --all
git add -u
git commit --amend --no-edit
```

### Pitfall 4: Using `unwrap()` Without Justification

**ISSUE**: Code works in testing but panics in production

**WHY IT FAILS**: Unexpected errors cause service crashes

**PREVENTION**:

```rust
// ❌ BAD - Will panic if file doesn't exist
let config = std::fs::read_to_string("config.yaml").unwrap();

// ✅ GOOD - Handles error gracefully
let config = std::fs::read_to_string("config.yaml")
    .map_err(|e| ConfigError::ReadFailed(e.to_string()))?;
```

**FIX IF YOU MADE THIS MISTAKE**:

```bash
# Find all unwrap calls
grep -rn "unwrap()" src/

# Replace with proper error handling
```

### Pitfall 5: Missing Documentation File

**ISSUE**: Task complete but no documentation created

**WHY IT FAILS**: Knowledge is lost, future developers confused

**PREVENTION**:

```bash
# Immediately after starting a task, create doc file
touch docs/explanation/feature_name_implementation.md

# Fill it in as you work
# Add final validation section when done
```

### Pitfall 6: Emojis in Documentation

**ISSUE**: Emojis used for "visual appeal"

**WHY IT FAILS**: Encoding issues, unprofessional, breaks tooling

**PREVENTION**:

```markdown
<!-- ❌ BAD -->

# Setup Guide 🚀

## Prerequisites ✅

<!-- ✅ GOOD -->

# Setup Guide

## Prerequisites
```

**FIX IF YOU MADE THIS MISTAKE**:

```bash
# Find all emoji usage
grep -r "[\x{1F600}-\x{1F64F}]" docs/

# Remove manually
```

### Pitfall 7: Ignoring Clippy Warnings

**ISSUE**: "It's just a warning, not an error"

**WHY IT FAILS**: CI treats warnings as errors (`-D warnings`)

**PREVENTION**:

```bash
# Fix ALL warnings before committing
cargo clippy --all-targets --all-features -- -D warnings

# If you see warnings, fix them one by one
# Re-run after each fix to ensure no new warnings introduced
```

---

## Emergency Procedures

### When Quality Checks Fail

**SYSTEMATIC DEBUG PROCESS:**

```bash
# Step 1: Fix formatting (always do this first)
cargo fmt --all

# Step 2: Fix compilation errors
cargo check --all-targets --all-features
# Read each error message
# Fix root cause, not symptoms
# Re-run after each fix

# Step 3: Fix clippy warnings (one at a time)
cargo clippy --all-targets --all-features -- -D warnings
# Fix first warning
# Re-run clippy
# Repeat until zero warnings

# Step 4: Fix failing tests
cargo test --all-features -- --nocapture
# Read test failure output
# Fix failing tests or update expectations
# Re-run tests

# Step 5: Verify all checks pass
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

### When Tests Fail

**DIAGNOSTIC COMMANDS:**

```bash
# Run with detailed output
cargo test -- --nocapture --test-threads=1

# Run specific test
cargo test test_name -- --nocapture

# Run tests in specific module
cargo test module::tests:: -- --nocapture

# Show backtrace on panic
RUST_BACKTRACE=1 cargo test

# Run with debug logging
RUST_LOG=debug cargo test
```

**DEBUGGING STRATEGY:**

1. Read the test failure message carefully
2. Understand what the test expects
3. Add `println!` or `dbg!` to see actual values
4. Fix the code or update the test
5. Re-run until passing

### When Clippy Reports Warnings

**FIXING PROCESS:**

```bash
# List all warnings
cargo clippy --all-targets --all-features 2>&1 | grep "warning:"

# Fix warnings by category:

# 1. Unused code
#    - Remove if truly unused
#    - Add #[allow(dead_code)] with justification if needed

# 2. Complexity warnings
#    - Refactor complex functions
#    - Extract helper functions

# 3. Style warnings
#    - Follow clippy suggestions
#    - Run cargo fix if available

# 4. Correctness warnings
#    - Fix immediately (these are bugs)

# Re-run after each fix
cargo clippy --all-targets --all-features -- -D warnings
```

---

## Validation Checklist

**BEFORE CLAIMING TASK IS COMPLETE, VERIFY ALL:**

### Code Quality

- [ ] `cargo fmt --all` applied successfully
- [ ] `cargo check --all-targets --all-features` passes with zero errors
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` shows zero
      warnings
- [ ] `cargo test --all-features` passes with >80% coverage
- [ ] No `unwrap()` or `expect()` without justification
- [ ] All public items have doc comments with examples
- [ ] All functions have at least 3 tests (success, failure, edge case)

### Testing

- [ ] Unit tests added for ALL new functions
- [ ] Integration tests added if needed
- [ ] Test count increased from before (verify with `cargo test --lib`)
- [ ] Both success and failure cases tested
- [ ] Edge cases and boundaries covered
- [ ] All tests use descriptive names: `test_{function}_{condition}_{expected}`

### Documentation

- [ ] Documentation file created in `docs/explanation/`
- [ ] Filename uses lowercase_with_underscores.md
- [ ] README.md exception is ONLY uppercase filename
- [ ] No emojis anywhere in documentation
- [ ] All code blocks specify language (`rust, not`)
- [ ] Documentation includes: Overview, Components, Details, Testing, Examples
- [ ] Markdownlint passes (if configured)

### Files and Structure

- [ ] All YAML files use `.yaml` extension (NOT `.yml`)
- [ ] All Markdown files use `.md` extension
- [ ] No uppercase in filenames except `README.md`
- [ ] Files placed in correct architecture layer
- [ ] Documentation in correct Diataxis category

### Git

- [ ] Branch name follows `pr-<feat>-<issue>` format (lowercase)
- [ ] Commit message follows conventional commits
- [ ] Commit message first line ≤72 characters
- [ ] Commit uses imperative mood ("add" not "added")

### Architecture

- [ ] Changes respect layer boundaries
- [ ] Domain layer has no infrastructure dependencies
- [ ] Proper separation of concerns maintained
- [ ] No circular dependencies introduced

---

## Quick Command Reference

### Essential Cargo Commands

```bash
# Build and check
cargo build                                      # Debug build
cargo build --release                            # Optimized build
cargo check --all-targets --all-features         # Fast compile check

# Quality
cargo fmt --all                                  # Format all code
cargo fmt --all -- --check                       # Check formatting
cargo clippy --all-targets --all-features -- -D warnings  # Lint

# Testing
cargo test                                       # Run all tests
cargo test --lib                                 # Library tests only
cargo test --all-features                        # With all features
cargo test -- --nocapture                        # Show output
cargo test test_name                             # Specific test

# Documentation
cargo doc --open                                 # Generate and open docs
cargo doc --no-deps --open                       # Without dependencies

# Maintenance
cargo clean                                      # Remove build artifacts
cargo update                                     # Update dependencies
cargo tree                                       # Show dependency tree
cargo audit                                      # Security check
```

### Project-Specific Commands

```bash
# Quality validation workflow
cargo fmt --all                                  # Format code
cargo check --all-targets --all-features         # Check compilation
cargo clippy --all-targets --all-features -- -D warnings  # Lint
cargo test --all-features                        # Run tests

# Additional make commands (if available)
make test                                        # Run tests
make build                                       # Build project
make clean                                       # Clean artifacts

# Adding dependencies
cargo add <crate_name>                           # Add to Cargo.toml
cargo add <crate_name> --dev                     # Dev dependency
cargo add <crate_name> --features=<feature>      # With feature
```

---

## Summary: The Three Golden Rules

**If you remember nothing else, remember these:**

### Rule 1: File Extensions

```text
.yaml (NOT .yml)
.md (NOT .MD or .markdown)
```

### Rule 2: Documentation Filenames

```text
lowercase_with_underscores.md
Exception: README.md ONLY
```

### Rule 3: Quality Checks

```text
All four cargo commands MUST pass before claiming done:
- cargo fmt --all
- cargo check --all-targets --all-features
- cargo clippy --all-targets --all-features -- -D warnings
- cargo test --all-features
```

---

## The Golden Workflow

**FOLLOW THIS SEQUENCE FOR EVERY TASK:**

```text
1. Create branch: pr-<feat>-<issue>
2. Implement code with /// doc comments
3. Add tests (>80% coverage)
4. Run: cargo fmt --all
5. Run: cargo check --all-targets --all-features
6. Run: cargo clippy --all-targets --all-features -- -D warnings
7. Run: cargo test --all-features
8. UPDATE: docs/explanation/implementations.md
9. Commit with proper format: <type>(<scope>): <description>
10. Verify: All checklist items above are checked
```

**IF YOU FOLLOW THIS WORKFLOW, YOUR CODE WILL BE ACCEPTED.**

**IF YOU SKIP STEPS OR VIOLATE RULES, YOUR CODE WILL BE REJECTED.**

---

## Living Document

This file is continuously updated as new patterns emerge. Last updated: 2024

**For AI Agents**: You are a master Rust developer. Follow these rules
precisely. Put all implementation summaries in `docs/explanation/` with
lowercase filenames.
