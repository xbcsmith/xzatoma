//! Generic Kafka watcher backend.
//!
//! This module contains the generic Kafka/Redpanda watcher implementation for
//! consuming generic plan events, matching them against configured criteria,
//! executing embedded plans, and publishing structured results.
//!
//! # Components
//!
//! - [`consumer`]: Consumer abstraction ([`GenericConsumerTrait`]) and
//!   implementations ([`RealGenericConsumer`], [`FakeGenericConsumer`]) plus
//!   the raw message bridge ([`RawKafkaMessage`])
//! - [`event`]: Inbound plan event type ([`GenericPlanEvent`])
//! - [`result_event`]: Outbound plan result type ([`GenericPlanResult`])
//! - [`event_handler`]: Five-step event pipeline ([`GenericEventHandler`]) and
//!   task type ([`GenericTask`])
//! - [`matcher`]: Semver-aware event matching for generic plan events
//! - [`result_producer`]: Producer abstraction ([`ResultProducerTrait`]) and
//!   implementations ([`GenericResultProducer`], [`FakeResultProducer`],
//!   [`BufferedResultProducer`]) for generic watcher results
//! - [`watcher`]: Core generic watcher service and dry-run processing flow
//!
//! # Wire Format
//!
//! The generic watcher requires every inbound Kafka message to be a standard
//! **CloudEvents 1.0** JSON object ([`GenericPlanCloudEvent`]).  Required
//! attributes: `id`, `specversion` (`"1.0"`), `type`, `source`, `data`.
//! The `data` field contains the [`Plan`] payload (task-based or step-based).
//!
//! This is intentionally distinct from the XZepr watcher, which uses an
//! XZepr-specific CloudEvents schema with extensions such as `success`,
//! `api_version`, `platform_id`, `release`, and `package`.
//!
//! # Loop prevention
//!
//! The generic watcher prevents same-topic re-trigger loops at the CloudEvents
//! parsing boundary:
//!
//! - [`GenericPlanEvent::new`] parses the payload as a [`GenericPlanCloudEvent`]
//!   and returns `Err` if the envelope is invalid or `data` cannot be validated
//!   as a [`Plan`].
//! - [`GenericPlanResult`] messages published to the output topic are plain JSON
//!   (not CloudEvents envelopes), so they fail at the first parse step when
//!   consumed back on the same topic.
//! - The [`GenericEventHandler`] propagates the parse error and the watcher
//!   classifies the message as `InvalidPayload` — no execution, no new result.
//!
//! # Matching model
//!
//! [`GenericMatcher`] is intentionally separate from the XZepr-specific
//! [`crate::watcher::xzepr::filter::EventFilter`] implementation. The generic
//! matcher operates only on [`GenericPlanEvent`] and
//! [`crate::config::GenericMatchConfig`] and supports the following modes:
//!
//! - `action` only
//! - `name` + `version`
//! - `name` + `action`
//! - `name` + `version` + `action`
//! - accept-all for all valid plan events when no match fields are configured
//!
//! # Accept-all semantics
//!
//! A [`GenericMatcher`] constructed from a [`crate::config::GenericMatchConfig`]
//! where all three fields (`action`, `name`, `version`) are `None` operates in
//! **accept-all** mode. In this mode every structurally valid plan event that
//! reaches the matcher is passed to the executor without any predicate check.
//!
//! Operators must be aware that an empty match config causes the watcher to
//! process **every** plan event consumed from the input topic. This is useful
//! for single-purpose watchers that are dedicated to one topic, but it can
//! lead to unintended execution if multiple plan types are published to the
//! same topic. Use [`GenericMatcher::has_predicates`] to confirm at startup
//! that at least one predicate is configured when selective matching is
//! required.
//!
//! # Version matching
//!
//! The `version` predicate uses [`crate::watcher::version_matches`] rather
//! than a regex. This allows operators to write standard semver constraints
//! such as `">=2.0.0"` or `"^1"`. When the configured constraint cannot be
//! parsed as a [`semver::VersionReq`], the function falls back to
//! case-insensitive exact string equality so that plain version tags continue
//! to work. The `action` and `name` predicates continue to use regex matching.
//!
//! # Examples
//!
//! ```
//! use xzatoma::config::GenericMatchConfig;
//! use xzatoma::watcher::generic::{GenericMatcher, GenericPlanEvent};
//!
//! let matcher = GenericMatcher::new(GenericMatchConfig {
//!     action: Some("deploy.*".to_string()),
//!     name: None,
//!     version: None,
//! })
//! .unwrap();
//!
//! // Messages must be CloudEvents 1.0 envelopes with the plan in `data`.
//! let cloud_event = r#"{
//!     "id": "01JTEST000000000000000001",
//!     "specversion": "1.0",
//!     "type": "xzatoma.plan.execute",
//!     "source": "my-producer",
//!     "data": {
//!         "name": "deploy",
//!         "action": "deploy-prod",
//!         "steps": [{ "name": "s1", "action": "kubectl apply" }]
//!     }
//! }"#;
//!
//! let event = GenericPlanEvent::new(cloud_event, "input.topic".to_string(), None).unwrap();
//! assert!(matcher.should_process(&event));
//! ```
//!
//! [`Plan`]: crate::tools::plan::Plan
//! [`PlanParser::parse_string`]: crate::tools::plan::PlanParser::parse_string

pub mod consumer;
pub mod event;
pub mod event_handler;
pub mod matcher;
pub mod message;
pub mod result_event;
pub mod result_producer;
pub mod watcher;

pub use crate::config::GenericMatchConfig;
pub use consumer::{
    FakeGenericConsumer, GenericConsumerTrait, RawKafkaMessage, RealGenericConsumer,
};
pub use event::GenericPlanEvent;
pub use event_handler::{GenericEventHandler, GenericTask};
pub use matcher::GenericMatcher;
pub use message::GenericPlanCloudEvent;
pub use result_event::GenericPlanResult;
pub use result_producer::{
    BufferedResultProducer, FakeResultProducer, GenericResultProducer, ResultProducerTrait,
    DEFAULT_DLQ_MAX_BUFFERED,
};
pub use watcher::GenericWatcher;
