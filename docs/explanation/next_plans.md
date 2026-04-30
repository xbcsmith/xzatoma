# Next Plans

Analyze the @src codebase for refactoring opportunities use subagents to do the following:

1. Find duplicate code patterns, look for places we can consolidate code and create reusable functions or components.
2. Identify unused exports, specific "#[ignore]", and dead code "#[dead_code]", #[allow(unused_mut)], #[allow(clippy::too_many_arguments)] to see if there are any exports that can be removed or refactored to reduce clutter and improve maintainability.
3. Review error handling consistency
4. Look for unfinished TODOs, FIXMEs, and place holders in the codebase as well as references to Phases in the codebase that should be removed.
5. References to Phases in the codebase that should be removed.

We do not care about backwards compatability. Compile the findings into a prioritized action plan with a phased approach.

Write a plan with a phased approach to cleaning up the codebase. THINK HARD and follow the rules in @PLAN.md

✅ COMPLETED - [Codebase Cleanup Plan](./codebase_cleanup_plan.md)

## ACP Support

We are going to add Agent Communication Protocol support to xzatoma. The Spec is here https://agentcommunicationprotocol.dev/introduction/welcome

Write a plan with a phased approach to add ACP support to XZatoma. THINK HARD and follow the rules in @PLAN.md

✅ COMPLETED - [ACP Support](./acp_implementation_plan.md)

## Skills Support

We are giong to add Agent Skills support to xzatoma. The spec is here https://agentskills.io/specification And doc to help add skill support https://agentskills.io/client-implementation/adding-skills-support

Write a plan with a phased approach to add Agent Skills support to XZAtoma. THINK HARD and follow the rules in @PLAN.md

✅ COMPLETE - [Agent Skills Support](./agent_skills_implementation_plan.md)

## Demos

We need demos for Chat, Run, Skills, MCP, Subagents, Vision, and Watcher. Demos should live in subfolders under @demos and include a README.md walking the user through running the demo, any scripts to setup the demo, and any config files required for the demos. Demos should be completely self contained and not need anything outside of the individual demo directory. XZatoma should be properly sandboxed to the demo directory. All created files and content should live in the tmp directory. All output from the demos should be contained in a tmp/output directory. The tmp dirs will include a .gitignore file to prevent any demo data being included in a git commit. All demos should use Ollama models only. Specifically "granite4:3b" except for the vision demo which will require the "granite3.2-vision:2b" model.

Write a plan with a phased approach to create the Demos for XZatoma. THINK HARD and follow the rules in @PLAN.md

ADD ACP to the demos

✅ COMPLETED - [Demo Plan](./demo_implementation_plan.md)

## Generic Watcher

We are going to make a new watcher that works with Redpanda and consumes a plan as a JSON event from a topic and posts the results to a topic. The topics are configurable at CLI or config file and can be the same topic for the event trigger and the work summary. The trigger event should be in the format of a plan file. The plan file will need to be expanded to contain a "action" field that we can match on. The matcher for the regular Kafka/Redpanda consumer should be able to match on any combination of "action", "name" + "version", "name" + "action", or "name" + "version" + "action". As part of this work we will move all the @xzepr code and anything in current @watcher directory that is specifically related to XZepr into a subdir of @watcher called xzepr. The watcher type sould be configured through config or CLI. The results will be that in watcher mode xzatoma can be configured to work with regular Kafka/Redpanda topics or the specific XZepr style topic through the CLI or Configuration file.

Write a plan with a phased approach to add the new generic watcher and move the xzepr watcher work. THINK HARD and follow the rules in @PLAN.md

✅ COMPLETED - [Generic Watcher Implementation Plan](./generic_watcher_implementation_plan.md)

## OpenAI Provider Support

We are going to add OpenAI provider support to XZatoma. This will allow users to use OpenAI models in addition to Ollama models. We will need to add configuration options for OpenAI API keys and model selection. We will also need to implement the necessary code to interface with the OpenAI API and handle responses. The implementation should be modular so that we can easily add support for other providers in the future. We want the provider to be compatable with other inference servers that have an OpenAI compatable API, like llama.cpp, vLLM, Candle-vLLM, and Mistral.rs.  [OpenAI OpenAPI Spec](./openapi.with-code-samples.yml)

Write a plan with a phased approach to add OpenAI support to XZatoma. THINK HARD and follow the rules in @PLAN.md

✅ COMPLETED - [OpenAI Support Implementation Plan](./openai_support_implementation_plan.md)


## Notes

FIXED - Error: tmp/output: File load error: Not a file: tmp/output (Not a file) @mentions needs to support directories.


### Add Thinkng Mode support

We want to add support for a "thinking mode" where the agent can indicate that it is thinking and not ready to respond yet. This will allow us to provide better feedback to the user and prevent timeouts when the agent is processing a complex task. We will need to define a new message type for "thinking" and update the agent loop to handle this new message type appropriately. We will also need to update the CLI and any relevant demos to demonstrate this new feature. We need to be able to set none, Extra High, High, Medium, or Low thinking modes. The thinking mode should be set by the agent in the response message and should be based on the complexity of the task being processed. For example, if the agent is processing a task that requires multiple steps or interactions with external tools, it may set the thinking mode to "High" to indicate that it is still working on the task and may not have a response ready yet. Default to whatever the provider returns if they have a specific field for it, otherwise default to "High" if not provided by the provider.

This includes support for setting it from Zed's Chat window in ACP mode.

Write a plan with a phased approach to add Thinking Mode support to XZatoma. THINK HARD and follow the rules in @PLAN.md

[Thinking Mode Implementation Plan](./thinking_mode_implementation_plan.md)

### Add Context Window Support to ACP Mode

Add support for Zed's context window in ACP mode. This will allow the agent to provide additional context to the user in the Zed editor while it is processing a task. We will need to define a new message type for "context" and update the agent loop to handle this new message type appropriately. We will also need to update the CLI and any relevant demos to demonstrate this new feature. The context window should be able to display additional information about the task being processed, such as intermediate results, tool outputs, or any other relevant information that can help the user understand what the agent is doing.

Write a plan with a phased approach to add Context Window support to ACP mode in XZatoma. THINK HARD and follow the rules in @PLAN.md

[Context Window Implementation Plan](./context_window_implementation_plan.md)
