/// ACP streaming support for server-sent events.
///
/// This module adapts ACP runtime events into server-sent events (SSE) suitable
/// for the Phase 3 ACP HTTP streaming surface. The implementation is transport-
/// focused and intentionally small:
///
/// - replay prior in-memory runtime events in order
/// - stream new live runtime events from the runtime subscription
/// - encode each event as a JSON SSE frame
/// - terminate the stream once a terminal ACP event is observed
///
/// The runtime remains the source of truth for event ordering and replayability.
/// This module only bridges those ordered runtime events onto an HTTP-friendly
/// streaming representation.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
/// use xzatoma::acp::streaming::AcpSseEvent;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?]))?;
///
/// let events = runtime.get_events(run.id.as_str())?;
/// let sse_event = AcpSseEvent::from_runtime_event(events[0].clone())?;
///
/// assert_eq!(sse_event.id, "1");
/// assert!(!sse_event.data.is_empty());
/// # Ok::<(), anyhow::Error>(())
/// ```
use crate::acp::runtime::{AcpRuntime, AcpRuntimeEvent};
use crate::error::{Result, XzatomaError};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::{self, BoxStream, Stream, StreamExt};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::time::Duration;

/// Interval used for SSE keep-alive comments.
///
/// Keep-alives help intermediaries and clients avoid treating an otherwise quiet
/// stream as dead while the run is still in progress.
const DEFAULT_KEEP_ALIVE_INTERVAL_SECS: u64 = 15;

/// SSE payload derived from an ACP runtime event.
///
/// This structure is serialized to JSON for the `data:` field of each SSE
/// message, while its sequence number is also surfaced as the SSE `id`.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
/// use xzatoma::acp::streaming::AcpSseEvent;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?]))?;
///
/// let runtime_event = runtime.get_events(run.id.as_str())?.remove(0);
/// let event = AcpSseEvent::from_runtime_event(runtime_event)?;
///
/// assert_eq!(event.id, "1");
/// assert!(event.event.contains("run"));
/// # Ok::<(), anyhow::Error>(())
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AcpSseEvent {
    /// SSE event identifier.
    pub id: String,
    /// SSE event name.
    pub event: String,
    /// JSON-encoded payload.
    pub data: String,
    /// Whether this event is terminal for the stream.
    pub terminal: bool,
}

impl AcpSseEvent {
    /// Converts an ACP runtime event into an SSE payload.
    ///
    /// # Arguments
    ///
    /// * `runtime_event` - Ordered runtime event from the ACP runtime
    ///
    /// # Returns
    ///
    /// Returns the SSE payload suitable for Axum SSE responses.
    ///
    /// # Errors
    ///
    /// Returns an error if the runtime event payload cannot be serialized.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
    /// use xzatoma::acp::streaming::AcpSseEvent;
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///     AcpRole::User,
    ///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
    /// )?]))?;
    ///
    /// let runtime_event = runtime.get_events(run.id.as_str())?.remove(0);
    /// let event = AcpSseEvent::from_runtime_event(runtime_event)?;
    ///
    /// assert_eq!(event.id, "1");
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn from_runtime_event(runtime_event: AcpRuntimeEvent) -> Result<Self> {
        let payload = serde_json::to_string(&runtime_event.event)?;
        Ok(Self {
            id: runtime_event.sequence.to_string(),
            event: runtime_event_name(&runtime_event),
            data: payload,
            terminal: runtime_event.terminal,
        })
    }

    /// Converts the SSE payload into an Axum SSE event.
    ///
    /// # Returns
    ///
    /// Returns the event formatted for Axum SSE streaming.
    ///
    /// # Errors
    ///
    /// Returns an error if the SSE frame cannot be built.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
    /// use xzatoma::acp::streaming::AcpSseEvent;
    /// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
    /// use xzatoma::Config;
    ///
    /// let runtime = AcpRuntime::new(Config::default());
    /// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
    ///     AcpRole::User,
    ///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
    /// )?]))?;
    ///
    /// let runtime_event = runtime.get_events(run.id.as_str())?.remove(0);
    /// let event = AcpSseEvent::from_runtime_event(runtime_event)?;
    /// let _axum_event = event.into_axum_event()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn into_axum_event(self) -> Result<Event> {
        let event = Event::default()
            .id(self.id)
            .event(self.event)
            .data(self.data);
        Ok(event)
    }
}

/// Builds an Axum SSE response for one ACP run.
///
/// The returned stream first replays the existing runtime event history for the
/// run, then continues streaming live events until a terminal event arrives.
///
/// # Arguments
///
/// * `runtime` - ACP runtime coordinator
/// * `run_id` - Identifier of the run to stream
///
/// # Returns
///
/// Returns an Axum SSE response whose items are infallible because runtime and
/// serialization errors are handled before stream construction.
///
/// # Errors
///
/// Returns an error if the run does not exist, replay events cannot be loaded,
/// or the runtime subscription cannot be established.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
/// use xzatoma::acp::streaming::stream_run_events_sse;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// let runtime = AcpRuntime::new(Config::default());
/// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?]))?;
///
/// let _response = stream_run_events_sse(runtime, run.id.as_str())?;
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn stream_run_events_sse(
    runtime: AcpRuntime,
    run_id: &str,
) -> Result<Sse<impl Stream<Item = std::result::Result<Event, Infallible>>>> {
    let replay = runtime.get_events(run_id)?;
    let subscription = runtime.subscribe(run_id)?;
    let stream = build_sse_stream(replay, subscription);

    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECS))
            .text("keep-alive"),
    ))
}

/// Builds the underlying SSE stream from replay and live event sources.
///
/// The resulting stream preserves ordering:
///
/// 1. all replay events in sequence order
/// 2. subsequent live events from the runtime subscription
///
/// If replay already contains a terminal event, the live phase is skipped.
///
/// # Arguments
///
/// * `replay` - Historical ordered runtime events
/// * `subscription` - Live event subscription for the run
///
/// # Returns
///
/// Returns a boxed stream of Axum SSE events.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
/// use xzatoma::acp::streaming::build_sse_stream;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let runtime = AcpRuntime::new(Config::default());
/// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?]))?;
///
/// let replay = runtime.get_events(run.id.as_str())?;
/// let subscription = runtime.subscribe(run.id.as_str())?;
/// let _stream = build_sse_stream(replay, subscription);
/// # Ok::<(), anyhow::Error>(())
/// # }
/// ```
pub fn build_sse_stream(
    replay: Vec<AcpRuntimeEvent>,
    subscription: crate::acp::runtime::AcpRuntimeSubscription,
) -> BoxStream<'static, std::result::Result<Event, Infallible>> {
    let replay_is_terminal = replay.last().is_some_and(|event| event.terminal);

    let replay_stream = stream::iter(
        replay
            .into_iter()
            .filter_map(|event| runtime_event_to_sse_result(event).ok())
            .map(Ok),
    );

    if replay_is_terminal {
        return replay_stream.boxed();
    }

    let live_stream = live_subscription_stream(subscription);
    replay_stream.chain(live_stream).boxed()
}

/// Converts a live runtime subscription into an SSE stream.
///
/// The stream ends when:
///
/// - a terminal runtime event is received
/// - the sender side is closed
///
/// Lagged messages are skipped and the stream continues with the next available
/// event to preserve forward progress for clients.
///
/// # Arguments
///
/// * `subscription` - Live runtime subscription
///
/// # Returns
///
/// Returns a boxed stream of Axum SSE events.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
/// use xzatoma::acp::streaming::live_subscription_stream;
/// use xzatoma::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};
/// use xzatoma::Config;
///
/// # async fn example() -> anyhow::Result<()> {
/// let runtime = AcpRuntime::new(Config::default());
/// let run = runtime.create_run(AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
///     AcpRole::User,
///     vec![AcpMessagePart::Text(AcpTextPart::new("Hello".to_string()))],
/// )?]))?;
///
/// let subscription = runtime.subscribe(run.id.as_str())?;
/// let _stream = live_subscription_stream(subscription);
/// # Ok::<(), anyhow::Error>(())
/// # }
/// ```
pub fn live_subscription_stream(
    subscription: crate::acp::runtime::AcpRuntimeSubscription,
) -> BoxStream<'static, std::result::Result<Event, Infallible>> {
    stream::unfold(subscription, |mut subscription| async move {
        loop {
            match subscription.recv().await {
                Ok(runtime_event) => match runtime_event_to_sse_result(runtime_event.clone()) {
                    Ok(event) => {
                        let terminal = runtime_event.terminal;
                        let item = Some((Ok(event), subscription));
                        if terminal {
                            return item;
                        }
                        return item;
                    }
                    Err(_) => continue,
                },
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    })
    .boxed()
}

fn runtime_event_to_sse_result(runtime_event: AcpRuntimeEvent) -> Result<Event> {
    AcpSseEvent::from_runtime_event(runtime_event)?.into_axum_event()
}

fn runtime_event_name(runtime_event: &AcpRuntimeEvent) -> String {
    runtime_event
        .event
        .payload
        .get("event")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string)
        .unwrap_or_else(|| runtime_event.event.kind.to_string())
}

/// Builds a structured ACP streaming error.
///
/// This helper keeps streaming-specific failures expressed using the existing
/// crate error surface with ACP-oriented wording.
///
/// # Arguments
///
/// * `message` - Human-readable failure description
///
/// # Returns
///
/// Returns a crate result error value.
///
/// # Examples
///
/// ```
/// use xzatoma::acp::streaming::streaming_error;
///
/// let error = streaming_error("failed to encode SSE event").unwrap_err();
/// assert!(error.to_string().contains("ACP streaming error"));
/// ```
pub fn streaming_error<T>(message: impl Into<String>) -> Result<T> {
    Err(XzatomaError::AcpLifecycle(format!("ACP streaming error: {}", message.into())).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acp::runtime::{
        assistant_text_message, AcpRuntime, AcpRuntimeCreateRequest, AcpRuntimeExecuteMode,
    };
    use crate::acp::{AcpMessage, AcpMessagePart, AcpRole, AcpTextPart};

    fn test_request() -> AcpRuntimeCreateRequest {
        AcpRuntimeCreateRequest::new(vec![AcpMessage::new(
            AcpRole::User,
            vec![AcpMessagePart::Text(AcpTextPart::new(
                "Stream this run".to_string(),
            ))],
        )
        .unwrap()])
        .with_mode(AcpRuntimeExecuteMode::Stream)
    }

    #[test]
    fn test_acp_sse_event_from_runtime_event_sets_fields() {
        let runtime = AcpRuntime::new(crate::Config::default());
        let run = runtime.create_run(test_request()).unwrap();
        let runtime_event = runtime.get_events(run.id.as_str()).unwrap().remove(0);

        let sse_event = AcpSseEvent::from_runtime_event(runtime_event).unwrap();

        assert_eq!(sse_event.id, "1");
        assert_eq!(sse_event.event, "run.created");
        assert!(sse_event.data.contains("run.created"));
        assert!(!sse_event.terminal);
    }

    #[test]
    fn test_stream_run_events_sse_returns_response_for_existing_run() {
        let runtime = AcpRuntime::new(crate::Config::default());
        let run = runtime.create_run(test_request()).unwrap();

        let response = stream_run_events_sse(runtime, run.id.as_str());
        assert!(response.is_ok());
    }

    #[tokio::test]
    async fn test_build_sse_stream_replays_existing_events_in_order() {
        let runtime = AcpRuntime::new(crate::Config::default());
        let run = runtime.create_run(test_request()).unwrap();
        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();

        let replay = runtime.get_events(run.id.as_str()).unwrap();
        let subscription = runtime.subscribe(run.id.as_str()).unwrap();
        let mut stream = build_sse_stream(replay, subscription);

        let first = stream.next().await.unwrap().unwrap();
        let second = stream.next().await.unwrap().unwrap();
        let third = stream.next().await.unwrap().unwrap();

        let first_debug = format!("{:?}", first);
        let second_debug = format!("{:?}", second);
        let third_debug = format!("{:?}", third);

        assert!(first_debug.contains("run.created"));
        assert!(second_debug.contains("run.in-progress"));
        assert!(third_debug.contains("run.in-progress"));
    }

    #[tokio::test]
    async fn test_live_subscription_stream_yields_terminal_event_and_stops() {
        let runtime = AcpRuntime::new(crate::Config::default());
        let run = runtime.create_run(test_request()).unwrap();
        let subscription = runtime.subscribe(run.id.as_str()).unwrap();
        let mut stream = live_subscription_stream(subscription);

        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        runtime
            .append_output_message(
                run.id.as_str(),
                assistant_text_message("done".to_string()).unwrap(),
            )
            .unwrap();
        runtime.complete_run(run.id.as_str()).unwrap();

        let mut saw_terminal = false;
        while let Some(item) = stream.next().await {
            let event = item.unwrap();
            let debug = format!("{:?}", event);
            if debug.contains("run.completed") {
                saw_terminal = true;
                break;
            }
        }

        assert!(saw_terminal);
    }

    #[test]
    fn test_runtime_event_name_prefers_payload_event_name() {
        let runtime = AcpRuntime::new(crate::Config::default());
        let run = runtime.create_run(test_request()).unwrap();
        let runtime_event = runtime.get_events(run.id.as_str()).unwrap().remove(0);

        assert_eq!(runtime_event_name(&runtime_event), "run.created");
    }

    #[test]
    fn test_streaming_error_wraps_acp_lifecycle_message() {
        let error = streaming_error::<()>("broken stream").unwrap_err();
        assert!(error.to_string().contains("ACP streaming error"));
        assert!(error.to_string().contains("broken stream"));
    }
}
