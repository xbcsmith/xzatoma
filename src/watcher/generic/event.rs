//! Inbound plan event type for the generic Kafka watcher.
//!
//! This module defines [`GenericPlanEvent`], the trigger message a producer
//! publishes to the watcher's input topic. The generic watcher deserializes
//! each consumed message into a `GenericPlanEvent`, checks that
//! `event_type == "plan"`, evaluates the optional `action`, `name`, and
//! `version` fields against the configured
//! [`GenericMatchConfig`](crate::config::GenericMatchConfig), and executes
//! the embedded plan when all criteria are satisfied.
//!
//! # Loop-break guarantee
//!
//! The primary guard against same-topic re-trigger loops is the `event_type`
//! field:
//!
//! - Producers MUST set `event_type = "plan"` on every trigger event.
//! - [`GenericPlanEvent::is_plan_event`] returns `false` for any other value.
//! - The generic matcher calls [`GenericPlanEvent::is_plan_event`] before
//!   evaluating any other criteria, so result events and unknown event types
//!   are discarded without plan execution.
//!
//! # Examples
//!
//! ```
//! use xzatoma::watcher::generic::event::GenericPlanEvent;
//! use serde_json::json;
//!
//! let plan_value = json!({
//!     "name": "deploy",
//!     "steps": [{"name": "apply", "action": "kubectl apply -f manifests/"}]
//! });
//!
//! let event = GenericPlanEvent::new("01JTEST0000000000000000001".to_string(), plan_value);
//! assert_eq!(event.event_type, "plan");
//! assert!(event.action.is_none());
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The trigger message schema for the generic Kafka watcher.
///
/// A producer publishes a `GenericPlanEvent` to the watcher's input topic. The
/// generic watcher deserializes each message, checks `event_type == "plan"`, then
/// evaluates the optional `action`, `name`, and `version` fields against the
/// configured [`GenericMatchConfig`](crate::config::GenericMatchConfig).
///
/// # Field summary
///
/// | Field        | Required | Purpose                                         |
/// |--------------|----------|-------------------------------------------------|
/// | `id`         | yes      | Unique event identifier (ULID recommended)      |
/// | `event_type` | yes      | Must be `"plan"` — all other values are skipped |
/// | `plan`       | yes      | Embedded plan (string, object, or array)        |
/// | `action`     | no       | Action label for action-based watcher matching  |
/// | `name`       | no       | Name label for name-based watcher matching      |
/// | `version`    | no       | Version label for version-based matching        |
/// | `timestamp`  | no       | RFC-3339 event creation timestamp               |
/// | `metadata`   | no       | Arbitrary extra fields for extensibility        |
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::event::GenericPlanEvent;
/// use serde_json::json;
///
/// let plan_value = json!({
///     "name": "deploy",
///     "steps": [{"name": "apply", "action": "kubectl apply -f manifests/"}]
/// });
///
/// let event = GenericPlanEvent::new("01JTEST0000000000000000001".to_string(), plan_value);
/// assert_eq!(event.event_type, "plan");
/// assert!(event.action.is_none());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenericPlanEvent {
    /// Unique event identifier. ULID format is preferred for its time-sortable
    /// properties, but any non-empty string is accepted.
    pub id: String,

    /// Event type discriminator. Must be `"plan"` for the generic watcher to
    /// process this message. Any other value (including `"result"` or empty string)
    /// causes the watcher to silently skip the message without plan execution.
    pub event_type: String,

    /// Optional name label used for name-based watcher matching.
    ///
    /// When [`GenericMatchConfig::name`](crate::config::GenericMatchConfig) is set,
    /// the watcher compares this field to the configured value. A `None` value
    /// does not satisfy a name-based match criterion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Optional version label used for version-based watcher matching.
    ///
    /// When [`GenericMatchConfig::version`](crate::config::GenericMatchConfig) is
    /// set, the watcher compares this field to the configured value. A `None` value
    /// does not satisfy a version-based match criterion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Optional action label used for action-based watcher matching.
    ///
    /// When [`GenericMatchConfig::action`](crate::config::GenericMatchConfig) is
    /// set, the watcher compares this field to the configured value. A `None` value
    /// does not satisfy an action-based match criterion.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,

    /// The embedded plan to execute. May be a JSON string, object, or array.
    ///
    /// The watcher passes this value to the plan executor after a successful match.
    /// String values are treated as YAML plan text; object and array values are
    /// serialized to JSON before being handed to the executor.
    pub plan: serde_json::Value,

    /// Optional RFC-3339 event creation timestamp.
    ///
    /// Consumers may use this for event ordering or staleness checks. The watcher
    /// does not enforce a maximum age by default; that logic belongs in the matcher.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<DateTime<Utc>>,

    /// Optional arbitrary metadata for extensibility.
    ///
    /// Producers may include any additional fields here (e.g. correlation IDs,
    /// environment tags). The watcher does not inspect this field; it is preserved
    /// in the corresponding
    /// [`GenericPlanResult`](crate::watcher::generic::result_event::GenericPlanResult)
    /// for traceability.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

impl GenericPlanEvent {
    /// Create a minimal plan trigger event.
    ///
    /// Sets `event_type` to `"plan"` and `timestamp` to the current UTC time.
    /// All optional matching fields (`action`, `name`, `version`) default to `None`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique event identifier (ULID format recommended)
    /// * `plan` - Embedded plan as a JSON value (string, object, or array)
    ///
    /// # Returns
    ///
    /// A new `GenericPlanEvent` with `event_type = "plan"` and the given id and plan.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::event::GenericPlanEvent;
    /// use serde_json::json;
    ///
    /// let event = GenericPlanEvent::new(
    ///     "01JTEST0000000000000000001".to_string(),
    ///     json!("name: hello\nsteps:\n  - name: s1\n    action: echo hi\n"),
    /// );
    /// assert_eq!(event.event_type, "plan");
    /// assert!(event.is_plan_event());
    /// ```
    pub fn new(id: String, plan: serde_json::Value) -> Self {
        Self {
            id,
            event_type: "plan".to_string(),
            name: None,
            version: None,
            action: None,
            plan,
            timestamp: Some(Utc::now()),
            metadata: None,
        }
    }

    /// Return `true` if this event should be processed as a plan trigger.
    ///
    /// This is the primary loop-break check: the generic matcher calls this before
    /// evaluating any other matching criteria. Events where `event_type != "plan"`
    /// — including result events (`event_type = "result"`) — are discarded without
    /// plan execution.
    ///
    /// # Returns
    ///
    /// `true` when `event_type == "plan"`, `false` for all other values.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::event::GenericPlanEvent;
    /// use serde_json::json;
    ///
    /// let plan_event = GenericPlanEvent::new("id-1".to_string(), json!(null));
    /// assert!(plan_event.is_plan_event());
    ///
    /// let mut result_event = GenericPlanEvent::new("id-2".to_string(), json!(null));
    /// result_event.event_type = "result".to_string();
    /// assert!(!result_event.is_plan_event());
    /// ```
    pub fn is_plan_event(&self) -> bool {
        self.event_type == "plan"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -------------------------------------------------------------------------
    // GenericPlanEvent tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generic_plan_event_new_sets_event_type_to_plan() {
        let event = GenericPlanEvent::new("id-001".to_string(), json!(null));
        assert_eq!(event.event_type, "plan");
    }

    #[test]
    fn test_generic_plan_event_new_sets_id() {
        let event = GenericPlanEvent::new("my-custom-id".to_string(), json!(null));
        assert_eq!(event.id, "my-custom-id");
    }

    #[test]
    fn test_generic_plan_event_new_optional_fields_are_none() {
        let event = GenericPlanEvent::new("id-002".to_string(), json!(null));
        assert!(event.name.is_none());
        assert!(event.version.is_none());
        assert!(event.action.is_none());
        assert!(event.metadata.is_none());
    }

    #[test]
    fn test_generic_plan_event_new_sets_timestamp() {
        let event = GenericPlanEvent::new("id-003".to_string(), json!(null));
        assert!(event.timestamp.is_some());
    }

    #[test]
    fn test_generic_plan_event_is_plan_event_returns_true_for_plan_type() {
        let event = GenericPlanEvent::new("id-004".to_string(), json!(null));
        assert!(event.is_plan_event());
    }

    #[test]
    fn test_generic_plan_event_is_plan_event_returns_false_for_result_type() {
        let mut event = GenericPlanEvent::new("id-005".to_string(), json!(null));
        event.event_type = "result".to_string();
        assert!(!event.is_plan_event());
    }

    #[test]
    fn test_generic_plan_event_is_plan_event_returns_false_for_unknown_type() {
        let mut event = GenericPlanEvent::new("id-006".to_string(), json!(null));
        event.event_type = "unknown".to_string();
        assert!(!event.is_plan_event());
    }

    #[test]
    fn test_generic_plan_event_is_plan_event_returns_false_for_empty_type() {
        let mut event = GenericPlanEvent::new("id-007".to_string(), json!(null));
        event.event_type = String::new();
        assert!(!event.is_plan_event());
    }

    #[test]
    fn test_generic_plan_event_roundtrip_all_fields() {
        // Construct an event with every optional field populated.
        let original = GenericPlanEvent {
            id: "01JFULL0000000000000000001".to_string(),
            event_type: "plan".to_string(),
            name: Some("deploy-service".to_string()),
            version: Some("2.1.0".to_string()),
            action: Some("deploy".to_string()),
            plan: json!({
                "name": "Deploy Service",
                "steps": [{"name": "apply", "action": "kubectl apply"}]
            }),
            timestamp: Some(
                DateTime::parse_from_rfc3339("2025-01-15T12:00:00Z")
                    .unwrap()
                    .with_timezone(&Utc),
            ),
            metadata: Some(json!({"env": "production", "region": "us-east-1"})),
        };

        let json_str = serde_json::to_string(&original).unwrap();
        let restored: GenericPlanEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored, original);
        assert_eq!(restored.name.as_deref(), Some("deploy-service"));
        assert_eq!(restored.version.as_deref(), Some("2.1.0"));
        assert_eq!(restored.action.as_deref(), Some("deploy"));
        assert!(restored.metadata.is_some());
    }

    #[test]
    fn test_generic_plan_event_roundtrip_only_action() {
        // Only `action` is provided among the optional matching fields.
        let original = GenericPlanEvent {
            id: "01JACTION000000000000000001".to_string(),
            event_type: "plan".to_string(),
            name: None,
            version: None,
            action: Some("quickstart".to_string()),
            plan: json!("name: Quick\nsteps:\n  - name: s1\n    action: echo hi\n"),
            timestamp: None,
            metadata: None,
        };

        let json_str = serde_json::to_string(&original).unwrap();

        // Verify optional None fields are omitted from JSON (skip_serializing_if).
        let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(
            json_value.get("name").is_none(),
            "name should be absent when None"
        );
        assert!(
            json_value.get("version").is_none(),
            "version should be absent when None"
        );
        assert!(
            json_value.get("timestamp").is_none(),
            "timestamp should be absent when None"
        );
        assert!(
            json_value.get("metadata").is_none(),
            "metadata should be absent when None"
        );
        assert_eq!(json_value["action"], "quickstart");

        let restored: GenericPlanEvent = serde_json::from_str(&json_str).unwrap();
        assert_eq!(restored, original);
    }

    #[test]
    fn test_generic_plan_event_roundtrip_only_name_and_version() {
        // Only `name` and `version` are provided — no `action`.
        let original = GenericPlanEvent {
            id: "01JNAMEVER00000000000000001".to_string(),
            event_type: "plan".to_string(),
            name: Some("my-service".to_string()),
            version: Some("1.0.0".to_string()),
            action: None,
            plan: json!({"name": "Service Plan", "steps": []}),
            timestamp: None,
            metadata: None,
        };

        let json_str = serde_json::to_string(&original).unwrap();
        let restored: GenericPlanEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored.name.as_deref(), Some("my-service"));
        assert_eq!(restored.version.as_deref(), Some("1.0.0"));
        assert!(restored.action.is_none());
        assert_eq!(restored, original);
    }

    #[test]
    fn test_generic_plan_event_roundtrip_minimal() {
        // Only the required fields: `id`, `event_type`, and `plan`.
        let original = GenericPlanEvent {
            id: "01JMINIMAL0000000000000001".to_string(),
            event_type: "plan".to_string(),
            name: None,
            version: None,
            action: None,
            plan: json!("name: Minimal\nsteps:\n  - name: s1\n    action: echo minimal\n"),
            timestamp: None,
            metadata: None,
        };

        let json_str = serde_json::to_string(&original).unwrap();
        let restored: GenericPlanEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored.id, "01JMINIMAL0000000000000001");
        assert_eq!(restored.event_type, "plan");
        assert!(restored.name.is_none());
        assert!(restored.version.is_none());
        assert!(restored.action.is_none());
        assert!(restored.timestamp.is_none());
        assert!(restored.metadata.is_none());
        assert_eq!(restored, original);
    }

    #[test]
    fn test_generic_plan_event_non_plan_event_type_roundtrips_correctly() {
        // A result event round-trips correctly but is_plan_event() must return false.
        let original = GenericPlanEvent {
            id: "01JRESULT000000000000000001".to_string(),
            event_type: "result".to_string(),
            name: None,
            version: None,
            action: None,
            plan: json!(null),
            timestamp: None,
            metadata: None,
        };

        let json_str = serde_json::to_string(&original).unwrap();
        let restored: GenericPlanEvent = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored.event_type, "result");
        assert!(
            !restored.is_plan_event(),
            "result event_type must be rejected by matcher"
        );
        assert_eq!(restored, original);
    }

    #[test]
    fn test_generic_plan_event_plan_field_accepts_string_value() {
        let event = GenericPlanEvent::new(
            "id-str".to_string(),
            json!("name: Plain\nsteps:\n  - name: s1\n    action: echo 1\n"),
        );
        assert!(event.plan.is_string());
    }

    #[test]
    fn test_generic_plan_event_plan_field_accepts_object_value() {
        let event = GenericPlanEvent::new(
            "id-obj".to_string(),
            json!({"name": "Object Plan", "steps": []}),
        );
        assert!(event.plan.is_object());
    }

    #[test]
    fn test_generic_plan_event_plan_field_accepts_array_value() {
        let event = GenericPlanEvent::new(
            "id-arr".to_string(),
            json!([{"task": "step1"}, {"task": "step2"}]),
        );
        assert!(event.plan.is_array());
    }
}
