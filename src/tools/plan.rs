//! Plan parsing tool for XZatoma
//!
//! This module provides plan file parsing functionality.
//! Phase 5 implementation: YAML, JSON, Markdown parsing and validation.

use crate::error::{Result, XzatomaError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// Execution Plan
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    /// Plan name (title)
    pub name: String,
    /// Optional plan description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional version label used by the generic watcher for version-based event matching.
    ///
    /// When set, this value is matched against the `version` field of an incoming
    /// `GenericPlanEvent`. Set to `None` to disable version-based matching; the plan
    /// remains eligible for name-based or action-based matching by the watcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Optional action label used by the generic watcher for event-to-plan matching.
    ///
    /// When set, this value is matched against the `action` field of an incoming
    /// `GenericPlanEvent`. Set to `None` to disable action-based matching; the plan
    /// remains eligible for name-based or version-based matching by the watcher.
    ///
    /// This field is ignored by the standard `run` command — it exists solely to
    /// allow plan authors to annotate plans for automated dispatch.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    /// Ordered list of plan steps
    pub steps: Vec<PlanStep>,
}

/// A single step in a plan
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
            version: None,
            action: None,
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

    /// Format the plan as an instruction prompt for the agent executor.
    ///
    /// Produces a human-readable task description containing the plan name and all
    /// step names and actions in order. This string is used by
    /// [`GenericEventHandler`](crate::watcher::generic::event_handler::GenericEventHandler)
    /// as the prompt passed to the agent when executing a plan received over Kafka.
    ///
    /// # Returns
    ///
    /// A formatted multi-line instruction string.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::plan::{Plan, PlanStep};
    ///
    /// let plan = Plan {
    ///     name: "Deploy Service".to_string(),
    ///     description: None,
    ///     version: None,
    ///     action: None,
    ///     steps: vec![
    ///         PlanStep::new("build".to_string()).with_action("cargo build --release".to_string()),
    ///         PlanStep::new("deploy".to_string()).with_action("kubectl apply -f deploy.yaml".to_string()),
    ///     ],
    /// };
    ///
    /// let instruction = plan.to_instruction();
    /// assert!(instruction.contains("Deploy Service"));
    /// assert!(instruction.contains("cargo build --release"));
    /// assert!(instruction.contains("kubectl apply -f deploy.yaml"));
    /// ```
    pub fn to_instruction(&self) -> String {
        let steps_s = self
            .steps
            .iter()
            .map(|s| format!("- {}: {}", s.name, s.action))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "Execute this plan:\n\nName: {}\n\nSteps:\n{}\n",
            self.name, steps_s
        )
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
    /// Parse a plan from a raw string, attempting JSON then YAML format.
    ///
    /// This is the primary entry point for parsing inbound Kafka message payloads
    /// into [`Plan`] values. When the trimmed content begins with `{`, JSON is
    /// attempted first; all other content is parsed as YAML (which also handles
    /// valid JSON that does not start with `{`). Validation is performed as part
    /// of each format's parser.
    ///
    /// # Arguments
    ///
    /// * `content` - Raw plan text in YAML or JSON format
    ///
    /// # Returns
    ///
    /// A validated [`Plan`] on success.
    ///
    /// # Errors
    ///
    /// Returns an error if the content cannot be parsed or if the parsed plan
    /// fails validation (empty name, no steps, or a step with no action).
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::plan::PlanParser;
    ///
    /// let yaml = "name: Deploy\nsteps:\n  - name: s1\n    action: echo ok\n";
    /// let plan = PlanParser::parse_string(yaml).unwrap();
    /// assert_eq!(plan.name, "Deploy");
    ///
    /// let json = r#"{"name":"Deploy","steps":[{"name":"s1","action":"echo ok"}]}"#;
    /// let plan2 = PlanParser::parse_string(json).unwrap();
    /// assert_eq!(plan2.name, "Deploy");
    /// ```
    pub fn parse_string(content: &str) -> Result<Plan> {
        let trimmed = content.trim();
        // JSON objects start with '{'; try JSON parsing first for those.
        if trimmed.starts_with('{') {
            if let Ok(plan) = Self::from_json(content) {
                return Ok(plan);
            }
        }
        // Default to YAML parsing for all other content (YAML is a superset of JSON,
        // so valid JSON without a leading '{' is also handled here).
        Self::from_yaml(content)
    }

    /// Parse a plan from a file.
    ///
    /// Supports `.yaml`, `.yml`, `.json`, and `.md` extensions.
    pub fn from_file(path: &Path) -> Result<Plan> {
        let content = fs::read_to_string(path)
            .map_err(|e| XzatomaError::Tool(format!("Failed to read plan file: {}", e)))?;

        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .ok_or_else(|| XzatomaError::Tool("Plan file has no extension".to_string()))?;

        match extension {
            "yaml" | "yml" => Self::from_yaml(&content),
            "json" => Self::from_json(&content),
            "md" => Self::from_markdown(&content),
            _ => Err(XzatomaError::Tool(format!(
                "Unsupported plan format: {}",
                extension
            ))),
        }
    }

    /// Parse YAML plan content
    pub fn from_yaml(content: &str) -> Result<Plan> {
        let plan: Plan = serde_yaml::from_str(content).map_err(XzatomaError::Yaml)?;
        Self::validate(&plan)?;
        Ok(plan)
    }

    /// Parse JSON plan content
    pub fn from_json(content: &str) -> Result<Plan> {
        let plan: Plan = serde_json::from_str(content).map_err(XzatomaError::Serialization)?;
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
            version: None,
            action: None,
            steps,
        };

        Self::validate(&plan)?;
        Ok(plan)
    }

    /// Validate a plan instance (structure and content)
    pub fn validate(plan: &Plan) -> Result<()> {
        if plan.name.trim().is_empty() {
            return Err(XzatomaError::Tool("Plan name cannot be empty".to_string()));
        }

        if plan.steps.is_empty() {
            return Err(XzatomaError::Tool(
                "Plan must have at least one step".to_string(),
            ));
        }

        for (i, step) in plan.steps.iter().enumerate() {
            if step.name.trim().is_empty() {
                return Err(XzatomaError::Tool(format!("Step {} has no name", i + 1)));
            }
            if step.action.trim().is_empty() {
                return Err(XzatomaError::Tool(format!(
                    "Step '{}' has no action",
                    step.name
                )));
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
            version: None,
            action: None,
            steps: vec![PlanStep::new("s".to_string()).with_action("a".to_string())],
        };
        assert!(PlanParser::validate(&plan).is_err());

        // Missing steps
        let plan2 = Plan {
            name: "n".to_string(),
            description: None,
            version: None,
            action: None,
            steps: Vec::new(),
        };
        assert!(PlanParser::validate(&plan2).is_err());

        // Missing action
        let plan3 = Plan {
            name: "n".to_string(),
            description: None,
            version: None,
            action: None,
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

    #[test]
    fn test_plan_version_field_optional_roundtrip() {
        // A plan without `version` must parse successfully.
        let yaml_no_version =
            "name: No Version Plan\nsteps:\n  - name: s1\n    action: do something\n";
        let plan = PlanParser::from_yaml(yaml_no_version).unwrap();
        assert_eq!(
            plan.version, None,
            "version should default to None when absent"
        );

        // A plan with `version` must round-trip through YAML correctly.
        let yaml_with_version = "name: Versioned Plan\nversion: v2.1.0\nsteps:\n  - name: s1\n    action: run deployment\n";
        let plan2 = PlanParser::from_yaml(yaml_with_version).unwrap();
        assert_eq!(
            plan2.version.as_deref(),
            Some("v2.1.0"),
            "version field should deserialize correctly"
        );
    }

    #[test]
    fn test_parse_string_parses_yaml() {
        let yaml = "name: YAML Plan\nsteps:\n  - name: s1\n    action: echo yaml\n";
        let plan = PlanParser::parse_string(yaml).unwrap();
        assert_eq!(plan.name, "YAML Plan");
        assert_eq!(plan.steps.len(), 1);
    }

    #[test]
    fn test_parse_string_parses_json() {
        let json = r#"{"name":"JSON Plan","steps":[{"name":"s1","action":"echo json"}]}"#;
        let plan = PlanParser::parse_string(json).unwrap();
        assert_eq!(plan.name, "JSON Plan");
        assert_eq!(plan.steps[0].name, "s1");
    }

    #[test]
    fn test_parse_string_returns_err_for_invalid_content() {
        let result = PlanParser::parse_string("this is not a plan");
        assert!(result.is_err(), "invalid content should return Err");
    }

    #[test]
    fn test_parse_string_returns_err_for_empty_steps() {
        let result = PlanParser::parse_string("name: Empty\nsteps: []\n");
        assert!(result.is_err(), "plan with no steps should return Err");
    }

    #[test]
    fn test_to_instruction_contains_plan_name_and_steps() {
        let plan = Plan::new(
            "Deploy App".to_string(),
            vec![
                PlanStep::new("build".to_string()).with_action("cargo build".to_string()),
                PlanStep::new("push".to_string()).with_action("docker push".to_string()),
            ],
        );
        let instruction = plan.to_instruction();
        assert!(
            instruction.contains("Deploy App"),
            "instruction must contain plan name"
        );
        assert!(
            instruction.contains("cargo build"),
            "instruction must contain step action"
        );
        assert!(
            instruction.contains("docker push"),
            "instruction must contain step action"
        );
        assert!(
            instruction.contains("Execute this plan"),
            "instruction must have header"
        );
    }

    #[test]
    fn test_plan_action_field_optional_roundtrip() {
        // A plan without `action` must parse successfully (backward compatibility).
        let yaml_no_action = r#"
name: No Action Plan
steps:
  - name: s1
    action: do something
"#;
        let plan = PlanParser::from_yaml(yaml_no_action).unwrap();
        assert_eq!(
            plan.action, None,
            "action should default to None when absent"
        );

        // A plan with `action` must round-trip through YAML correctly.
        let yaml_with_action = r#"
name: Action Plan
action: deploy
steps:
  - name: s1
    action: run deployment
"#;
        let plan2 = PlanParser::from_yaml(yaml_with_action).unwrap();
        assert_eq!(
            plan2.action.as_deref(),
            Some("deploy"),
            "action field should deserialize correctly"
        );

        // Round-trip through JSON.
        let json_str = serde_json::to_string(&plan2).unwrap();
        let plan3: crate::tools::plan::Plan = serde_json::from_str(&json_str).unwrap();
        assert_eq!(plan3.action.as_deref(), Some("deploy"));
    }
}
