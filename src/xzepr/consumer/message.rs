//! CloudEvents message types for XZepr consumer.
//!
//! This module provides Rust structs to deserialize CloudEvents 1.0.1
//! messages received from XZepr's Kafka topics.
//!
//! # Example
//!
//! ```rust
//! use xzatoma::xzepr::consumer::message::CloudEventMessage;
//!
//! let json = r#"{
//!   "success": true,
//!   "id": "01JXXXXXXXXXXXXXXXXXXXXXXX",
//!   "specversion": "1.0.1",
//!   "type": "deployment.success",
//!   "source": "xzepr.event.receiver.01JXXXXXXXXXXXXXXXXXXXXXXX",
//!   "api_version": "v1",
//!   "name": "deployment.success",
//!   "version": "1.0.0",
//!   "release": "1.0.0-rc.1",
//!   "platform_id": "kubernetes",
//!   "package": "myapp",
//!   "data": {
//!     "events": [],
//!     "event_receivers": [],
//!     "event_receiver_groups": []
//!   }
//! }"#;
//!
//! let event: CloudEventMessage = serde_json::from_str(json).unwrap();
//! assert_eq!(event.id, "01JXXXXXXXXXXXXXXXXXXXXXXX");
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// CloudEvents 1.0.1 message from XZepr.
///
/// This struct represents the full CloudEvents message format used by XZepr
/// for publishing events to Kafka topics. The message includes standard
/// CloudEvents attributes plus XZepr-specific extensions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudEventMessage {
    /// Indicates if the event represents success.
    pub success: bool,

    /// Unique event identifier (ULID).
    pub id: String,

    /// CloudEvents specification version (typically "1.0.1").
    pub specversion: String,

    /// Event type/name (e.g., "deployment.success").
    #[serde(rename = "type")]
    pub event_type: String,

    /// Event source URI.
    pub source: String,

    /// XZepr API version.
    pub api_version: String,

    /// Event name.
    pub name: String,

    /// Event version.
    pub version: String,

    /// Release identifier.
    pub release: String,

    /// Platform identifier (e.g., "kubernetes").
    pub platform_id: String,

    /// Package name.
    pub package: String,

    /// Event payload data containing entities.
    pub data: CloudEventData,
}

/// Data payload containing entities from XZepr.
///
/// This struct contains the actual payload data of a CloudEvents message,
/// including events, event receivers, and event receiver groups.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CloudEventData {
    /// Events in this message.
    #[serde(default)]
    pub events: Vec<EventEntity>,

    /// Event receivers in this message.
    #[serde(default)]
    pub event_receivers: Vec<EventReceiverEntity>,

    /// Event receiver groups in this message.
    #[serde(default)]
    pub event_receiver_groups: Vec<EventReceiverGroupEntity>,
}

/// Event entity from XZepr.
///
/// Represents a single event that was triggered in XZepr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEntity {
    /// Unique event identifier.
    pub id: String,

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

    /// Event payload data.
    pub payload: JsonValue,

    /// Whether the event represents success.
    pub success: bool,

    /// Associated event receiver ID.
    pub event_receiver_id: String,

    /// Timestamp when the event was created.
    pub created_at: DateTime<Utc>,
}

/// Event receiver entity from XZepr.
///
/// Represents an event receiver that can receive events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverEntity {
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

    /// Unique fingerprint for the receiver.
    pub fingerprint: String,

    /// Timestamp when the receiver was created.
    pub created_at: DateTime<Utc>,
}

/// Event receiver group entity from XZepr.
///
/// Represents a group of event receivers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventReceiverGroupEntity {
    /// Unique group identifier.
    pub id: String,

    /// Group name.
    pub name: String,

    /// Group type.
    #[serde(rename = "type")]
    pub group_type: String,

    /// Group version.
    pub version: String,

    /// Group description.
    pub description: String,

    /// Whether the group is enabled.
    pub enabled: bool,

    /// List of event receiver IDs in this group.
    pub event_receiver_ids: Vec<String>,

    /// Timestamp when the group was created.
    pub created_at: DateTime<Utc>,

    /// Timestamp when the group was last updated.
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cloud_event_message_deserialization() {
        let json = r#"{
            "success": true,
            "id": "01JTEST1234567890123456",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "xzepr.event.receiver.01JTEST1234567890123456",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0-rc.1",
            "platform_id": "kubernetes",
            "package": "myapp",
            "data": {
                "events": [],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }"#;

        let event: CloudEventMessage = serde_json::from_str(json).unwrap();

        assert!(event.success);
        assert_eq!(event.id, "01JTEST1234567890123456");
        assert_eq!(event.specversion, "1.0.1");
        assert_eq!(event.event_type, "deployment.success");
        assert_eq!(event.source, "xzepr.event.receiver.01JTEST1234567890123456");
        assert_eq!(event.api_version, "v1");
        assert_eq!(event.name, "deployment.success");
        assert_eq!(event.version, "1.0.0");
        assert_eq!(event.release, "1.0.0-rc.1");
        assert_eq!(event.platform_id, "kubernetes");
        assert_eq!(event.package, "myapp");
        assert!(event.data.events.is_empty());
        assert!(event.data.event_receivers.is_empty());
        assert!(event.data.event_receiver_groups.is_empty());
    }

    #[test]
    fn test_cloud_event_message_with_event_entity() {
        let json = r#"{
            "success": true,
            "id": "01JTEST1234567890123456",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "xzepr.event.receiver.01JTEST1234567890123456",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0-rc.1",
            "platform_id": "kubernetes",
            "package": "myapp",
            "data": {
                "events": [{
                    "id": "01JEVENT123456789012345",
                    "name": "test.event",
                    "version": "1.0.0",
                    "release": "1.0.0",
                    "platform_id": "kubernetes",
                    "package": "testpkg",
                    "description": "Test event description",
                    "payload": {"key": "value"},
                    "success": true,
                    "event_receiver_id": "01JRECEIVER1234567890",
                    "created_at": "2025-01-15T10:30:00Z"
                }],
                "event_receivers": [],
                "event_receiver_groups": []
            }
        }"#;

        let event: CloudEventMessage = serde_json::from_str(json).unwrap();

        assert_eq!(event.data.events.len(), 1);
        let inner_event = &event.data.events[0];
        assert_eq!(inner_event.id, "01JEVENT123456789012345");
        assert_eq!(inner_event.name, "test.event");
        assert_eq!(inner_event.payload["key"], "value");
    }

    #[test]
    fn test_cloud_event_message_with_receiver_entity() {
        let json = r#"{
            "success": true,
            "id": "01JTEST1234567890123456",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "xzepr.event.receiver.01JTEST1234567890123456",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0-rc.1",
            "platform_id": "kubernetes",
            "package": "myapp",
            "data": {
                "events": [],
                "event_receivers": [{
                    "id": "01JRECEIVER1234567890",
                    "name": "test-receiver",
                    "type": "worker",
                    "version": "1.0.0",
                    "description": "Test receiver",
                    "schema": {"type": "object"},
                    "fingerprint": "abc123",
                    "created_at": "2025-01-15T10:30:00Z"
                }],
                "event_receiver_groups": []
            }
        }"#;

        let event: CloudEventMessage = serde_json::from_str(json).unwrap();

        assert_eq!(event.data.event_receivers.len(), 1);
        let receiver = &event.data.event_receivers[0];
        assert_eq!(receiver.id, "01JRECEIVER1234567890");
        assert_eq!(receiver.name, "test-receiver");
        assert_eq!(receiver.receiver_type, "worker");
    }

    #[test]
    fn test_cloud_event_message_with_group_entity() {
        let json = r#"{
            "success": true,
            "id": "01JTEST1234567890123456",
            "specversion": "1.0.1",
            "type": "deployment.success",
            "source": "xzepr.event.receiver.01JTEST1234567890123456",
            "api_version": "v1",
            "name": "deployment.success",
            "version": "1.0.0",
            "release": "1.0.0-rc.1",
            "platform_id": "kubernetes",
            "package": "myapp",
            "data": {
                "events": [],
                "event_receivers": [],
                "event_receiver_groups": [{
                    "id": "01JGROUP12345678901234",
                    "name": "test-group",
                    "type": "deployment",
                    "version": "1.0.0",
                    "description": "Test group",
                    "enabled": true,
                    "event_receiver_ids": ["01JRECEIVER1", "01JRECEIVER2"],
                    "created_at": "2025-01-15T10:30:00Z",
                    "updated_at": "2025-01-15T11:00:00Z"
                }]
            }
        }"#;

        let event: CloudEventMessage = serde_json::from_str(json).unwrap();

        assert_eq!(event.data.event_receiver_groups.len(), 1);
        let group = &event.data.event_receiver_groups[0];
        assert_eq!(group.id, "01JGROUP12345678901234");
        assert_eq!(group.name, "test-group");
        assert_eq!(group.group_type, "deployment");
        assert!(group.enabled);
        assert_eq!(group.event_receiver_ids.len(), 2);
    }

    #[test]
    fn test_cloud_event_message_serialization_roundtrip() {
        let original = CloudEventMessage {
            success: true,
            id: "test-id".to_string(),
            specversion: "1.0.1".to_string(),
            event_type: "test.event".to_string(),
            source: "test-source".to_string(),
            api_version: "v1".to_string(),
            name: "test.event".to_string(),
            version: "1.0.0".to_string(),
            release: "1.0.0".to_string(),
            platform_id: "test".to_string(),
            package: "testpkg".to_string(),
            data: CloudEventData::default(),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: CloudEventMessage = serde_json::from_str(&json).unwrap();

        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.event_type, deserialized.event_type);
        assert_eq!(original.success, deserialized.success);
    }

    #[test]
    fn test_cloud_event_data_default() {
        let data = CloudEventData::default();
        assert!(data.events.is_empty());
        assert!(data.event_receivers.is_empty());
        assert!(data.event_receiver_groups.is_empty());
    }
}
