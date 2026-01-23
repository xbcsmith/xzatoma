//! GitHub Copilot provider implementation for XZatoma
//!
//! This module implements the Provider trait for GitHub Copilot, including
//! OAuth device flow authentication and token caching in the system keyring.

use crate::config::CopilotConfig;
use crate::error::{Result, XzatomaError};
use crate::providers::{
    CompletionResponse, FunctionCall, Message, ModelCapability, ModelInfo, Provider,
    ProviderCapabilities, TokenUsage, ToolCall,
};

use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// GitHub OAuth device code endpoint
const GITHUB_DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
/// GitHub OAuth token endpoint
const GITHUB_TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
/// Copilot token exchange endpoint
const COPILOT_TOKEN_URL: &str = "https://api.github.com/copilot_internal/v2/token";
/// Copilot chat completions endpoint
const COPILOT_COMPLETIONS_URL: &str = "https://api.githubcopilot.com/chat/completions";
/// Copilot models endpoint
const COPILOT_MODELS_URL: &str = "https://api.githubcopilot.com/models";
/// GitHub Copilot OAuth client ID
const GITHUB_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";

/// GitHub Copilot provider
///
/// This provider connects to GitHub Copilot's API to generate completions.
/// It implements OAuth device flow for authentication and caches tokens
/// in the system keyring.
///
/// # Examples
///
/// ```no_run
/// use xzatoma::config::CopilotConfig;
/// use xzatoma::providers::{CopilotProvider, Provider, Message};
///
/// # async fn example() -> xzatoma::error::Result<()> {
/// let config = CopilotConfig {
///     model: "gpt-5-mini".to_string(),
///     ..Default::default()
/// };
/// let provider = CopilotProvider::new(config)?;
/// let messages = vec![Message::user("Hello!")];
/// let completion = provider.complete(&messages, &[]).await?;
/// let message = completion.message;
/// # Ok(())
/// # }
/// ```
///
/// Type alias to reduce type complexity in struct fields (satisfies clippy)
type ModelsCache = Arc<RwLock<Option<(Vec<ModelInfo>, u64)>>>;

pub struct CopilotProvider {
    client: Client,
    config: Arc<RwLock<CopilotConfig>>,
    keyring_service: String,
    keyring_user: String,
    /// Cached models and expiry time (epoch seconds). Uses RwLock for cheap reads.
    models_cache: ModelsCache,
    /// TTL (seconds) for the models cache
    models_cache_ttl_secs: u64,
}

/// Request for GitHub device code
#[derive(Debug, Serialize)]
struct DeviceCodeRequest {
    client_id: String,
    scope: String,
}

/// Response containing device code and user verification URL
#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    interval: u64,
}

/// Request to exchange device code for access token
#[derive(Debug, Serialize)]
struct TokenRequest {
    client_id: String,
    device_code: String,
    grant_type: String,
}

/// Response containing GitHub access token
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[allow(dead_code)]
    token_type: String,
    #[allow(dead_code)]
    scope: String,
}

/// Cached token information stored in keyring
#[derive(Debug, Serialize, Deserialize)]
struct CachedToken {
    github_token: String,
    copilot_token: String,
    expires_at: u64,
}

/// Request structure for Copilot API
#[derive(Debug, Serialize)]
struct CopilotRequest {
    model: String,
    messages: Vec<CopilotMessage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<CopilotTool>,
    stream: bool,
}

/// Message structure for Copilot API
#[derive(Debug, Serialize, Deserialize)]
struct CopilotMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<CopilotToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// Tool definition for Copilot API
#[derive(Debug, Serialize)]
struct CopilotTool {
    r#type: String,
    function: CopilotFunction,
}

/// Function definition for Copilot tools
#[derive(Debug, Serialize)]
struct CopilotFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Tool call in Copilot format
#[derive(Debug, Serialize, Deserialize)]
struct CopilotToolCall {
    id: String,
    r#type: String,
    function: CopilotFunctionCall,
}

/// Function call details in Copilot format
#[derive(Debug, Serialize, Deserialize)]
struct CopilotFunctionCall {
    name: String,
    arguments: String,
}

/// Response structure from Copilot API
#[derive(Debug, Deserialize)]
struct CopilotResponse {
    choices: Vec<CopilotChoice>,
    #[allow(dead_code)]
    usage: Option<CopilotUsage>,
}

/// Choice in Copilot response
#[derive(Debug, Deserialize)]
struct CopilotChoice {
    message: CopilotMessage,
    #[allow(dead_code)]
    finish_reason: String,
}

/// Token usage information from Copilot
#[derive(Debug, Deserialize)]
struct CopilotUsage {
    #[allow(dead_code)]
    prompt_tokens: usize,
    #[allow(dead_code)]
    completion_tokens: usize,
    #[allow(dead_code)]
    total_tokens: usize,
}

/// Response from Copilot models API
#[derive(Debug, Deserialize)]
struct CopilotModelsResponse {
    data: Vec<CopilotModelData>,
}

/// Model data from Copilot API
#[derive(Debug, Deserialize)]
struct CopilotModelData {
    id: String,
    name: String,
    #[serde(default)]
    capabilities: Option<CopilotModelCapabilities>,
    #[serde(default)]
    policy: Option<CopilotModelPolicy>,
}

/// Model policy information
#[derive(Debug, Deserialize)]
struct CopilotModelPolicy {
    state: String,
}

/// Model capabilities from Copilot API
#[derive(Debug, Deserialize)]
struct CopilotModelCapabilities {
    #[serde(default)]
    limits: Option<CopilotModelLimits>,
    #[serde(default)]
    supports: Option<CopilotModelSupports>,
}

/// Model limits
#[derive(Debug, Deserialize)]
struct CopilotModelLimits {
    #[serde(default)]
    max_context_window_tokens: Option<usize>,
}

/// Model support flags
#[derive(Debug, Deserialize)]
struct CopilotModelSupports {
    #[serde(default)]
    tool_calls: Option<bool>,
    #[serde(default)]
    vision: Option<bool>,
}

fn format_copilot_api_error(status: reqwest::StatusCode, body: &str) -> XzatomaError {
    if status == reqwest::StatusCode::UNAUTHORIZED {
        XzatomaError::Authentication(format!(
            "Copilot returned error {}: {}. Token may have expired; please re-authenticate with `xzatoma auth --provider copilot`",
            status, body
        ))
    } else {
        XzatomaError::Provider(format!("Copilot returned error {}: {}", status, body))
    }
}

impl CopilotProvider {
    /// Create a new Copilot provider instance
    ///
    /// # Arguments
    ///
    /// * `config` - Copilot configuration containing model name
    ///
    /// # Returns
    ///
    /// Returns a new CopilotProvider instance
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client initialization fails
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::CopilotConfig;
    /// use xzatoma::providers::CopilotProvider;
    ///
    /// let config = CopilotConfig {
    ///     model: "gpt-5-mini".to_string(),
    ///     ..Default::default()
    /// };
    /// let provider = CopilotProvider::new(config);
    /// assert!(provider.is_ok());
    /// ```
    pub fn new(config: CopilotConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(120))
            .user_agent("xzatoma/0.1.0")
            .build()
            .map_err(|e| XzatomaError::Provider(format!("Failed to create HTTP client: {}", e)))?;

        tracing::info!("Initialized Copilot provider: model={}", config.model);

        Ok(Self {
            client,
            config: Arc::new(RwLock::new(config)),
            keyring_service: "xzatoma".to_string(),
            keyring_user: "github_copilot".to_string(),
            models_cache: Arc::new(RwLock::new(None)),
            models_cache_ttl_secs: 300, // default 5 minutes
        })
    }

    /// Get the configured model name
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::config::CopilotConfig;
    /// use xzatoma::providers::{CopilotProvider, Provider};
    ///
    /// let config = CopilotConfig {
    ///     model: "gpt-5-mini".to_string(),
    ///     ..Default::default()
    /// };
    /// let provider = CopilotProvider::new(config).unwrap();
    /// assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    /// ```
    ///
    /// Authenticate and get Copilot token
    ///
    /// Checks keyring for cached token first. If not found or expired,
    /// performs OAuth device flow to get new token.
    pub async fn authenticate(&self) -> Result<String> {
        if let Ok(cached) = self.get_cached_token() {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if cached.expires_at > now + 300 {
                tracing::debug!("Using cached Copilot token");
                return Ok(cached.copilot_token);
            }

            tracing::debug!("Cached token expired, refreshing");
        }

        tracing::info!("Starting GitHub OAuth device flow");
        let github_token = self.device_flow().await?;

        tracing::debug!("Exchanging GitHub token for Copilot token");
        let copilot_token = self.get_copilot_token(&github_token).await?;

        let cached = CachedToken {
            github_token,
            copilot_token: copilot_token.clone(),
            expires_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 3600,
        };

        if let Err(e) = self.cache_token(&cached) {
            tracing::warn!("Failed to cache token: {}", e);
        }

        Ok(copilot_token)
    }

    /// Perform OAuth device flow to get GitHub token
    async fn device_flow(&self) -> Result<String> {
        // Request a device code (GitHub expects form-encoded body for these endpoints)
        // Request a device code.  If the server returns a non-2xx response
        // read the body and present a detailed error (status + body) so users
        // can see why the request failed (network / API / enterprise GitHub).
        let resp = self
            .client
            .post(GITHUB_DEVICE_CODE_URL)
            .header("Accept", "application/json")
            .form(&DeviceCodeRequest {
                client_id: GITHUB_CLIENT_ID.to_string(),
                scope: "read:user".to_string(),
            })
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Device code request failed: {}", e)))?;

        if !resp.status().is_success() {
            // Try to include the response body (best-effort) for a clearer error.
            let status = resp.status();
            let body = resp
                .text()
                .await
                .unwrap_or_else(|_| "<failed to read error body>".to_string());
            return Err(XzatomaError::Provider(format!(
                "Device code request returned {}: {}",
                status, body
            ))
            .into());
        }

        let device_response: DeviceCodeResponse = resp
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse device code: {}", e)))?;

        println!("\nGitHub Authentication Required:");
        println!("  1. Visit: {}", device_response.verification_uri);
        println!("  2. Enter code: {}", device_response.user_code);
        println!("\nWaiting for authorization...");

        let interval = Duration::from_secs(device_response.interval);
        let max_attempts = device_response.expires_in / device_response.interval;

        for attempt in 0..max_attempts {
            tokio::time::sleep(interval).await;

            let response = self
                .client
                .post(GITHUB_TOKEN_URL)
                .header("Accept", "application/json")
                .form(&TokenRequest {
                    client_id: GITHUB_CLIENT_ID.to_string(),
                    device_code: device_response.device_code.clone(),
                    grant_type: "urn:ietf:params:oauth:grant-type:device_code".to_string(),
                })
                .send()
                .await
                .map_err(|e| XzatomaError::Provider(format!("Token poll failed: {}", e)))?;

            // The token endpoint may return 200 with an error body (e.g. authorization_pending).
            // Parse the JSON and handle known device-flow error responses explicitly.
            let body: serde_json::Value = response.json().await.map_err(|e| {
                XzatomaError::Provider(format!("Failed to parse token poll response: {}", e))
            })?;

            // If an access token is present, we're done.
            if let Some(access) = body.get("access_token").and_then(|v| v.as_str()) {
                println!("Authorization successful!");
                tracing::info!("GitHub OAuth device flow completed successfully");
                return Ok(access.to_string());
            }

            // Handle standard OAuth device-flow transient/fatal errors.
            if let Some(err) = body.get("error").and_then(|v| v.as_str()) {
                match err {
                    "authorization_pending" => {
                        tracing::debug!("authorization_pending; continuing to poll");
                        // keep polling
                    }
                    "slow_down" => {
                        tracing::debug!("slow_down received; backing off an extra interval");
                        // apply a small backoff before the next attempt
                        tokio::time::sleep(Duration::from_secs(5)).await;
                    }
                    "expired_token" => {
                        return Err(XzatomaError::Provider(
                            "Device flow expired before authorization".to_string(),
                        )
                        .into());
                    }
                    other => {
                        return Err(XzatomaError::Provider(format!(
                            "Device flow error from provider: {}",
                            other
                        ))
                        .into());
                    }
                }
            } else {
                // No access_token and no explicit error — continue polling conservatively.
                tracing::debug!("Token poll returned no token and no error; continuing");
            }

            tracing::debug!("Polling attempt {}/{}", attempt + 1, max_attempts);
        }

        Err(
            XzatomaError::Provider("Device flow timed out waiting for authorization".to_string())
                .into(),
        )
    }

    /// Exchange GitHub token for Copilot token
    async fn get_copilot_token(&self, github_token: &str) -> Result<String> {
        #[derive(Deserialize)]
        struct CopilotTokenResponse {
            token: String,
        }

        let token_url = self.api_endpoint("copilot_internal/v2/token");
        let response: CopilotTokenResponse = self
            .client
            .get(&token_url)
            .header("Authorization", format!("token {}", github_token))
            .send()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Copilot token request failed: {}", e)))?
            .json()
            .await
            .map_err(|e| XzatomaError::Provider(format!("Failed to parse Copilot token: {}", e)))?;

        Ok(response.token)
    }

    // parse_github_token_poll moved to module scope (see function below)
    // (kept a short placeholder here so impl remains readable)

    /// Get cached token from system keyring
    fn get_cached_token(&self) -> Result<CachedToken> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)?;

        let json = entry.get_password()?;

        Ok(serde_json::from_str(&json)?)
    }

    /// Cache token in system keyring
    fn cache_token(&self, token: &CachedToken) -> Result<()> {
        let entry = keyring::Entry::new(&self.keyring_service, &self.keyring_user)?;

        let json = serde_json::to_string(token)?;

        entry.set_password(&json)?;

        Ok(())
    }

    /// Clear cached token from system keyring (best-effort).
    ///
    /// If the provider sees an authentication failure (401) it will attempt
    /// to invalidate the cached token so the next `authenticate()` call will
    /// perform the device flow again. This uses `set_password("")` as a
    /// safe, widely-available invalidation step rather than relying on a
    /// specific delete API that may vary between environments.
    fn clear_cached_token(&self) -> Result<()> {
        match keyring::Entry::new(&self.keyring_service, &self.keyring_user) {
            Ok(entry) => {
                if let Err(e) = entry.set_password("") {
                    tracing::warn!("Failed to clear cached Copilot token: {}", e);
                } else {
                    tracing::info!("Cleared cached Copilot token (set empty password) in keyring");
                }
            }
            Err(e) => {
                tracing::warn!("Keyring not available while clearing cached token: {}", e);
            }
        }
        Ok(())
    }

    /// Convert XZatoma messages to Copilot format
    fn convert_messages(&self, messages: &[Message]) -> Vec<CopilotMessage> {
        messages
            .iter()
            .filter_map(|m| {
                if m.content.is_none() && m.tool_calls.is_none() {
                    return None;
                }

                let tool_calls = m.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|tc| CopilotToolCall {
                            id: tc.id.clone(),
                            r#type: "function".to_string(),
                            function: CopilotFunctionCall {
                                name: tc.function.name.clone(),
                                arguments: tc.function.arguments.clone(),
                            },
                        })
                        .collect()
                });

                Some(CopilotMessage {
                    role: m.role.clone(),
                    content: m.content.clone().unwrap_or_default(),
                    tool_calls,
                    tool_call_id: m.tool_call_id.clone(),
                })
            })
            .collect()
    }

    /// Convert tool schemas to Copilot format
    fn convert_tools(&self, tools: &[serde_json::Value]) -> Vec<CopilotTool> {
        tools
            .iter()
            .filter_map(|t| {
                let obj = t.as_object()?;
                let name = obj.get("name")?.as_str()?.to_string();
                let description = obj.get("description")?.as_str()?.to_string();
                let parameters = obj.get("parameters")?.clone();

                Some(CopilotTool {
                    r#type: "function".to_string(),
                    function: CopilotFunction {
                        name,
                        description,
                        parameters,
                    },
                })
            })
            .collect()
    }

    /// Build an API endpoint URL using optional `CopilotConfig::api_base` override.
    fn api_endpoint(&self, path: &str) -> String {
        if let Ok(cfg) = self.config.read() {
            if let Some(base) = &cfg.api_base {
                return format!(
                    "{}/{}",
                    base.trim_end_matches('/'),
                    path.trim_start_matches('/')
                );
            }
        }
        match path {
            "models" => COPILOT_MODELS_URL.to_string(),
            "chat/completions" => COPILOT_COMPLETIONS_URL.to_string(),
            "copilot_internal/v2/token" => COPILOT_TOKEN_URL.to_string(),
            other => format!(
                "https://api.githubcopilot.com/{}",
                other.trim_start_matches('/')
            ),
        }
    }

    /// Convert Copilot response message back to XZatoma format
    fn convert_response_message(&self, copilot_msg: CopilotMessage) -> Message {
        if let Some(tool_calls) = copilot_msg.tool_calls {
            let converted_calls: Vec<ToolCall> = tool_calls
                .into_iter()
                .map(|tc| ToolCall {
                    id: tc.id,
                    function: FunctionCall {
                        name: tc.function.name,
                        arguments: tc.function.arguments,
                    },
                })
                .collect();

            Message::assistant_with_tools(converted_calls)
        } else {
            Message::assistant(copilot_msg.content)
        }
    }

    /// Fetch the list of available Copilot models from the API.
    ///
    /// This function uses an in-memory TTL cache to avoid frequent API calls.
    /// If `CopilotConfig::api_base` is set, it will be used to construct the
    /// models endpoint (useful for tests/local mocking).
    async fn fetch_copilot_models(&self) -> Result<Vec<ModelInfo>> {
        // Check models cache first
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Ok(cache_guard) = self.models_cache.read() {
            if let Some((cached_models, expires_at)) = &*cache_guard {
                if now < *expires_at {
                    tracing::debug!("Using cached Copilot models");
                    return Ok(cached_models.clone());
                } else {
                    tracing::debug!("Copilot models cache expired");
                }
            }
        }

        let token = self.authenticate().await?;
        let models_url = self.api_endpoint("models");

        let response = self
            .client
            .get(&models_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch Copilot models: {}", e);
                XzatomaError::Provider(format!("Failed to fetch Copilot models: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!(
                "Copilot models API returned error {}: {}",
                status,
                error_text
            );

            // If unauthorized, attempt a non-interactive refresh using the cached GitHub token.
            if status == reqwest::StatusCode::UNAUTHORIZED {
                tracing::warn!("Copilot returned 401 Unauthorized; attempting non-interactive refresh using cached GitHub token");
                if let Ok(cached) = self.get_cached_token() {
                    match self.get_copilot_token(&cached.github_token).await {
                        Ok(new_token) => {
                            // Store the refreshed Copilot token (best-effort)
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let refreshed = CachedToken {
                                github_token: cached.github_token.clone(),
                                copilot_token: new_token.clone(),
                                expires_at: now + 3600,
                            };
                            if let Err(e) = self.cache_token(&refreshed) {
                                tracing::warn!("Failed to cache refreshed Copilot token: {}", e);
                            } else {
                                tracing::info!("Successfully refreshed Copilot token using cached GitHub token");
                            }

                            // Retry models request with refreshed token
                            let retry_resp = self
                                .client
                                .get(&models_url)
                                .header("Authorization", format!("Bearer {}", new_token))
                                .header("Editor-Version", "vscode/1.85.0")
                                .send()
                                .await
                                .map_err(|e| {
                                    tracing::error!(
                                        "Failed to fetch Copilot models on retry: {}",
                                        e
                                    );
                                    XzatomaError::Provider(format!(
                                        "Failed to fetch Copilot models: {}",
                                        e
                                    ))
                                })?;

                            let status2 = retry_resp.status();
                            if !status2.is_success() {
                                let error_text2 = retry_resp.text().await.unwrap_or_default();
                                tracing::error!(
                                    "Copilot models API retry returned error {}: {}",
                                    status2,
                                    error_text2
                                );
                                if status2 == reqwest::StatusCode::UNAUTHORIZED {
                                    if let Err(e) = self.clear_cached_token() {
                                        tracing::warn!(
                                            "Failed to clear cached Copilot token: {}",
                                            e
                                        );
                                    }
                                }
                                return Err(format_copilot_api_error(status2, &error_text2).into());
                            }

                            // Parse and return models from the successful retry response
                            let models_response: CopilotModelsResponse =
                                retry_resp.json().await.map_err(|e| {
                                    tracing::error!(
                                        "Failed to parse Copilot models response on retry: {}",
                                        e
                                    );
                                    XzatomaError::Provider(format!(
                                        "Failed to parse Copilot models response: {}",
                                        e
                                    ))
                                })?;

                            let mut models = Vec::new();
                            for model_data in models_response.data {
                                // Only include enabled models
                                if let Some(policy) = &model_data.policy {
                                    if policy.state != "enabled" {
                                        continue;
                                    }
                                }

                                // Extract context window size
                                let context_window = model_data
                                    .capabilities
                                    .as_ref()
                                    .and_then(|c| c.limits.as_ref())
                                    .and_then(|l| l.max_context_window_tokens)
                                    .unwrap_or(128000); // Default to 128k if not specified

                                let mut model_info = ModelInfo::new(
                                    &model_data.id,
                                    &model_data.name,
                                    context_window,
                                );

                                // Add capabilities based on supports flags
                                if let Some(caps) = &model_data.capabilities {
                                    if let Some(supports) = &caps.supports {
                                        if supports.tool_calls.unwrap_or(false) {
                                            model_info
                                                .add_capability(ModelCapability::FunctionCalling);
                                        }
                                        if supports.vision.unwrap_or(false) {
                                            model_info.add_capability(ModelCapability::Vision);
                                        }
                                    }
                                }

                                // Add LongContext capability for models with >32k context
                                if context_window > 32000 {
                                    model_info.add_capability(ModelCapability::LongContext);
                                }

                                models.push(model_info);
                            }

                            // Cache the successful result
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let expires_at = now + self.models_cache_ttl_secs;
                            if let Ok(mut cache_guard) = self.models_cache.write() {
                                *cache_guard = Some((models.clone(), expires_at));
                            } else {
                                tracing::warn!("Failed to acquire write lock on models cache");
                            }

                            return Ok(models);
                        }
                        Err(e) => {
                            tracing::warn!("Non-interactive refresh failed: {}", e);
                            if let Err(e) = self.clear_cached_token() {
                                tracing::warn!("Failed to clear cached Copilot token: {}", e);
                            }
                            return Err(format_copilot_api_error(status, &error_text).into());
                        }
                    }
                } else {
                    // No cached GitHub token available; invalidate cache and return auth error
                    if let Err(e) = self.clear_cached_token() {
                        tracing::warn!("Failed to clear cached Copilot token: {}", e);
                    }
                    return Err(format_copilot_api_error(status, &error_text).into());
                }
            }

            // Non-auth failures fall back to provider error
            return Err(format_copilot_api_error(status, &error_text).into());
        }

        let models_response: CopilotModelsResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Copilot models response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Copilot models response: {}", e))
        })?;

        let mut models = Vec::new();
        for model_data in models_response.data {
            // Only include enabled models
            if let Some(policy) = &model_data.policy {
                if policy.state != "enabled" {
                    continue;
                }
            }

            // Extract context window size
            let context_window = model_data
                .capabilities
                .as_ref()
                .and_then(|c| c.limits.as_ref())
                .and_then(|l| l.max_context_window_tokens)
                .unwrap_or(128000); // Default to 128k if not specified

            let mut model_info = ModelInfo::new(&model_data.id, &model_data.name, context_window);

            // Add capabilities based on supports flags
            if let Some(caps) = &model_data.capabilities {
                if let Some(supports) = &caps.supports {
                    if supports.tool_calls.unwrap_or(false) {
                        model_info.add_capability(ModelCapability::FunctionCalling);
                    }
                    if supports.vision.unwrap_or(false) {
                        model_info.add_capability(ModelCapability::Vision);
                    }
                }
            }

            // Add LongContext capability for models with >32k context
            if context_window > 32000 {
                model_info.add_capability(ModelCapability::LongContext);
            }

            models.push(model_info);
        }

        // Cache the result
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let expires_at = now + self.models_cache_ttl_secs;
        if let Ok(mut cache_guard) = self.models_cache.write() {
            *cache_guard = Some((models.clone(), expires_at));
        } else {
            tracing::warn!("Failed to acquire write lock on models cache");
        }

        Ok(models)
    }
}

#[async_trait]
impl Provider for CopilotProvider {
    async fn complete(
        &self,
        messages: &[Message],
        tools: &[serde_json::Value],
    ) -> Result<CompletionResponse> {
        let token = self.authenticate().await?;

        let model = self
            .config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string())
            })?
            .model
            .clone();
        let copilot_request = CopilotRequest {
            model,
            messages: self.convert_messages(messages),
            tools: self.convert_tools(tools),
            stream: false,
        };

        tracing::debug!(
            "Sending Copilot request: {} messages, {} tools",
            copilot_request.messages.len(),
            copilot_request.tools.len()
        );

        let completions_url = self.api_endpoint("chat/completions");
        let response = self
            .client
            .post(&completions_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Editor-Version", "vscode/1.85.0")
            .json(&copilot_request)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("Copilot request failed: {}", e);
                XzatomaError::Provider(format!("Copilot request failed: {}", e))
            })?;

        let status = response.status();
        if !status.is_success() {
            let error_text = response.text().await.unwrap_or_default();
            tracing::error!("Copilot returned error {}: {}", status, error_text);

            if status == reqwest::StatusCode::UNAUTHORIZED {
                tracing::warn!("Copilot returned 401 Unauthorized; attempting non-interactive refresh using cached GitHub token");
                if let Ok(cached) = self.get_cached_token() {
                    match self.get_copilot_token(&cached.github_token).await {
                        Ok(new_token) => {
                            let now = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_secs();
                            let refreshed = CachedToken {
                                github_token: cached.github_token.clone(),
                                copilot_token: new_token.clone(),
                                expires_at: now + 3600,
                            };
                            if let Err(e) = self.cache_token(&refreshed) {
                                tracing::warn!("Failed to cache refreshed Copilot token: {}", e);
                            } else {
                                tracing::info!("Successfully refreshed Copilot token using cached GitHub token");
                            }

                            // Retry the original completion request with refreshed token
                            let retry_response = self
                                .client
                                .post(&completions_url)
                                .header("Authorization", format!("Bearer {}", new_token))
                                .header("Editor-Version", "vscode/1.85.0")
                                .json(&copilot_request)
                                .send()
                                .await
                                .map_err(|e| {
                                    tracing::error!("Copilot request retry failed: {}", e);
                                    XzatomaError::Provider(format!(
                                        "Copilot request retry failed: {}",
                                        e
                                    ))
                                })?;

                            let status2 = retry_response.status();
                            if !status2.is_success() {
                                let error_text2 = retry_response.text().await.unwrap_or_default();
                                tracing::error!(
                                    "Copilot retry returned error {}: {}",
                                    status2,
                                    error_text2
                                );
                                if status2 == reqwest::StatusCode::UNAUTHORIZED {
                                    if let Err(e) = self.clear_cached_token() {
                                        tracing::warn!(
                                            "Failed to clear cached Copilot token: {}",
                                            e
                                        );
                                    }
                                }
                                return Err(format_copilot_api_error(status2, &error_text2).into());
                            }

                            // Parse and return the completion from retry_response
                            let copilot_response: CopilotResponse =
                                retry_response.json().await.map_err(|e| {
                                    tracing::error!(
                                        "Failed to parse Copilot response on retry: {}",
                                        e
                                    );
                                    XzatomaError::Provider(format!(
                                        "Failed to parse Copilot response: {}",
                                        e
                                    ))
                                })?;

                            let choice =
                                copilot_response.choices.into_iter().next().ok_or_else(|| {
                                    XzatomaError::Provider(
                                        "No choices in Copilot response".to_string(),
                                    )
                                })?;

                            tracing::debug!("Copilot response received successfully (retry)");

                            let message = self.convert_response_message(choice.message);

                            // Extract token usage if available
                            let usage = copilot_response
                                .usage
                                .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

                            let response = match usage {
                                Some(u) => CompletionResponse::with_usage(message, u),
                                None => CompletionResponse::new(message),
                            };
                            return Ok(response);
                        }
                        Err(e) => {
                            tracing::warn!("Non-interactive refresh failed: {}", e);
                            if let Err(e) = self.clear_cached_token() {
                                tracing::warn!("Failed to clear cached Copilot token: {}", e);
                            }
                            return Err(format_copilot_api_error(status, &error_text).into());
                        }
                    }
                } else {
                    // No cached GitHub token available; invalidate cache and return auth error
                    if let Err(e) = self.clear_cached_token() {
                        tracing::warn!("Failed to clear cached Copilot token: {}", e);
                    }
                    return Err(format_copilot_api_error(status, &error_text).into());
                }
            }

            return Err(format_copilot_api_error(status, &error_text).into());
        }

        let copilot_response: CopilotResponse = response.json().await.map_err(|e| {
            tracing::error!("Failed to parse Copilot response: {}", e);
            XzatomaError::Provider(format!("Failed to parse Copilot response: {}", e))
        })?;

        let choice =
            copilot_response.choices.into_iter().next().ok_or_else(|| {
                XzatomaError::Provider("No choices in Copilot response".to_string())
            })?;

        tracing::debug!("Copilot response received successfully");

        let message = self.convert_response_message(choice.message);

        // Extract token usage if available
        let usage = copilot_response
            .usage
            .map(|u| TokenUsage::new(u.prompt_tokens, u.completion_tokens));

        let response = match usage {
            Some(u) => CompletionResponse::with_usage(message, u),
            None => CompletionResponse::new(message),
        };
        Ok(response)
    }

    async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        tracing::debug!("Listing Copilot models from API");
        self.fetch_copilot_models().await
    }

    async fn get_model_info(&self, model_name: &str) -> Result<ModelInfo> {
        tracing::debug!("Getting info for model: {}", model_name);
        let models = self.fetch_copilot_models().await?;
        models
            .into_iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                XzatomaError::Provider(format!("Model not found: {}", model_name)).into()
            })
    }

    fn get_current_model(&self) -> Result<String> {
        self.config
            .read()
            .map_err(|_| {
                XzatomaError::Provider("Failed to acquire read lock on config".to_string()).into()
            })
            .map(|config| config.model.clone())
    }

    fn get_provider_capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_model_listing: true,
            supports_model_details: true,
            supports_model_switching: true,
            supports_token_counts: true,
            supports_streaming: false,
        }
    }

    async fn set_model(&mut self, model_name: String) -> Result<()> {
        // Fetch available models from API to validate
        let models = self.fetch_copilot_models().await?;

        // Find the requested model
        let model_info = models
            .iter()
            .find(|m| m.name == model_name)
            .ok_or_else(|| {
                let available: Vec<String> = models.iter().map(|m| m.name.clone()).collect();
                XzatomaError::Provider(format!(
                    "Model '{}' not found. Available models: {}",
                    model_name,
                    available.join(", ")
                ))
            })?;

        // Check if model supports tool calling
        if !model_info.supports_capability(ModelCapability::FunctionCalling) {
            let tool_models: Vec<String> = models
                .iter()
                .filter(|m| m.supports_capability(ModelCapability::FunctionCalling))
                .map(|m| m.name.clone())
                .collect();

            return Err(XzatomaError::Provider(format!(
                "Model '{}' does not support tool calling, which is required for XZatoma. Models with tool support: {}",
                model_name,
                tool_models.join(", ")
            ))
            .into());
        }

        // Update the model in the config
        let mut config = self.config.write().map_err(|_| {
            XzatomaError::Provider("Failed to acquire write lock on config".to_string())
        })?;
        config.model = model_name;
        Ok(())
    }
}

/// Parse a token-poll response from GitHub's device token endpoint.
///
/// Returns:
/// - Ok(Some(token)) when an access token is present
/// - Ok(None) when polling should continue (authorization_pending / slow_down)
/// - Err(...) for fatal errors (expired_token or unknown error)
fn parse_github_token_poll(value: &serde_json::Value) -> Result<Option<String>> {
    if let Some(tok) = value.get("access_token").and_then(|v| v.as_str()) {
        return Ok(Some(tok.to_string()));
    }

    if let Some(err) = value.get("error").and_then(|v| v.as_str()) {
        match err {
            "authorization_pending" => return Ok(None),
            "slow_down" => return Ok(None),
            "expired_token" => {
                return Err(XzatomaError::Provider("Device flow expired".to_string()).into());
            }
            other => {
                return Err(XzatomaError::Provider(format!("Device flow error: {}", other)).into());
            }
        }
    }

    // No token and no recognizable error -> keep polling
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copilot_provider_creation() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config);
        assert!(provider.is_ok());
    }

    // --- Device-flow response parsing tests (no network) ---
    #[test]
    fn test_parse_github_token_poll_pending() {
        let v = serde_json::json!({ "error": "authorization_pending" });
        assert!(parse_github_token_poll(&v).unwrap().is_none());
    }

    #[test]
    fn test_parse_github_token_poll_slow_down() {
        let v = serde_json::json!({ "error": "slow_down" });
        assert!(parse_github_token_poll(&v).unwrap().is_none());
    }

    #[test]
    fn test_parse_github_token_poll_success() {
        let v = serde_json::json!({ "access_token": "gho_ABC123" });
        assert_eq!(
            parse_github_token_poll(&v).unwrap(),
            Some("gho_ABC123".to_string())
        );
    }

    #[test]
    fn test_parse_github_token_poll_expired() {
        let v = serde_json::json!({ "error": "expired_token" });
        assert!(parse_github_token_poll(&v).is_err());
    }

    #[test]
    fn test_copilot_config_default_model() {
        let config = CopilotConfig::default();
        assert_eq!(config.model, "gpt-5-mini");
    }

    #[test]
    fn test_copilot_models_response_deserialization() {
        // Test that we can deserialize the Copilot models API response
        // The testdata/models.json is an array, not wrapped in {"data": [...]}
        let test_data = include_str!("../../testdata/models.json");
        let result: std::result::Result<Vec<CopilotModelData>, _> = serde_json::from_str(test_data);
        assert!(result.is_ok());
        let models = result.unwrap();
        assert!(!models.is_empty());
    }

    #[test]
    fn test_copilot_provider_model() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    }

    #[test]
    fn test_convert_messages_basic() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let messages = vec![
            Message::system("You are helpful"),
            Message::user("Hello"),
            Message::assistant("Hi"),
        ];

        let copilot_messages = provider.convert_messages(&messages);
        assert_eq!(copilot_messages.len(), 3);
        assert_eq!(copilot_messages[0].role, "system");
        assert_eq!(copilot_messages[1].role, "user");
        assert_eq!(copilot_messages[2].role, "assistant");
    }

    #[test]
    fn test_convert_messages_with_tool_calls() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let tool_call = ToolCall {
            id: "call_123".to_string(),
            function: FunctionCall {
                name: "read_file".to_string(),
                arguments: r#"{"path":"test.txt"}"#.to_string(),
            },
        };

        let messages = vec![Message::assistant_with_tools(vec![tool_call])];

        let copilot_messages = provider.convert_messages(&messages);
        assert_eq!(copilot_messages.len(), 1);
        assert!(copilot_messages[0].tool_calls.is_some());
    }

    #[test]
    fn test_convert_tools() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let tools = vec![serde_json::json!({
            "name": "read_file",
            "description": "Read a file",
            "parameters": {
                "type": "object",
                "properties": {
                    "path": {"type": "string"}
                }
            }
        })];

        let copilot_tools = provider.convert_tools(&tools);
        assert_eq!(copilot_tools.len(), 1);
        assert_eq!(copilot_tools[0].function.name, "read_file");
        assert_eq!(copilot_tools[0].function.description, "Read a file");
    }

    #[test]
    fn test_convert_response_message_text() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let copilot_msg = CopilotMessage {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            tool_calls: None,
            tool_call_id: None,
        };

        let msg = provider.convert_response_message(copilot_msg);
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, Some("Hello!".to_string()));
        assert!(msg.tool_calls.is_none());
    }

    #[test]
    fn test_convert_response_message_with_tools() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();

        let copilot_msg = CopilotMessage {
            role: "assistant".to_string(),
            content: String::new(),
            tool_calls: Some(vec![CopilotToolCall {
                id: "call_123".to_string(),
                r#type: "function".to_string(),
                function: CopilotFunctionCall {
                    name: "read_file".to_string(),
                    arguments: r#"{"path":"test.txt"}"#.to_string(),
                },
            }]),
            tool_call_id: None,
        };

        let msg = provider.convert_response_message(copilot_msg);
        assert_eq!(msg.role, "assistant");
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn test_keyring_service_names() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.keyring_service, "xzatoma");
        assert_eq!(provider.keyring_user, "github_copilot");
    }

    #[tokio::test]
    async fn test_list_copilot_models() {
        // This test would require authentication and API calls
        // For unit testing, we verify the parsing logic with test data instead
        // See test_parse_models_from_testdata below
    }

    #[test]
    fn test_parse_models_from_testdata() {
        // Test that we can parse the real Copilot models API response
        // The testdata is an array of models directly
        let test_data = include_str!("../../testdata/models.json");
        let models: Vec<CopilotModelData> = serde_json::from_str(test_data).unwrap();

        // Filter to enabled models only
        let enabled_models: Vec<_> = models
            .iter()
            .filter(|m| {
                m.policy
                    .as_ref()
                    .map(|p| p.state == "enabled")
                    .unwrap_or(false)
            })
            .collect();

        assert!(!enabled_models.is_empty());

        // Check for expected models from the testdata
        let model_ids: Vec<_> = enabled_models.iter().map(|m| m.id.as_str()).collect();
        assert!(model_ids.contains(&"gpt-5-mini"));
        assert!(model_ids.contains(&"claude-sonnet-4.5"));

        // Verify models have capabilities
        let gpt5_mini = enabled_models
            .iter()
            .find(|m| m.id == "gpt-5-mini")
            .unwrap();
        assert!(gpt5_mini.capabilities.is_some());
        let caps = gpt5_mini.capabilities.as_ref().unwrap();
        assert!(caps.supports.is_some());
        assert_eq!(caps.supports.as_ref().unwrap().tool_calls, Some(true));
    }

    #[test]
    fn test_model_context_window_extraction() {
        // Test that we correctly extract context windows from the API response
        let test_data = include_str!("../../testdata/models.json");
        let models: Vec<CopilotModelData> = serde_json::from_str(test_data).unwrap();

        // Find gpt-5-mini and verify context window
        let gpt5_mini = models.iter().find(|m| m.id == "gpt-5-mini").unwrap();
        let context_window = gpt5_mini
            .capabilities
            .as_ref()
            .and_then(|c| c.limits.as_ref())
            .and_then(|l| l.max_context_window_tokens);
        assert_eq!(context_window, Some(264000));
    }

    #[test]
    fn test_get_current_model() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        assert_eq!(provider.get_current_model().unwrap(), "gpt-5-mini");
    }

    #[test]
    fn test_provider_capabilities() {
        let config = CopilotConfig::default();
        let provider = CopilotProvider::new(config).unwrap();
        let caps = provider.get_provider_capabilities();

        assert!(caps.supports_model_listing);
        assert!(caps.supports_model_details);
        assert!(caps.supports_model_switching);
        assert!(caps.supports_token_counts);
        assert!(!caps.supports_streaming);
    }

    #[tokio::test]
    async fn test_set_model_with_valid_model() {
        // This test requires API authentication and would make real API calls
        // In a real test environment with proper mocking, we would:
        // 1. Mock the Copilot models API to return test data
        // 2. Call set_model with a known-good model from that data
        // 3. Verify the model was updated
        // For now, we just verify the structure compiles
    }

    #[tokio::test]
    async fn test_set_model_with_invalid_model() {
        // This test requires API authentication and would make real API calls
        // In a real test environment with proper mocking, we would:
        // 1. Mock the Copilot models API to return test data
        // 2. Call set_model with an invalid model name
        // 3. Verify it returns an error with helpful message
        // For now, we just verify the structure compiles
    }

    #[tokio::test]
    async fn test_list_models_returns_all_supported_models() {
        // This test requires API authentication and would make real API calls
        // The actual model count varies based on what GitHub enables
        // See test_parse_models_from_testdata for parsing validation
    }

    #[tokio::test]
    async fn test_get_model_info_valid_model() {
        // This test requires API authentication and would make real API calls
        // See test_parse_models_from_testdata for parsing validation
    }

    #[tokio::test]
    async fn test_get_model_info_invalid_model() {
        // This test requires API authentication and would make real API calls
        // In a proper test environment, we would verify error handling
    }

    #[test]
    fn test_token_usage_extraction() {
        // Test that TokenUsage is correctly created
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.prompt_tokens, 100);
        assert_eq!(usage.completion_tokens, 50);
        assert_eq!(usage.total_tokens, 150);
    }

    #[test]
    fn test_completion_response_with_usage() {
        let message = Message::assistant("Test response");
        let usage = TokenUsage::new(100, 50);
        let response = CompletionResponse::with_usage(message, usage);

        assert_eq!(response.message.content, Some("Test response".to_string()));
        assert!(response.usage.is_some());
        assert_eq!(response.usage.unwrap().total_tokens, 150);
    }

    #[test]
    fn test_completion_response_without_usage() {
        let message = Message::assistant("Test response");
        let response = CompletionResponse::new(message);

        assert_eq!(response.message.content, Some("Test response".to_string()));
        assert!(response.usage.is_none());
    }

    #[test]
    fn test_copilot_message_deserialize_missing_content() {
        // Test that CopilotMessage can be deserialized when content field is missing
        // This happens when Copilot returns tool calls without content
        let json = r#"{
            "role": "assistant",
            "tool_calls": [{
                "id": "call_abc123",
                "type": "function",
                "function": {
                    "name": "read_file",
                    "arguments": "{\"path\":\"test.txt\"}"
                }
            }]
        }"#;

        let result: std::result::Result<CopilotMessage, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let msg = result.unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, ""); // Should default to empty string
        assert!(msg.tool_calls.is_some());
        assert_eq!(msg.tool_calls.as_ref().unwrap().len(), 1);
        assert_eq!(msg.tool_calls.as_ref().unwrap()[0].id, "call_abc123");
    }

    #[test]
    fn test_copilot_response_deserialize_missing_content() {
        // Test that a complete CopilotResponse can be deserialized when content is missing
        let json = r#"{
            "choices": [{
                "message": {
                    "role": "assistant",
                    "tool_calls": [{
                        "id": "call_xyz789",
                        "type": "function",
                        "function": {
                            "name": "terminal",
                            "arguments": "{\"command\":\"ls\"}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 50,
                "total_tokens": 150
            }
        }"#;

        let result: std::result::Result<CopilotResponse, _> = serde_json::from_str(json);
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].message.content, "");
        assert!(response.choices[0].message.tool_calls.is_some());
        assert!(response.usage.is_some());
    }

    #[test]
    fn test_format_copilot_api_error_unauthorized() {
        use crate::error::XzatomaError;

        let err = format_copilot_api_error(
            reqwest::StatusCode::UNAUTHORIZED,
            "unauthorized: token expired",
        );
        assert!(matches!(err, XzatomaError::Authentication(_)));
        assert!(
            err.to_string().contains("token expired")
                || err.to_string().contains("Token may have expired")
        );
    }

    #[test]
    fn test_format_copilot_api_error_other() {
        use crate::error::XzatomaError;

        let err =
            format_copilot_api_error(reqwest::StatusCode::INTERNAL_SERVER_ERROR, "internal error");
        assert!(matches!(err, XzatomaError::Provider(_)));
        assert!(err.to_string().contains("internal error"));
    }
}
