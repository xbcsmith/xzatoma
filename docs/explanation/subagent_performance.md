# Subagent Performance and Scalability

## Overview

Phase 5 implements advanced execution patterns for subagent deployment in production environments. This document covers resource management, performance metrics, quotas, and tuning guidelines for optimizing subagent performance and scalability.

## Resource Quotas

XZatoma supports three types of resource quotas to prevent runaway executions and control costs:

### Execution Quotas

Limits the number of subagent invocations per session.

```yaml
agent:
  subagent:
    max_executions: 10
```

Use when:
- Cost control is critical
- You want to prevent accidental infinite loops
- Operating under strict API rate limits
- Running in time-boxed environments

### Token Quotas

Limits total tokens consumed across all subagent executions.

```yaml
agent:
  subagent:
    max_total_tokens: 100000
```

Use when:
- API billing is token-based
- You need precise cost budgeting
- Managing expensive LLM providers
- Testing with limited compute budgets

### Time Quotas

Limits total wall-clock time for all subagent executions.

```yaml
agent:
  subagent:
    max_total_time: 300  # seconds
```

Use when:
- Operating in time-constrained environments
- CI/CD pipelines with timeout requirements
- Interactive sessions with time limits
- Managing infrastructure costs tied to runtime

## Performance Metrics

SubagentMetrics automatically track execution performance:

- `subagent_executions_total` - Total number of executions
- `subagent_active_count` - Currently executing subagents
- `subagent_duration_seconds` - Execution time histogram
- `subagent_turns_used` - Conversation turns histogram
- `subagent_tokens_consumed` - Token consumption histogram
- `subagent_completions_total` - Completed executions
- `subagent_errors_total` - Failed executions by error type

Metrics are collected automatically for all subagent executions. Enable Prometheus export with the `prometheus` feature:

```bash
cargo build --features prometheus
```

## Tuning Guidelines

### Token Consumption

Monitor token usage patterns:

```rust
let usage = tracker.get_usage();
println!("Tokens used: {}", usage.total_tokens);
```

If token consumption is high:
1. Reduce `default_max_turns` in configuration
2. Implement better prompt engineering
3. Use smaller models for simpler tasks
4. Consider summarization between turns

### Execution Time

Track execution duration:

```rust
let metrics = SubagentMetrics::new("task".to_string(), 1);
// ... execution ...
let elapsed = metrics.elapsed();
```

If execution is slow:
1. Check network latency to provider
2. Verify provider is not rate-limiting
3. Consider parallelizing independent subtasks
4. Optimize prompts for clarity and brevity

### Quota Management

Set realistic quotas based on:

1. **Available budget**: Total tokens you can afford
2. **Average execution cost**: Tokens per typical subagent
3. **Session duration**: Expected wall-clock time
4. **Safety margin**: Buffer for retries and errors

Example calculation:

```
Token budget: 100,000 tokens
Average task: 1,500 tokens
Max tasks: 100,000 / 1,500 = 66 tasks
Set max_executions: 50 (with safety margin)
```

## Configuration Examples

### Cost-Optimized (Budget-Conscious)

```yaml
agent:
  subagent:
    max_depth: 2
    default_max_turns: 3
    max_executions: 5
    max_total_tokens: 10000
    output_max_size: 2048
```

For: Minimal cost, simple tasks, strict budgets

### Performance-Optimized (Speed-Focused)

```yaml
agent:
  subagent:
    max_depth: 4
    default_max_turns: 20
    max_executions: 50
    max_total_tokens: 500000
    max_total_time: 3600
```

For: Complex tasks, quick results, unlimited budget

### Balanced (Production Default)

```yaml
agent:
  subagent:
    max_depth: 3
    default_max_turns: 10
    max_executions: 20
    max_total_tokens: 100000
    max_total_time: 600
```

For: Most production deployments

## Monitoring and Alerts

### Track Quota Utilization

```rust
let usage = tracker.get_usage();
let remaining_tokens = tracker.remaining_tokens();
let remaining_time = tracker.remaining_time();

if remaining_tokens.unwrap_or(0) < 5000 {
    warn!("Low token budget remaining");
}
```

### Detect Performance Issues

Monitor these metrics:

1. **High error rates**: Check `subagent_errors_total`
2. **Slow executions**: Check `subagent_duration_seconds` histogram
3. **High token usage**: Check `subagent_tokens_consumed`
4. **Many active subagents**: Check `subagent_active_count`

### Set Up Alerts

For production, alert when:

- Execution time exceeds expected threshold
- Error rate above 5%
- Quota utilization above 80%
- Active subagent count exceeds max concurrency

## Testing Quotas

Test quota enforcement in development:

```rust
let limits = QuotaLimits {
    max_executions: Some(2),
    max_total_tokens: None,
    max_total_time: None,
};
let tracker = QuotaTracker::new(limits);

// First execution succeeds
assert!(tracker.check_and_reserve().is_ok());
tracker.record_execution(100).ok();

// Second execution succeeds
assert!(tracker.check_and_reserve().is_ok());
tracker.record_execution(100).ok();

// Third execution fails
assert!(tracker.check_and_reserve().is_err());
```

## Common Patterns

### Pattern 1: Cost-Limited Exploration

```yaml
agent:
  subagent:
    max_total_tokens: 50000  # Fixed budget
    max_executions: 20        # Or hit execution limit first
```

Use for: Exploring solutions with budget constraints

### Pattern 2: Time-Boxed Analysis

```yaml
agent:
  subagent:
    max_total_time: 300      # 5 minutes
    max_executions: 50       # Allow many attempts
    max_total_tokens: 500000 # Generous tokens
```

Use for: Time-constrained environments (CI/CD)

### Pattern 3: Safety-First Delegation

```yaml
agent:
  subagent:
    max_depth: 2
    max_executions: 5
    max_total_tokens: 10000
    max_total_time: 60
```

Use for: Untrusted prompts, safety-critical work

## Troubleshooting

### "Execution limit reached"

**Cause**: Too many subagents spawned

**Solution**:
1. Increase `max_executions` if quota is too low
2. Reduce `default_max_turns` to save retries
3. Implement better task batching
4. Filter unnecessary subagent calls

### "Token limit exceeded"

**Cause**: Average task costs too many tokens

**Solution**:
1. Increase `max_total_tokens` if budget allows
2. Reduce `default_max_turns` per subagent
3. Optimize prompts for brevity
4. Use smaller models for simpler tasks

### "Time limit exceeded"

**Cause**: Subagents taking too long

**Solution**:
1. Increase `max_total_time` if deadline allows
2. Check provider latency
3. Reduce prompt complexity
4. Parallelize independent subtasks

## Best Practices

1. **Always set quotas**: Prevent accidental cost overruns
2. **Monitor metrics**: Track real-world performance
3. **Test quotas**: Verify limits in development
4. **Start conservative**: Increase limits gradually
5. **Alert early**: Warn before hitting limits
6. **Log executions**: Debug slow or expensive tasks
7. **Review regularly**: Adjust based on actual usage

## References

- Configuration: `docs/how-to/configure_quotas.md`
- Metrics API: `src/agent/metrics.rs`
- Quota API: `src/agent/quota.rs`
- Implementation: `docs/explanation/phase5_implementation.md`
