//! XZepr API client for downstream services.
//!
//! This module provides an HTTP client for interacting with the XZepr API,
//! including event receiver management and event creation.
//!
//! # Example
//!
//! ```rust,no_run
//! use xzatoma::xzepr::consumer::client::{XzeprClient, XzeprClientConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = XzeprClientConfig {
//!         base_url: "http://localhost:8042".to_string(),
//!         token: "your-jwt-token".to_string(),
//!         timeout_secs: 30,
//!     };
//!     let client = XzeprClient::new(config)?;
//!     
//!     // Discover or create an event receiver
//!     let receiver_id = client.discover_or_create_event_receiver(
//!         "my-receiver",
//!         "worker",
//!         "1.0.0",
//!         "My service receiver",
//!         serde_json::json!({"type": "object"}),
//!     ).await?;
//!     
//!     Ok(())
//! }
//! ```

use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;
use tracing::{debug, error, info};

/// Errors that can occur during client operations.
#[derive(Error, Debug)]
pub enum ClientError {
    /// HTTP request error.
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// API error with status code.
    #[error("API error ({status}): {message}")]
    Api {
        /// HTTP status code.
        status: u16,
        /// Error message from API.
        message: String,
    },

    /// Authentication error.
    #[error("Authentication error: {0}")]
    Authentication(String),

    /// Resource not found.
    #[error("Not found: {0}")]
    NotFound(String),

    /// Serialization error.
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Request to create an event receiver.
#[derive(Debug, Serialize)]
pub struct CreateEventReceiverRequest {
    /// Receiver name.
    pub name: String,
    /// Receiver type.
    #[serde(rename = "type")]
    pub receiver_type: String,
    /// Receiver version.
    pub version: String,
    /// Receiver description.
    pub description: String,
    /// JSON schema for validation.
    pub schema: JsonValue,
}

/// Response from creating an event receiver.
#[derive(Debug, Deserialize)]
pub struct CreateEventReceiverResponse {
    /// Created receiver ID.
    pub data: String,
}

/// Event receiver entity from list response.
#[derive(Debug, Clone, Deserialize)]
pub struct EventReceiverResponse {
    /// Unique receiver identifier.
    pub id: String,
    /// Receiver name.
    pub name: String,
    /// Receiver type.
    #[serde(rename = "type")]
    pub receiver_type: String,
    /// Receiver version.
    pub version: String,
    /// Receiver description.
    pub description: String,
    /// JSON schema for validation.
    pub schema: JsonValue,
    /// Unique fingerprint.
    pub fingerprint: String,
    /// Creation timestamp.
    pub created_at: String,
}

/// Paginated response wrapper.
#[derive(Debug, Deserialize)]
pub struct PaginatedResponse<T> {
    /// Response data.
    pub data: Vec<T>,
    /// Pagination metadata.
    pub pagination: PaginationMeta,
}

/// Pagination metadata.
#[derive(Debug, Deserialize)]
pub struct PaginationMeta {
    /// Maximum items per page.
    pub limit: usize,
    /// Current offset.
    pub offset: usize,
    /// Total items available.
    pub total: usize,
    /// Whether more items exist.
    pub has_more: bool,
}

/// Request to create an event.
#[derive(Debug, Serialize)]
pub struct CreateEventRequest {
    /// Event name.
    pub name: String,
    /// Event version.
    pub version: String,
    /// Release identifier.
    pub release: String,
    /// Platform identifier.
    pub platform_id: String,
    /// Package name.
    pub package: String,
    /// Event description.
    pub description: String,
    /// Event payload.
    pub payload: JsonValue,
    /// Success status.
    pub success: bool,
    /// Associated event receiver ID.
    pub event_receiver_id: String,
}

/// Response from creating an event.
#[derive(Debug, Deserialize)]
pub struct CreateEventResponse {
    /// Created event ID.
    pub data: String,
}

/// XZepr API client configuration.
#[derive(Debug, Clone)]
pub struct XzeprClientConfig {
    /// Base URL of the XZepr API.
    pub base_url: String,
    /// JWT token for authentication.
    pub token: String,
    /// Request timeout in seconds.
    pub timeout_secs: u64,
}

impl XzeprClientConfig {
    /// Creates configuration from environment variables.
    ///
    /// # Environment Variables
    ///
    /// * `XZEPR_API_URL` - Base URL (default: http://localhost:8042)
    /// * `XZEPR_API_TOKEN` - JWT token (required)
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Authentication` if `XZEPR_API_TOKEN` is not set.
    pub fn from_env() -> Result<Self, ClientError> {
        let base_url =
            std::env::var("XZEPR_API_URL").unwrap_or_else(|_| "http://localhost:8042".to_string());

        let token = std::env::var("XZEPR_API_TOKEN")
            .map_err(|_| ClientError::Authentication("XZEPR_API_TOKEN not set".to_string()))?;

        Ok(Self {
            base_url,
            token,
            timeout_secs: 30,
        })
    }
}

/// XZepr API client for downstream services.
///
/// Provides methods for:
/// - Creating and discovering event receivers
/// - Creating events
/// - Posting work lifecycle events (started, completed, failed)
pub struct XzeprClient {
    client: Client,
    config: XzeprClientConfig,
}

impl XzeprClient {
    /// Creates a new XZepr client.
    ///
    /// # Arguments
    ///
    /// * `config` - Client configuration
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Http` if the HTTP client cannot be created.
    pub fn new(config: XzeprClientConfig) -> Result<Self, ClientError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()?;

        Ok(Self { client, config })
    }

    /// Creates a client from environment variables.
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Authentication` if `XZEPR_API_TOKEN` is not set.
    pub fn from_env() -> Result<Self, ClientError> {
        let config = XzeprClientConfig::from_env()?;
        Self::new(config)
    }

    /// Builds a request with authentication headers.
    fn build_request(&self, method: reqwest::Method, path: &str) -> reqwest::RequestBuilder {
        let url = format!("{}{}", self.config.base_url, path);
        self.client
            .request(method, &url)
            .header("Authorization", format!("Bearer {}", self.config.token))
            .header("Content-Type", "application/json")
    }

    /// Creates a new event receiver.
    ///
    /// # Arguments
    ///
    /// * `request` - Event receiver creation request
    ///
    /// # Returns
    ///
    /// The created receiver ID.
    ///
    /// # Errors
    ///
    /// Returns `ClientError::Api` if the API returns an error.
    pub async fn create_event_receiver(
        &self,
        request: CreateEventReceiverRequest,
    ) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/receivers")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventReceiverResponse = response.json().await?;
            info!(
                receiver_id = %result.data,
                name = %request.name,
                "Created event receiver"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Lists event receivers with optional filters.
    ///
    /// # Arguments
    ///
    /// * `name_filter` - Filter by name (optional)
    /// * `receiver_type` - Filter by type (optional)
    /// * `limit` - Maximum results
    /// * `offset` - Pagination offset
    ///
    /// # Returns
    ///
    /// Paginated list of event receivers.
    pub async fn list_event_receivers(
        &self,
        name_filter: Option<&str>,
        receiver_type: Option<&str>,
        limit: usize,
        offset: usize,
    ) -> Result<PaginatedResponse<EventReceiverResponse>, ClientError> {
        let mut query = vec![("limit", limit.to_string()), ("offset", offset.to_string())];

        if let Some(name) = name_filter {
            query.push(("name", name.to_string()));
        }
        if let Some(rtype) = receiver_type {
            query.push(("type", rtype.to_string()));
        }

        let response = self
            .build_request(reqwest::Method::GET, "/api/v1/receivers")
            .query(&query)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: PaginatedResponse<EventReceiverResponse> = response.json().await?;
            Ok(result)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Gets an event receiver by ID.
    ///
    /// # Arguments
    ///
    /// * `id` - Receiver ID
    ///
    /// # Errors
    ///
    /// Returns `ClientError::NotFound` if the receiver doesn't exist.
    pub async fn get_event_receiver(&self, id: &str) -> Result<EventReceiverResponse, ClientError> {
        let response = self
            .build_request(reqwest::Method::GET, &format!("/api/v1/receivers/{}", id))
            .send()
            .await?;

        let status = response.status();
        match status {
            StatusCode::OK => {
                let result: EventReceiverResponse = response.json().await?;
                Ok(result)
            }
            StatusCode::NOT_FOUND => Err(ClientError::NotFound(format!("Event receiver {}", id))),
            _ => {
                let body = response.text().await.unwrap_or_default();
                Err(ClientError::Api {
                    status: status.as_u16(),
                    message: body,
                })
            }
        }
    }

    /// Discovers an existing event receiver by name, or creates one if not found.
    ///
    /// This is the recommended method for obtaining an event receiver ID,
    /// as it ensures the receiver exists before use.
    ///
    /// # Arguments
    ///
    /// * `name` - Receiver name
    /// * `receiver_type` - Receiver type
    /// * `version` - Receiver version
    /// * `description` - Receiver description
    /// * `schema` - JSON schema for validation
    ///
    /// # Returns
    ///
    /// The receiver ID (existing or newly created).
    pub async fn discover_or_create_event_receiver(
        &self,
        name: &str,
        receiver_type: &str,
        version: &str,
        description: &str,
        schema: JsonValue,
    ) -> Result<String, ClientError> {
        // First, try to find existing receiver by name
        let receivers = self
            .list_event_receivers(Some(name), Some(receiver_type), 10, 0)
            .await?;

        // Check for exact match
        for receiver in &receivers.data {
            if receiver.name == name && receiver.receiver_type == receiver_type {
                info!(
                    receiver_id = %receiver.id,
                    name = %name,
                    "Discovered existing event receiver"
                );
                return Ok(receiver.id.clone());
            }
        }

        // Not found, create new receiver
        info!(name = %name, "Event receiver not found, creating new one");
        let request = CreateEventReceiverRequest {
            name: name.to_string(),
            receiver_type: receiver_type.to_string(),
            version: version.to_string(),
            description: description.to_string(),
            schema,
        };

        self.create_event_receiver(request).await
    }

    /// Creates a new event.
    ///
    /// # Arguments
    ///
    /// * `request` - Event creation request
    ///
    /// # Returns
    ///
    /// The created event ID.
    pub async fn create_event(&self, request: CreateEventRequest) -> Result<String, ClientError> {
        let response = self
            .build_request(reqwest::Method::POST, "/api/v1/events")
            .json(&request)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            let result: CreateEventResponse = response.json().await?;
            debug!(
                event_id = %result.data,
                event_name = %request.name,
                "Created event"
            );
            Ok(result.data)
        } else {
            let body = response.text().await.unwrap_or_default();
            error!(
                status = status.as_u16(),
                body = %body,
                "Failed to create event"
            );
            Err(ClientError::Api {
                status: status.as_u16(),
                message: body,
            })
        }
    }

    /// Posts a work started event.
    ///
    /// Creates an event indicating that work has started processing.
    ///
    /// # Arguments
    ///
    /// * `receiver_id` - Event receiver ID
    /// * `work_id` - Unique work identifier
    /// * `work_name` - Name of the work
    /// * `version` - Work version
    /// * `platform_id` - Platform identifier
    /// * `package` - Package name
    /// * `payload` - Additional payload data
    ///
    /// # Returns
    ///
    /// The created event ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_work_started(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let request = CreateEventRequest {
            name: format!("{}.started", work_name),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work started: {} ({})", work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": "started",
                "started_at": chrono::Utc::now().to_rfc3339(),
                "details": payload
            }),
            success: true,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }

    /// Posts a work completed event.
    ///
    /// Creates an event indicating that work has completed (successfully or failed).
    ///
    /// # Arguments
    ///
    /// * `receiver_id` - Event receiver ID
    /// * `work_id` - Unique work identifier
    /// * `work_name` - Name of the work
    /// * `version` - Work version
    /// * `platform_id` - Platform identifier
    /// * `package` - Package name
    /// * `success` - Whether the work completed successfully
    /// * `payload` - Additional payload data (e.g., error details if failed)
    ///
    /// # Returns
    ///
    /// The created event ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_work_completed(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        success: bool,
        payload: JsonValue,
    ) -> Result<String, ClientError> {
        let status_suffix = if success { "completed" } else { "failed" };
        let request = CreateEventRequest {
            name: format!("{}.{}", work_name, status_suffix),
            version: version.to_string(),
            release: version.to_string(),
            platform_id: platform_id.to_string(),
            package: package.to_string(),
            description: format!("Work {}: {} ({})", status_suffix, work_name, work_id),
            payload: serde_json::json!({
                "work_id": work_id,
                "status": status_suffix,
                "completed_at": chrono::Utc::now().to_rfc3339(),
                "success": success,
                "details": payload
            }),
            success,
            event_receiver_id: receiver_id.to_string(),
        };

        self.create_event(request).await
    }

    /// Posts a work failed event (convenience method).
    ///
    /// This is a convenience wrapper around `post_work_completed` with `success=false`.
    ///
    /// # Arguments
    ///
    /// * `receiver_id` - Event receiver ID
    /// * `work_id` - Unique work identifier
    /// * `work_name` - Name of the work
    /// * `version` - Work version
    /// * `platform_id` - Platform identifier
    /// * `package` - Package name
    /// * `error_message` - Error message describing the failure
    /// * `error_code` - Optional error code
    ///
    /// # Returns
    ///
    /// The created event ID.
    #[allow(clippy::too_many_arguments)]
    pub async fn post_work_failed(
        &self,
        receiver_id: &str,
        work_id: &str,
        work_name: &str,
        version: &str,
        platform_id: &str,
        package: &str,
        error_message: &str,
        error_code: Option<&str>,
    ) -> Result<String, ClientError> {
        let mut payload = serde_json::json!({
            "error": error_message,
        });

        if let Some(code) = error_code {
            payload["error_code"] = serde_json::Value::String(code.to_string());
        }

        self.post_work_completed(
            receiver_id,
            work_id,
            work_name,
            version,
            platform_id,
            package,
            false,
            payload,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: This test is marked #[ignore] because it modifies environment
    // variables which can interfere with parallel test execution. Run with:
    // cargo test -- --ignored --test-threads=1

    #[test]
    #[ignore = "modifies global environment variables"]
    fn test_client_config_from_env() {
        // SAFETY: These tests run single-threaded and manage their own env var cleanup
        unsafe {
            std::env::remove_var("XZEPR_API_URL");
            std::env::remove_var("XZEPR_API_TOKEN");
        }

        // Without token, should fail
        let result = XzeprClientConfig::from_env();
        assert!(matches!(result, Err(ClientError::Authentication(_))));

        // With token, should succeed
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::set_var("XZEPR_API_TOKEN", "test-token");
        }
        let config = XzeprClientConfig::from_env().unwrap();

        assert_eq!(config.base_url, "http://localhost:8042");
        assert_eq!(config.token, "test-token");
        assert_eq!(config.timeout_secs, 30);

        // With custom URL
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::set_var("XZEPR_API_URL", "http://custom:8080");
        }
        let config = XzeprClientConfig::from_env().unwrap();
        assert_eq!(config.base_url, "http://custom:8080");

        // Clean up
        // SAFETY: Test environment, no concurrent access
        unsafe {
            std::env::remove_var("XZEPR_API_URL");
            std::env::remove_var("XZEPR_API_TOKEN");
        }
    }

    #[test]
    fn test_create_event_receiver_request_serialization() {
        let request = CreateEventReceiverRequest {
            name: "test-receiver".to_string(),
            receiver_type: "worker".to_string(),
            version: "1.0.0".to_string(),
            description: "Test receiver".to_string(),
            schema: serde_json::json!({"type": "object"}),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"name\":\"test-receiver\""));
        assert!(json.contains("\"type\":\"worker\""));
    }

    #[test]
    fn test_create_event_request_serialization() {
        let request = CreateEventRequest {
            name: "test.event".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "kubernetes".to_string(),
            package: "testpkg".to_string(),
            description: "Test event".to_string(),
            payload: serde_json::json!({"key": "value"}),
            success: true,
            event_receiver_id: "receiver-id".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"name\":\"test.event\""));
        assert!(json.contains("\"success\":true"));
    }

    #[test]
    fn test_event_receiver_response_deserialization() {
        let json = r#"{
            "id": "receiver-123",
            "name": "test-receiver",
            "type": "worker",
            "version": "1.0.0",
            "description": "Test receiver",
            "schema": {"type": "object"},
            "fingerprint": "abc123",
            "created_at": "2025-01-15T10:30:00Z"
        }"#;

        let receiver: EventReceiverResponse = serde_json::from_str(json).unwrap();
        assert_eq!(receiver.id, "receiver-123");
        assert_eq!(receiver.name, "test-receiver");
        assert_eq!(receiver.receiver_type, "worker");
    }

    #[test]
    fn test_paginated_response_deserialization() {
        let json = r#"{
            "data": [{
                "id": "receiver-123",
                "name": "test-receiver",
                "type": "worker",
                "version": "1.0.0",
                "description": "Test receiver",
                "schema": {"type": "object"},
                "fingerprint": "abc123",
                "created_at": "2025-01-15T10:30:00Z"
            }],
            "pagination": {
                "limit": 10,
                "offset": 0,
                "total": 1,
                "has_more": false
            }
        }"#;

        let response: PaginatedResponse<EventReceiverResponse> =
            serde_json::from_str(json).unwrap();
        assert_eq!(response.data.len(), 1);
        assert_eq!(response.pagination.total, 1);
        assert!(!response.pagination.has_more);
    }

    #[test]
    fn test_client_new() {
        let config = XzeprClientConfig {
            base_url: "http://localhost:8042".to_string(),
            token: "test-token".to_string(),
            timeout_secs: 30,
        };

        let result = XzeprClient::new(config);
        assert!(result.is_ok());
    }
}
