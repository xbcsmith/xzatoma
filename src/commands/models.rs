//! Model management commands for XZatoma
//!
//! This module provides commands for discovering and managing AI models,
//! including listing available models, showing model details, and displaying
//! the currently active model.

use crate::config::Config;
use crate::error::Result;
use crate::providers;
use prettytable::{cell, row, Table};

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
/// list_models(&config, None).await?;
/// # Ok(())
/// # }
/// ```
pub async fn list_models(config: &Config, provider_name: Option<&str>) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);

    tracing::info!("Listing models from provider: {}", provider_type);

    let provider = providers::create_provider(provider_type, &config.provider)?;

    let models = provider.list_models().await?;

    if models.is_empty() {
        println!("No models available from provider: {}", provider_type);
        return Ok(());
    }

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
/// show_model_info(&config, "gpt-4", None).await?;
/// # Ok(())
/// # }
/// ```
pub async fn show_model_info(
    config: &Config,
    model_name: &str,
    provider_name: Option<&str>,
) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);

    tracing::info!(
        "Getting model info for '{}' from provider: {}",
        model_name,
        provider_type
    );

    let provider = providers::create_provider(provider_type, &config.provider)?;

    let model_info = provider.get_model_info(model_name).await?;

    println!("\nModel Information ({})\n", model_info.display_name);
    println!("Name:            {}", model_info.name);
    println!("Display Name:    {}", model_info.display_name);
    println!("Context Window:  {} tokens", model_info.context_window);
    println!(
        "Capabilities:    {}",
        if model_info.capabilities.is_empty() {
            "None".to_string()
        } else {
            model_info
                .capabilities
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        }
    );

    if !model_info.provider_specific.is_empty() {
        println!("\nProvider-Specific Metadata:");
        for (key, value) in &model_info.provider_specific {
            println!("  {}: {}", key, value);
        }
    }

    println!();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_models_module_compiles() {
        // This test verifies that the models module compiles correctly
        // The actual functionality is tested via integration tests with mock providers
    }
}
