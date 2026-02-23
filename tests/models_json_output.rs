use serde_json::json;
use xzatoma::providers::{ModelCapability, ModelInfo, ModelInfoSummary};

/// Integration tests to ensure JSON output for models and summaries is
/// valid and parseable by downstream tooling (e.g. `jq`, Python's json).
#[test]
fn test_list_models_json_output_parseable() {
    let a = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000)
        .with_capabilities(vec![ModelCapability::FunctionCalling]);
    let b = ModelInfo::new("llama3.2:3b", "llama3.2:3b", 131072)
        .with_capabilities(vec![ModelCapability::LongContext, ModelCapability::Vision]);

    let models = vec![a.clone(), b.clone()];
    let json = serde_json::to_string_pretty(&models).expect("serialize models to JSON");
    let parsed: Vec<ModelInfo> =
        serde_json::from_str(&json).expect("parse models JSON back into struct");

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "gpt-5.3-codex");
    assert!(parsed[1].capabilities.contains(&ModelCapability::Vision));
}

#[test]
fn test_list_models_summary_json_output_parseable() {
    let info = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000);
    let summary = ModelInfoSummary::new(
        info.clone(),
        Some("enabled".to_string()),
        Some(6144),
        Some(2048),
        Some(true),
        Some(false),
        json!({"version": "2024-01"}),
    );

    let summaries = vec![summary.clone()];
    let json = serde_json::to_string_pretty(&summaries).expect("serialize summaries");
    let parsed: Vec<ModelInfoSummary> =
        serde_json::from_str(&json).expect("parse summaries JSON back into struct");

    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0].info.name, info.name);
    assert_eq!(parsed[0].state.as_deref(), Some("enabled"));
}

#[test]
fn test_model_info_json_output_parseable() {
    let model = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000)
        .with_capabilities(vec![ModelCapability::FunctionCalling]);

    let json = serde_json::to_string_pretty(&model).expect("serialize model info");
    let parsed: ModelInfo = serde_json::from_str(&json).expect("parse model info JSON");

    assert_eq!(parsed.name, "gpt-5.3-codex");
    assert!(parsed
        .capabilities
        .contains(&ModelCapability::FunctionCalling));
}

#[test]
fn test_model_summary_json_output_parseable() {
    let info = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000);
    let summary = ModelInfoSummary::new(
        info.clone(),
        Some("disabled".to_string()),
        None,
        None,
        Some(false),
        Some(true),
        json!({"raw": "data"}),
    );

    let json = serde_json::to_string_pretty(&summary).expect("serialize model summary");
    let parsed: ModelInfoSummary =
        serde_json::from_str(&json).expect("parse model summary JSON back into struct");

    assert_eq!(parsed.info.name, info.name);
    assert_eq!(parsed.state.as_deref(), Some("disabled"));
    assert_eq!(parsed.supports_vision, Some(true));
}

#[test]
fn test_empty_model_list_json_is_array() {
    let models: Vec<ModelInfo> = Vec::new();
    let json = serde_json::to_string_pretty(&models).expect("serialize empty model list");
    assert_eq!(json.trim(), "[]");
}

#[test]
fn test_list_models_summary_table_output() {
    let info1 = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000)
        .with_capabilities(vec![ModelCapability::FunctionCalling]);
    let info2 = ModelInfo::new("llama3.2:3b", "llama3.2:3b", 131072);

    let summary1 = ModelInfoSummary::new(
        info1.clone(),
        Some("enabled".to_string()),
        Some(6144),
        Some(2048),
        Some(true),
        Some(true),
        json!({"version": "2024-01"}),
    );

    let summary2 = ModelInfoSummary::new(info2.clone(), None, None, None, None, None, json!(null));

    // Call public render helper for table output and assert on content
    let output =
        xzatoma::commands::models::render_models_summary_table(&[summary1, summary2], "copilot");
    assert!(output.contains("Available models from copilot (summary):"));
    assert!(output.contains("Model Name"));
    assert!(output.contains("Display Name"));
    assert!(output.contains("Context Window"));
    assert!(output.contains("State"));
    assert!(output.contains("Tool Calls"));
    assert!(output.contains("Vision"));
    // Check that 'Unknown' shows for missing optional booleans
    assert!(output.contains("Unknown"));
    // And that 'Yes' appears for a true boolean
    assert!(output.contains("Yes"));
}

#[test]
fn test_model_info_summary_detailed_output() {
    let mut info = ModelInfo::new("gpt-5.3-codex", "GPT-5.3 Codex", 264000);
    info.provider_specific
        .insert("policy".to_string(), "standard".to_string());

    let summary = ModelInfoSummary::new(
        info.clone(),
        Some("enabled".to_string()),
        Some(6144),
        Some(2048),
        Some(true),
        Some(false),
        json!({"meta": "value"}),
    );

    let output = xzatoma::commands::models::render_model_summary_detailed(&summary);
    assert!(output.contains("Model Information (GPT-5.3 Codex)"));
    assert!(output.contains("Name:"));
    assert!(output.contains("Display Name:"));
    assert!(output.contains("Context Window:  264000 tokens"));
    assert!(output.contains("State:           enabled"));
    assert!(output.contains("Max Prompt:      6144 tokens"));
    assert!(output.contains("Max Completion:  2048 tokens"));
    assert!(output.contains("Tool Calls:"));
    assert!(output.contains("Vision:"));
    assert!(output.contains("Provider-Specific Metadata:"));
    assert!(output.contains("policy: standard"));
    assert!(output.contains("Raw API Data Available: Yes"));
}
