//! Outbound plan result type for the generic Kafka watcher.
//!
//! This module defines [`GenericPlanResult`], the result message the watcher
//! publishes to the output topic after plan execution completes. Its
//! `event_type` is always hardcoded to `"result"` so that if the output topic
//! and input topic are the same, the watcher reads the result message,
//! immediately detects `event_type != "plan"`, and discards it — preventing
//! infinite re-trigger loops.
//!
//! # Loop-break guarantee
//!
//! [`GenericPlanResult::new`] unconditionally sets `event_type = "result"`.
//! The generic matcher calls
//! [`GenericPlanEvent::is_plan_event`](crate::watcher::generic::event::GenericPlanEvent::is_plan_event)
//! as the first check, so result messages are silently discarded before any
//! matching criteria are evaluated.
//!
//! # Examples
//!
//! ```
//! use xzatoma::watcher::generic::result_event::GenericPlanResult;
//!
//! let result = GenericPlanResult::new(
//!     "01JORIGINAL00000000000000".to_string(),
//!     true,
//!     "All steps completed successfully".to_string(),
//! );
//! assert_eq!(result.event_type, "result");
//! assert!(result.success);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

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
/// use xzatoma::watcher::generic::result_event::GenericPlanResult;
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

    /// The `id` of the
    /// [`GenericPlanEvent`](crate::watcher::generic::event::GenericPlanEvent)
    /// that triggered plan execution.
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
    /// * `trigger_event_id` - The `id` from the
    ///   [`GenericPlanEvent`](crate::watcher::generic::event::GenericPlanEvent)
    ///   that triggered execution
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
    /// use xzatoma::watcher::generic::result_event::GenericPlanResult;
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
