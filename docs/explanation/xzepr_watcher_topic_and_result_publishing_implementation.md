# XZepr Watcher Topic Creation and Result Publishing

This document describes how the XZepr watcher creates Kafka topics at startup
and publishes plan execution results to the configured output topic.

## Topic auto-creation

When `watcher.kafka.auto_create_topics` is `true` (or the `--create-topics` CLI
flag is set), `run_watch` in `commands/mod.rs` constructs a
[`WatcherTopicAdmin`](../../src/watcher/topic_admin.rs) and calls
`ensure_xzepr_watcher_topics()` before the watcher enters its consume loop.

The XZepr path now mirrors the generic watcher:

- always ensure the input `topic` exists
- when `output_topic` is configured and differs from the input topic, also
  ensure the output topic exists

Topic creation is idempotent: `TopicAlreadyExists` is treated as success.

## Result publishing

After a plan is extracted from an XZepr CloudEvent and executed (or processed in
dry-run mode), the watcher publishes a
[`GenericPlanResult`](../reference/workflow_format.md) to the effective output
topic via [`GenericResultProducer`](../reference/architecture.md).

Output topic resolution:

1. use `watcher.kafka.output_topic` when set
2. otherwise fall back to `watcher.kafka.topic`

This reuses the generic watcher's result producer so both backends publish the
same JSON result schema (`event_type: "result"`) to their output topics.

Dry-run mode uses
[`FakeResultProducer`](../../src/watcher/generic/result_producer.rs) so no
broker connection is required while still exercising the publish path in tests.

## Configuration example

```yaml
watcher:
  watcher_type: xzepr
  kafka:
    brokers: "localhost:19092"
    topic: "epr.dev.events"
    output_topic: "xzepr.results"
    auto_create_topics: true
    num_partitions: 1
    replication_factor: 1
```

With this configuration, startup creates both `epr.dev.events` and
`xzepr.results`, and each completed plan execution publishes a result event to
`xzepr.results`.
