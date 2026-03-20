//! Generic watcher message types
//!
//! This module defines the JSON message schemas exchanged between producers and
//! the generic Kafka watcher backend.
//!
//! Two message types are defined:
//!
//! - [`GenericPlanEvent`]: The trigger message a producer publishes to the input
//!   topic. The generic watcher consumes this, evaluates the matching criteria,
//!   and executes the embedded plan when the criteria are satisfied.
//!
//! - [`GenericPlanResult`]: The result message the watcher publishes to the output
//!   topic after plan execution. Its `event_type` is hardcoded to `"result"` so
//!   that if the output topic and input topic are the same, the watcher will read
//!   the result message, immediately detect `event_type != "plan"`, and discard it
//!   — preventing infinite re-trigger loops.
//!
//! # Loop-break guarantee
//!
//! The primary guard against same-topic loops is the `event_type` field:
//!
//! - Producers MUST set `event_type = "plan"` on every trigger event.
//! - The generic matcher rejects any event where `event_type != "plan"` before
//!   evaluating any other criteria.
//! - [`GenericPlanResult`] always carries `event_type = "result"` (enforced by
//!   [`GenericPlanResult::new`]) so it is silently discarded on re-consumption.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

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
/// use xzatoma::watcher::generic::message::GenericPlanEvent;
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
    /// in the corresponding [`GenericPlanResult`] for traceability.
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
    /// use xzatoma::watcher::generic::message::GenericPlanEvent;
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
    /// use xzatoma::watcher::generic::message::GenericPlanEvent;
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

/// The result message schema published by the generic watcher after plan execution.
///
/// After a plan completes (successfully or not), the watcher constructs a
/// `GenericPlanResult` and publishes it to the configured output topic. The
/// `event_type` field is always `"result"` (enforced by [`GenericPlanResult::new`]),
/// which guarantees that the watcher will discard the message if the output topic
/// and input topic are the same — closing the re-trigger loop.
///
/// # Examples
///
/// ```
/// use xzatoma::watcher::generic::message::GenericPlanResult;
///
/// let result = GenericPlanResult::new(
///     "01JORIGINAL00000000000000".to_string(),
///     true,
///     "All steps completed successfully".to_string(),
/// );
/// assert_eq!(result.event_type, "result");
/// assert!(result.success);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GenericPlanResult {
    /// Unique identifier for this result event (ULID format).
    pub id: String,

    /// Event type discriminator. Always `"result"` — enforced by [`GenericPlanResult::new`].
    ///
    /// Keeping this field hardcoded to `"result"` prevents the watcher from
    /// re-processing its own output when the output topic equals the input topic.
    pub event_type: String,

    /// The `id` of the [`GenericPlanEvent`] that triggered plan execution.
    ///
    /// Used for correlation: consumers can match result events back to the original
    /// trigger event using this field.
    pub trigger_event_id: String,

    /// Whether plan execution completed without errors.
    pub success: bool,

    /// Human-readable execution summary (step counts, error messages, etc.).
    pub summary: String,

    /// RFC-3339 timestamp of when this result was produced.
    pub timestamp: DateTime<Utc>,

    /// Optional structured execution output (step results, stdout, etc.).
    ///
    /// The schema of this value is not enforced; the watcher may populate it with
    /// any JSON-serializable data that helps downstream consumers interpret the result.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_output: Option<serde_json::Value>,
}

impl GenericPlanResult {
    /// Construct a new result event for a completed plan execution.
    ///
    /// Generates a fresh ULID for `id` and sets `event_type` to `"result"`.
    /// The timestamp is set to the current UTC time.
    ///
    /// # Arguments
    ///
    /// * `trigger_event_id` - The `id` from the [`GenericPlanEvent`] that triggered
    ///   execution
    /// * `success` - Whether plan execution completed without errors
    /// * `summary` - Human-readable description of the execution outcome
    ///
    /// # Returns
    ///
    /// A new `GenericPlanResult` with a generated ULID, `event_type = "result"`,
    /// and `plan_output = None`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::watcher::generic::message::GenericPlanResult;
    ///
    /// let result = GenericPlanResult::new(
    ///     "01JTRIGGER0000000000000000".to_string(),
    ///     true,
    ///     "3 steps completed".to_string(),
    /// );
    /// assert_eq!(result.event_type, "result");
    /// assert_eq!(result.trigger_event_id, "01JTRIGGER0000000000000000");
    /// assert!(result.success);
    /// assert!(result.plan_output.is_none());
    /// ```
    pub fn new(trigger_event_id: String, success: bool, summary: String) -> Self {
        Self {
            id: Ulid::new().to_string(),
            event_type: "result".to_string(),
            trigger_event_id,
            success,
            summary,
            timestamp: Utc::now(),
            plan_output: None,
        }
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

    // -------------------------------------------------------------------------
    // GenericPlanResult tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_generic_plan_result_new_sets_event_type_to_result() {
        let result = GenericPlanResult::new("trigger-id".to_string(), true, "success".to_string());
        assert_eq!(result.event_type, "result");
    }

    #[test]
    fn test_generic_plan_result_new_sets_trigger_event_id() {
        let trigger = "01JTRIGGER0000000000000000".to_string();
        let result = GenericPlanResult::new(trigger.clone(), true, "ok".to_string());
        assert_eq!(result.trigger_event_id, trigger);
    }

    #[test]
    fn test_generic_plan_result_new_generates_unique_ids() {
        let r1 = GenericPlanResult::new("t1".to_string(), true, "ok".to_string());
        let r2 = GenericPlanResult::new("t2".to_string(), true, "ok".to_string());
        assert_ne!(
            r1.id, r2.id,
            "each result must have a unique generated ULID"
        );
    }

    #[test]
    fn test_generic_plan_result_new_plan_output_defaults_to_none() {
        let result = GenericPlanResult::new("t".to_string(), false, "failed".to_string());
        assert!(result.plan_output.is_none());
    }

    #[test]
    fn test_generic_plan_result_new_sets_success_flag() {
        let ok = GenericPlanResult::new("t".to_string(), true, "done".to_string());
        assert!(ok.success);

        let fail = GenericPlanResult::new("t".to_string(), false, "error".to_string());
        assert!(!fail.success);
    }

    #[test]
    fn test_generic_plan_result_roundtrip_all_fields() {
        let mut original = GenericPlanResult::new(
            "01JTRIGGER0000000000000000".to_string(),
            true,
            "3 steps completed".to_string(),
        );
        original.plan_output = Some(json!({
            "steps": [
                {"name": "s1", "exit_code": 0},
                {"name": "s2", "exit_code": 0}
            ]
        }));

        let json_str = serde_json::to_string(&original).unwrap();
        let restored: GenericPlanResult = serde_json::from_str(&json_str).unwrap();

        assert_eq!(restored.event_type, "result");
        assert_eq!(restored.trigger_event_id, "01JTRIGGER0000000000000000");
        assert!(restored.success);
        assert_eq!(restored.summary, "3 steps completed");
        assert!(restored.plan_output.is_some());
        assert_eq!(
            restored.plan_output.unwrap()["steps"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn test_generic_plan_result_event_type_serializes_as_result() {
        // Confirm that even when serialized to JSON, event_type is the literal
        // string "result" — this is the loop-break guarantee.
        let result = GenericPlanResult::new("t".to_string(), true, "ok".to_string());
        let json_str = serde_json::to_string(&result).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(json_value["event_type"], "result");
    }

    #[test]
    fn test_generic_plan_result_plan_output_omitted_when_none() {
        let result = GenericPlanResult::new("t".to_string(), true, "ok".to_string());
        let json_str = serde_json::to_string(&result).unwrap();
        let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert!(
            json_value.get("plan_output").is_none(),
            "plan_output should be omitted from JSON when None"
        );
    }

    #[test]
    fn test_generic_plan_result_timestamp_is_recent() {
        let before = Utc::now();
        let result = GenericPlanResult::new("t".to_string(), true, "ok".to_string());
        let after = Utc::now();
        assert!(result.timestamp >= before);
        assert!(result.timestamp <= after);
    }
}
