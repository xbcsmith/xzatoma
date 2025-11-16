//! Plan parsing tool for XZatoma
//!
//! This module provides plan file parsing functionality.
//! Full implementation will be completed in Phase 5.

use crate::error::Result;
use serde::{Deserialize, Serialize};

/// Plan structure representing a task execution plan
///
/// A plan contains a goal description and a list of steps to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    /// Goal description for the plan
    pub goal: String,
    /// List of steps to execute
    pub steps: Vec<PlanStep>,
}

/// A single step in a plan
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanStep {
    /// Description of the step
    pub description: String,
    /// Optional command to execute
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Expected outcome
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
}

impl Plan {
    /// Create a new plan
    ///
    /// # Arguments
    ///
    /// * `goal` - Goal description
    /// * `steps` - List of plan steps
    ///
    /// # Returns
    ///
    /// Returns a new Plan instance
    pub fn new(goal: String, steps: Vec<PlanStep>) -> Self {
        Self { goal, steps }
    }

    /// Get the number of steps in the plan
    ///
    /// # Returns
    ///
    /// Returns the count of steps
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Check if the plan is empty
    ///
    /// # Returns
    ///
    /// Returns true if there are no steps
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl PlanStep {
    /// Create a new plan step
    ///
    /// # Arguments
    ///
    /// * `description` - Step description
    ///
    /// # Returns
    ///
    /// Returns a new PlanStep instance
    pub fn new(description: String) -> Self {
        Self {
            description,
            command: None,
            expected: None,
        }
    }

    /// Add a command to the step
    ///
    /// # Arguments
    ///
    /// * `command` - Command to execute
    ///
    /// # Returns
    ///
    /// Returns self for chaining
    pub fn with_command(mut self, command: String) -> Self {
        self.command = Some(command);
        self
    }

    /// Add expected outcome to the step
    ///
    /// # Arguments
    ///
    /// * `expected` - Expected outcome
    ///
    /// # Returns
    ///
    /// Returns self for chaining
    pub fn with_expected(mut self, expected: String) -> Self {
        self.expected = Some(expected);
        self
    }
}

/// Parse a plan from YAML string
///
/// # Arguments
///
/// * `yaml` - YAML string containing the plan
///
/// # Returns
///
/// Returns the parsed Plan
///
/// # Errors
///
/// Returns error if YAML parsing fails
pub fn parse_plan(_yaml: &str) -> Result<Plan> {
    // Placeholder implementation
    // Full implementation will be in Phase 5
    Ok(Plan {
        goal: "placeholder".to_string(),
        steps: Vec::new(),
    })
}

/// Load a plan from a file
///
/// # Arguments
///
/// * `path` - Path to the plan file
///
/// # Returns
///
/// Returns the loaded Plan
///
/// # Errors
///
/// Returns error if file cannot be read or parsed
pub async fn load_plan(_path: &str) -> Result<Plan> {
    // Placeholder implementation
    // Full implementation will be in Phase 5
    Ok(Plan {
        goal: "placeholder".to_string(),
        steps: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_creation() {
        let steps = vec![PlanStep::new("step 1".to_string())];
        let plan = Plan::new("test goal".to_string(), steps);
        assert_eq!(plan.goal, "test goal");
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
        assert_eq!(step.description, "description");
        assert!(step.command.is_none());
        assert!(step.expected.is_none());
    }

    #[test]
    fn test_plan_step_with_command() {
        let step = PlanStep::new("description".to_string()).with_command("echo test".to_string());
        assert_eq!(step.command, Some("echo test".to_string()));
    }

    #[test]
    fn test_plan_step_with_expected() {
        let step = PlanStep::new("description".to_string()).with_expected("success".to_string());
        assert_eq!(step.expected, Some("success".to_string()));
    }

    #[test]
    fn test_plan_step_chaining() {
        let step = PlanStep::new("description".to_string())
            .with_command("cmd".to_string())
            .with_expected("result".to_string());
        assert_eq!(step.command, Some("cmd".to_string()));
        assert_eq!(step.expected, Some("result".to_string()));
    }

    #[test]
    fn test_parse_plan_placeholder() {
        let yaml = "goal: test\nsteps: []";
        let result = parse_plan(yaml);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_load_plan_placeholder() {
        let result = load_plan("test.yaml").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_plan_serialization() {
        let plan = Plan::new(
            "test goal".to_string(),
            vec![PlanStep::new("step 1".to_string()).with_command("echo test".to_string())],
        );
        let yaml = serde_yaml::to_string(&plan).unwrap();
        assert!(yaml.contains("goal: test goal"));
        assert!(yaml.contains("description: step 1"));
    }

    #[test]
    fn test_plan_deserialization() {
        let yaml = r#"
goal: test goal
steps:
  - description: step 1
    command: echo test
    expected: output
"#;
        let plan: Plan = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(plan.goal, "test goal");
        assert_eq!(plan.step_count(), 1);
        assert_eq!(plan.steps[0].description, "step 1");
        assert_eq!(plan.steps[0].command, Some("echo test".to_string()));
        assert_eq!(plan.steps[0].expected, Some("output".to_string()));
    }
}
