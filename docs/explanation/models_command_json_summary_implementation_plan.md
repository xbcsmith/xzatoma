# Models Command JSON and Summary Flags Implementation Plan

## Overview

This plan details the implementation of `--json` and `--summary` flags for the `models list` and `models info` subcommands. These flags will enable machine-readable JSON output and detailed summary views with full API data.

**Features to Implement:**

- `--json` flag: Output model data in JSON format
- `--summary` flag: Display full data from models API endpoint
- Combined `--summary --json`: Return complete API data in JSON format
- Combined `--summary` alone: Return full data in well-formatted human-readable text
- Default behavior (no flags): Current concise human-readable output

## Current State Analysis

### Existing Infrastructure

**CLI Structure** (`src/cli.rs`):

- `ModelCommand::List { provider: Option<String> }`
- `ModelCommand::Info { model: String, provider: Option<String> }`
- `ModelCommand::Current { provider: Option<String> }`

**Implementation** (`src/commands/models.rs`):

- `list_models()`: Uses `prettytable` to format model list with name, display name, context window, and capabilities
- `show_model_info()`: Displays model details with provider-specific metadata
- `show_current_model()`: Shows active model name

**Data Structures** (`src/providers/base.rs`):

- `ModelInfo`: Contains `name`, `display_name`, `context_window`, `capabilities`, `provider_specific` (HashMap)
- `ModelCapability`: Enum with variants like `ToolCalls`, `Vision`, etc.

**Provider API Data** (`src/providers/copilot.rs`):

- `CopilotModelsResponse`: Contains `data: Vec<CopilotModelData>`
- `CopilotModelData`: Full API response with `id`, `name`, `capabilities`, `policy`
- `CopilotModelCapabilities`: Contains `limits`, `supports`
- `CopilotModelLimits`: Contains `max_context_window_tokens`
- `CopilotModelSupports`: Contains `tool_calls`, `vision` flags
- `CopilotModelPolicy`: Contains `state` field

**Current Limitations:**

- No JSON output option for automation/scripting
- Summary data (policy, state, detailed capabilities) is discarded during conversion to `ModelInfo`
- Only basic fields exposed in human-readable output
- No access to full provider-specific metadata

### Identified Issues

1. **Data Loss**: Provider API returns rich data (policy, state, limits) that is not exposed to users
2. **No Machine-Readable Format**: CLI output is human-only; scripts cannot parse it reliably
3. **Limited Observability**: Users cannot see full model details for debugging or detailed comparison
4. **Inflexible Output**: Single output format cannot serve both quick checks and detailed analysis
5. **Provider-Specific Metadata**: HashMap is generic; no typed access to known fields

## Implementation Phases

### Phase 1: CLI Flag Definition and Parsing

**Objective**: Add `--json` and `--summary` flags to `ModelCommand::List` and `ModelCommand::Info` variants.

#### Task 1.1: Update CLI Data Structures

**File**: `src/cli.rs`

**Changes**:

- Add `json: bool` field to `ModelCommand::List`
- Add `summary: bool` field to `ModelCommand::List`
- Add `json: bool` field to `ModelCommand::Info`
- Add `summary: bool` field to `ModelCommand::Info`

**Expected Enum Variants**:

```rust
ModelCommand::List {
    provider: Option<String>,
    #[arg(short, long)]
    json: bool,
    #[arg(short = 's', long)]
    summary: bool,
}

ModelCommand::Info {
    model: String,
    provider: Option<String>,
    #[arg(short, long)]
    json: bool,
    #[arg(short = 's', long)]
    summary: bool,
}
```

**Flag Behavior**:

- `--json` short form: `-j`
- `--summary` short form: `-s`
- Both flags are optional (default: `false`)
- Flags are independent and can be combined

#### Task 1.2: Update Command Dispatcher

**File**: `src/main.rs`

**Changes**:

- Update `ModelCommand::List` match arm to pass `json` and `summary` flags
- Update `ModelCommand::Info` match arm to pass `json` and `summary` flags

**Expected Call Signatures**:

```rust
ModelCommand::List { provider, json, summary } => {
    commands::models::list_models(&config, provider.as_deref(), json, summary).await?;
}

ModelCommand::Info { model, provider, json, summary } => {
    commands::models::show_model_info(&config, &model, provider.as_deref(), json, summary).await?;
}
```

#### Task 1.3: Update Command Function Signatures

**File**: `src/commands/models.rs`

**Changes**:

- Update `list_models()` signature: add `json: bool` and `summary: bool` parameters
- Update `show_model_info()` signature: add `json: bool` and `summary: bool` parameters

**New Signatures**:

```rust
pub async fn list_models(
    config: &Config,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()>

pub async fn show_model_info(
    config: &Config,
    model_name: &str,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()>
```

#### Task 1.4: Testing Requirements

**Unit Tests** (`src/cli.rs`):

- `test_cli_parse_models_list_with_json`
- `test_cli_parse_models_list_with_summary`
- `test_cli_parse_models_list_with_json_and_summary`
- `test_cli_parse_models_info_with_json`
- `test_cli_parse_models_info_with_summary`
- `test_cli_parse_models_info_with_json_and_summary`
- `test_cli_parse_models_list_short_flags` (verify `-j` and `-s`)
- `test_cli_parse_models_info_short_flags` (verify `-j` and `-s`)

**Test Coverage Target**: 100% of new CLI parsing code paths

#### Task 1.5: Deliverables

- Updated `ModelCommand` enum with `json` and `summary` fields (2 variants modified)
- Updated command dispatcher in `main.rs` (2 match arms modified)
- Updated function signatures in `commands/models.rs` (2 functions)
- 8 new CLI parsing tests
- All existing CLI tests pass without modification

#### Task 1.6: Success Criteria

- `cargo fmt --all` passes
- `cargo check --all-targets --all-features` passes with zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- `cargo test --all-features` passes (existing + 8 new tests)
- CLI help text shows new flags: `xzatoma models list --help` and `xzatoma models info --help`
- Flags parse correctly but behavior unchanged (implementation in Phase 2)

---

### Phase 2: Enhanced Data Structures for Summary Data

**Objective**: Create typed structures to capture full provider API data for summary output.

#### Task 2.1: Define Summary Data Structures

**File**: `src/providers/base.rs`

**Imports Required**:

```rust
// Add to existing imports at top of file (after line 1):
use serde_json;
```

**New Structures**:

```rust
/// Extended model information with full provider API data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfoSummary {
    /// Core model information
    pub info: ModelInfo,

    /// Provider API state (e.g., "enabled", "disabled")
    pub state: Option<String>,

    /// Maximum prompt tokens allowed
    pub max_prompt_tokens: Option<usize>,

    /// Maximum completion tokens allowed
    pub max_completion_tokens: Option<usize>,

    /// Whether the model supports tool calls
    pub supports_tool_calls: Option<bool>,

    /// Whether the model supports vision/image input
    pub supports_vision: Option<bool>,

    /// Raw provider-specific data (fallback for unknown fields)
    pub raw_data: serde_json::Value,
}
```

**Constants** (add after line 205, after ModelCapability Display impl):

```rust
/// Default context window size when not provided by API
const DEFAULT_CONTEXT_WINDOW: usize = 4096;
```

**Helper Methods**:

```rust
impl ModelInfoSummary {
    /// Create summary from core ModelInfo
    pub fn from_model_info(info: ModelInfo) -> Self {
        Self {
            info,
            state: None,
            max_prompt_tokens: None,
            max_completion_tokens: None,
            supports_tool_calls: None,
            supports_vision: None,
            raw_data: serde_json::Value::Null,
        }
    }

    /// Create summary with full data
    pub fn new(
        info: ModelInfo,
        state: Option<String>,
        max_prompt_tokens: Option<usize>,
        max_completion_tokens: Option<usize>,
        supports_tool_calls: Option<bool>,
        supports_vision: Option<bool>,
        raw_data: serde_json::Value,
    ) -> Self {
        Self {
            info,
            state,
            max_prompt_tokens,
            max_completion_tokens,
            supports_tool_calls,
            supports_vision,
            raw_data,
        }
    }

    /// Builder method to set capabilities (for fluent API)
    pub fn with_capabilities(mut self, capabilities: Vec<ModelCapability>) -> Self {
        self.capabilities = capabilities;
        self
    }
}
```

**Module Exports** (update `src/providers/mod.rs`):

```rust
pub use base::{
    // ... existing exports ...
    ModelInfoSummary,  // ADD THIS
};
```

#### Task 2.2: Add Provider Trait Methods

**File**: `src/providers/base.rs`

**New Methods**:

```rust
pub trait Provider {
    // ... existing methods ...

    /// List models with full summary data
    ///
    /// Default implementation converts basic ModelInfo to ModelInfoSummary
    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models = self.list_models().await?;
        Ok(models.into_iter()
            .map(ModelInfoSummary::from_model_info)
            .collect())
    }

    /// Get model info with full summary data
    ///
    /// Default implementation converts basic ModelInfo to ModelInfoSummary
    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let info = self.get_model_info(model_name).await?;
        Ok(ModelInfoSummary::from_model_info(info))
    }
}
```

**Note**: Default implementations provide backward compatibility for providers that don't need summary data.

#### Task 2.3: Update Copilot Provider Implementation

**File**: `src/providers/copilot.rs`

**Imports Required**:

```rust
// Verify these are already imported; if not, add:
use serde_json;
```

**Make API Structs Public** (for serialization):

- Change `struct CopilotModelsResponse` to `pub(crate) struct` (line ~196)
- Change `struct CopilotModelData` to `pub(crate) struct` (line ~202)
- Add `#[derive(Serialize)]` to `CopilotModelData`, `CopilotModelCapabilities`, `CopilotModelLimits`, `CopilotModelSupports`, `CopilotModelPolicy`

**Implement Summary Methods**:

```rust
impl Provider for CopilotProvider {
    // ... existing methods ...

    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models_data = self.fetch_copilot_models_raw().await?;
        Ok(models_data.into_iter()
            .map(|data| self.convert_to_summary(data))
            .collect())
    }

    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let models_data = self.fetch_copilot_models_raw().await?;
        let data = models_data.into_iter()
            .find(|m| m.id == model_name || m.name == model_name)
            .ok_or_else(|| XzatomaError::Provider(
                format!("Model '{}' not found", model_name)
            ))?;
        Ok(self.convert_to_summary(data))
    }
}

impl CopilotProvider {
    /// Fetch raw Copilot model data without conversion
    async fn fetch_copilot_models_raw(&self) -> Result<Vec<CopilotModelData>> {
        let response = self.fetch_copilot_models_response().await?;
        Ok(response.data)
    }

    /// Convert CopilotModelData to ModelInfoSummary
    fn convert_to_summary(&self, data: CopilotModelData) -> ModelInfoSummary {
        let context_window = data.capabilities
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.max_context_window_tokens)
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);

        let supports_tool_calls = data.capabilities
            .as_ref()
            .and_then(|c| c.supports.as_ref())
            .and_then(|s| s.tool_calls);

        let supports_vision = data.capabilities
            .as_ref()
            .and_then(|c| c.supports.as_ref())
            .and_then(|s| s.vision);

        let state = data.policy
            .as_ref()
            .map(|p| p.state.clone());

        // Build capabilities vector
        let mut capabilities = Vec::new();
        if supports_tool_calls == Some(true) {
            capabilities.push(ModelCapability::FunctionCalling);
        }
        if supports_vision == Some(true) {
            capabilities.push(ModelCapability::Vision);
        }

        let info = ModelInfo::new(&data.id, &data.name, context_window)
            .with_capabilities(capabilities);

        let raw_data = serde_json::to_value(&data).unwrap_or(serde_json::Value::Null);

        ModelInfoSummary::new(
            info,
            state,
            None, // max_prompt_tokens not in Copilot API
            None, // max_completion_tokens not in Copilot API
            supports_tool_calls,
            supports_vision,
            raw_data,
        )
    }
}
```

**Refactor `fetch_copilot_models()`**:

- Extract response fetching to `fetch_copilot_models_response()`
- Call new method from both `fetch_copilot_models()` and `fetch_copilot_models_raw()`

#### Task 2.4: Update Ollama Provider Implementation

**File**: `src/providers/ollama.rs`

**Imports Required**:

```rust
// Verify these are already imported; if not, add:
use serde_json;
```

**Implementation**:

Implement the Provider trait methods for summary data:

```rust
impl Provider for OllamaProvider {
    // ... existing methods ...

    async fn list_models_summary(&self) -> Result<Vec<ModelInfoSummary>> {
        let models_data = self.fetch_ollama_models_raw().await?;
        Ok(models_data.into_iter()
            .map(|data| self.convert_to_summary(data))
            .collect())
    }

    async fn get_model_info_summary(&self, model_name: &str) -> Result<ModelInfoSummary> {
        let models_data = self.fetch_ollama_models_raw().await?;
        let data = models_data.into_iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| XzatomaError::Provider(
                format!("Model '{}' not found", model_name)
            ))?;
        Ok(self.convert_to_summary(data))
    }
}

impl OllamaProvider {
    /// Fetch raw Ollama model data without conversion
    async fn fetch_ollama_models_raw(&self) -> Result<Vec<OllamaModelData>> {
        // Extract from existing list_models() implementation
        // Ollama API returns different structure than Copilot
        // Adapt based on actual Ollama API response format
        let response = self.fetch_ollama_models_response().await?;
        Ok(response.models)
    }

    /// Convert OllamaModelData to ModelInfoSummary
    fn convert_to_summary(&self, data: OllamaModelData) -> ModelInfoSummary {
        // Ollama API structure differs from Copilot
        // Map available fields from Ollama response:
        // - name: Model identifier
        // - size: Model size (optional metadata)
        // - digest: Model version hash (optional metadata)
        // - modified_at: Last update time (optional metadata)

        // Note: Ollama may not provide all fields that Copilot does
        // Use None for unavailable fields

        let context_window = data.context_window
            .unwrap_or(DEFAULT_CONTEXT_WINDOW);

        // Ollama doesn't expose capability flags in same way as Copilot
        // Infer from model name or use empty capabilities
        let capabilities = Vec::new();

        let info = ModelInfo::new(&data.name, &data.name, context_window)
            .with_capabilities(capabilities);

        let raw_data = serde_json::to_value(&data).unwrap_or(serde_json::Value::Null);

        ModelInfoSummary::new(
            info,
            None, // state - not provided by Ollama API
            None, // max_prompt_tokens - not provided by Ollama API
            None, // max_completion_tokens - not provided by Ollama API
            None, // supports_tool_calls - not explicitly provided by Ollama
            None, // supports_vision - not explicitly provided by Ollama
            raw_data,
        )
    }
}
```

**Note**: Ollama API provides less detailed capability information than Copilot. Fields not available in Ollama API are set to `None`. The raw_data field contains the complete Ollama response for advanced users who need access to Ollama-specific fields.

#### Task 2.5: Testing Requirements

**Unit Tests** (`src/providers/base.rs`):

- `test_model_info_summary_from_model_info`
- `test_model_info_summary_new`
- `test_model_info_summary_serialization`
- `test_model_info_summary_deserialization`

**Unit Tests** (`src/providers/copilot.rs`):

- `test_convert_to_summary_full_data`
- `test_convert_to_summary_minimal_data`
- `test_convert_to_summary_missing_capabilities`
- `test_convert_to_summary_missing_policy`
- `test_list_models_summary_returns_full_data`
- `test_get_model_info_summary_returns_full_data`

**Unit Tests** (`src/providers/ollama.rs`):

- Similar tests for Ollama-specific summary conversion

**Integration Tests**:

- Mock provider responses and verify summary data extraction
- Test default trait implementation fallback

**Test Coverage Target**: >80% for new code paths

#### Task 2.6: Deliverables

- `ModelInfoSummary` struct in `src/providers/base.rs` (~70 lines)
- `with_capabilities` builder method in `ModelInfo` impl (~10 lines)
- New trait methods `list_models_summary()` and `get_model_info_summary()` (~30 lines)
- Module export updates in `src/providers/mod.rs` (1 line)
- Copilot provider summary implementation (~120 lines)
- Ollama provider summary implementation (~80 lines)
- 15+ new unit tests
- Updated documentation strings with examples for all new public functions

#### Task 2.7: Success Criteria

- `cargo fmt --all` passes with no changes
- `cargo check --all-targets --all-features` passes with zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- `cargo test --all-features` passes with >80% coverage
- ALL existing tests pass without modification (backward compatibility verified)
- `ModelInfoSummary` serializes/deserializes correctly to/from JSON
- Copilot provider returns complete summary data with all available fields populated
- Ollama provider returns summary data with fields set to None where unavailable
- Default trait implementation works for providers without summary support
- All new public functions have doc comments with runnable examples

---

### Phase 3: JSON Output Implementation

**Objective**: Implement JSON serialization for model data based on `--json` flag.

#### Task 3.1: JSON Output for List Command

**File**: `src/commands/models.rs`

**Imports Required**:

```rust
// Add to existing imports at top of file:
use serde_json;
```

**Update `list_models()` Function**:

```rust
pub async fn list_models(
    config: &Config,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);
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
            // Current human-readable output (existing code)
            output_models_table(&models, provider_type);
        }
    }

    Ok(())
}
```

**Helper Functions**:

```rust
/// Output models in JSON format (basic data)
fn output_models_json(models: &[ModelInfo]) -> Result<()> {
    let json = serde_json::to_string_pretty(models)
        .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize models: {}", e)))?;
    println!("{}", json);
    Ok(())
}

/// Output models summary in JSON format
fn output_models_summary_json(models: &[ModelInfoSummary]) -> Result<()> {
    let json = serde_json::to_string_pretty(models)
        .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize models summary: {}", e)))?;
    println!("{}", json);
    Ok(())
}

/// Output models in table format (basic data) - existing code refactored
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
            model.capabilities.iter()
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

    println!("\nAvailable models from {} (summary):\n", provider_type);
    table.printstd();
    println!();
}

/// Format optional boolean for display
fn format_optional_bool(value: Option<bool>) -> String {
    match value {
        Some(true) => "Yes",
        Some(false) => "No",
        None => "Unknown",
    }.to_string()
}
```

#### Task 3.2: JSON Output for Info Command

**File**: `src/commands/models.rs`

**Update `show_model_info()` Function**:

```rust
pub async fn show_model_info(
    config: &Config,
    model_name: &str,
    provider_name: Option<&str>,
    json: bool,
    summary: bool,
) -> Result<()> {
    let provider_type = provider_name.unwrap_or(&config.provider.provider_type);
    tracing::info!("Getting model info for '{}' from provider: {}", model_name, provider_type);

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
            // Current human-readable output (existing code)
            output_model_info_detailed(&model_info);
        }
    }

    Ok(())
}
```

**Helper Functions**:

```rust
/// Output model info in JSON format (basic data)
fn output_model_info_json(model: &ModelInfo) -> Result<()> {
    let json = serde_json::to_string_pretty(model)
        .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize model info: {}", e)))?;
    println!("{}", json);
    Ok(())
}

/// Output model summary in JSON format
fn output_model_summary_json(model: &ModelInfoSummary) -> Result<()> {
    let json = serde_json::to_string_pretty(model)
        .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize model summary: {}", e)))?;
    println!("{}", json);
    Ok(())
}

/// Output model info in detailed format (basic data) - existing code refactored
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
            model.capabilities.iter()
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
    println!("\nModel Information ({})\n", model.info.display_name);
    println!("Name:            {}", model.info.name);
    println!("Display Name:    {}", model.info.display_name);
    println!("Context Window:  {} tokens", model.info.context_window);

    if let Some(state) = &model.state {
        println!("State:           {}", state);
    }

    if let Some(max_prompt) = model.max_prompt_tokens {
        println!("Max Prompt:      {} tokens", max_prompt);
    }

    if let Some(max_completion) = model.max_completion_tokens {
        println!("Max Completion:  {} tokens", max_completion);
    }

    println!("\nCapabilities:");
    println!("  Tool Calls:    {}", format_optional_bool(model.supports_tool_calls));
    println!("  Vision:        {}", format_optional_bool(model.supports_vision));

    if !model.info.capabilities.is_empty() {
        println!("  Full List:     {}",
            model.info.capabilities.iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    if !model.info.provider_specific.is_empty() {
        println!("\nProvider-Specific Metadata:");
        for (key, value) in &model.info.provider_specific {
            println!("  {}: {}", key, value);
        }
    }

    if model.raw_data != serde_json::Value::Null {
        println!("\nRaw API Data Available: Yes");
    }

    println!();
}
```

#### Task 3.3: Add Serialization Error Type

**File**: `src/error.rs`

**Imports Required**: None (serde should already be imported)

**Add Error Variant** (add to existing XzatomaError enum):

```rust
#[derive(Error, Debug)]
pub enum XzatomaError {
    // ... existing variants ...

    /// Serialization error
    #[error("Serialization error: {0}")]
    Serialization(String),
}
```

**Usage Example**:

```rust
// In output_models_json function:
let json = serde_json::to_string_pretty(models)
    .map_err(|e| XzatomaError::Serialization(format!("Failed to serialize models: {}", e)))?;
```

#### Task 3.4: Testing Requirements

**Unit Tests** (`src/commands/models.rs`):

- `test_output_models_json_empty_array`
- `test_output_models_json_single_model`
- `test_output_models_json_multiple_models`
- `test_output_models_summary_json_with_full_data`
- `test_output_model_info_json_basic_fields`
- `test_output_model_summary_json_all_fields`
- `test_format_optional_bool_values`

**Integration Tests**:

- `test_list_models_json_output_parseable`
- `test_list_models_summary_json_output_parseable`
- `test_model_info_json_output_parseable`
- `test_model_summary_json_output_parseable`
- Verify JSON output can be parsed back into structs

**Test Coverage Target**: >80% for JSON output functions

#### Task 3.5: Deliverables

- Updated `list_models()` with branching logic (~80 lines modified)
- Updated `show_model_info()` with branching logic (~60 lines modified)
- 8 new helper functions for output formatting (~200 lines)
- New `Serialization` error variant in `src/error.rs`
- 11+ new unit/integration tests in `#[cfg(test)] mod tests` within `src/commands/models.rs`
- Integration tests in `tests/models_json_output.rs` (create new file)
- Refactored existing output code into helper functions
- All new functions have doc comments with runnable examples

#### Task 3.6: Success Criteria

- `cargo fmt --all` passes with no changes
- `cargo check --all-targets --all-features` passes with zero errors
- `cargo clippy --all-targets --all-features -- -D warnings` shows zero warnings
- `cargo test --all-features` passes with >80% coverage
- ALL existing tests pass without modification (backward compatibility verified)
- `xzatoma models list --json` outputs valid, parseable JSON array
- `xzatoma models info --model gpt-4 --json` outputs valid, parseable JSON object
- JSON output can be parsed by `jq`: `xzatoma models list --json | jq .` succeeds
- Empty model lists produce `[]` in JSON mode
- Serialization errors are handled gracefully and produce clear error messages
- Default (no flags) output remains unchanged from current behavior

---

### Phase 4: Summary Output Implementation

**Objective**: Implement human-readable summary output for `--summary` flag (already covered in Phase 3 helper functions).

#### Task 4.1: Verify Summary Table Output

**Validation**:

- `output_models_summary_table()` function implemented in Phase 3.1
- Displays exactly these columns: Name, Display Name, Context Window, State, Tool Calls, Vision
- Table headers are properly aligned using prettytable library
- Optional fields show "Unknown" when value is None
- Table format matches existing models.rs table structure

#### Task 4.2: Verify Summary Detailed Output

**Validation**:

- `output_model_summary_detailed()` function implemented in Phase 3.2
- Displays all available fields from `ModelInfoSummary`: name, display_name, context_window, state, max_prompt_tokens, max_completion_tokens, supports_tool_calls, supports_vision
- Grouped sections: Basic Info, Capabilities, Provider-Specific Metadata
- Shows "Raw API Data Available: Yes" when raw_data is not Null
- Format is consistent with existing `output_model_info_detailed()` function style

#### Task 4.3: Testing Requirements

**Manual Testing Checklist**:

- [ ] `xzatoma models list --summary` displays extended table with all 6 columns
- [ ] `xzatoma models info --model gpt-4 --summary` shows all available fields
- [ ] Column headers are aligned and readable
- [ ] Optional fields show "Unknown" (not empty or error)
- [ ] Output matches XZatoma CLI style (consistent with existing commands)

**Integration Tests** (in `tests/models_json_output.rs`):

- `test_list_models_summary_table_output` - verify table structure and content
- `test_model_info_summary_detailed_output` - verify all fields displayed
- Capture stdout and verify formatting programmatically

**Test Coverage Target**: >80% for summary output functions

**Backward Compatibility Validation**:

```bash
# Verify existing behavior unchanged
cargo test --all-features 2>&1 | tee test_results.txt
grep "test result:" test_results.txt
# Should show increased test count but all passing
```

#### Task 4.4: Deliverables

- No new code (implemented in Phase 3)
- Verification tests (2 integration tests)
- Manual testing documentation

#### Task 4.5: Success Criteria

- `cargo test --all-features` passes with >80% coverage
- ALL existing tests pass without modification (backward compatibility verified)
- Summary table includes all required columns: Name, Display Name, Context Window, State, Tool Calls, Vision
- Summary detailed output displays all fields: name, display_name, context_window, state, max_prompt/completion tokens, supports_tool_calls, supports_vision
- Table format uses prettytable library consistent with existing code
- Optional fields display "Unknown" when None (not empty string or missing)
- Output follows existing XZatoma CLI conventions (same spacing, formatting style)

---

### Phase 5: Documentation and Examples

**Objective**: Document new flags and provide usage examples.

#### Task 5.1: Update Reference Documentation

**File**: `docs/reference/model_management.md`

**Add Sections**:

```markdown
## JSON Output

The `models` command supports JSON output for automation and scripting:

### List Models in JSON

xzatoma models list --json

Output:
[
{
"name": "gpt-4",
"display_name": "GPT-4",
"context_window": 8192,
"capabilities": ["ToolCalls"],
"provider_specific": {}
}
]

### Model Info in JSON

xzatoma models info --model gpt-4 --json

## Summary Output

The `--summary` flag displays full API data:

### List Models with Summary

xzatoma models list --summary

### Model Info with Summary

xzatoma models info --model gpt-4 --summary

### Combined JSON and Summary

For complete machine-readable data:

xzatoma models list --summary --json
xzatoma models info --model gpt-4 --summary --json
```

#### Task 5.2: Update How-To Guide

**File**: `docs/how-to/manage_models.md`

**Add Sections**:

```markdown
## Exporting Model Data

### Export All Models to JSON

xzatoma models list --json > models.json

### Export Specific Model Details

xzatoma models info --model gpt-4 --json > gpt4-info.json

### Get Full API Data for Analysis

xzatoma models list --summary --json > models-full.json

## Comparing Models

### View Detailed Comparison

xzatoma models info --model gpt-4 --summary
xzatoma models info --model llama3.2:13b --summary --provider ollama

### Script-Based Comparison

models=$(xzatoma models list --json)
echo "$models" | jq '.[] | select(.context_window > 8000)'
```

#### Task 5.3: Add Usage Examples

**File**: `docs/explanation/models_command_usage_examples.md`

**Content**:

- Basic usage of `--json` and `--summary` flags
- Scripting examples with `jq`
- Integration with CI/CD pipelines
- Comparison workflows
- Error handling in scripts

**Examples**:

```bash
# Get models that support tool calls
xzatoma models list --json | jq '.[] | select(.capabilities[] == "ToolCalls")'

# Compare context windows
xzatoma models list --summary --json | jq -r '.[] | "\(.info.name): \(.info.context_window)"'

# Get model state
xzatoma models info --model gpt-4 --summary --json | jq '.state'
```

#### Task 5.4: Update CLI Help Text

**File**: `src/cli.rs`

**Update Documentation Strings** (modify existing enum variants):

```rust
/// List available models
List {
    /// Filter by provider (copilot, ollama)
    #[arg(short, long)]
    provider: Option<String>,

    /// Output in JSON format
    #[arg(short, long)]
    json: bool,

    /// Display full model data from API
    #[arg(short = 's', long)]
    summary: bool,
},
```

**Expected Help Output** (verify with `xzatoma models list --help`):

```
List available models

Usage: xzatoma models list [OPTIONS]

Options:
  -p, --provider <PROVIDER>  Filter by provider (copilot, ollama)
  -j, --json                 Output in JSON format
  -s, --summary              Display full model data from API
  -h, --help                 Print help
```

#### Task 5.5: Testing Requirements

**Documentation Tests**:

- Verify all code examples compile (use `cargo test --doc`)
- Test JSON examples with actual output
- Validate `jq` examples work correctly

**Validation**:

- `cargo doc --no-deps --open` generates documentation without errors
- All examples in doc comments are runnable

#### Task 5.6: Deliverables

- Updated `docs/reference/model_management.md` (~100 lines added to existing file)
- Updated `docs/how-to/manage_models.md` (~80 lines added to existing file)
- New `docs/explanation/models_command_usage_examples.md` (~200 lines, new file)
- Updated CLI help strings in `src/cli.rs` (4 documentation strings modified)
- 5+ tested and validated examples (all commands verified to work)
- All code examples in documentation are runnable and tested

#### Task 5.7: Success Criteria

- `cargo doc --no-deps` passes with zero warnings
- `cargo test --doc` passes (all documentation examples compile and run)
- All documentation examples are accurate and tested manually
- Help text displays correctly: `xzatoma models list --help` and `xzatoma models info --help`
- Help text shows both short (`-j`, `-s`) and long (`--json`, `--summary`) flag forms
- Examples cover all common use cases: basic usage, automation with jq, model comparison, CI/CD integration
- Documentation follows Diataxis framework:
  - Reference material in `docs/reference/` ✓
  - How-to guides in `docs/how-to/` ✓
  - Explanations in `docs/explanation/` ✓
- All filenames use lowercase_with_underscores.md ✓
- No emojis in documentation ✓

---

## Dependencies

**No New External Dependencies Required**

Existing dependencies are sufficient:

- `serde` and `serde_json`: Already used for serialization
- `prettytable`: Already used for table output
- `clap`: Already used for CLI parsing

## File Modification Summary

| File                                                | Lines Modified | Lines Added | Purpose                                                    |
| --------------------------------------------------- | -------------- | ----------- | ---------------------------------------------------------- |
| `src/cli.rs`                                        | 6 fields       | 80 tests    | Add `--json` and `--summary` flags                         |
| `src/main.rs`                                       | 4 lines        | 0           | Pass new flags to commands                                 |
| `src/commands/models.rs`                            | 150 modified   | 250 added   | Implement output formatting logic                          |
| `src/providers/base.rs`                             | 0              | 130         | Add `ModelInfoSummary`, `with_capabilities`, trait methods |
| `src/providers/mod.rs`                              | 0              | 1           | Export `ModelInfoSummary`                                  |
| `src/providers/copilot.rs`                          | 30 modified    | 150 added   | Implement summary data extraction                          |
| `src/providers/ollama.rs`                           | 20 modified    | 100 added   | Implement summary data extraction                          |
| `src/error.rs`                                      | 0              | 5           | Add `Serialization` error variant                          |
| `tests/models_json_output.rs`                       | 0              | 150         | New integration test file                                  |
| `docs/reference/model_management.md`                | 0              | 100         | Document new flags (existing file)                         |
| `docs/how-to/manage_models.md`                      | 0              | 80          | Add how-to examples (existing file)                        |
| `docs/explanation/models_command_usage_examples.md` | 0              | 200         | New example document                                       |

**Total Estimated Changes**: ~560 lines modified, ~1,246 lines added

**Note**: All existing files retain backward compatibility. New functionality is additive only.

## Validation Commands

**After Each Phase**:

```bash
cargo fmt --all
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
```

**Phase-Specific Validation**:

**Phase 1 (CLI)**:

```bash
xzatoma models list --help
xzatoma models info --help
cargo test test_cli_parse_models
```

**Phase 2 (Data Structures)**:

```bash
cargo test test_model_info_summary
cargo test providers::base::tests
cargo test providers::copilot::tests
```

**Phase 3 (JSON Output)**:

```bash
xzatoma models list --json | jq .
xzatoma models info --model gpt-4 --json | jq .
cargo test test_output_models_json
```

**Phase 4 (Summary Output)**:

```bash
xzatoma models list --summary
xzatoma models info --model gpt-4 --summary
xzatoma models list --summary --json | jq .
```

**Phase 5 (Documentation)**:

```bash
cargo doc --no-deps --open
cargo test --doc
```

## Testing Strategy

### Unit Test Coverage

**Target**: >80% coverage for all new code

**Test Locations**:

- Unit tests in `#[cfg(test)] mod tests` within respective source files
- Integration tests in `tests/models_json_output.rs` (new file)

**Categories**:

1. CLI parsing tests (8 tests in `src/cli.rs`)
2. Data structure tests (4 tests in `src/providers/base.rs`)
3. Provider conversion tests (12 tests in `src/providers/copilot.rs` and `src/providers/ollama.rs`)
4. JSON serialization tests (7 tests in `src/commands/models.rs`)
5. Output formatting tests (4 tests in `src/commands/models.rs`)

**Total New Tests**: ~35 unit tests

**Backward Compatibility Tests**:

- ALL existing tests must pass unchanged
- Verify with: `cargo test --all-features 2>&1 | grep "test result"`

### Integration Test Coverage

**Test File**: `tests/models_json_output.rs` (create new file)

**Tests**:

1. `test_list_models_json_output` - End-to-end CLI invocation with --json
2. `test_list_models_summary_json_output` - CLI with --summary --json
3. `test_model_info_json_parsing` - JSON output is valid and parseable
4. `test_summary_data_completeness` - All expected fields present
5. `test_copilot_provider_summary_extraction` - Copilot-specific data
6. `test_ollama_provider_summary_extraction` - Ollama-specific data
7. `test_json_error_handling` - Serialization error handling
8. `test_empty_models_list_json` - Empty list produces []
9. `test_model_not_found_json` - Error handling in JSON mode
10. `test_backward_compatibility` - Default behavior unchanged

**Total Integration Tests**: ~10 tests

### Manual Testing Checklist

- [ ] `xzatoma models list` (default output unchanged)
- [ ] `xzatoma models list --json` (valid JSON array)
- [ ] `xzatoma models list --summary` (extended table)
- [ ] `xzatoma models list --summary --json` (full JSON)
- [ ] `xzatoma models info --model gpt-4` (default output unchanged)
- [ ] `xzatoma models info --model gpt-4 --json` (valid JSON object)
- [ ] `xzatoma models info --model gpt-4 --summary` (extended details)
- [ ] `xzatoma models info --model gpt-4 --summary --json` (full JSON)
- [ ] `xzatoma models list --provider copilot --json`
- [ ] `xzatoma models list --provider ollama --summary`
- [ ] Error handling: model not found with `--json`
- [ ] Empty results with `--json` (outputs `[]`)

## Rollout Timeline

**Estimated Duration**: 8–12 days (1.5–2.5 weeks)

| Phase                    | Duration | Dependencies |
| ------------------------ | -------- | ------------ |
| Phase 1: CLI Flags       | 1–2 days | None         |
| Phase 2: Data Structures | 2–3 days | Phase 1      |
| Phase 3: JSON Output     | 2–3 days | Phases 1–2   |
| Phase 4: Summary Output  | 1 day    | Phases 1–3   |
| Phase 5: Documentation   | 2–3 days | Phases 1–4   |

**Milestones**:

- Day 2: CLI parsing complete and tested
- Day 5: Summary data structures implemented
- Day 8: JSON output working end-to-end
- Day 9: Summary output verified
- Day 12: Documentation and examples complete

## Risk Mitigation

### Risk 1: Breaking Changes to Existing Output

**Mitigation**:

- Preserve default behavior (no flags = current output)
- Make flags optional with default values of false
- No changes to existing function logic when flags are false

**Validation**:

- Regression tests for default output format
- ALL existing tests must pass without modification
- Manual verification: `xzatoma models list` produces identical output to current version

### Risk 2: Provider API Data Variations

**Mitigation**: Use `Option<T>` for all summary fields; graceful fallbacks

**Validation**: Test with missing/partial API data

### Risk 3: Serialization Failures

**Mitigation**: Comprehensive error handling; serialize to `Value` first

**Validation**: Test edge cases (large models, special characters, null fields)

### Risk 4: Performance Impact

**Mitigation**: Summary methods reuse existing API calls; minimal overhead

**Validation**: Benchmark `list_models()` vs `list_models_summary()`

## Success Metrics

**Functional**:

- All 4 flag combinations work correctly for both `list` and `info`: none, --json, --summary, --json --summary
- JSON output is valid and parseable by standard tools: `jq`, Python `json.loads()`, JavaScript `JSON.parse()`
- Summary data includes all fields available from provider APIs (Copilot: state, tool_calls, vision; Ollama: raw_data)
- Default behavior (no flags) remains 100% unchanged (verified by existing tests passing)

**Quality**:

- Test coverage >80% for all new code paths
- Zero clippy warnings: `cargo clippy --all-targets --all-features -- -D warnings`
- Zero compilation errors: `cargo check --all-targets --all-features`
- ALL existing tests pass without ANY modification (backward compatibility)
- Test count increases by ~45 tests (35 unit + 10 integration)

**Documentation**:

- Help text clear and accurate: `-j, --json` and `-s, --summary` shown correctly
- All examples tested manually and working
- Reference documentation complete with JSON/summary sections
- How-to guide updated with practical examples
- All doc comments include runnable examples (tested by `cargo test --doc`)

**User Experience**:

- Flags follow Unix conventions: short form (-j, -s) and long form (--json, --summary)
- Error messages are clear and actionable (e.g., "Failed to serialize models: ...", "Model 'xyz' not found")
- Output formats consistent with existing XZatoma CLI style (prettytable for tables, clean formatting)
- Performance impact <100ms for typical model lists (verified by manual testing)
- No noticeable slowdown compared to current implementation

## Open Questions

1. **Should `--summary` include raw JSON in human-readable output?**

   - **Option A**: Yes, show formatted excerpt (e.g., first 3 fields)
   - **Option B**: No, only mention "Raw API Data Available: Yes"
   - **Recommendation**: Option B (cleaner output; use `--json` for raw data)

2. **How to handle provider-specific fields not in `ModelInfoSummary`?**

   - **Option A**: Add to `raw_data` only
   - **Option B**: Add new optional fields to `ModelInfoSummary` as discovered
   - **Recommendation**: Option A initially; extend struct in future phases if needed

3. **Should JSON output use compact or pretty formatting?**

   - **Option A**: Compact (`to_string()`) for efficiency
   - **Option B**: Pretty (`to_string_pretty()`) for readability
   - **Recommendation**: Option B (user-facing CLI; readability > size)

4. **Should `--summary --json` include both `info` and flat fields?**
   - **Current Design**: Nested structure with `info` field
   - **Alternative**: Flatten all fields to top level
   - **Recommendation**: Keep nested (clearer structure; matches Rust types)

---

## Appendix: Example Outputs

### Example 1: List Models (Default)

```bash
xzatoma models list
```

**Output** (unchanged from current):

```
Available models from copilot:

+----------------+----------------+----------------+----------------+
| Model Name     | Display Name   | Context Window | Capabilities   |
+----------------+----------------+----------------+----------------+
| gpt-4          | GPT-4          | 8192 tokens    | ToolCalls      |
| gpt-4o         | GPT-4 Omni     | 128000 tokens  | ToolCalls      |
+----------------+----------------+----------------+----------------+
```

### Example 2: List Models with JSON

```bash
xzatoma models list --json
```

**Output**:

```json
[
  {
    "name": "gpt-4",
    "display_name": "GPT-4",
    "context_window": 8192,
    "capabilities": ["ToolCalls"],
    "provider_specific": {}
  },
  {
    "name": "gpt-4o",
    "display_name": "GPT-4 Omni",
    "context_window": 128000,
    "capabilities": ["ToolCalls"],
    "provider_specific": {}
  }
]
```

### Example 3: List Models with Summary

```bash
xzatoma models list --summary
```

**Output**:

```
Available models from copilot (summary):

+----------------+----------------+----------------+---------+------------+--------+
| Model Name     | Display Name   | Context Window | State   | Tool Calls | Vision |
+----------------+----------------+----------------+---------+------------+--------+
| gpt-4          | GPT-4          | 8192 tokens    | enabled | Yes        | No     |
| gpt-4o         | GPT-4 Omni     | 128000 tokens  | enabled | Yes        | Yes    |
+----------------+----------------+----------------+---------+------------+--------+
```

### Example 4: List Models with Summary and JSON

```bash
xzatoma models list --summary --json
```

**Output**:

```json
[
  {
    "info": {
      "name": "gpt-4",
      "display_name": "GPT-4",
      "context_window": 8192,
      "capabilities": ["ToolCalls"],
      "provider_specific": {}
    },
    "state": "enabled",
    "max_prompt_tokens": null,
    "max_completion_tokens": null,
    "supports_tool_calls": true,
    "supports_vision": false,
    "raw_data": {
      "id": "gpt-4",
      "name": "GPT-4",
      "capabilities": {
        "limits": {
          "max_context_window_tokens": 8192
        },
        "supports": {
          "tool_calls": true,
          "vision": false
        }
      },
      "policy": {
        "state": "enabled"
      }
    }
  }
]
```

### Example 5: Model Info with Summary

```bash
xzatoma models info --model gpt-4 --summary
```

**Output**:

```
Model Information (GPT-4)

Name:            gpt-4
Display Name:    GPT-4
Context Window:  8192 tokens
State:           enabled

Capabilities:
  Tool Calls:    Yes
  Vision:        No
  Full List:     ToolCalls

Raw API Data Available: Yes
```

---

## Implementation Plan Summary

This plan provides a structured approach to adding `--json` and `--summary` flags to the `models` command. The implementation is broken into 5 phases with clear deliverables, testing requirements, and success criteria for each phase.

**Key Design Decisions**:

- Preserve backward compatibility (default behavior unchanged)
- Use typed `ModelInfoSummary` struct for full API data
- Implement at provider trait level for consistency
- Support all 4 flag combinations (none, `--json`, `--summary`, both)
- Pretty-print JSON for human readability
- Graceful handling of missing/optional fields

**Expected Outcome**: Users can:

1. Export model data in JSON for scripting and automation
2. View full API data for detailed model comparison
3. Integrate model queries into CI/CD pipelines
4. Filter and analyze models using standard tools like `jq`
