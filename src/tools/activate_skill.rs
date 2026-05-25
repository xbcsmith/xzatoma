//! Synthetic `activate_skill` tool implementation.
//!
//! This tool is the only supported activation path for agent skills.
//! It activates one valid visible skill by name, deduplicates repeated
//! activation requests, and returns a structured wrapper suitable for prompt
//! injection by the runtime layer.

use crate::error::{Result, XzatomaError};
use crate::skills::activation::{ActiveSkill, ActiveSkillRegistry};
use crate::skills::catalog::SkillCatalog;

use crate::tools::{ToolExecutor, ToolResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};

/// Tool name for skill activation.
///
/// This name is part of the runtime contract and must remain stable.
pub const ACTIVATE_SKILL_TOOL_NAME: &str = "activate_skill";

/// Input for the `activate_skill` tool.
///
/// The model may only select from valid visible skill names exposed in the tool
/// schema.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::activate_skill::ActivateSkillInput;
///
/// let input = ActivateSkillInput {
///     skill_name: "example_skill".to_string(),
/// };
///
/// assert_eq!(input.skill_name, "example_skill");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivateSkillInput {
    /// Name of the skill to activate.
    #[serde(rename = "skill_name")]
    pub skill_name: String,
}

/// Structured output for the `activate_skill` tool.
///
/// This wrapped response is intended for prompt-layer injection and runtime
/// state updates. It must not expose arbitrary filesystem reads from model
/// input.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::activate_skill::ActivateSkillOutput;
///
/// let output = ActivateSkillOutput {
///     activated: true,
///     deduplicated: false,
///     skill_name: "example_skill".to_string(),
///     skill_dir: "/tmp/example_skill".to_string(),
///     allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
///     body: "# Example".to_string(),
///     resources: Vec::new(),
/// };
///
/// assert!(output.activated);
/// assert_eq!(output.skill_name, "example_skill");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivateSkillOutput {
    /// Whether activation succeeded.
    pub activated: bool,
    /// Whether the skill was already active and activation was deduplicated.
    pub deduplicated: bool,
    /// Activated skill name.
    pub skill_name: String,
    /// Absolute skill directory path.
    pub skill_dir: String,
    /// Normalized advisory allowed-tools list.
    pub allowed_tools: Vec<String>,
    /// Full skill body content for prompt-layer injection.
    pub body: String,
    /// Optional enumerated resource list.
    pub resources: Vec<String>,
}

impl ActivateSkillOutput {
    /// Converts the structured activation output into JSON.
    ///
    /// # Returns
    ///
    /// Returns a JSON value representing the activation result.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::activate_skill::ActivateSkillOutput;
    ///
    /// let output = ActivateSkillOutput {
    ///     activated: true,
    ///     deduplicated: false,
    ///     skill_name: "example_skill".to_string(),
    ///     skill_dir: "/tmp/example_skill".to_string(),
    ///     allowed_tools: vec!["read_file".to_string()],
    ///     body: "# Example".to_string(),
    ///     resources: Vec::new(),
    /// };
    ///
    /// let value = output.to_json();
    /// assert_eq!(value["skill_name"], "example_skill");
    /// ```
    pub fn to_json(&self) -> Value {
        json!({
            "activated": self.activated,
            "deduplicated": self.deduplicated,
            "skill_name": self.skill_name,
            "skill_dir": self.skill_dir,
            "allowed_tools": self.allowed_tools,
            "body": self.body,
            "resources": self.resources,
        })
    }
}

/// Synthetic tool that activates one valid visible skill by name.
///
/// This tool is registered only when:
///
/// - skills are enabled
/// - activation tool support is enabled
/// - at least one valid visible skill exists
///
/// It accepts exactly one required input parameter and restricts valid inputs
/// to the disclosed visible skill names.
///
/// # Examples
///
/// ```no_run
/// use std::sync::{Arc, Mutex};
/// use xzatoma::skills::activation::ActiveSkillRegistry;
/// use xzatoma::skills::catalog::SkillCatalog;
/// use xzatoma::tools::activate_skill::ActivateSkillTool;
/// use xzatoma::tools::ToolExecutor;
///
/// let tool = ActivateSkillTool::new(
///     Arc::new(SkillCatalog::new()),
///     Arc::new(Mutex::new(ActiveSkillRegistry::new())),
///     Vec::new(),
/// );
///
/// let definition = tool.tool_definition();
/// assert_eq!(definition["name"], "activate_skill");
/// ```
pub struct ActivateSkillTool {
    catalog: Arc<SkillCatalog>,
    registry: Arc<Mutex<ActiveSkillRegistry>>,
    visible_skill_names: Vec<String>,
}

impl ActivateSkillTool {
    /// Creates a new `activate_skill` tool.
    ///
    /// # Arguments
    ///
    /// * `catalog` - Valid visible skill catalog
    /// * `registry` - Shared active-skill registry for the current session
    /// * `visible_skill_names` - Valid visible skill names exposed in schema
    ///
    /// # Returns
    ///
    /// Returns a new `ActivateSkillTool`.
    pub fn new(
        catalog: Arc<SkillCatalog>,
        registry: Arc<Mutex<ActiveSkillRegistry>>,
        visible_skill_names: Vec<String>,
    ) -> Self {
        Self {
            catalog,
            registry,
            visible_skill_names,
        }
    }

    fn parse_input(args: Value) -> Result<ActivateSkillInput> {
        crate::tools::parse_tool_args(args).map_err(|error| match error {
            XzatomaError::Tool(message) => XzatomaError::Tool(
                message.replace("Invalid tool parameters", "Invalid activate_skill input"),
            ),
            other => other,
        })
    }

    fn ensure_visible_skill_name(&self, skill_name: &str) -> Result<()> {
        if self
            .visible_skill_names
            .iter()
            .any(|name| name == skill_name)
        {
            Ok(())
        } else {
            Err(XzatomaError::Tool(format!(
                "Skill '{}' is not visible or not available for activation",
                skill_name
            )))
        }
    }

    fn output_from_active_skill(
        active_skill: &ActiveSkill,
        deduplicated: bool,
    ) -> ActivateSkillOutput {
        ActivateSkillOutput {
            activated: true,
            deduplicated,
            skill_name: active_skill.skill_name.clone(),
            skill_dir: active_skill.skill_directory.display().to_string(),
            allowed_tools: active_skill.allowed_tools.clone(),
            body: active_skill.body_content.clone(),
            resources: active_skill
                .resources
                .iter()
                .map(|path| path.display().to_string())
                .collect(),
        }
    }

    fn activate_skill(&self, skill_name: &str) -> Result<ActivateSkillOutput> {
        self.ensure_visible_skill_name(skill_name)?;

        let mut registry = self.registry.lock().map_err(|_| {
            XzatomaError::Internal("Failed to lock active skill registry".to_string())
        })?;

        let status = registry.activate(self.catalog.as_ref(), skill_name)?;
        let active_skill = status.active_skill().ok_or_else(|| {
            XzatomaError::Internal("Skill activation did not return an active skill".to_string())
        })?;

        Ok(Self::output_from_active_skill(
            active_skill,
            !status.is_new_activation(),
        ))
    }
}

#[async_trait]
impl ToolExecutor for ActivateSkillTool {
    /// Returns the OpenAI-compatible tool definition.
    ///
    /// The schema contains exactly one required parameter named `skill_name`,
    /// restricted to the valid visible skill names.
    fn tool_definition(&self) -> Value {
        json!({
            "name": ACTIVATE_SKILL_TOOL_NAME,
            "description": "Activate one valid visible skill by name for use in the current session.",
            "parameters": {
                "type": "object",
                "properties": {
                    "skill_name": {
                        "type": "string",
                        "description": "Name of the visible skill to activate.",
                        "enum": self.visible_skill_names,
                    }
                },
                "required": ["skill_name"],
                "additionalProperties": false
            }
        })
    }

    /// Executes the activation request.
    ///
    /// # Arguments
    ///
    /// * `args` - Tool input arguments containing exactly one required skill name
    ///
    /// # Returns
    ///
    /// Returns a structured wrapped activation result suitable for prompt-layer
    /// injection and registry updates.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    ///
    /// - input is invalid
    /// - the skill is missing
    /// - the skill is hidden or not visible
    /// - registry access fails
    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let input = Self::parse_input(args)?;
        let output = self.activate_skill(&input.skill_name)?;

        Ok(ToolResult::success(output.to_json().to_string())
            .with_metadata("tool".to_string(), ACTIVATE_SKILL_TOOL_NAME.to_string())
            .with_metadata("skill_name".to_string(), output.skill_name.clone())
            .with_metadata("deduplicated".to_string(), output.deduplicated.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::activation::ActiveSkillRegistry;
    use crate::skills::catalog::SkillCatalog;
    use crate::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    fn sample_record(name: &str) -> SkillRecord {
        SkillRecord {
            metadata: SkillMetadata {
                name: name.to_string(),
                description: format!("Description for {}", name),
                license: None,
                compatibility: None,
                metadata: BTreeMap::new(),
                allowed_tools_raw: Some("read_file, grep".to_string()),
                allowed_tools: vec!["read_file".to_string(), "grep".to_string()],
            },
            skill_dir: PathBuf::from(format!("/tmp/{}", name)),
            skill_file: PathBuf::from(format!("/tmp/{}/SKILL.md", name)),
            source_scope: SkillSourceScope::UserClientSpecific,
            source_order: 0,
            body: format!("# {}", name),
        }
    }

    fn build_tool(name: &str) -> ActivateSkillTool {
        let catalog =
            SkillCatalog::from_records(vec![sample_record(name)]).expect("catalog should build");
        ActivateSkillTool::new(
            Arc::new(catalog),
            Arc::new(Mutex::new(ActiveSkillRegistry::new())),
            vec![name.to_string()],
        )
    }

    #[test]
    fn test_tool_definition_uses_required_name_and_single_required_input() {
        let tool = build_tool("example_skill");
        let definition = tool.tool_definition();

        assert_eq!(definition["name"], ACTIVATE_SKILL_TOOL_NAME);
        assert_eq!(definition["parameters"]["required"], json!(["skill_name"]));
        assert_eq!(
            definition["parameters"]["properties"]["skill_name"]["enum"],
            json!(["example_skill"])
        );
        assert_eq!(definition["parameters"]["additionalProperties"], false);
    }

    #[test]
    fn test_activate_skill_succeeds_for_valid_visible_skill() {
        let tool = build_tool("example_skill");
        let output = tool
            .activate_skill("example_skill")
            .expect("activation should succeed");

        assert!(output.activated);
        assert!(!output.deduplicated);
        assert_eq!(output.skill_name, "example_skill");
        assert_eq!(output.allowed_tools, vec!["read_file", "grep"]);
        assert_eq!(output.body, "# example_skill");
    }

    #[test]
    fn test_activate_skill_deduplicates_reactivation() {
        let tool = build_tool("example_skill");

        let first = tool
            .activate_skill("example_skill")
            .expect("first activation should succeed");
        let second = tool
            .activate_skill("example_skill")
            .expect("second activation should succeed");

        assert!(!first.deduplicated);
        assert!(second.deduplicated);

        let registry = tool.registry.lock().expect("registry lock should succeed");
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn test_activate_skill_fails_for_missing_skill() {
        let tool = build_tool("example_skill");
        let error = tool.activate_skill("missing_skill");

        assert!(error.is_err());
    }

    #[test]
    fn test_activate_skill_fails_for_hidden_skill_name() {
        let catalog =
            SkillCatalog::from_records(vec![sample_record("example_skill")]).expect("catalog ok");
        let tool = ActivateSkillTool::new(
            Arc::new(catalog),
            Arc::new(Mutex::new(ActiveSkillRegistry::new())),
            vec!["other_visible_skill".to_string()],
        );

        let error = tool.activate_skill("example_skill");
        assert!(error.is_err());
    }

    #[tokio::test]
    async fn test_execute_returns_structured_wrapped_content() {
        let tool = build_tool("example_skill");

        let result = tool
            .execute(json!({ "skill_name": "example_skill" }))
            .await
            .expect("tool execution should succeed");

        assert!(result.success);
        assert!(result.output.contains("\"skill_name\":\"example_skill\""));
        assert!(result
            .output
            .contains("\"allowed_tools\":[\"read_file\",\"grep\"]"));
        assert_eq!(
            result.metadata.get("tool"),
            Some(&ACTIVATE_SKILL_TOOL_NAME.to_string())
        );
    }

    #[tokio::test]
    async fn test_execute_fails_on_invalid_input_shape() {
        let tool = build_tool("example_skill");
        let result = tool
            .execute(json!({ "wrong_field": "example_skill" }))
            .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_output_to_json_contains_expected_fields() {
        let output = ActivateSkillOutput {
            activated: true,
            deduplicated: false,
            skill_name: "example_skill".to_string(),
            skill_dir: "/tmp/example_skill".to_string(),
            allowed_tools: vec!["read_file".to_string()],
            body: "# Example".to_string(),
            resources: vec!["README.md".to_string()],
        };

        let value = output.to_json();
        assert_eq!(value["activated"], true);
        assert_eq!(value["skill_name"], "example_skill");
        assert_eq!(value["skill_dir"], "/tmp/example_skill");
        assert_eq!(value["resources"], json!(["README.md"]));
    }
}
