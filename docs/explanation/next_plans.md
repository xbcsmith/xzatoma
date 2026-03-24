# Next Plans

## ACP Support

We are going to add Agent Communication Protocol support to xzatoma. The Spec is here https://agentcommunicationprotocol.dev/introduction/welcome

Write a plan with a phased approach to add ACP support to XZatoma. THINK HARD and follow the rules in @PLAN.md

PLAN WRITTEN - [ACP Support](./acp_implementation_plan.md)

## Skills Support

We are giong to add Agent Skills support to xzatoma. The spec is here https://agentskills.io/specification And doc to help add skill support https://agentskills.io/client-implementation/adding-skills-support

Write a plan with a phased approach to add Agent Skills support to XZAtoma. THINK HARD and follow the rules in @PLAN.md

PLAN WRITTEN - [Agent Skills Support](./agent_skills_implementation_plan.md)

## Demos

We need demos for Chat, Run, Skills, MCP, Subagents, Vision, and Watcher. Demos should live in subfolders under @demos and include a README.md walking the user through running the demo, any scripts to setup the demo, and any config files required for the demos. Demos should be completely self contained and not need anything outside of the individual demo directory. XZatoma should be properly sandboxed to the demo directory. All created files and content should live in the tmp directory. All output from the demos should be contained in a tmp/output directory. The tmp dirs will include a .gitignore file to prevent any demo data being included in a git commit. All demos should use Ollama models only. Specifically "granite4:3b" except for the vision demo which will require the "granite3.2-vision:2b" model.

Write a plan with a phased approach to create the Demos for XZatoma. THINK HARD and follow the rules in @PLAN.md

PLAN WRITTEN - [Demo Plan](./demo_implementation_plan.md)

## Generic Watcher

We are going to make a new watcher that works with Redpanda and consumes a plan as a JSON event from a topic and posts the results to a topic. The topics are configurable at CLI or config file and can be the same topic for the event trigger and the work summary. The trigger event should be in the format of a plan file. The plan file will need to be expanded to contain a "action" field that we can match on. The matcher for the regular Kafka/Redpanda consumer should be able to match on any combination of "action", "name" + "version", "name" + "action", or "name" + "version" + "action". As part of this work we will move all the @xzepr code and anything in current @watcher directory that is specifically related to XZepr into a subdir of @watcher called xzepr. The watcher type sould be configured through config or CLI. The results will be that in watcher mode xzatoma can be configured to work with regular Kafka/Redpanda topics or the specific XZepr style topic through the CLI or Configuration file.

Write a plan with a phased approach to add the new generic watcher and move the xzepr watcher work. THINK HARD and follow the rules in @PLAN.md

COMPLETED - [Generic Watcher Implementation Plan](./generic_watcher_implementation_plan.md)
