# Subagent Support Implementation Plan

## Overview

This plan outlines the integration of subagent capabilities into `xzatoma`, enabling the main agent to delegate tasks to recursive agent instances. This feature mirrors the functionality found in Zed's agent implementation, allowing for task decomposition and specialized execution within a sandboxed context.

## Current State Analysis

### Existing Infrastructure

-   **Agent Core (`src/agent/core.rs`)**: The `Agent` struct manages the conversation loop, tool execution, and provider interaction. It is currently designed as a single executing entity.
-   **Conversation Management (`src/agent/conversation.rs`)**: Handles message history and token budgeting.
-   **Tool Registry (`src/tools/mod.rs`)**: A unified registry for registering and executing tools.
-   **Provider Abstraction (`src/providers/base.rs`)**: A thread-safe (`Send + Sync`) trait for LLM interactions.

### Identified Issues

-   **Single-Threaded Execution**: complex tasks that require exploring multiple paths or isolating context currently pollute the main conversation history.
-   **Lack of Delegation**: The agent cannot "think" about a sub-problem independently without the entire context window growing significantly.
-   **Missing Recursive Capabilities**: There is no mechanism to spawn an agent from within an agent.

## Implementation Phases

### Phase 1: Core Implementation

#### Task 1.1 Foundation Work
-   Define `SubagentToolInput` struct in `src/tools/subagent.rs` to match the schema required for delegation (label, task_prompt, summary_prompt, etc.).
-   Add `Agent::new_from_provider` (or similar) to `src/agent/core.rs` to allow efficient creation of new agent instances that share the existing underlying `Arc<dyn Provider>`.

#### Task 1.2 Add Foundation Functionality
-   Create `src/tools/subagent.rs`.
-   Implement the `SubagentTool` struct, which holds a reference to the `Provider`, `AgentConfig`, and the parent's `ToolRegistry`.
-   Implement logic to construct a fresh `ToolRegistry` for the subagent (filtering allowed tools if specified).

#### Task 1.3 Integrate Foundation Work
-   Implement `ToolExecutor` for `SubagentTool`.
-   The `execute` method will:
    1.  Check recursion depth limits.
    2.  Instantiate a new `Agent` with the shared provider and filtered tools.
    3.  Run the sub-agent's `execute` loop with the provided `task_prompt`.
    4.  Return the sub-agent's final response as the tool output.

#### Task 1.4 Testing Requirements
-   Unit tests for `SubagentTool` input parsing.
-   Mock provider tests ensuring a `SubagentTool` call correctly triggers a secondary "conversation" (simulated via mock responses).

#### Task 1.5 Deliverables
-   `src/tools/subagent.rs` module.
-   Updated `Agent` struct creation methods.

#### Task 1.6 Success Criteria
-   An agent can be instantiated programmatically and run within another agent's tool execution flow.

### Phase 2: Feature Implementation

#### Task 2.1 Feature Work
-   Integrate `SubagentTool` into the main application.
-   Export `subagent` from `src/tools/mod.rs`.

#### Task 2.2 Integrate Feature
-   Modify `src/commands/mod.rs` (specifically `run_chat`) to register the `SubagentTool`.
-   Ensure the `SubagentTool` is instantiated with the correct current recursion depth (starting at 0).

#### Task 2.3 Configuration Updates
-   Review `AgentConfig` to see if subagent-specific limits (e.g., `max_subagent_depth`) need to be exposed in `config.rs`. For now, a constant `MAX_DEPTH` (e.g., 3) in code is acceptable as a starting point.

#### Task 2.4 Testing Requirements
-   Manual verification using `xzatoma chat`.
-   Interact with the agent and explicitly ask it to "use a subagent to research X".
-   Verify that the sub-agent runs, completes its task, and returns a summary to the main conversation.

#### Task 2.5 Deliverables
-   A working `xzatoma` CLI that supports the `subagent` tool.

#### Task 2.6 Success Criteria
-   The "Subagent" tool is visible in the tool definitions sent to the LLM.
-   The LLM successfully calls the tool and incorporates its output.
