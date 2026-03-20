# Next Plans

## ACP Support

We are going to add Agent Communication Protocol support to xzatoma. The Spec is here https://agentcommunicationprotocol.dev/introduction/welcome

Write a plan with a phased approach to add ACP support to XZatoma. THINK HARD and follow the rules in @PLAN.md

[ACP Support](./acp_implementation_plan.md)

## Generic Watcher

We are going to make a new watcher that works with Redpanda and consumes a plan as a JSON event from a topic and posts the results to a topic. The topics are configurable at CLI or config file and can be the same topic for the event trigger and the work summary. The trigger event should be in the format of a plan file. The plan file will need to be expanded to contain a "action" field that we can match on. The matcher for the regular Kafka/Redpanda consumer should be able to match on any combination of "action", "name" + "version", "name" + "action", or "name" + "version" + "action". As part of this work we will move all the @xzepr code and anything in current @watcher directory that is specifically related to XZepr into a subdir of @watcher called xzepr. The watcher type sould be configured through config or CLI. The results will be that in watcher mode xzatoma can be configured to work with regular Kafka/Redpanda topics or the specific XZepr style topic through the CLI or Configuration file.

Write a plan with a phased approach to add the new generic watcher and move the xzepr watcher work. THINK HARD and follow the rules in @PLAN.md

[Generic Watcher Implementation Plan](./generic_watcher_implementation_plan.md)
