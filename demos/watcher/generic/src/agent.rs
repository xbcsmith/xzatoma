//! Agent execution and lifecycle management.

use std::collections::VecDeque;

/// The execution state of an agent.
#[derive(Debug, Clone, PartialEq)]
pub enum AgentState {
    /// The agent is idle and has not started.
    Idle,
    /// The agent is waiting for the next user message.
    WaitingForUser,
    /// The agent is actively processing a request.
    Running,
    /// The agent has finished and will accept no further input.
    Finished,
}

/// A single turn in the agent conversation.
#[derive(Debug, Clone)]
pub struct Turn {
    /// The role that produced this turn: `"user"` or `"assistant"`.
    pub role: String,
    /// The text content of the turn.
    pub content: String,
}

/// Holds the runtime state for a single agent session.
#[derive(Debug)]
pub struct Agent {
    /// Human-readable label used in logs and tool output.
    pub label: String,
    /// Current execution state.
    pub state: AgentState,
    /// Ordered conversation history (oldest first).
    pub history: VecDeque<Turn>,
    /// Number of turns consumed so far.
    turns_used: usize,
    /// Maximum turns this agent may consume.
    max_turns: usize,
}

impl Agent {
    /// Creates a new `Agent` with the given label and turn budget.
    ///
    /// The agent starts in the [`AgentState::Idle`] state with an empty
    /// conversation history.
    pub fn new(label: &str, max_turns: usize) -> Self {
        Agent {
            label: label.to_string(),
            state: AgentState::Idle,
            history: VecDeque::new(),
            turns_used: 0,
            max_turns,
        }
    }

    /// Appends a user message to the conversation history.
    ///
    /// Transitions the agent to [`AgentState::WaitingForUser`] if it was
    /// previously idle.
    pub fn push_user_message(&mut self, content: &str) {
        self.history.push_back(Turn {
            role: "user".to_string(),
            content: content.to_string(),
        });
        if self.state == AgentState::Idle {
            self.state = AgentState::WaitingForUser;
        }
    }
}

pub fn run_agent(agent: &mut Agent, prompt: &str) -> Result<String, String> {
    if agent.state == AgentState::Finished {
        return Err(format!("agent '{}' has already finished", agent.label));
    }
    if agent.turns_used >= agent.max_turns {
        return Err(format!(
            "agent '{}' exceeded turn budget of {}",
            agent.label, agent.max_turns
        ));
    }
    agent.push_user_message(prompt);
    agent.state = AgentState::Running;
    agent.turns_used += 1;

    // Stub: in a real implementation this would call the AI provider.
    let response = format!(
        "Agent '{}' processed prompt (turn {}/{})",
        agent.label, agent.turns_used, agent.max_turns
    );
    agent.history.push_back(Turn {
        role: "assistant".to_string(),
        content: response.clone(),
    });
    agent.state = AgentState::WaitingForUser;
    Ok(response)
}

/// Returns the number of turns this agent has consumed so far.
///
/// The value is incremented once per [`run_agent`] call and never decreases.
/// Compare against `max_turns` to determine the remaining budget.
pub fn turns_used(agent: &Agent) -> usize {
    agent.turns_used
}

pub fn reset_agent(agent: &mut Agent) {
    agent.state = AgentState::Idle;
    agent.history.clear();
    agent.turns_used = 0;
}

pub fn turn_budget(agent: &Agent) -> usize {
    agent.max_turns.saturating_sub(agent.turns_used)
}

/// Returns `true` when the agent has consumed its full turn budget or has
/// transitioned to the [`AgentState::Finished`] state.
pub fn is_exhausted(agent: &Agent) -> bool {
    agent.turns_used >= agent.max_turns || agent.state == AgentState::Finished
}
