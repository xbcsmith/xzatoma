use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::json;
use xzatoma::skills::activation::ActiveSkillRegistry;
use xzatoma::skills::catalog::SkillCatalog;
use xzatoma::skills::types::{SkillMetadata, SkillRecord, SkillSourceScope};
use xzatoma::tools::activate_skill::{ActivateSkillTool, ACTIVATE_SKILL_TOOL_NAME};
use xzatoma::tools::ToolExecutor;

fn sample_record(name: &str, description: &str, body: &str) -> SkillRecord {
    SkillRecord {
        metadata: SkillMetadata {
            name: name.to_string(),
            description: description.to_string(),
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
        body: body.to_string(),
    }
}

fn sample_catalog(records: Vec<SkillRecord>) -> SkillCatalog {
    SkillCatalog::from_records(records).expect("catalog should build")
}

fn build_tool(
    catalog: SkillCatalog,
    visible_skill_names: Vec<&str>,
    registry: Arc<Mutex<ActiveSkillRegistry>>,
) -> ActivateSkillTool {
    ActivateSkillTool::new(
        Arc::new(catalog),
        registry,
        visible_skill_names
            .into_iter()
            .map(str::to_string)
            .collect(),
    )
}

#[tokio::test]
async fn test_activate_skill_tool_schema_only_lists_valid_visible_skill_names() {
    let catalog = sample_catalog(vec![
        sample_record("alpha_skill", "Alpha description", "Alpha body"),
        sample_record("beta_skill", "Beta description", "Beta body"),
    ]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["alpha_skill"], registry);

    let definition = tool.tool_definition();

    assert_eq!(definition["name"], ACTIVATE_SKILL_TOOL_NAME);
    assert_eq!(definition["parameters"]["type"], "object");
    assert_eq!(definition["parameters"]["required"], json!(["skill_name"]));
    assert_eq!(definition["parameters"]["additionalProperties"], false);
    assert_eq!(
        definition["parameters"]["properties"]["skill_name"]["enum"],
        json!(["alpha_skill"])
    );
    assert_ne!(
        definition["parameters"]["properties"]["skill_name"]["enum"],
        json!(["alpha_skill", "beta_skill"])
    );
}

#[tokio::test]
async fn test_activate_skill_tool_succeeds_for_valid_visible_skill() {
    let catalog = sample_catalog(vec![sample_record(
        "example_skill",
        "Example description",
        "Use this skill body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["example_skill"], Arc::clone(&registry));

    let result = tool
        .execute(json!({ "skill_name": "example_skill" }))
        .await
        .expect("tool execution should succeed");

    assert!(result.success);
    assert!(result.error.is_none());
    assert_eq!(
        result.metadata.get("tool"),
        Some(&ACTIVATE_SKILL_TOOL_NAME.to_string())
    );
    assert_eq!(
        result.metadata.get("skill_name"),
        Some(&"example_skill".to_string())
    );
    assert_eq!(
        result.metadata.get("deduplicated"),
        Some(&"false".to_string())
    );

    let output: serde_json::Value =
        serde_json::from_str(&result.output).expect("output should be valid JSON");
    assert_eq!(output["activated"], true);
    assert_eq!(output["deduplicated"], false);
    assert_eq!(output["skill_name"], "example_skill");
    assert_eq!(output["skill_dir"], "/tmp/example_skill");
    assert_eq!(output["allowed_tools"], json!(["read_file", "grep"]));
    assert_eq!(output["body"], "Use this skill body");
    assert_eq!(output["resources"], json!([]));

    let locked = registry.lock().expect("registry lock should succeed");
    assert!(locked.is_active("example_skill"));
    assert_eq!(locked.len(), 1);
}

#[tokio::test]
async fn test_activate_skill_tool_duplicate_activation_does_not_duplicate_registry_state() {
    let catalog = sample_catalog(vec![sample_record(
        "example_skill",
        "Example description",
        "Use this skill body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["example_skill"], Arc::clone(&registry));

    let first = tool
        .execute(json!({ "skill_name": "example_skill" }))
        .await
        .expect("first execution should succeed");
    let second = tool
        .execute(json!({ "skill_name": "example_skill" }))
        .await
        .expect("second execution should succeed");

    let first_output: serde_json::Value =
        serde_json::from_str(&first.output).expect("first output should parse");
    let second_output: serde_json::Value =
        serde_json::from_str(&second.output).expect("second output should parse");

    assert_eq!(first_output["deduplicated"], false);
    assert_eq!(second_output["deduplicated"], true);
    assert_eq!(
        second.metadata.get("deduplicated"),
        Some(&"true".to_string())
    );

    let locked = registry.lock().expect("registry lock should succeed");
    assert_eq!(locked.len(), 1);
    assert!(locked.is_active("example_skill"));
}

#[tokio::test]
async fn test_activate_skill_tool_activation_fails_for_missing_skill() {
    let catalog = sample_catalog(vec![sample_record(
        "example_skill",
        "Example description",
        "Use this skill body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["example_skill"], registry);

    let result = tool.execute(json!({ "skill_name": "missing_skill" })).await;

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(
        message.contains("not visible")
            || message.contains("not available")
            || message.contains("missing")
    );
}

#[tokio::test]
async fn test_activate_skill_tool_activation_fails_for_invalid_skill_hidden_from_visible_schema() {
    let catalog = sample_catalog(vec![sample_record(
        "valid_skill",
        "Visible skill",
        "Visible body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["valid_skill"], registry);

    let result = tool.execute(json!({ "skill_name": "invalid_skill" })).await;

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("not visible") || message.contains("not available"));
}

#[tokio::test]
async fn test_activate_skill_tool_activation_fails_for_untrusted_project_skill_hidden_from_visible_schema(
) {
    let catalog = sample_catalog(vec![SkillRecord {
        metadata: SkillMetadata {
            name: "project_skill".to_string(),
            description: "Project-only skill".to_string(),
            license: None,
            compatibility: None,
            metadata: BTreeMap::new(),
            allowed_tools_raw: Some("read_file".to_string()),
            allowed_tools: vec!["read_file".to_string()],
        },
        skill_dir: PathBuf::from("/tmp/project_skill"),
        skill_file: PathBuf::from("/tmp/project_skill/SKILL.md"),
        source_scope: SkillSourceScope::ProjectClientSpecific,
        source_order: 0,
        body: "Project body".to_string(),
    }]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, Vec::new(), registry);

    let result = tool.execute(json!({ "skill_name": "project_skill" })).await;

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("not visible") || message.contains("not available"));
}

#[tokio::test]
async fn test_activate_skill_tool_returns_structured_wrapped_content() {
    let catalog = sample_catalog(vec![sample_record(
        "wrapped_skill",
        "Wrapped description",
        "Wrapped body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["wrapped_skill"], registry);

    let result = tool
        .execute(json!({ "skill_name": "wrapped_skill" }))
        .await
        .expect("execution should succeed");

    let output: serde_json::Value =
        serde_json::from_str(&result.output).expect("output should be valid JSON");

    assert_eq!(output["skill_name"], "wrapped_skill");
    assert_eq!(output["skill_dir"], "/tmp/wrapped_skill");
    assert_eq!(output["allowed_tools"], json!(["read_file", "grep"]));
    assert_eq!(output["body"], "Wrapped body");
    assert_eq!(output["resources"], json!([]));
}

#[test]
fn test_activate_skill_tool_not_registered_when_no_valid_visible_skills() {
    let catalog = sample_catalog(vec![sample_record(
        "hidden_skill",
        "Hidden description",
        "Hidden body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, Vec::new(), registry);

    let definition = tool.tool_definition();
    assert_eq!(definition["name"], ACTIVATE_SKILL_TOOL_NAME);
    assert_eq!(
        definition["parameters"]["properties"]["skill_name"]["enum"],
        json!([])
    );
}

#[tokio::test]
async fn test_activate_skill_tool_execute_fails_on_invalid_input_shape() {
    let catalog = sample_catalog(vec![sample_record(
        "example_skill",
        "Example description",
        "Use this skill body",
    )]);
    let registry = Arc::new(Mutex::new(ActiveSkillRegistry::new()));
    let tool = build_tool(catalog, vec!["example_skill"], registry);

    let result = tool
        .execute(json!({ "wrong_field": "example_skill" }))
        .await;

    assert!(result.is_err());
    let message = result
        .err()
        .map(|error| error.to_string())
        .unwrap_or_default();
    assert!(message.contains("Invalid activate_skill input"));
}
