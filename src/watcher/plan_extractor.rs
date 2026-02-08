//! Plan extraction from CloudEvents
//!
//! Provides strategies for extracting and parsing plans from CloudEvent message payloads.

use crate::xzepr::CloudEventMessage;
use anyhow::{anyhow, Result};
use serde_json::Value as JsonValue;

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

/// Extract plans from CloudEvent messages.
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
    /// use xzatoma::watcher::PlanExtractor;
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

    /// Extract plan string from CloudEvent message.
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
    /// # Examples
    ///
    /// ```ignore
    /// use xzatoma::watcher::PlanExtractor;
    /// use xzatoma::xzepr::CloudEventMessage;
    ///
    /// let extractor = PlanExtractor::new();
    /// let plan = extractor.extract(&event)?;
    /// ```
    pub fn extract(&self, event: &CloudEventMessage) -> Result<String> {
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

        Err(anyhow!(
            "Failed to extract plan from event {} using any strategy",
            event.id
        ))
    }

    fn try_extract(
        &self,
        event: &CloudEventMessage,
        strategy: &PlanExtractionStrategy,
    ) -> Result<String> {
        let json_value = match strategy {
            PlanExtractionStrategy::EventPayload => event
                .data
                .events
                .first()
                .ok_or_else(|| anyhow!("No events in data"))?
                .payload
                .clone(),
            PlanExtractionStrategy::EventPayloadPlan => {
                let event_entity = event
                    .data
                    .events
                    .first()
                    .ok_or_else(|| anyhow!("No events in data"))?;

                event_entity
                    .payload
                    .get("plan")
                    .ok_or_else(|| anyhow!("No plan field in event payload"))?
                    .clone()
            }
            PlanExtractionStrategy::DataRoot => serde_json::to_value(&event.data)?,
            PlanExtractionStrategy::DataPlan => serde_json::to_value(&event.data)?
                .get("plan")
                .ok_or_else(|| anyhow!("No plan field in data"))?
                .clone(),
        };

        self.parse_plan_from_json(&json_value)
    }

    fn parse_plan_from_json(&self, value: &JsonValue) -> Result<String> {
        match value {
            JsonValue::String(s) => Ok(s.clone()),
            JsonValue::Object(_) | JsonValue::Array(_) => Ok(serde_json::to_string(value)?),
            _ => Err(anyhow!("Plan value is not a string, object, or array")),
        }
    }
}

/// Trait-based abstraction for plan extraction.
///
/// This allows consumers to depend on the trait (e.g. in tests) and enables
/// mocking or alternative implementations without changing the core
/// `PlanExtractor` implementation.
pub trait PlanExtractorTrait: Send + Sync {
    /// Extract plan YAML/text from a CloudEvent message.
    ///
    /// Implementors should attempt to locate and return a plan string using
    /// whatever strategy is appropriate.
    fn extract(&self, event: &CloudEventMessage) -> Result<String>;
}

impl PlanExtractorTrait for PlanExtractor {
    fn extract(&self, event: &CloudEventMessage) -> Result<String> {
        // Delegate to the existing inherent method implementation.
        PlanExtractor::extract(self, event)
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
    use crate::xzepr::consumer::message::{CloudEventData, EventEntity};
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
    fn test_plan_extractor_trait_is_send_sync() {
        // Compile-time assertion that the trait object is Send + Sync.
        // If `PlanExtractorTrait` is not `Send + Sync`, this will not compile.
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<PlanExtractor>();
        _assert_send_sync::<std::sync::Arc<dyn PlanExtractorTrait>>();
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
}
