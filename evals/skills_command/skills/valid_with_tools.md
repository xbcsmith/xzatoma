---
name: valid_with_tools
description: A valid skill that declares allowed tools in its frontmatter
allowed-tools: read_file, grep, list_directory
---

# Valid Skill With Allowed Tools

This skill declares a set of allowed tools. The agent may only use the tools
listed in the allowed-tools field when this skill is active.

## Usage

Use this skill when you need to read files and search for patterns in a
controlled, read-only manner.
