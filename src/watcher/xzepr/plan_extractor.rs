//! Plan extraction from XZepr CloudEvents
//!
//! Provides strategies for extracting and parsing plans from XZepr CloudEvent
//! message payloads.
//!
//! This module was relocated from `src/watcher/plan_extractor.rs` into
//! `src/watcher/xzepr/` as part of the generic watcher architecture.

use crate::watcher::xzepr::consumer::CloudEventMessage;
use serde_json::Value as JsonValue;
use thiserror::Error;

/// Result type for XZepr plan extraction operations.
pub type PlanExtractionResult<T> = std::result::Result<T, PlanExtractionError>;

/// Errors that can occur while extracting a plan from an XZepr event.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::xzepr::plan_extractor::PlanExtractionError;
///
/// let error = PlanExtractionError::NoEvents;
/// assert!(error.to_string().contains("no events"));
/// ```
#[derive(Debug, Error)]
pub enum PlanExtractionError {
    /// Event data did not contain any events.
    #[error("no events in data")]
    NoEvents,
    /// The selected payload did not contain a plan field.
    #[error("missing plan field at {location}")]
    MissingPlanField {
        /// Location inspected for the plan field.
        location: &'static str,
    },
    /// JSON serialization failed while turning structured data into plan text.
    #[error("failed to serialize plan value from {location}: {source}")]
    Serialization {
        /// Location being serialized.
        location: &'static str,
        /// Underlying JSON serialization error.
        #[source]
        source: serde_json::Error,
    },
    /// A candidate value had an unsupported JSON type.
    #[error("plan value at {location} is not a string, object, or array")]
    UnsupportedValueType {
        /// Location that contained the unsupported value.
        location: &'static str,
    },
    /// No extraction strategy produced a plan.
    #[error("failed to extract plan from event {event_id} using any strategy")]
    NoStrategyMatched {
        /// Event identifier.
        event_id: String,
    },
}

/// Strategies for extracting plans from event data.
///
/// Different CloudEvent payloads may store plans in different locations within
/// the event data structure. This enum represents the different known strategies
/// for locating plans within event payloads.
#[derive(Debug, Clone)]
pub enum PlanExtractionStrategy {
    /// Plan is in data.events[0].payload.plan field
    EventPayloadPlan,

    /// Plan is in data.events[0].payload field (entire payload is the plan)
    EventPayload,

    /// Plan is in data.plan field
    DataPlan,

    /// Plan is the entire data field
    DataRoot,
}

/// Extract plans from XZepr CloudEvent messages.
///
/// Attempts multiple strategies to find and extract plans from CloudEvent payloads.
/// The extractor tries strategies in priority order and returns the first successfully
/// extracted plan.
#[derive(Clone)]
pub struct PlanExtractor {
    strategies: Vec<PlanExtractionStrategy>,
}

impl PlanExtractor {
    /// Create a new plan extractor with default strategies.
    ///
    /// # Returns
    ///
    /// Returns a new PlanExtractor configured with default extraction strategies
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::xzepr::plan_extractor::PlanExtractor;
    ///
    /// let extractor = PlanExtractor::new();
    /// // Can now extract from CloudEventMessage instances
    /// ```
    pub fn new() -> Self {
        Self {
            strategies: vec![
                PlanExtractionStrategy::EventPayloadPlan,
                PlanExtractionStrategy::EventPayload,
                PlanExtractionStrategy::DataPlan,
                PlanExtractionStrategy::DataRoot,
            ],
        }
    }

    /// Extract plan string from XZepr CloudEvent message.
    ///
    /// Tries each extraction strategy in order until one successfully extracts a plan.
    /// Logs which strategy was used for debugging purposes.
    ///
    /// # Arguments
    ///
    /// * `event` - The CloudEvent message to extract from
    ///
    /// # Returns
    ///
    /// Returns the extracted plan as a string, or error if no strategy succeeded
    ///
    /// # Errors
    ///
    /// Returns an error if no extraction strategy succeeds for the given event.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use xzatoma::watcher::xzepr::plan_extractor::PlanExtractor;
    /// use xzatoma::watcher::xzepr::consumer::message::CloudEventMessage;
    ///
    /// let extractor = PlanExtractor::new();
    /// let json = r#"{
    ///   "success": true,
    ///   "id": "01JTEST1234567890123456",
    ///   "specversion": "1.0.1",
    ///   "type": "test.event",
    ///   "source": "test/source",
    ///   "api_version": "v1",
    ///   "name": "test",
    ///   "version": "1.0.0",
    ///   "release": "1.0.0",
    ///   "platform_id": "test-platform",
    ///   "package": "test-package",
    ///   "data": {
    ///     "events": [
    ///       {
    ///         "id": "event-1",
    ///         "name": "evt",
    ///         "version": "1.0",
    ///         "release": "1.0.0",
    ///         "platform_id": "test-platform",
    ///         "package": "test-package",
    ///         "description": "desc",
    ///         "payload": { "plan": "- task: setup\n  commands: echo hello" },
    ///         "success": true,
    ///         "event_receiver_id": "receiver-1",
    ///         "created_at": "2025-01-01T00:00:00Z"
    ///       }
    ///     ],
    ///     "event_receivers": [],
    ///     "event_receiver_groups": []
    ///   }
    /// }"#;
    /// let event: CloudEventMessage = serde_json::from_str(json).unwrap();
    /// let plan = extractor.extract(&event).unwrap();
    /// assert!(plan.contains("setup"));
    /// ```
    pub fn extract(&self, event: &CloudEventMessage) -> PlanExtractionResult<String> {
        for strategy in &self.strategies {
            if let Ok(plan) = self.try_extract(event, strategy) {
                tracing::debug!(
                    strategy = ?strategy,
                    event_id = %event.id,
                    "Successfully extracted plan using strategy"
                );
                return Ok(plan);
            }
        }

        Err(PlanExtractionError::NoStrategyMatched {
            event_id: event.id.clone(),
        })
    }

    fn try_extract(
        &self,
        event: &CloudEventMessage,
        strategy: &PlanExtractionStrategy,
    ) -> PlanExtractionResult<String> {
        let json_value = match strategy {
            PlanExtractionStrategy::EventPayload => event
                .data
                .events
                .first()
                .ok_or(PlanExtractionError::NoEvents)?
                .payload
                .clone(),
            PlanExtractionStrategy::EventPayloadPlan => {
                let event_entity = event
                    .data
                    .events
                    .first()
                    .ok_or(PlanExtractionError::NoEvents)?;

                event_entity
                    .payload
                    .get("plan")
                    .ok_or(PlanExtractionError::MissingPlanField {
                        location: "data.events[0].payload",
                    })?
                    .clone()
            }
            PlanExtractionStrategy::DataRoot => {
                serde_json::to_value(&event.data).map_err(|source| {
                    PlanExtractionError::Serialization {
                        location: "data",
                        source,
                    }
                })?
            }
            PlanExtractionStrategy::DataPlan => serde_json::to_value(&event.data)
                .map_err(|source| PlanExtractionError::Serialization {
                    location: "data",
                    source,
                })?
                .get("plan")
                .ok_or(PlanExtractionError::MissingPlanField { location: "data" })?
                .clone(),
        };

        self.parse_plan_from_json(&json_value, strategy.location())
    }

    fn parse_plan_from_json(
        &self,
        value: &JsonValue,
        location: &'static str,
    ) -> PlanExtractionResult<String> {
        match value {
            JsonValue::String(s) => Ok(s.clone()),
            JsonValue::Object(_) | JsonValue::Array(_) => serde_json::to_string(value)
                .map_err(|source| PlanExtractionError::Serialization { location, source }),
            _ => Err(PlanExtractionError::UnsupportedValueType { location }),
        }
    }
}

impl PlanExtractionStrategy {
    fn location(&self) -> &'static str {
        match self {
            Self::EventPayloadPlan => "data.events[0].payload.plan",
            Self::EventPayload => "data.events[0].payload",
            Self::DataPlan => "data.plan",
            Self::DataRoot => "data",
        }
    }
}

impl Default for PlanExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::xzepr::consumer::message::{CloudEventData, EventEntity};
    use serde_json::json;

    fn create_test_event_with_payload(payload: serde_json::Value) -> CloudEventMessage {
        use chrono::Utc;

        CloudEventMessage {
            success: true,
            id: "test-id-123".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "test/source".to_string(),
            api_version: "v1".to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "test-platform".to_string(),
            package: "test-package".to_string(),
            data: CloudEventData {
                events: vec![EventEntity {
                    id: "event-1".to_string(),
                    name: "test-event".to_string(),
                    version: "1.0.0".to_string(),
                    release: "1.0.0".to_string(),
                    platform_id: "test-platform".to_string(),
                    package: "test-package".to_string(),
                    description: "Test event".to_string(),
                    payload,
                    success: true,
                    event_receiver_id: "receiver-1".to_string(),
                    created_at: Utc::now(),
                }],
                event_receivers: vec![],
                event_receiver_groups: vec![],
            },
        }
    }

    #[test]
    fn test_extract_from_event_payload_plan() {
        let payload = json!({
            "plan": "- task: setup\n  commands: echo hello"
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "- task: setup\n  commands: echo hello");
    }

    #[test]
    fn test_extract_from_event_payload_entire() {
        let payload = json!("- task: setup\n  commands: echo hello");
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "- task: setup\n  commands: echo hello");
    }

    #[test]
    fn test_extract_payload_as_object() {
        let payload = json!({
            "name": "test-plan",
            "tasks": [
                {"name": "task1", "command": "echo 1"},
                {"name": "task2", "command": "echo 2"}
            ]
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(extracted.contains("test-plan"));
        assert!(extracted.contains("task1"));
    }

    #[test]
    fn test_extract_from_data_plan() {
        let payload = json!({
            "plan": "- task: deploy\n  commands: kubectl apply"
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "- task: deploy\n  commands: kubectl apply");
    }

    #[test]
    fn test_extract_from_data_root() {
        let event = CloudEventMessage {
            success: true,
            id: "test-id-root".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "test/source".to_string(),
            api_version: "v1".to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "test-platform".to_string(),
            package: "test-package".to_string(),
            data: CloudEventData::default(),
        };

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        // Should succeed with DataRoot strategy returning empty data structure
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(extracted.contains("events"));
    }

    #[test]
    fn test_extract_no_events_falls_back_to_data_root() {
        let event = CloudEventMessage {
            success: true,
            id: "test-id-empty".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "test/source".to_string(),
            api_version: "v1".to_string(),
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "test-platform".to_string(),
            package: "test-package".to_string(),
            data: CloudEventData {
                events: vec![],
                event_receivers: vec![],
                event_receiver_groups: vec![],
            },
        };

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        // Should succeed by falling back to DataRoot strategy
        assert!(result.is_ok());
        let extracted = result.unwrap();
        assert!(extracted.contains("events"));
    }

    #[test]
    fn test_extract_plan_as_json_object() {
        let payload = json!({
            "plan": {
                "version": "1.0",
                "tasks": [
                    {"id": "1", "name": "setup"},
                    {"id": "2", "name": "run"}
                ]
            }
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        let extracted = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&extracted).unwrap();
        assert_eq!(parsed["version"], "1.0");
        assert_eq!(parsed["tasks"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_extract_plan_as_json_array() {
        let payload = json!({
            "plan": [
                {"task": "first", "cmd": "echo 1"},
                {"task": "second", "cmd": "echo 2"}
            ]
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        let extracted = result.unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&extracted).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn test_strategy_priority_event_payload_plan_first() {
        let payload = json!({
            "plan": "from-payload-plan",
            "other": "from-payload"
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        // Should extract from "plan" field, not entire payload
        assert_eq!(result.unwrap(), "from-payload-plan");
    }

    #[test]
    fn test_default_extractor() {
        let extractor = PlanExtractor::default();
        let payload = json!({"plan": "test-plan"});
        let event = create_test_event_with_payload(payload);

        let result = extractor.extract(&event);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "test-plan");
    }

    #[test]
    fn test_extract_multiline_yaml_string() {
        let plan_yaml = "---\ntasks:\n  - name: Deploy\n    cmd: kubectl apply";
        let payload = json!({
            "plan": plan_yaml
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), plan_yaml);
    }

    #[test]
    fn test_extract_with_special_characters() {
        let plan = "tasks:\n  - cmd: 'echo \"hello world\"'\n  - cmd: 'echo $PATH'";
        let payload = json!({
            "plan": plan
        });
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), plan);
    }

    #[test]
    fn test_extract_empty_string_plan_succeeds() {
        let payload = json!("");
        let event = create_test_event_with_payload(payload);

        let extractor = PlanExtractor::new();
        let result = extractor.extract(&event);

        // Empty string is a valid plan string
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_try_extract_numeric_payload_reports_unsupported_value_type() {
        let payload = json!(42);
        let event = create_test_event_with_payload(payload);
        let extractor = PlanExtractor::new();

        let error = extractor
            .try_extract(&event, &PlanExtractionStrategy::EventPayload)
            .unwrap_err();

        assert!(matches!(
            error,
            PlanExtractionError::UnsupportedValueType {
                location: "data.events[0].payload"
            }
        ));
    }

    #[test]
    fn test_extract_missing_plan_with_limited_strategy_reports_failure() {
        let payload = json!({ "not_plan": "missing" });
        let event = create_test_event_with_payload(payload);
        let extractor = PlanExtractor {
            strategies: vec![PlanExtractionStrategy::EventPayloadPlan],
        };

        let error = extractor.extract(&event).unwrap_err();

        assert!(matches!(
            error,
            PlanExtractionError::NoStrategyMatched { event_id }
                if event_id == "test-id-123"
        ));
    }
}
