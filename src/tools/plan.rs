#![allow(dead_code)]
//! Plan parsing tool for XZatoma
//!
//! This module provides plan file parsing functionality.
//! Phase 5 implementation: YAML, JSON, Markdown parsing and validation.

use crate::error::{Result, XzatomaError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Execution Plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Plan name (title)
    pub name: String,
    /// Optional plan description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Ordered list of plan steps
    pub steps: Vec<PlanStep>,
}

/// A single step in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Step name/title
    pub name: String,
    /// Action to perform (a human readable description)
    pub action: String,
    /// Optional context (e.g., a code block or command)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

impl Plan {
    /// Create a new plan
    pub fn new(name: String, steps: Vec<PlanStep>) -> Self {
        Self {
            name,
            description: None,
            steps,
        }
    }

    /// Number of steps
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// If plan has no steps
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl PlanStep {
    /// Create a new plan step
    pub fn new(name: String) -> Self {
        Self {
            name,
            action: String::new(),
            context: None,
        }
    }

    /// Add action to the step
    pub fn with_action(mut self, action: String) -> Self {
        self.action = action;
        self
    }

    /// Add contextual information to the step
    pub fn with_context(mut self, context: String) -> Self {
        self.context = Some(context);
        self
    }
}

/// Plan Parser - supports YAML, JSON, Markdown formats
pub struct PlanParser;

impl PlanParser {
    /// Parse a plan from a file.
    ///
    /// Supports `.yaml`, `.yml`, `.json`, and `.md` extensions.
    pub fn from_file(path: &Path) -> Result<Plan> {
        let content = fs::read_to_string(path).map_err(|e| {
            anyhow::Error::from(XzatomaError::Tool(format!(
                "Failed to read plan file: {}",
                e
            )))
        })?;

        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| XzatomaError::Tool("Plan file has no extension".to_string()))?;

        match extension {
            "yaml" | "yml" => Self::from_yaml(&content),
            "json" => Self::from_json(&content),
            "md" => Self::from_markdown(&content),
            _ => Err(anyhow::Error::from(XzatomaError::Tool(format!(
                "Unsupported plan format: {}",
                extension
            )))),
        }
    }

    /// Parse YAML plan content
    pub fn from_yaml(content: &str) -> Result<Plan> {
        let plan: Plan = serde_yaml::from_str(content)
            .map_err(|e| anyhow::Error::from(XzatomaError::Yaml(e)))?;
        Self::validate(&plan)?;
        Ok(plan)
    }

    /// Parse JSON plan content
    pub fn from_json(content: &str) -> Result<Plan> {
        let plan: Plan = serde_json::from_str(content)
            .map_err(|e| anyhow::Error::from(XzatomaError::Serialization(e)))?;
        Self::validate(&plan)?;
        Ok(plan)
    }

    /// Parse Markdown plan content
    ///
    /// - `#` H1 heading -> plan name
    /// - `##` H2 headings -> steps
    /// - First non-empty line under a step -> action
    /// - Code fences -> step context
    pub fn from_markdown(content: &str) -> Result<Plan> {
        let mut name = String::new();
        let mut description: Option<String> = None;
        let mut steps: Vec<PlanStep> = Vec::new();

        let mut current_step: Option<PlanStep> = None;
        let mut in_code_block = false;
        let mut code_buffer = String::new();

        for line in content.lines() {
            let trimmed = line.trim();

            // Plan title: first H1
            if let Some(stripped) = trimmed.strip_prefix("# ") {
                if name.is_empty() {
                    name = stripped.trim().to_string();
                    continue;
                }
            }

            // Step header H2
            if let Some(stripped) = trimmed.strip_prefix("## ") {
                // Save previous step (if present)
                if let Some(mut step) = current_step.take() {
                    if !code_buffer.is_empty() {
                        step.context = Some(code_buffer.trim_end().to_string());
                        code_buffer.clear();
                    }
                    steps.push(step);
                }

                current_step = Some(PlanStep::new(stripped.trim().to_string()));
                continue;
            }

            // Code fence toggles
            if trimmed.starts_with("```") {
                in_code_block = !in_code_block;
                if in_code_block {
                    code_buffer.clear();
                } else {
                    if let Some(ref mut step) = current_step {
                        let cat = if let Some(prev) = step.context.take() {
                            if prev.is_empty() {
                                code_buffer.clone()
                            } else {
                                format!("{}\n{}", prev, code_buffer)
                            }
                        } else {
                            code_buffer.clone()
                        };
                        step.context = Some(cat.trim_end().to_string());
                    }
                    code_buffer.clear();
                }
                continue;
            }

            if in_code_block {
                // Preserve exact lines in code blocks
                code_buffer.push_str(line);
                code_buffer.push('\n');
                continue;
            }

            if let Some(ref mut step) = current_step {
                if step.action.is_empty() && !trimmed.is_empty() {
                    // The first non-empty line is the action
                    step.action = trimmed.to_string();
                } else if !trimmed.is_empty() {
                    // Additional lines appended to action (paragraph form)
                    step.action.push(' ');
                    step.action.push_str(trimmed);
                }
                continue;
            }

            // Outside of steps: treat the first paragraph (non-empty) after H1 as description
            if !name.is_empty() && description.is_none() && !trimmed.is_empty() {
                description = Some(trimmed.to_string());
            }
        }

        // Finalize last step
        if let Some(mut step) = current_step {
            if !code_buffer.is_empty() && step.context.is_none() {
                step.context = Some(code_buffer.trim_end().to_string());
            }
            steps.push(step);
        }

        let plan = Plan {
            name,
            description,
            steps,
        };

        Self::validate(&plan)?;
        Ok(plan)
    }

    /// Validate a plan instance (structure and content)
    pub fn validate(plan: &Plan) -> Result<()> {
        if plan.name.trim().is_empty() {
            return Err(anyhow::Error::from(XzatomaError::Tool(
                "Plan name cannot be empty".to_string(),
            )));
        }

        if plan.steps.is_empty() {
            return Err(anyhow::Error::from(XzatomaError::Tool(
                "Plan must have at least one step".to_string(),
            )));
        }

        for (i, step) in plan.steps.iter().enumerate() {
            if step.name.trim().is_empty() {
                return Err(anyhow::Error::from(XzatomaError::Tool(format!(
                    "Step {} has no name",
                    i + 1
                ))));
            }
            if step.action.trim().is_empty() {
                return Err(anyhow::Error::from(XzatomaError::Tool(format!(
                    "Step '{}' has no action",
                    step.name
                ))));
            }
        }

        Ok(())
    }
}

/// Convenience wrapper that parses YAML plan text
pub fn parse_plan(yaml: &str) -> Result<Plan> {
    PlanParser::from_yaml(yaml)
}

/// Load a plan from a file; kept async for compatibility with the rest of the codebase
pub async fn load_plan(path: &str) -> Result<Plan> {
    // In tests and CLI this is a simple wrapper around the sync parser.
    // For now we use blocking IO since parsing is not CPU bound here.
    PlanParser::from_file(Path::new(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs as stdfs;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    #[test]
    fn test_plan_creation() {
        let steps =
            vec![PlanStep::new("step 1".to_string()).with_action("do something".to_string())];
        let plan = Plan::new("test plan".to_string(), steps);
        assert_eq!(plan.name, "test plan");
        assert_eq!(plan.step_count(), 1);
        assert!(!plan.is_empty());
    }

    #[test]
    fn test_plan_empty() {
        let plan = Plan::new("goal".to_string(), Vec::new());
        assert!(plan.is_empty());
        assert_eq!(plan.step_count(), 0);
    }

    #[test]
    fn test_plan_step_creation() {
        let step = PlanStep::new("description".to_string());
        assert_eq!(step.name, "description");
        assert!(step.action.is_empty());
        assert!(step.context.is_none());
    }

    #[test]
    fn test_plan_step_with_action_and_context() {
        let step = PlanStep::new("s1".to_string())
            .with_action("do it".to_string())
            .with_context("cmd here".to_string());
        assert_eq!(step.action, "do it");
        assert_eq!(step.context, Some("cmd here".to_string()));
    }

    #[test]
    fn test_from_yaml() {
        let yaml = r#"
name: Setup Project
description: Initialize a new Rust project
steps:
  - name: Create project
    action: Run cargo init command
    context: cargo init --bin my-project
  - name: Verify setup
    action: Check that project compiles
    context: cargo check
"#;
        let plan = PlanParser::from_yaml(yaml).unwrap();
        assert_eq!(plan.name, "Setup Project");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].name, "Create project");
        assert!(plan.steps[0].context.is_some());
    }

    #[test]
    fn test_from_json() {
        let json = r#"{
            "name": "JSON Plan",
            "description": "desc",
            "steps": [
                { "name": "s1", "action": "do 1" }
            ]
        }"#;
        let plan = PlanParser::from_json(json).unwrap();
        assert_eq!(plan.name, "JSON Plan");
        assert_eq!(plan.steps[0].name, "s1");
    }

    #[test]
    fn test_from_markdown() {
        let md = r#"
# Markdown Plan
Initialize from MD

## Create project
Run cargo init command
```bash
cargo init --bin my-project
```

## Verify setup
Check that project compiles
```bash
cargo check
```
"#;
        let plan = PlanParser::from_markdown(md).unwrap();
        assert_eq!(plan.name, "Markdown Plan");
        assert_eq!(plan.steps.len(), 2);
        assert_eq!(plan.steps[0].name, "Create project");
        assert!(plan.steps[0].context.is_some());
        assert!(plan.steps[0]
            .context
            .as_ref()
            .unwrap()
            .contains("cargo init"));
    }

    #[test]
    fn test_from_file_yaml() {
        let dir = tempdir().unwrap();
        let yaml = r#"
name: File Plan
steps:
  - name: s1
    action: echo hi
"#;
        let p = {
            let p = dir.path().join("plan.yaml");
            stdfs::write(&p, yaml).expect("Failed to write plan file");
            p
        };
        let plan = PlanParser::from_file(&p).unwrap();
        assert_eq!(plan.name, "File Plan");
    }

    #[test]
    fn test_validate_errors() {
        // Missing name
        let plan = Plan {
            name: "".to_string(),
            description: None,
            steps: vec![PlanStep::new("s".to_string()).with_action("a".to_string())],
        };
        assert!(PlanParser::validate(&plan).is_err());

        // Missing steps
        let plan2 = Plan {
            name: "n".to_string(),
            description: None,
            steps: Vec::new(),
        };
        assert!(PlanParser::validate(&plan2).is_err());

        // Missing action
        let plan3 = Plan {
            name: "n".to_string(),
            description: None,
            steps: vec![PlanStep::new("step".to_string())],
        };
        assert!(PlanParser::validate(&plan3).is_err());
    }

    #[tokio::test]
    async fn test_parse_plan_and_load_plan() {
        let yaml = r#"
name: Quick Plan
steps:
  - name: s1
    action: echo ok
"#;
        let plan = parse_plan(yaml).unwrap();
        assert_eq!(plan.name, "Quick Plan");

        let dir = tempdir().unwrap();
        let p = {
            let p = dir.path().join("file_plan.yaml");
            stdfs::write(&p, yaml).expect("Failed to write plan file");
            p
        };
        let loaded = load_plan(p.to_str().unwrap()).await.unwrap();
        assert_eq!(loaded.name, "Quick Plan");
    }
}
