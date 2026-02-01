//! Model management commands for XZatoma
//!
//! This module provides commands for discovering and managing AI models,
//! including listing available models, showing model details, and displaying
//! the currently active model.

use crate::config::Config;
use crate::error::{Result, XzatomaError};
use crate::providers;
use crate::providers::{ModelInfo, ModelInfoSummary};
use prettytable::{cell, row, Table};
use serde_json;
use std::io::Write;

/// List available models from a provider
///
/// # Arguments
///
/// * `config` - Configuration containing provider settings
/// * `provider_name` - Optional provider filter; if None, uses configured provider
///
/// # Returns
///
/// Returns Ok(()) on success, error if provider unavailable or listing fails
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::Config;
/// use xzatoma::commands::models::list_models;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::load("config/config.yaml", &Default::default())?;
/// // Pass json and summary flags (both default to false)
/// list_models(&config, None, false, false).await?;
/// # Ok(())
/// # }
/// ```
pub async fn list_models(
    config: &Config,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);

    // Acknowledge flags
    tracing::debug!(
        "models::list_models flags - json: {}, summary: {}",
        json,
        summary
    );

    tracing::info!("Listing models from provider: {}", provider_type);

    let provider = providers::create_provider(provider_type, &config.provider)?;

    // Branch on summary flag
    if summary {
        // Get full summary data
        let models_summary = provider.list_models_summary().await?;

        if models_summary.is_empty() {
            if json {
                println!("[]");
            } else {
                println!("No models available from provider: {}", provider_type);
            }
            return Ok(());
        }

        if json {
            // JSON output with summary data
            output_models_summary_json(&models_summary)?;
        } else {
            // Human-readable output with summary data
            output_models_summary_table(&models_summary, provider_type);
        }
    } else {
        // Get basic model info
        let models = provider.list_models().await?;

        if models.is_empty() {
            if json {
                println!("[]");
            } else {
                println!("No models available from provider: {}", provider_type);
            }
            return Ok(());
        }

        if json {
            // JSON output with basic data
            output_models_json(&models)?;
        } else {
            // Human-readable output (refactored)
            output_models_table(&models, provider_type);
        }
    }

    Ok(())
}

/// Show detailed information about a specific model
///
/// # Arguments
///
/// * `config` - Configuration containing provider settings
/// * `model_name` - Name/identifier of the model
/// * `provider_name` - Optional provider filter; if None, uses configured provider
///
/// # Returns
///
/// Returns Ok(()) on success, error if model not found or provider unavailable
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::Config;
/// use xzatoma::commands::models::show_model_info;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::load("config/config.yaml", &Default::default())?;
/// // Pass json and summary flags (both default to false)
/// show_model_info(&config, "gpt-4", None, false, false).await?;
/// # Ok(())
/// # }
/// ```
pub async fn show_model_info(
    config: &Config,
    model_name: &str,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);

    // Acknowledge flags to avoid unused variable warnings
    tracing::debug!(
        "models::show_model_info flags - json: {}, summary: {}",
        json,
        summary
    );

    tracing::info!(
        "Getting model info for '{}' from provider: {}",
        model_name,
        provider_type
    );

    let provider = providers::create_provider(provider_type, &config.provider)?;

    if summary {
        // Get full summary data
        let model_summary = provider.get_model_info_summary(model_name).await?;

        if json {
            // JSON output with summary data
            output_model_summary_json(&model_summary)?;
        } else {
            // Human-readable output with summary data
            output_model_summary_detailed(&model_summary);
        }
    } else {
        // Get basic model info
        let model_info = provider.get_model_info(model_name).await?;

        if json {
            // JSON output with basic data
            output_model_info_json(&model_info)?;
        } else {
            // Human-readable output (refactored)
            output_model_info_detailed(&model_info);
        }
    }

    Ok(())
}

/// Show the currently active model
///
/// # Arguments
///
/// * `config` - Configuration containing provider settings
/// * `provider_name` - Optional provider filter; if None, uses configured provider
///
/// # Returns
///
/// Returns Ok(()) on success, error if current model unavailable
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::Config;
/// use xzatoma::commands::models::show_current_model;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = Config::load("config/config.yaml", &Default::default())?;
/// show_current_model(&config, None).await?;
/// # Ok(())
/// # }
/// ```
pub async fn show_current_model(config: &Config, provider_name: Option<&str>) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);

    tracing::info!("Getting current model from provider: {}", provider_type);

    let provider = providers::create_provider(provider_type, &config.provider)?;

    let current_model = provider.get_current_model()?;

    println!("\nCurrent Model Information\n");
    println!("Provider:       {}", provider_type);
    println!("Active Model:   {}", current_model);
    println!();

    Ok(())
}

/// Serialize a serializable value into pretty JSON string.
///
/// Returns the JSON string or the serde_json error.
fn serialize_pretty<T: serde::Serialize + ?Sized>(
    value: &T,
) -> std::result::Result<String, serde_json::Error> {
    serde_json::to_string_pretty(value)
}

/// Output models in JSON format (basic data)
///
/// # Errors
///
/// Returns `XzatomaError::Serialization` if serialization fails
fn output_models_json(models: &[ModelInfo]) -> Result<()> {
    let json = serialize_pretty(models).map_err(XzatomaError::Serialization)?;
    println!("{}", json);
    Ok(())
}

/// Output models summary in JSON format
///
/// # Errors
///
/// Returns `XzatomaError::Serialization` if serialization fails
fn output_models_summary_json(models: &[ModelInfoSummary]) -> Result<()> {
    let json = serialize_pretty(models).map_err(XzatomaError::Serialization)?;
    println!("{}", json);
    Ok(())
}

/// Output models in table format (basic data)
fn output_models_table(models: &[ModelInfo], provider_type: &str) {
    let mut table = Table::new();
    table.add_row(row![
        "Model Name",
        "Display Name",
        "Context Window",
        "Capabilities"
    ]);

    for model in models {
        let capabilities = if model.capabilities.is_empty() {
            "None".to_string()
        } else {
            model
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        };

        table.add_row(row![
            model.name,
            model.display_name,
            format!("{} tokens", model.context_window),
            capabilities
        ]);
    }

    println!("\nAvailable models from {}:\n", provider_type);
    table.printstd();
    println!();
}

/// Output models summary in table format (full data)
fn output_models_summary_table(models: &[ModelInfoSummary], provider_type: &str) {
    // Print the rendered table string. Rendering is done by a helper so unit tests
    // can capture and assert on the string output without redirecting stdout.
    let output = render_models_summary_table(models, provider_type);
    print!("{}", output);
}

/// Format optional boolean for display
fn format_optional_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "Yes".to_string(),
        Some(false) => "No".to_string(),
        None => "Unknown".to_string(),
    }
}

/// Render a prettytable `Table` into a String.
///
/// Public helper used by other rendering helpers to capture table output into a
/// String buffer. Useful for testing and integration assertions.
pub fn render_table_to_string(table: &Table) -> String {
    let mut buf: Vec<u8> = Vec::new();
    // Table::print writes to any `Write` sink, so capture into a Vec<u8>.
    let _ = table.print(&mut buf);
    String::from_utf8(buf).unwrap_or_default()
}

/// Render models summary table into a string (includes header lines).
///
/// Public helper: returns the formatted table string, useful for testing and
/// integration tests that need to assert on CLI output.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::ModelInfoSummary;
/// use xzatoma::commands::models::render_models_summary_table;
///
/// let summaries: Vec<ModelInfoSummary> = vec![];
/// let _ = render_models_summary_table(&summaries, "copilot");
/// ```
pub fn render_models_summary_table(models: &[ModelInfoSummary], provider_type: &str) -> String {
    let mut table = Table::new();
    table.add_row(row![
        "Model Name",
        "Display Name",
        "Context Window",
        "State",
        "Tool Calls",
        "Vision"
    ]);

    for model in models {
        let state = model.state.as_deref().unwrap_or("unknown");
        let tool_calls = format_optional_bool(model.supports_tool_calls);
        let vision = format_optional_bool(model.supports_vision);

        table.add_row(row![
            model.info.name,
            model.info.display_name,
            format!("{} tokens", model.info.context_window),
            state,
            tool_calls,
            vision
        ]);
    }

    let mut output = String::new();
    output.push_str(&format!(
        "\nAvailable models from {} (summary):\n\n",
        provider_type
    ));
    output.push_str(&render_table_to_string(&table));
    output.push('\n');
    output
}

/// Render a `ModelInfoSummary` into a detailed string (public helper).
///
/// Returns a string containing all fields displayed in a human-readable form.
/// Useful for testing and programmatic inspection of CLI output.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::ModelInfoSummary;
/// use xzatoma::commands::models::render_model_summary_detailed;
///
/// let summary = ModelInfoSummary::new(
///     xzatoma::providers::ModelInfo::new("gpt-4", "GPT-4", 8192),
///     None,
///     None,
///     None,
///     None,
///     None,
///     serde_json::json!(null),
/// );
/// let _ = render_model_summary_detailed(&summary);
/// ```
pub fn render_model_summary_detailed(model: &ModelInfoSummary) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "\nModel Information ({})\n\n",
        model.info.display_name
    ));
    s.push_str(&format!("Name:            {}\n", model.info.name));
    s.push_str(&format!("Display Name:    {}\n", model.info.display_name));
    s.push_str(&format!(
        "Context Window:  {} tokens\n",
        model.info.context_window
    ));

    if let Some(state) = &model.state {
        s.push_str(&format!("State:           {}\n", state));
    }

    if let Some(max_prompt) = model.max_prompt_tokens {
        s.push_str(&format!("Max Prompt:      {} tokens\n", max_prompt));
    }

    if let Some(max_completion) = model.max_completion_tokens {
        s.push_str(&format!("Max Completion:  {} tokens\n", max_completion));
    }

    s.push_str("\nCapabilities:\n");
    s.push_str(&format!(
        "  Tool Calls:    {}\n",
        format_optional_bool(model.supports_tool_calls)
    ));
    s.push_str(&format!(
        "  Vision:        {}\n",
        format_optional_bool(model.supports_vision)
    ));

    if !model.info.capabilities.is_empty() {
        s.push_str(&format!(
            "  Full List:     {}\n",
            model
                .info
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    if !model.info.provider_specific.is_empty() {
        s.push_str("\nProvider-Specific Metadata:\n");
        for (key, value) in &model.info.provider_specific {
            s.push_str(&format!("  {}: {}\n", key, value));
        }
    }

    if model.raw_data != serde_json::Value::Null {
        s.push_str("\nRaw API Data Available: Yes\n");
    }

    s.push('\n');
    s
}

/// Output model info in JSON format (basic data)
fn output_model_info_json(model: &ModelInfo) -> Result<()> {
    let json = serialize_pretty(model).map_err(XzatomaError::Serialization)?;
    println!("{}", json);
    Ok(())
}

/// Output model summary in JSON format
fn output_model_summary_json(model: &ModelInfoSummary) -> Result<()> {
    let json = serialize_pretty(model).map_err(XzatomaError::Serialization)?;
    println!("{}", json);
    Ok(())
}

/// Output model info in detailed format (basic data)
fn output_model_info_detailed(model: &ModelInfo) {
    println!("\nModel Information ({})\n", model.display_name);
    println!("Name:            {}", model.name);
    println!("Display Name:    {}", model.display_name);
    println!("Context Window:  {} tokens", model.context_window);
    println!(
        "Capabilities:    {}",
        if model.capabilities.is_empty() {
            "None".to_string()
        } else {
            model
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if !model.provider_specific.is_empty() {
        println!("\nProvider-Specific Metadata:");
        for (key, value) in &model.provider_specific {
            println!("  {}: {}", key, value);
        }
    }

    println!();
}

/// Output model summary in detailed format (full data)
fn output_model_summary_detailed(model: &ModelInfoSummary) {
    let output = render_model_summary_detailed(model);
    print!("{}", output);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ModelCapability;
    use serde_json::json;

    #[test]
    fn test_output_models_json_empty_array() {
        let models: Vec<ModelInfo> = vec![];
        let json = serialize_pretty(&models).unwrap();
        assert_eq!(json, "[]");
    }

    #[test]
    fn test_output_models_json_single_model() {
        let model = ModelInfo::new("gpt-4", "GPT-4", 8192)
            .with_capabilities(vec![ModelCapability::FunctionCalling]);
        let models = vec![model.clone()];
        let json = serialize_pretty(&models).unwrap();
        let parsed: Vec<ModelInfo> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, model.name);
        assert!(parsed[0]
            .capabilities
            .contains(&ModelCapability::FunctionCalling));
    }

    #[test]
    fn test_output_models_json_multiple_models() {
        let a = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let b = ModelInfo::new("llama-3", "Llama 3", 65536);
        let json = serialize_pretty(&vec![a.clone(), b.clone()]).unwrap();
        let parsed: Vec<ModelInfo> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].name, a.name);
        assert_eq!(parsed[1].name, b.name);
    }

    #[test]
    fn test_output_models_summary_json_with_full_data() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info.clone(),
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(true),
            json!({"version": "2024-01"}),
        );
        let json = serialize_pretty(&vec![summary.clone()]).unwrap();
        let parsed: Vec<ModelInfoSummary> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed[0].info.name, info.name);
        assert_eq!(parsed[0].state, Some("enabled".to_string()));
        assert_eq!(parsed[0].supports_tool_calls, Some(true));
    }

    #[test]
    fn test_output_model_info_json_basic_fields() {
        let model = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let json = serialize_pretty(&model).unwrap();
        let parsed: ModelInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "gpt-4");
        assert_eq!(parsed.display_name, "GPT-4");
    }

    #[test]
    fn test_output_model_summary_json_all_fields() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info.clone(),
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(false),
            json!({"meta": "value"}),
        );
        let json = serialize_pretty(&summary).unwrap();
        let parsed: ModelInfoSummary = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.info.name, info.name);
        assert_eq!(parsed.max_prompt_tokens, Some(6144));
        assert_eq!(parsed.supports_vision, Some(false));
    }

    #[test]
    fn test_format_optional_bool_values() {
        assert_eq!(format_optional_bool(Some(true)), "Yes");
        assert_eq!(format_optional_bool(Some(false)), "No");
        assert_eq!(format_optional_bool(None), "Unknown");
    }

    #[test]
    fn test_output_models_json_returns_ok() {
        let model = ModelInfo::new("gpt-test", "GPT Test", 4096);
        let models = vec![model];
        assert!(output_models_json(&models).is_ok());
    }

    #[test]
    fn test_output_models_summary_json_returns_ok() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            Some(true),
            json!({"version": "2024-01"}),
        );
        let summaries = vec![summary];
        assert!(output_models_summary_json(&summaries).is_ok());
    }

    #[test]
    fn test_output_model_info_json_returns_ok() {
        let model = ModelInfo::new("gpt-test", "GPT Test", 4096);
        assert!(output_model_info_json(&model).is_ok());
    }

    #[test]
    fn test_output_model_summary_json_returns_ok() {
        let info = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let summary = ModelInfoSummary::new(
            info,
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(false),
            Some(false),
            json!({"meta": "value"}),
        );
        assert!(output_model_summary_json(&summary).is_ok());
    }

    #[test]
    fn test_list_models_summary_table_output() {
        let info1 = ModelInfo::new("gpt-4", "GPT-4", 8192);
        let info2 = ModelInfo::new("llama-3", "Llama 3", 65536);

        let summary1 = ModelInfoSummary::new(
            info1.clone(),
            Some("enabled".to_string()),
            Some(6144),
            Some(2048),
            Some(true),
            None,
            json!({"version": "2024-01"}),
        );

        let summary2 = ModelInfoSummary::new(
            info2.clone(),
            None,
            None,
            None,
            None,
            Some(true),
            json!(null),
        );

        let output = render_models_summary_table(&[summary1, summary2], "copilot");
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
        let mut info = ModelInfo::new("gpt-4", "GPT-4", 8192);
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

        let output = render_model_summary_detailed(&summary);
        assert!(output.contains("Model Information (GPT-4)"));
        assert!(output.contains("Name:"));
        assert!(output.contains("Display Name:"));
        assert!(output.contains("Context Window:  8192 tokens"));
        assert!(output.contains("State:           enabled"));
        assert!(output.contains("Max Prompt:      6144 tokens"));
        assert!(output.contains("Max Completion:  2048 tokens"));
        assert!(output.contains("Tool Calls:"));
        assert!(output.contains("Vision:"));
        assert!(output.contains("Provider-Specific Metadata:"));
        assert!(output.contains("policy: standard"));
        assert!(output.contains("Raw API Data Available: Yes"));
    }
}
