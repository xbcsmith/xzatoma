use serde_json::json;
use xzatoma::providers::{ModelCapability, ModelInfo, ModelInfoSummary};

/// Integration tests to ensure JSON output for models and summaries is
/// valid and parseable by downstream tooling (e.g. `jq`, Python's json).
#[test]
fn test_list_models_json_output_parseable() {
    let a = ModelInfo::new("gpt-4", "GPT-4", 8192)
        .with_capabilities(vec![ModelCapability::FunctionCalling]);
    let b = ModelInfo::new("llama-3", "Llama 3", 65536)
        .with_capabilities(vec![ModelCapability::LongContext, ModelCapability::Vision]);

    let models = vec![a.clone(), b.clone()];
    let json = serde_json::to_string_pretty(&models).expect("serialize models to JSON");
    let parsed: Vec<ModelInfo> =
        serde_json::from_str(&json).expect("parse models JSON back into struct");

    assert_eq!(parsed.len(), 2);
    assert_eq!(parsed[0].name, "gpt-4");
    assert!(parsed[1].capabilities.contains(&ModelCapability::Vision));
}

#[test]
fn test_list_models_summary_json_output_parseable() {
    let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
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
    let model = ModelInfo::new("gpt-4", "GPT-4", 8192)
        .with_capabilities(vec![ModelCapability::FunctionCalling]);

    let json = serde_json::to_string_pretty(&model).expect("serialize model info");
    let parsed: ModelInfo = serde_json::from_str(&json).expect("parse model info JSON");

    assert_eq!(parsed.name, "gpt-4");
    assert!(parsed
        .capabilities
        .contains(&ModelCapability::FunctionCalling));
}

#[test]
fn test_model_summary_json_output_parseable() {
    let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
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
