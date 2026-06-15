# XZepr Null Event Arrays Implementation

XZepr events may include entity collection fields as explicit JSON `null`
values. The watcher treats those values the same as missing collections because
the rest of the event processing pipeline expects empty vectors for absent
events, receivers, and receiver groups.

The consumer message model keeps `CloudEventData` collection fields as `Vec<T>`.
A shared serde deserializer maps `null` to an empty vector while the existing
`serde(default)` handling continues to map omitted fields to empty vectors. This
preserves the existing caller API, including `.first()`, `.len()`, and direct
iteration, without spreading `Option<Vec<T>>` checks throughout the watcher.

Regression tests cover both explicit `null` arrays and omitted arrays so Kafka
messages from EPR can deserialize without changing plan extraction behavior.
