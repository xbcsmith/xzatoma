# Phase 3: Configuration and Documentation Implementation

## Overview

Phase 3 delivers configuration examples, environment-variable reference documentation, and a How-To guide for the XZatoma watcher. It also adds runtime environment variable support for watcher-specific overrides and adds tests to validate the configuration examples and env-var behavior.

Goals:
- Provide ready-to-use configuration examples for development and production.
- Document environment variables (Kafka, filters, logging, execution).
- Add a step-by-step How-To guide for getting the watcher running.
- Ensure example configs parse and that environment overrides work as expected.

## Components Delivered

- `config/watcher.yaml` (example configuration containing development and production sections)
  - Development and production examples (Kafka, filters, logging, execution).
- `docs/reference/watcher_environment_variables.md`
  - Complete list of environment variables with examples and security guidance.
- `docs/how-to/setup_watcher.md`
  - Step-by-step user guide: prerequisites, basic setup, advanced options, troubleshooting.
- `docs/explanation/phase3_configuration_and_documentation.md`
  - (this file) Implementation summary and usage notes.
- Code changes:
  - `src/config.rs` — extended `Config::apply_env_vars` to:
    - Apply watcher-specific overrides from `XZATOMA_WATCHER_*` variables.
    - Populate watcher Kafka settings from `XZEPR_KAFKA_*` variables when `watcher.kafka` is not present in config.
- Tests:
  - New tests that validate example YAML files parse as `Config`.
  - Tests (marked `#[ignore]` because they modify environment) that verify env-var overrides are applied to `Config`.

## Implementation Details

1. Configuration Examples
   - A canonical example file `config/watcher.yaml` was added containing:
     - Development configuration: `broker`, `topic`, `filters`, `logging`, `execution` settings.
     - Production example demonstrating `SASL_SSL` usage (and noting that passwords should come from environment).
   - The examples are complete, intentionally commented with security best-practices, and use `.yaml` extension.

2. Environment Variable Support
   - Extended `Config::apply_env_vars()` so it picks up watcher-specific environment variables and overrides configuration fields when present.
   - The env var mapping implemented:
     - Kafka: `XZEPR_KAFKA_BROKERS`, `XZEPR_KAFKA_TOPIC`, `XZEPR_KAFKA_GROUP_ID`, `XZEPR_KAFKA_SECURITY_PROTOCOL`, `XZEPR_KAFKA_SASL_*`, `XZEPR_KAFKA_SSL_*`
       - If `watcher.kafka` is None and `XZEPR_KAFKA_BROKERS` (or related vars) are present, `watcher.kafka` will be populated from these variables so the watcher can run without a YAML `watcher.kafka` block.
     - Filters: `XZATOMA_WATCHER_EVENT_TYPES` (comma-separated), `XZATOMA_WATCHER_SOURCE_PATTERN`, `XZATOMA_WATCHER_PLATFORM_ID`, `XZATOMA_WATCHER_PACKAGE`, `XZATOMA_WATCHER_API_VERSION`, `XZATOMA_WATCHER_SUCCESS_ONLY`.
     - Logging: `XZATOMA_WATCHER_LOG_LEVEL`, `XZATOMA_WATCHER_JSON_LOGS`, `XZATOMA_WATCHER_LOG_FILE`, `XZATOMA_WATCHER_INCLUDE_PAYLOAD`.
     - Execution: `XZATOMA_WATCHER_ALLOW_DANGEROUS`, `XZATOMA_WATCHER_MAX_CONCURRENT`, `XZATOMA_WATCHER_EXECUTION_TIMEOUT`.
   - Parsing behavior:
     - Boolean env vars are parsed with `str::parse::<bool>()` and ignored with a log warning if parse fails.
     - Comma-separated lists are split by `,` and trimmed.
     - Numeric values are parsed (usize/u64) and validated when parsing succeeds; parsing failures are logged.

3. Security Considerations
   - SASL passwords remain sensitive and should not be committed in YAML. The watcher checks `XZEPR_KAFKA_SASL_PASSWORD` (or the SASL password in config) and will error if required credentials are missing when applying SASL config (this mirrors the Kafka consumer behavior).
   - Documentation explicitly advises using secret managers or injected environment files and avoiding committed secrets.

4. Tests Added
   - Parsing examples: a test reads `config/watcher.yaml` and ensures it deserializes to `Config` and that key values (e.g., watcher filters, logging, execution limits) match expectations.
   - Environment-var tests:
     - Tests that set `XZEPR_KAFKA_BROKERS`/`XZEPR_KAFKA_TOPIC` and validate `Config::load()` populates `watcher.kafka`.
     - Tests that set `XZATOMA_WATCHER_*` variables and validate the corresponding fields are applied.
     - Tests that modify env variables are marked with `#[ignore]` (see notes below); run them explicitly when you want to validate env behavior.

## Testing

How to run the validation suite (follow the project's quality gates in order):

1. Format
```bash
cargo fmt --all
```

2. Compile check
```bash
cargo check --all-targets --all-features
```

3. Lint
```bash
cargo clippy --all-targets --all-features -- -D warnings
```

4. Tests
```bash
cargo test --all-features
```

Notes on the env-var tests:
- Tests that set or modify environment variables are marked with `#[ignore]` to avoid interference with parallel test runs.
- Run ignored tests (single-threaded) when you want to validate env-var behavior:
```bash
cargo test -- --ignored --test-threads=1
```

Test cases added (examples):
- `tests::test_example_watcher_config_parses`
  - Reads `config/watcher.yaml` and validates key fields.
- `tests::test_apply_env_vars_populates_kafka_from_xzepr_vars` (ignored)
  - Sets `XZEPR_KAFKA_BROKERS`, `XZEPR_KAFKA_TOPIC`, etc and ensures `Config::load` populates `watcher.kafka`.
- `tests::test_apply_env_vars_overrides_watcher_fields` (ignored)
  - Sets `XZATOMA_WATCHER_*` variables and ensures the `Config` fields are overridden.

## Usage Examples

- Minimal `config/watcher.yaml` snippet:
```yaml
watcher:
  kafka:
    brokers: "localhost:9092"
    topic: "xzepr.events"
  filters:
    event_types:
      - "deployment.success"
```

- Start watcher (from config file):
```bash
xzatoma watch --config config/watcher.yaml
```

- Start watcher with env-provided Kafka configuration (no `watcher.kafka` in YAML required):
```bash
export XZEPR_KAFKA_BROKERS="kafka1.prod:9093,kafka2.prod:9093"
export XZEPR_KAFKA_TOPIC="xzepr.production.events"
export XZEPR_KAFKA_SECURITY_PROTOCOL="SASL_SSL"
export XZEPR_KAFKA_SASL_USERNAME="xzatoma-consumer"
# inject password securely via secret manager
xzatoma watch --config config/watcher.yaml
```

- Dry run (parse plans but do not execute):
```bash
xzatoma watch --config config/watcher.yaml --dry-run
```

## Validation Checklist

Before marking Phase 3 complete, ensure:
- [ ] `config/watcher.yaml` is valid YAML (checked by tests).
- [ ] `docs/reference/watcher_environment_variables.md` exists and documents all variables in use.
- [ ] `docs/how-to/setup_watcher.md` exists and walks through setup & troubleshooting.
- [ ] Tests pass (including running ignored env-var tests when validating env behavior manually).
- [ ] `cargo fmt --all`, `cargo check --all-targets --all-features`, `cargo clippy --all-targets --all-features -- -D warnings` all report success.

## References

- Example configuration: `config/watcher.yaml`
- Environment variables reference: `docs/reference/watcher_environment_variables.md`
- How-To guide: `docs/how-to/setup_watcher.md`
- Watcher implementation: `src/watcher/watcher.rs`
- Config/environment handling: `src/config.rs`

## Next Steps / Recommendations

- Add integration tests using a Kafka test harness (or a mock consumer) that exercise the full flow: consume event -> filter -> extract plan -> execute plan.
- Add monitoring/metrics for processed events, execution durations, and failures (Prometheus integration).
- Add graceful draining/cancellation behavior for in-flight executions on shutdown.
- Consider a short guide for operators about secrets management and producing XZepr CloudEvents suitable for automated testing.

---

If you want, I can:
- Run the validation steps and fix any issues we encounter.
- Add an additional dedicated `config/watcher-production.yaml` file (currently the production example is included in `config/watcher.yaml`).
- Implement lightweight integration tests that use a local Redpanda/Kafka container for CI.

Which of those would you like me to do next?
