//! Shared provider capability heuristics.
//!
//! This module is the single source of truth for provider/model capability
//! heuristics that are not available from a live model-list response. Callers
//! should prefer explicit [`ModelInfo`](crate::providers::ModelInfo)
//! capabilities when available and use these helpers only for static fallback
//! decisions such as ACP prompt validation.

/// Returns whether a provider/model combination is known to support vision.
///
/// The check is intentionally conservative. Unknown providers and unknown model
/// families return `false` so callers do not accept image input for models that
/// may reject it at execution time.
///
/// # Arguments
///
/// * `provider_name` - Provider name such as `openai`, `copilot`, or `ollama`.
/// * `model_name` - Selected model identifier.
///
/// # Returns
///
/// Returns `true` for allowlisted vision-capable combinations.
///
/// # Examples
///
/// ```
/// use xzatoma::providers::provider_model_supports_vision;
///
/// assert!(provider_model_supports_vision("openai", "gpt-4o-mini"));
/// assert!(provider_model_supports_vision("ollama", "llava:latest"));
/// assert!(!provider_model_supports_vision("copilot", "gpt-5-mini"));
/// ```
pub fn provider_model_supports_vision(provider_name: &str, model_name: &str) -> bool {
    let provider = provider_name.to_ascii_lowercase();
    let model = model_name.to_ascii_lowercase();

    match provider.as_str() {
        "openai" => openai_model_supports_vision(&model),
        "copilot" => false,
        "ollama" => ollama_model_supports_vision(&model),
        _ => false,
    }
}

/// Returns whether an OpenAI or OpenAI-compatible model name is known to support vision.
///
/// # Arguments
///
/// * `model` - Lowercase or mixed-case model identifier.
///
/// # Returns
///
/// Returns `true` for OpenAI model families that currently support image input.
pub fn openai_model_supports_vision(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("gpt-4o")
        || model.contains("gpt-4.1")
        || model.contains("gpt-4-turbo")
        || model.contains("vision")
        || model.contains("o3")
        || model.contains("o4")
}

/// Returns whether an Ollama model name is known to support vision.
///
/// # Arguments
///
/// * `model` - Lowercase or mixed-case model identifier.
///
/// # Returns
///
/// Returns `true` for common Ollama vision model families.
pub fn ollama_model_supports_vision(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("llava")
        || model.contains("bakllava")
        || model.contains("moondream")
        || model.contains("minicpm-v")
        || model.contains("gemma3")
        || model.contains("vision")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_model_supports_vision_openai_allowlisted_model() {
        assert!(provider_model_supports_vision("openai", "gpt-4o-mini"));
    }

    #[test]
    fn test_provider_model_supports_vision_ollama_allowlisted_model() {
        assert!(provider_model_supports_vision("ollama", "llava:latest"));
    }

    #[test]
    fn test_provider_model_supports_vision_copilot_is_false() {
        assert!(!provider_model_supports_vision("copilot", "gpt-5-mini"));
    }

    #[test]
    fn test_provider_model_supports_vision_unknown_provider_is_false() {
        assert!(!provider_model_supports_vision("unknown", "gpt-4o"));
    }
}
