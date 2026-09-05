//! Await-input tool for pausing ACP run execution pending external input
//!
//! This module implements `AwaitInputTool`, which allows an agent to pause
//! the current ACP run and wait for a human or orchestrator to supply a
//! resume payload. The run transitions to `Awaiting` state while blocked.
//!
//! The tool is registered per-run by `AcpExecutor::execute_prompt` and holds
//! a reference to the runtime and the current run identifier. It must not be
//! placed in a shared registry across runs.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::oneshot;

use crate::acp::runtime::AcpRuntime;
use crate::error::{Result, XzatomaError};
use crate::tools::{ToolExecutor, ToolResult};

/// Tool name constant for `await_input`.
pub const TOOL_AWAIT_INPUT: &str = "await_input";

/// Parameters accepted by `AwaitInputTool`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct AwaitInputParams {
    /// Discriminator describing what kind of input is needed.
    kind: String,
    /// Human-readable detail explaining what is being awaited.
    detail: String,
}

/// Tool that pauses the current ACP run and waits for a resume payload.
///
/// When invoked, the tool:
/// 1. Creates a one-shot channel.
/// 2. Registers the sender with the ACP runtime for this run.
/// 3. Transitions the run to `Awaiting` state.
/// 4. Blocks on the receiver until `resume_run` delivers a payload.
/// 5. Returns the payload as the tool result.
///
/// This tool is registered per-run by the ACP executor and must not be
/// shared across multiple runs.
///
/// # Examples
///
/// ```
/// use xzatoma::tools::await_input::AwaitInputTool;
/// use xzatoma::acp::runtime::AcpRuntime;
/// use xzatoma::Config;
///
/// let config = Config::default();
/// let runtime = AcpRuntime::new_in_memory(config);
/// let tool = AwaitInputTool::new(runtime, "run-001".to_string());
/// ```
pub struct AwaitInputTool {
    runtime: AcpRuntime,
    run_id: String,
}

impl AwaitInputTool {
    /// Creates a new `AwaitInputTool` for the given run.
    ///
    /// # Arguments
    ///
    /// * `runtime` - Shared ACP runtime handle
    /// * `run_id` - Identifier of the run this tool instance is attached to
    ///
    /// # Returns
    ///
    /// Returns a new `AwaitInputTool`.
    ///
    /// # Examples
    ///
    /// ```
    /// use xzatoma::tools::await_input::AwaitInputTool;
    /// use xzatoma::acp::runtime::AcpRuntime;
    /// use xzatoma::Config;
    ///
    /// let config = Config::default();
    /// let runtime = AcpRuntime::new_in_memory(config);
    /// let tool = AwaitInputTool::new(runtime, "run-001".to_string());
    /// ```
    pub fn new(runtime: AcpRuntime, run_id: String) -> Self {
        Self { runtime, run_id }
    }
}

impl std::fmt::Debug for AwaitInputTool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AwaitInputTool")
            .field("run_id", &self.run_id)
            .finish()
    }
}

#[async_trait]
impl ToolExecutor for AwaitInputTool {
    fn tool_definition(&self) -> Value {
        json!({
            "name": TOOL_AWAIT_INPUT,
            "description": "Pause the current ACP run and wait for a resume payload. The run transitions to Awaiting state until the server receives a POST /runs/{run_id} request with a resume payload.",
            "parameters": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "description": "Discriminator describing what kind of input is needed (e.g. 'approval_required', 'user_input')"
                    },
                    "detail": {
                        "type": "string",
                        "description": "Human-readable explanation of what is being awaited"
                    }
                },
                "required": ["kind", "detail"]
            }
        })
    }

    async fn execute(&self, args: Value) -> Result<ToolResult> {
        let params: AwaitInputParams = crate::error::parse_tool_args(args)
            .map_err(|e| XzatomaError::Tool(format!("await_input: invalid parameters: {}", e)))?;

        // Create a one-shot channel before transitioning to Awaiting so that
        // a concurrent resume call cannot arrive before the channel is ready.
        let (tx, rx) = oneshot::channel::<Value>();

        // Register the sender so resume_run can deliver the payload.
        self.runtime
            .register_await_channel(&self.run_id, tx)
            .map_err(|e| {
                XzatomaError::Tool(format!("await_input: failed to register channel: {}", e))
            })?;

        // Transition the run to Awaiting state.
        self.runtime
            .set_awaiting(&self.run_id, params.kind.clone(), params.detail.clone())
            .map_err(|e| {
                XzatomaError::Tool(format!("await_input: failed to set awaiting state: {}", e))
            })?;

        tracing::debug!(
            run_id = %self.run_id,
            kind = %params.kind,
            "ACP run entered Awaiting state; blocking on resume"
        );

        // Block until the resume payload arrives or the sender is dropped.
        match rx.await {
            Ok(payload) => {
                let output =
                    serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
                Ok(ToolResult::success(output))
            }
            Err(_) => Err(XzatomaError::Tool(format!(
                "await_input: resume channel closed for run '{}' without delivering a payload; the run may have been cancelled",
                self.run_id
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;
    use crate::acp::runtime::{AcpRuntime, AcpRuntimeCreateRequest};
    use crate::acp::{
        AcpMessage, AcpMessagePart, AcpRole, AcpRunId, AcpRunResumeRequest, AcpTextPart,
    };

    fn make_runtime() -> AcpRuntime {
        AcpRuntime::new_in_memory(Config::default())
    }

    fn make_run(runtime: &AcpRuntime) -> crate::acp::AcpRun {
        let input = vec![
            AcpMessage::new(
                AcpRole::User,
                vec![AcpMessagePart::Text(AcpTextPart::new("test".to_string()))],
            )
            .unwrap(),
        ];
        let run = runtime
            .create_run(AcpRuntimeCreateRequest::new(input))
            .unwrap();
        runtime.mark_queued(run.id.as_str()).unwrap();
        runtime.mark_running(run.id.as_str()).unwrap();
        run
    }

    #[test]
    fn test_tool_definition_has_required_fields() {
        let runtime = make_runtime();
        let tool = AwaitInputTool::new(runtime, "run-001".to_string());
        let def = tool.tool_definition();
        assert_eq!(def["name"], TOOL_AWAIT_INPUT);
        let required = &def["parameters"]["required"];
        let req_arr = required.as_array().unwrap();
        assert!(req_arr.contains(&json!("kind")));
        assert!(req_arr.contains(&json!("detail")));
    }

    #[test]
    fn test_debug_impl() {
        let runtime = make_runtime();
        let tool = AwaitInputTool::new(runtime, "run-001".to_string());
        let s = format!("{:?}", tool);
        assert!(s.contains("AwaitInputTool"));
        assert!(s.contains("run-001"));
    }

    #[tokio::test]
    async fn test_await_input_round_trip_transitions_awaiting_and_delivers_payload() {
        let runtime = make_runtime();
        let run = make_run(&runtime);
        let run_id = run.id.as_str().to_string();

        let tool = AwaitInputTool::new(runtime.clone(), run_id.clone());

        // Spawn the tool execution in the background -- it will block.
        let execute_handle = tokio::spawn({
            let tool = tool;
            async move {
                tool.execute(json!({
                    "kind": "approval_required",
                    "detail": "Please approve the action"
                }))
                .await
            }
        });

        // Give the tool time to register the channel and set Awaiting.
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify run is in Awaiting state.
        let run_snapshot = runtime.get_run(&run_id).unwrap();
        assert_eq!(
            run_snapshot.status.state,
            crate::acp::AcpRunState::Awaiting,
            "run should be Awaiting after await_input invocation"
        );

        // Resume the run.
        let resume_payload = json!({"approved": true, "comment": "looks good"});
        let request = AcpRunResumeRequest::new(AcpRunId::new(run_id.clone()).unwrap());
        runtime.resume_run(request, resume_payload.clone()).unwrap();

        // The tool should return the payload.
        let result = execute_handle.await.unwrap().unwrap();
        assert!(result.success);
        assert!(result.output.contains("approved"));
    }
}
