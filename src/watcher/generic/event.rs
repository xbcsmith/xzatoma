//! Inbound plan event type for the generic Kafka watcher.
//!
//! This module defines [`GenericPlanEvent`]: the parsed, validated, in-memory
//! representation of an inbound plan trigger.
//!
//! The raw Kafka message boundary type [`RawKafkaMessage`] lives in
//! [`crate::watcher::generic::consumer`] and is re-exported from
//! [`crate::watcher::generic`].
//!
//! # Parsing model
//!
//! Rather than deserializing the raw wire format into a loosely-typed JSON
//! value and deferring plan validation to the executor, an improvement introduced
//! *early parsing*: [`GenericPlanEvent::new`] calls
//! [`PlanParser::parse_string`] on the raw payload string and returns `Err`
//! immediately if the payload cannot be parsed or validated as a [`Plan`].
//!
//! Any `GenericPlanEvent` value that exists in memory is therefore guaranteed
//! to hold a structurally valid plan.
//!
//! # Loop-break guarantee
//!
//! Previously the loop-break was enforced by an `event_type` discriminator
//! on the wire format. The loop-break is now implicit:
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
//! use xzatoma::watcher::generic::event::GenericPlanEvent;
//!
//! let yaml = "name: deploy\nsteps:\n  - name: apply\n    action: kubectl apply -f manifests/\n";
//! let event = GenericPlanEvent::new(yaml, "input.topic".to_string(), None).unwrap();
//! assert_eq!(event.plan.name, "deploy");
//! assert_eq!(event.source_topic, "input.topic");
//! ```

use crate::error::{Result, XzatomaError};
use crate::tools::plan::{Plan, PlanParser};
use crate::watcher::generic::message::GenericPlanCloudEvent;
use chrono::{DateTime, Utc};

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
    /// Parse a raw Kafka payload as a standard CloudEvents 1.0 envelope and
    /// extract the embedded plan from the `data` field.
    ///
    /// The payload must be a JSON object that satisfies the `GenericPlanCloudEvent`
    /// schema (i.e., it must carry `id`, `specversion`, `type`, `source`, and
    /// `data`). The `data` field is then deserialized and validated as a [`Plan`].
    ///
    /// This is intentionally distinct from the XZepr consumer, which expects
    /// XZepr-specific CloudEvent extensions (`success`, `api_version`,
    /// `platform_id`, etc.).
    pub fn new(payload: &str, source_topic: String, key: Option<String>) -> Result<Self> {
        let cloud_event: GenericPlanCloudEvent = serde_json::from_str(payload)
            .map_err(|e| XzatomaError::Watcher(format!("Error parsing CloudEvent: {}", e)))?;
        let plan = PlanParser::from_value(cloud_event.data)?;
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
    use crate::watcher::generic::consumer::RawKafkaMessage;

    // ---------------------------------------------------------------------------
    // Shared test payloads — standard CloudEvents 1.0 envelopes
    // ---------------------------------------------------------------------------

    // Steps-based plan wrapped in a CloudEvents envelope.
    const VALID_CE: &str = r#"{"id":"01JTEST000000000000000001","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","steps":[{"name":"apply","action":"kubectl apply"}]}}"#;

    // Steps-based plan with action field.
    const VALID_CE_WITH_ACTION: &str = r#"{"id":"01JTEST000000000000000002","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","action":"deploy-prod","steps":[{"name":"apply","action":"kubectl apply"}]}}"#;

    // Steps-based plan with version field.
    const VALID_CE_WITH_VERSION: &str = r#"{"id":"01JTEST000000000000000003","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","version":"v1.2.3","steps":[{"name":"apply","action":"kubectl apply"}]}}"#;

    // Task-based plan wrapped in a CloudEvents envelope.
    const VALID_CE_TASKS: &str = r#"{"id":"01JTEST000000000000000004","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"deploy","tasks":[{"id":"t1","description":"Run: kubectl apply"}]}}"#;

    // ---------------------------------------------------------------------------
    // Task 2.6 required tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_new_valid_cloud_event_steps() {
        let event =
            GenericPlanEvent::new(VALID_CE, "input.topic".to_string(), Some("k1".to_string()))
                .unwrap();
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(event.source_topic, "input.topic");
        assert_eq!(event.key.as_deref(), Some("k1"));
        assert_eq!(event.name.as_deref(), Some("deploy"));
    }

    #[test]
    fn test_new_valid_cloud_event_tasks() {
        let event = GenericPlanEvent::new(VALID_CE_TASKS, "input.topic".to_string(), None).unwrap();
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(event.plan.tasks.len(), 1);
        assert!(event.plan.steps.is_empty());
    }

    #[test]
    fn test_new_raw_plan_without_envelope_returns_err() {
        // Raw plan JSON without a CloudEvents envelope must fail — specversion,
        // source, and type are required CloudEvents attributes.
        let raw_plan = r#"{"name":"deploy","steps":[{"name":"apply","action":"kubectl apply"}]}"#;
        let result = GenericPlanEvent::new(raw_plan, "t".to_string(), None);
        assert!(
            result.is_err(),
            "raw plan without CloudEvents envelope must return Err"
        );
    }

    #[test]
    fn test_new_invalid_payload_returns_err() {
        let result = GenericPlanEvent::new("not a valid plan", "t".to_string(), None);
        assert!(result.is_err(), "malformed payload must return Err");
    }

    #[test]
    fn test_new_cloud_event_with_empty_tasks_returns_err() {
        let ce = r#"{"id":"01J","specversion":"1.0","type":"xzatoma.plan.execute","source":"test","data":{"name":"test","tasks":[]}}"#;
        let result = GenericPlanEvent::new(ce, "t".to_string(), None);
        assert!(
            result.is_err(),
            "plan with no tasks or steps must return Err from validation"
        );
    }

    #[test]
    fn test_new_received_at_is_recent() {
        let before = Utc::now();
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        let after = Utc::now();
        assert!(event.received_at >= before);
        assert!(event.received_at <= after);
    }

    #[test]
    fn test_clone_produces_independent_copy() {
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        let mut cloned = event.clone();
        cloned.plan.name = "different".to_string();
        assert_eq!(event.plan.name, "deploy");
        assert_eq!(cloned.plan.name, "different");
    }

    // ---------------------------------------------------------------------------
    // Additional coverage
    // ---------------------------------------------------------------------------

    #[test]
    fn test_new_action_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_CE_WITH_ACTION, "t".to_string(), None).unwrap();
        assert_eq!(event.action.as_deref(), Some("deploy-prod"));
    }

    #[test]
    fn test_new_action_is_none_when_plan_has_no_action() {
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        assert!(event.action.is_none());
    }

    #[test]
    fn test_new_version_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_CE_WITH_VERSION, "t".to_string(), None).unwrap();
        assert_eq!(event.version.as_deref(), Some("v1.2.3"));
    }

    #[test]
    fn test_new_version_defaults_to_none_when_plan_has_no_version() {
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        assert!(event.version.is_none());
    }

    #[test]
    fn test_new_name_auto_populated_from_plan() {
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        assert_eq!(event.name.as_deref(), Some("deploy"));
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
            VALID_CE,
            "t".to_string(),
            Some("correlation-abc".to_string()),
        )
        .unwrap();
        assert_eq!(event.key.as_deref(), Some("correlation-abc"));
    }

    #[test]
    fn test_new_key_is_none_when_not_provided() {
        let event = GenericPlanEvent::new(VALID_CE, "t".to_string(), None).unwrap();
        assert!(event.key.is_none());
    }

    #[test]
    fn test_bare_result_json_returns_err() {
        // A bare GenericPlanResult JSON (no CloudEvents envelope) must fail —
        // it lacks specversion, source, and type. Loop-break guarantee preserved.
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
            "bare result JSON must fail CloudEvents parsing"
        );
    }

    #[test]
    fn test_result_cloud_event_with_non_plan_data_returns_err() {
        // Even when wrapped in a valid CloudEvents envelope, if data is not a Plan
        // (no name/tasks/steps), validation must fail.
        let ce = r#"{
            "id": "01JRESULT000000000000000001",
            "specversion": "1.0",
            "type": "xzatoma.plan.result",
            "source": "xzatoma.watcher",
            "data": { "success": true, "summary": "done" }
        }"#;
        let err = GenericPlanEvent::new(ce, "t".to_string(), None);
        assert!(
            err.is_err(),
            "result data without plan fields must fail validation"
        );
    }
}
