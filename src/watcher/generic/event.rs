//! Inbound plan event and raw Kafka message types for the generic watcher.
//!
//! This module defines two closely related types:
//!
//! - [`RawKafkaMessage`]: the bridge between the raw Kafka byte stream and the
//!   parsed plan event.
//! - [`GenericPlanEvent`]: the parsed, validated, in-memory representation of
//!   an inbound plan trigger.
//!
//! # Parsing model
//!
//! Rather than deserializing the raw wire format into a loosely-typed JSON
//! value and deferring plan validation to the executor, Phase 2 introduces
//! *early parsing*: [`GenericPlanEvent::new`] calls
//! [`PlanParser::parse_string`] on the raw payload string and returns `Err`
//! immediately if the payload cannot be parsed or validated as a [`Plan`].
//!
//! Any `GenericPlanEvent` value that exists in memory is therefore guaranteed
//! to hold a structurally valid plan.
//!
//! # Loop-break guarantee
//!
//! In the Phase 1 design the loop-break was enforced by an `event_type`
//! discriminator on the wire format. In Phase 2 the loop-break is implicit:
//! when the watcher publishes a
//! [`GenericPlanResult`](crate::watcher::generic::result_event::GenericPlanResult)
//! and that JSON payload is later consumed from the same topic, it fails to
//! parse as a [`Plan`] (the result JSON has no `name` or `steps` fields),
//! causing [`GenericPlanEvent::new`] to return `Err`. The
//! [`GenericEventHandler`](crate::watcher::generic::event_handler::GenericEventHandler)
//! propagates the error and the watcher discards the message as an invalid
//! payload without producing a new result.
//!
//! # Examples
//!
//! ```
//! use xzatoma::watcher::generic::event::{GenericPlanEvent, RawKafkaMessage};
//!
//! let yaml = "name: deploy\nsteps:\n  - name: apply\n    action: kubectl apply -f manifests/\n";
//! let event = GenericPlanEvent::new(yaml, "input.topic".to_string(), None).unwrap();
//! assert_eq!(event.plan.name, "deploy");
//! assert_eq!(event.source_topic, "input.topic");
//! ```

use crate::error::Result;
use crate::tools::plan::{Plan, PlanParser};
use chrono::{DateTime, Utc};

/// A raw Kafka message payload before plan parsing.
///
/// `RawKafkaMessage` carries the raw Kafka payload string alongside the source
/// topic and optional message key. It is the primary input to
/// [`GenericEventHandler::handle`](crate::watcher::generic::event_handler::GenericEventHandler::handle).
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::event::RawKafkaMessage;
///
/// let msg = RawKafkaMessage {
///     payload: "name: deploy\nsteps:\n  - name: s1\n    action: echo hi\n".to_string(),
///     topic: "plans.input".to_string(),
///     key: Some("correlation-123".to_string()),
/// };
/// assert_eq!(msg.topic, "plans.input");
/// assert!(msg.key.is_some());
/// ```
#[derive(Debug, Clone)]
pub struct RawKafkaMessage {
    /// The raw Kafka message payload (UTF-8 encoded).
    pub payload: String,
    /// The Kafka topic from which this message was consumed.
    pub topic: String,
    /// Optional Kafka message key used as the correlation key for result tracking.
    pub key: Option<String>,
}

/// A parsed and validated inbound plan event for the generic Kafka watcher.
///
/// A `GenericPlanEvent` is constructed from a raw Kafka payload via
/// [`GenericPlanEvent::new`]. The constructor delegates to
/// [`PlanParser::parse_string`] so any instance that reaches the matcher or
/// executor is guaranteed to hold a structurally valid [`Plan`].
///
/// The `name`, `version`, and `action` fields are auto-populated from the
/// parsed plan at construction time and may be overridden afterward (e.g. in
/// tests or when version information is injected from an external source).
///
/// # Field summary
///
/// | Field          | Source                                                      |
/// |----------------|-------------------------------------------------------------|
/// | `plan`         | Parsed and validated from the raw payload                   |
/// | `source_topic` | Kafka topic the message was consumed from                   |
/// | `key`          | Kafka message key (correlation identifier)                  |
/// | `received_at`  | Set to `Utc::now()` at construction time                    |
/// | `name`         | Auto-populated from `plan.name`                             |
/// | `version`      | Auto-populated from `plan.version`; `None` when absent      |
/// | `action`       | Auto-populated from `plan.action`; `None` when absent       |
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::event::GenericPlanEvent;
///
/// let yaml = "name: deploy\nsteps:\n  - name: s1\n    action: run deploy\n";
/// let event = GenericPlanEvent::new(
///     yaml,
///     "input.topic".to_string(),
///     Some("key-1".to_string()),
/// )
/// .unwrap();
///
/// assert_eq!(event.plan.name, "deploy");
/// assert_eq!(event.name.as_deref(), Some("deploy"));
/// assert_eq!(event.key.as_deref(), Some("key-1"));
/// ```
#[derive(Debug, Clone)]
pub struct GenericPlanEvent {
    /// The parsed and validated plan.
    pub plan: Plan,

    /// The Kafka topic from which the triggering message was consumed.
    pub source_topic: String,

    /// The Kafka message key, used as the correlation identifier for result tracking.
    pub key: Option<String>,

    /// UTC timestamp of when this event was received and parsed.
    pub received_at: DateTime<Utc>,

    /// Name label used for name-based watcher matching.
    ///
    /// Auto-populated from [`Plan::name`] at construction time.
    pub name: Option<String>,

    /// Version label used for version-based watcher matching.
    ///
    /// Auto-populated from [`Plan::version`] at construction time when the
    /// parsed plan carries a `version` field. `None` when the plan has no
    /// `version`.
    pub version: Option<String>,

    /// Action label used for action-based watcher matching.
    ///
    /// Auto-populated from [`Plan::action`] at construction time when the
    /// parsed plan carries an `action` field. `None` when the plan has no
    /// `action`.
    pub action: Option<String>,
}

impl GenericPlanEvent {
    /// Parse a raw Kafka payload into a validated plan event.
    ///
    /// Calls [`PlanParser::parse_string`] to parse the payload as YAML or
    /// JSON, then populates the event fields. Returns `Err` if the payload
    /// cannot be parsed or if the parsed plan fails validation (e.g. empty
    /// `name`, empty `steps`, or a step with no `action`).
    ///
    /// # Arguments
    ///
    /// * `payload`      - Raw UTF-8 Kafka payload containing a YAML or JSON plan
    /// * `source_topic` - The Kafka topic from which the message was consumed
    /// * `key`          - Optional Kafka message key (used as the correlation key)
    ///
    /// # Returns
    ///
    /// A `GenericPlanEvent` with `received_at` set to the current UTC time and
    /// `name`, `version`, and `action` auto-populated from the parsed plan.
    ///
    /// # Errors
    ///
    /// Returns an error if `payload` cannot be deserialized as a [`Plan`] or if
    /// plan validation fails.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::event::GenericPlanEvent;
    ///
    /// let yaml = "name: deploy\nsteps:\n  - name: s1\n    action: run deploy\n";
    /// let event = GenericPlanEvent::new(yaml, "input.topic".to_string(), None).unwrap();
    /// assert_eq!(event.plan.name, "deploy");
    /// assert_eq!(event.name.as_deref(), Some("deploy"));
    /// ```
    pub fn new(payload: &str, source_topic: String, key: Option<String>) -> Result<Self> {
        let plan = PlanParser::parse_string(payload)?;
        let name = Some(plan.name.clone());
        let version = plan.version.clone();
        let action = plan.action.clone();
        Ok(Self {
            plan,
            source_topic,
            key,
            received_at: Utc::now(),
            name,
            version,
            action,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------------------
    // Shared test payloads
    // ---------------------------------------------------------------------------

    const VALID_YAML: &str = "name: deploy\nsteps:\n  - name: apply\n    action: kubectl apply\n";
    const VALID_YAML_WITH_ACTION: &str =
        "name: deploy\naction: deploy-prod\nsteps:\n  - name: apply\n    action: kubectl apply\n";
    const VALID_YAML_WITH_VERSION: &str =
        "name: deploy\nversion: v1.2.3\nsteps:\n  - name: apply\n    action: kubectl apply\n";
    const VALID_JSON: &str =
        r#"{"name":"deploy","steps":[{"name":"apply","action":"kubectl apply"}]}"#;

    // ---------------------------------------------------------------------------
    // Task 2.6 required tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_new_valid_yaml_payload() {
        let event = GenericPlanEvent::new(
            VALID_YAML,
            "input.topic".to_string(),
            Some("k1".to_string()),
        )
        .unwrap();
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(event.source_topic, "input.topic");
        assert_eq!(event.key.as_deref(), Some("k1"));
        assert_eq!(event.name.as_deref(), Some("deploy"));
    }

    #[test]
    fn test_new_valid_json_payload() {
        let event = GenericPlanEvent::new(VALID_JSON, "input.topic".to_string(), None).unwrap();
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(event.plan.steps.len(), 1);
    }

    #[test]
    fn test_new_invalid_payload_returns_err() {
        let result = GenericPlanEvent::new("not a valid plan", "t".to_string(), None);
        assert!(result.is_err(), "malformed payload must return Err");
    }

    #[test]
    fn test_new_missing_tasks_returns_err() {
        // Structurally valid YAML with an empty steps list must fail validation.
        let result = GenericPlanEvent::new("name: test\nsteps: []\n", "t".to_string(), None);
        assert!(
            result.is_err(),
            "plan with no tasks must return Err from validation"
        );
    }

    #[test]
    fn test_new_received_at_is_recent() {
        let before = Utc::now();
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        let after = Utc::now();
        assert!(
            event.received_at >= before,
            "received_at must not be before construction"
        );
        assert!(
            event.received_at <= after,
            "received_at must not be after construction"
        );
    }

    #[test]
    fn test_clone_produces_independent_copy() {
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        let mut cloned = event.clone();
        cloned.plan.name = "different".to_string();
        // The original must not be affected.
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(cloned.plan.name, "different");
    }

    // ---------------------------------------------------------------------------
    // Additional coverage
    // ---------------------------------------------------------------------------

    #[test]
    fn test_new_action_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_YAML_WITH_ACTION, "t".to_string(), None).unwrap();
        assert_eq!(
            event.action.as_deref(),
            Some("deploy-prod"),
            "action should be auto-populated from plan.action"
        );
    }

    #[test]
    fn test_new_action_is_none_when_plan_has_no_action() {
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        assert!(
            event.action.is_none(),
            "action should be None when plan carries no action field"
        );
    }

    #[test]
    fn test_new_version_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_YAML_WITH_VERSION, "t".to_string(), None).unwrap();
        assert_eq!(
            event.version.as_deref(),
            Some("v1.2.3"),
            "version should be auto-populated from plan.version"
        );
    }

    #[test]
    fn test_new_version_defaults_to_none_when_plan_has_no_version() {
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        assert!(
            event.version.is_none(),
            "version should be None when plan carries no version field"
        );
    }

    #[test]
    fn test_new_name_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        assert_eq!(
            event.name.as_deref(),
            Some("deploy"),
            "name should mirror plan.name"
        );
    }

    #[test]
    fn test_raw_kafka_message_fields_accessible() {
        let msg = RawKafkaMessage {
            payload: "payload-data".to_string(),
            topic: "my-topic".to_string(),
            key: Some("my-key".to_string()),
        };
        assert_eq!(msg.payload, "payload-data");
        assert_eq!(msg.topic, "my-topic");
        assert_eq!(msg.key.as_deref(), Some("my-key"));
    }

    #[test]
    fn test_raw_kafka_message_key_can_be_none() {
        let msg = RawKafkaMessage {
            payload: "data".to_string(),
            topic: "t".to_string(),
            key: None,
        };
        assert!(msg.key.is_none());
    }

    #[test]
    fn test_new_key_propagated_to_event() {
        let event = GenericPlanEvent::new(
            VALID_YAML,
            "t".to_string(),
            Some("correlation-abc".to_string()),
        )
        .unwrap();
        assert_eq!(event.key.as_deref(), Some("correlation-abc"));
    }

    #[test]
    fn test_new_key_is_none_when_not_provided() {
        let event = GenericPlanEvent::new(VALID_YAML, "t".to_string(), None).unwrap();
        assert!(event.key.is_none());
    }

    #[test]
    fn test_new_result_json_payload_returns_err() {
        // Simulate a GenericPlanResult JSON being consumed back on the same
        // topic. It must fail to parse as a Plan, providing the loop-break.
        let result_json = r#"{
            "id": "01JRESULT000000000000000001",
            "event_type": "result",
            "trigger_event_id": "01JTRIGGER000000000000000",
            "success": true,
            "summary": "done",
            "timestamp": "2025-01-01T00:00:00Z"
        }"#;
        let err = GenericPlanEvent::new(result_json, "t".to_string(), None);
        assert!(
            err.is_err(),
            "result event JSON must fail to parse as a plan"
        );
    }
}
