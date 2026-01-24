# XZatoma Documentation

Welcome to the XZatoma documentation! This guide will help you navigate the documentation based on what you're trying to accomplish.

## What is XZatoma?

XZatoma is an autonomous AI agent CLI application written in Rust that executes workflows from structured plans to analyze repositories and generate high-quality documentation following the Diataxis framework.

## Documentation Structure

This documentation follows the [Diataxis framework](https://diataxis.fr/), organizing content into four categories based on your needs:

### Tutorials (Learning-Oriented)

**When to use**: You're new to XZatoma and want to learn by doing.

- [Quickstart Tutorial](tutorials/quickstart.md) - Get started with your first documentation generation

### How-To Guides (Task-Oriented)

**When to use**: You have a specific task to accomplish.

- [Configure AI Providers](how-to/configure_providers.md) - Set up GitHub Copilot or Ollama
- [Create Workflows](how-to/create_workflows.md) - Build custom workflow plans
- [Generate Documentation](how-to/generate_documentation.md) - Generate specific doc types

### Explanations (Understanding-Oriented)

**When to use**: You want to understand how XZatoma works and why it's designed that way.

- [Project Overview](explanation/overview.md) - High-level project vision and goals
- [Implementation Plan](explanation/implementation_plan.md) - Phased development roadmap
- [Design Decisions](explanation/design_decisions.md) - Why we made certain choices

### Reference (Information-Oriented)

**When to use**: You need to look up specific technical details.

- [Architecture](reference/architecture.md) - Complete technical architecture
- [Quick Reference](reference/quick_reference.md) - Commands, patterns, and cheat sheet
- [CLI Reference](reference/cli.md) - Command-line interface documentation
- [Configuration Reference](reference/configuration.md) - All configuration options
- [Workflow Format](reference/workflow_format.md) - Workflow file specification
- [API Reference](reference/api.md) - Library API documentation

## Quick Navigation

### I want to...

- **Get started quickly** → [Quickstart Tutorial](tutorials/quickstart.md)
- **Understand the project** → [Project Overview](explanation/overview.md)
- **See the technical design** → [Architecture](reference/architecture.md)
- **Know the implementation plan** → [Implementation Plan](explanation/implementation_plan.md)
- **Find a command or pattern** → [Quick Reference](reference/quick_reference.md)
- **Configure a provider** → [Configure Providers](how-to/configure_providers.md)
- **Create a workflow** → [Create Workflows](how-to/create_workflows.md)

### I am a...

#### New User

1. Start with [Project Overview](explanation/overview.md)
2. Follow [Quickstart Tutorial](tutorials/quickstart.md)
3. Learn to [Configure Providers](how-to/configure_providers.md)
4. Create your first workflow with [Create Workflows](how-to/create_workflows.md)

#### Developer

1. Read [Architecture](reference/architecture.md)
2. Review [Implementation Plan](explanation/implementation_plan.md)
3. Check [Quick Reference](reference/quick_reference.md)
4. Follow coding standards in [AGENTS.md](../AGENTS.md)

#### Contributor

1. Review [Project Overview](explanation/overview.md)
2. Read [Implementation Plan](explanation/implementation_plan.md)
3. Follow [AGENTS.md](../AGENTS.md) guidelines
4. Check [PLAN.md](../PLAN.md) for planning approach

## Current Status

**Phase**: Planning Complete
**Next Milestone**: Phase 1 - Foundation
**Target Release**: v1.0.0 (14-19 weeks)

## External Resources

- [GitHub Repository](https://github.com/xbcsmith/xzatoma)
- [Diataxis Framework](https://diataxis.fr/)
- [Rust Documentation](https://doc.rust-lang.org/)
- [Goose Project](https://github.com/block/goose) (Architecture inspiration)
- [Zed Editor](https://github.com/zed-industries/zed) (Provider patterns)

## Contributing to Documentation

Documentation follows the Diataxis framework:

- **Tutorials**: Step-by-step learning experiences
- **How-To Guides**: Practical, task-oriented solutions
- **Explanations**: Conceptual, understanding-focused content
- **Reference**: Technical specifications and API docs

When adding documentation:

1. Choose the appropriate category based on purpose
2. Use descriptive lowercase filenames with underscores
3. Follow the writing style of existing docs in that category
4. Add links to this index

## Documentation Standards

- All markdown files use `.md` extension
- Filenames are lowercase with underscores (e.g., `implementation_plan.md`)
- No emojis in content
- Code blocks include language/path specification
- Links are relative where possible

See the contributor-facing conventions for full guidance: [Documentation Conventions](explanation/documentation_conventions.md). That document includes filename rules, Diataxis placement guidance, a PR checklist, and validation recommendations.

## Roadmap & Deferred Items

The documentation cleanup work is staged into phases. Outstanding or deferred items are tracked in the documentation cleanup summary and implementation plan. For larger or cross-cutting changes, please open an issue and link it to the relevant plan.

- Phase 4: Index update & documentation conventions — completed (this index updated and `docs/explanation/documentation_conventions.md` added).
- Phase 5: Docs CI — completed. A GitHub Actions workflow (`.github/workflows/docs_ci.yaml`) now runs documentation validation checks (internal link checks, emoji scans, code-fence language enforcement, and filename checks) on docs-only pull requests and when changes are made to `scripts/`. To run the same checks locally, use the convenience Make target:

```bash
make docs-check
```

- If you propose a deferred change, open an issue and reference `docs/explanation/documentation_cleanup_summary.md` or the implementation plan so reviewers can triage properly.

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/xbcsmith/xzatoma/issues)
- **Discussions**: [GitHub Discussions](https://github.com/xbcsmith/xzatoma/discussions)
- **Questions**: Check [Quick Reference](reference/quick_reference.md) first

---

**Last Updated**: 2025-01-07
**Documentation Version**: 0.1.0-planning
**Maintained By**: XZatoma Development Team
