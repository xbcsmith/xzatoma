# Contributing to XZatoma

Thank you for your interest in contributing to XZatoma! This document provides guidelines and information for contributors.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Coding Standards](#coding-standards)
- [Testing Requirements](#testing-requirements)
- [Documentation](#documentation)
- [Pull Request Process](#pull-request-process)
- [Issue Guidelines](#issue-guidelines)

## Code of Conduct

This project follows standard open source community guidelines. Be respectful, constructive, and collaborative.

## Getting Started

### Prerequisites

- Rust (stable) - Install from [rustup.rs](https://rustup.rs/)
- Git
- Basic understanding of Rust async programming
- Familiarity with AI agents and LLMs (helpful but not required)

### Setting Up Development Environment

```bash
# Clone the repository
git clone https://github.com/xbcsmith/xzatoma.git
cd xzatoma

# Build the project
cargo build

# Run tests
cargo test

# Check code quality
cargo fmt --check
cargo clippy -- -D warnings
```

### Project Structure

See [docs/reference/architecture.md](docs/reference/architecture.md) for detailed architecture and [docs/reference/quick_reference.md](docs/reference/quick_reference.md) for quick navigation.

## Development Workflow

### 1. Find or Create an Issue

- Check existing issues for tasks
- Create a new issue if you have an idea
- Discuss major changes before starting work

### 2. Create a Branch

```bash
# Feature branch
git checkout -b feature/your-feature-name

# Bug fix branch
git checkout -b fix/issue-description

# Documentation branch
git checkout -b docs/what-you-are-documenting
```

### 3. Make Changes

Follow the guidelines in [AGENTS.md](AGENTS.md) which contains:
- Detailed coding standards
- Testing requirements
- Documentation requirements
- Common pitfalls to avoid

### 4. Quality Checks

Before committing, ensure all checks pass:

```bash
# Format code
cargo fmt

# Run linter
cargo clippy -- -D warnings

# Run tests
cargo test --all-features

# Build release (optional but recommended)
cargo build --release
```

### 5. Commit Changes

Follow conventional commit format:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `test`: Test additions or changes
- `refactor`: Code refactoring
- `perf`: Performance improvements
- `chore`: Maintenance tasks

Examples:
```
feat(provider): add support for Anthropic provider

fix(agent): resolve context overflow issue

docs(reference): update architecture diagram

test(workflow): add integration tests for plan parser
```

### 6. Push and Create Pull Request

```bash
git push origin your-branch-name
```

Then create a pull request on GitHub.

## Coding Standards

### Rust Style Guide

1. **Follow Rust conventions**
   - Use `cargo fmt` for formatting
   - Follow Rust naming conventions
   - Use idiomatic Rust patterns

2. **Error Handling**
   ```rust
   // Use Result for fallible operations
   pub fn load_config(path: &Path) -> Result<Config> {
       let content = fs::read_to_string(path)
           .context("Failed to read config file")?;
       
       serde_yaml::from_str(&content)
           .context("Failed to parse config")
   }
   ```

3. **Async Code**
   ```rust
   // Use async/await consistently
   pub async fn fetch_data(&self) -> Result<Data> {
       let response = self.client
           .get(url)
           .send()
           .await?;
       
       response.json().await
   }
   ```

4. **Documentation**
   ```rust
   /// Loads configuration from the specified path.
   ///
   /// # Arguments
   ///
   /// * `path` - Path to the configuration file
   ///
   /// # Errors
   ///
   /// Returns an error if the file cannot be read or parsed.
   ///
   /// # Example
   ///
   /// ```
   /// let config = load_config(Path::new("config.yaml"))?;
   /// ```
   pub fn load_config(path: &Path) -> Result<Config> {
       // Implementation
   }
   ```

### Project-Specific Guidelines

See [AGENTS.md](AGENTS.md) for comprehensive guidelines including:
- File naming conventions (use `.yaml` not `.yml`)
- Documentation naming (lowercase with underscores)
- No emojis in code or documentation
- Mandatory quality gates
- Module organization

## Testing Requirements

### Test Coverage

- Minimum 80% code coverage required
- All new features must include tests
- Bug fixes must include regression tests

### Test Types

1. **Unit Tests**
   ```rust
   #[cfg(test)]
   mod tests {
       use super::*;

       #[test]
       fn test_function_name() {
           let result = function_under_test();
           assert_eq!(result, expected_value);
       }

       #[tokio::test]
       async fn test_async_function() {
           let result = async_function().await.unwrap();
           assert!(result.is_valid());
       }
   }
   ```

2. **Integration Tests**
   - Place in `tests/integration/`
   - Test component interactions
   - Use fixtures from `tests/fixtures/`

3. **End-to-End Tests**
   - Test complete workflows
   - Use realistic scenarios
   - Verify deliverables

### Running Tests

```bash
# All tests
cargo test

# Specific test
cargo test test_name

# With output
cargo test -- --nocapture

# Integration tests only
cargo test --test integration_tests

# With coverage
cargo tarpaulin --out Html
```

## Documentation

### Documentation Types (Diataxis Framework)

1. **Tutorials** (`docs/tutorials/`)
   - Learning-oriented, step-by-step guides
   - Example: Quickstart tutorial

2. **How-To Guides** (`docs/how_to/`)
   - Task-oriented, problem-solving guides
   - Example: Configuring providers

3. **Explanations** (`docs/explanation/`)
   - Understanding-oriented, conceptual discussion
   - Example: Architecture decisions

4. **Reference** (`docs/reference/`)
   - Information-oriented, technical specifications
   - Example: API reference, CLI commands

### Documentation Standards

- Use `.md` extension (never `.txt`)
- Lowercase filenames with underscores: `my_document.md`
- No emojis in documentation content
- Include code examples where relevant
- Update docs index when adding new files

### API Documentation

All public APIs must have documentation:

```rust
/// Brief description of the function.
///
/// Longer description if needed.
///
/// # Arguments
///
/// * `arg1` - Description of arg1
/// * `arg2` - Description of arg2
///
/// # Returns
///
/// Description of return value
///
/// # Errors
///
/// Description of possible errors
///
/// # Example
///
/// ```
/// // Usage example
/// ```
pub fn public_function(arg1: Type1, arg2: Type2) -> Result<ReturnType> {
    // Implementation
}
```

## Pull Request Process

### Before Submitting

1. Ensure all tests pass
2. Run quality checks (fmt, clippy)
3. Update documentation
4. Add entry to CHANGELOG.md (for features/fixes)
5. Rebase on latest main if needed

### PR Template

Your PR should include:

**Description**
- What does this PR do?
- Why is this change needed?

**Related Issues**
- Fixes #123
- Relates to #456

**Type of Change**
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

**Testing**
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing completed

**Checklist**
- [ ] Code follows project style guidelines
- [ ] Tests pass locally
- [ ] Documentation updated
- [ ] CHANGELOG.md updated (if applicable)

### Review Process

1. Automated checks must pass (CI/CD)
2. At least one maintainer approval required
3. Address review comments
4. Maintain constructive discussion

### After Merge

- Branch will be deleted automatically
- Close related issues if applicable
- Monitor for any issues in main

## Issue Guidelines

### Creating Issues

Use appropriate templates:

- **Bug Report**: For reporting bugs
- **Feature Request**: For suggesting features
- **Documentation**: For documentation improvements
- **Question**: For asking questions

### Issue Labels

- `bug` - Something isn't working
- `enhancement` - New feature or request
- `documentation` - Documentation improvements
- `good first issue` - Good for newcomers
- `help wanted` - Extra attention needed
- `phase-1` through `phase-5` - Implementation phase

### Working on Issues

1. Comment on the issue to claim it
2. Ask questions if requirements are unclear
3. Link your PR to the issue
4. Update the issue with progress if needed

## Development Phases

The project is implemented in 5 phases. See [docs/explanation/implementation_plan.md](docs/explanation/implementation_plan.md) for details.

Current focus areas by phase:
- **Phase 1**: Foundation, configuration, error handling
- **Phase 2**: Provider integration (Copilot, Ollama)
- **Phase 3**: Agent core, conversation management
- **Phase 4**: Workflow engine, repository analysis
- **Phase 5**: Documentation generation, production readiness

Check the implementation plan to find tasks aligned with current phase.

## Getting Help

- **Questions**: Open a GitHub Discussion
- **Bugs**: Create an issue with bug report template
- **Chat**: (Future: Discord/Slack channel)
- **Documentation**: Check [docs/](docs/) directory

## Recognition

Contributors will be:
- Listed in CONTRIBUTORS.md
- Acknowledged in release notes
- Credited in relevant documentation

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.

## Additional Resources

- [Architecture Document](docs/reference/architecture.md)
- [Implementation Plan](docs/explanation/implementation_plan.md)
- [Quick Reference](docs/reference/quick_reference.md)
- [Agent Guidelines](AGENTS.md)
- [Planning Guidelines](PLAN.md)

## Thank You!

Your contributions help make XZatoma better for everyone. We appreciate your time and effort!

---

**Last Updated**: 2025-01-07
**Maintained By**: XZatoma Development Team
