//! Planning mode system prompt
//!
//! This module provides the system prompt for planning mode, which guides the agent
//! to analyze requests and create detailed plans without modifying any files or
//! executing commands.

use crate::chat_mode::SafetyMode;

/// Generates the system prompt for planning mode
///
/// In planning mode, the agent is constrained to read-only operations and should
/// focus on creating comprehensive, actionable plans. The safety mode is included
/// in the prompt to clarify expectations around confirmations.
///
/// # Arguments
///
/// * `safety` - The current SafetyMode (affects messaging about confirmations)
///
/// # Returns
///
/// A system prompt string tailored for planning mode
///
/// # Examples
///
/// ```
/// use xzatoma::prompts::planning_prompt::generate_planning_prompt;
/// use xzatoma::chat_mode::SafetyMode;
///
/// let prompt = generate_planning_prompt(SafetyMode::AlwaysConfirm);
/// assert!(prompt.contains("PLANNING"));
/// ```
pub fn generate_planning_prompt(safety: SafetyMode) -> String {
    let safety_note = match safety {
        SafetyMode::AlwaysConfirm => {
            "Note: Safety mode is ENABLED. Always consider confirmation requirements."
        }
        SafetyMode::NeverConfirm => {
            "Note: Safety mode is DISABLED (YOLO). Operations will proceed without confirmation."
        }
    };

    format!(
        r#"You are in PLANNING mode. Your role is to analyze requests and create detailed, actionable plans.

MODE CAPABILITIES:
You HAVE access to:
- Read files to understand the current state
- List directory contents to explore structure
- Search for patterns in files to find relevant code
- Analyze the codebase and requirements

CONSTRAINTS - You CANNOT:
- Modify files
- Create or delete files
- Execute terminal commands
- Make any changes to the system
- Run code or tests

OUTPUT EXPECTATIONS:
Create plans that are:
- Clear and well-structured
- Actionable by another agent or human
- Comprehensive (including edge cases)
- Ready for implementation

PLAN FORMATS:
You should output plans in one of these formats:
1. YAML format - Structured key-value format for machine parsing
2. Markdown format - Readable with clear sections and step numbering
3. Markdown with YAML frontmatter - Combines both for readability and structure

Example YAML plan:
```yaml
title: "Feature Implementation Plan"
description: "Overview of what needs to be done"
steps:
  - name: "Step 1"
    description: "What to do"
    dependencies: []
  - name: "Step 2"
    description: "What to do next"
    dependencies: ["Step 1"]
```

Example Markdown plan:
# Feature Implementation Plan

## Overview
Brief description of the task

## Steps
1. **Step 1**: Description
2. **Step 2**: Description (depends on Step 1)

## Validation
Criteria for success

{}

REMEMBER:
- If you need to understand the codebase, use read and search tools first
- Create comprehensive plans that leave nothing ambiguous
- For complex tasks, break them into smaller, clear steps
- Note any assumptions or open questions in your plan
"#,
        safety_note
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat_mode::SafetyMode;

    #[test]
    fn test_planning_prompt_safe_mode() {
        let prompt = generate_planning_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("PLANNING"));
        assert!(prompt.contains("Read files"));
        assert!(prompt.contains("Search for patterns"));
        assert!(prompt.to_lowercase().contains("cannot"));
        assert!(prompt.to_lowercase().contains("modify"));
        assert!(prompt.to_lowercase().contains("execute"));
        assert!(prompt.contains("ENABLED"));
    }

    #[test]
    fn test_planning_prompt_yolo_mode() {
        let prompt = generate_planning_prompt(SafetyMode::NeverConfirm);
        assert!(prompt.contains("PLANNING"));
        assert!(prompt.contains("Read files"));
        assert!(prompt.contains("DISABLED (YOLO)"));
    }

    #[test]
    fn test_planning_prompt_includes_format_info() {
        let prompt = generate_planning_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("YAML format"));
        assert!(prompt.contains("Markdown format"));
        assert!(prompt.contains("frontmatter"));
    }

    #[test]
    fn test_planning_prompt_includes_example() {
        let prompt = generate_planning_prompt(SafetyMode::AlwaysConfirm);
        assert!(prompt.contains("Example"));
        assert!(prompt.contains("title:"));
        assert!(prompt.contains("steps:"));
    }

    #[test]
    fn test_planning_prompt_not_empty() {
        let prompt = generate_planning_prompt(SafetyMode::AlwaysConfirm);
        assert!(!prompt.is_empty());
        assert!(prompt.len() > 200);
    }

    #[test]
    fn test_planning_prompt_different_from_write() {
        let planning = generate_planning_prompt(SafetyMode::AlwaysConfirm);
        // This is just a sanity check - planning should mention read-only operations
        assert!(planning.to_lowercase().contains("read"));
        assert!(planning.to_lowercase().contains("cannot"));
        assert!(planning.to_lowercase().contains("modify"));
    }
}
