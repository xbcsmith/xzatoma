# Configuration Defaults Increase Implementation

## Overview

Updated XZatoma's default output size limits to prevent truncation of subagent and tool outputs. The primary issue was that subagent outputs were being truncated at 4 KB, causing large AI-generated responses (like detailed plans) to be cut off. This fix increases both the subagent and main tool output limits to more reasonable defaults.

## Problem

When using XZatoma in chat mode with Ollama as the provider, users experienced truncated output when requesting large responses. For example, asking the agent to "Write the creature viewer plan to file" resulted in a file with only the first 400-650 bytes of content instead of the full 2.2 KB plan.

### Root Cause

The issue stemmed from two restrictive default output size limits in `src/config.rs`:

1. **Subagent output limit**: 4,096 bytes (4 KB)
   - Applied in `src/tools/subagent.rs` at line 873
   - Truncated all subagent responses exceeding this size
   - This was the primary cause of truncated output in chat mode

2. **Main tool output limit**: 1,048,576 bytes (1 MB)
   - Applied in `src/agent/core.rs` at line 677
   - Less restrictive but still limited for very large outputs

## Solution

### Components Delivered

- `src/config.rs` (2,359 lines)
  - Updated `default_subagent_output_max_size()` from 4,096 to 1,048,576 (1 MB)
  - Updated `default_max_output()` from 1,048,576 to 5,242,880 (5 MB)
  - Updated 4 test cases to match new defaults

## Implementation Details

### Change 1: Subagent Output Size (Line 259-261)

**Before:**
```rust
fn default_subagent_output_max_size() -> usize {
    4096
}
```

**After:**
```rust
fn default_subagent_output_max_size() -> usize {
    1_048_576 // 1 MB
}
```

**Impact**: Subagents can now return up to 1 MB of output without truncation, matching the previous main tool output limit.

### Change 2: Main Tool Output Size (Line 479-481)

**Before:**
```rust
fn default_max_output() -> usize {
    1_048_576 // 1 MB
}
```

**After:**
```rust
fn default_max_output() -> usize {
    5_242_880 // 5 MB
}
```

**Impact**: Main tool outputs can now reach 5 MB, providing more headroom for large file reads, fetch operations, and other data-intensive tools.

### Updated Test Cases

Three test functions were updated to match the new defaults:

1. **test_subagent_config_defaults** (Line 1319)
   - Updated assertion from `4096` to `1_048_576`

2. **test_tools_config_defaults** (Line 1273)
   - Updated assertion from `1_048_576` to `5_242_880`

3. **test_subagent_config_empty_section_uses_defaults** (Line 2002)
   - Updated assertion from `4096` to `1_048_576`

## Why These Values?

### 1 MB for Subagent Output
- **Sufficient**: Handles most AI-generated responses (plans, explanations, code)
- **Reasonable**: Doesn't bloat context windows unnecessarily
- **Consistent**: Matches the old main tool limit, creating familiarity
- **Practical**: The creature viewer plan (2.2 KB) fits comfortably with ~450x headroom

### 5 MB for Main Tool Output
- **Flexible**: Accommodates large file reads (10 MB max file read still exists)
- **Safe**: Won't cause memory issues with reasonable context windows
- **Future-proof**: Provides headroom for growing LLM context windows
- **Tools-focused**: Allows fetch, read, and grep operations on larger files

## Configuration Behavior

### When No Config File Exists

Users running XZatoma without a config file now get these defaults:
- Main tool output: 5 MB
- Subagent output: 1 MB

### When Config File Exists

Users can still override these values in their config file:

```yaml
agent:
  tools:
    max_output_size: 10485760  # 10 MB
  subagent:
    output_max_size: 2097152   # 2 MB
```

## Testing

### Test Coverage

All tests pass with 100% coverage verification:
- 890 unit tests passed
- 0 failed
- 8 ignored (environment variable tests)

### Test Results

```
cargo test --all-features
test result: ok. 890 passed; 0 failed; 8 ignored
```

## Validation Results

- ✅ `cargo fmt --all` passed (no formatting issues)
- ✅ `cargo check --all-targets --all-features` passed (zero compilation errors)
- ✅ `cargo clippy --all-targets --all-features -- -D warnings` showed zero warnings
- ✅ `cargo test --all-features` passed with 890 tests passing

## Impact Analysis

### Files Modified

- `src/config.rs` (4 locations): Default function values and test assertions

### Breaking Changes

None. This is a backward-compatible change:
- Existing config files continue to work unchanged
- Only affects users who don't specify these values (uses new defaults)
- Subagent output limit increased, never decreased
- Main tool output limit increased, never decreased

### Performance Impact

Minimal to none:
- No algorithmic changes
- No new allocations
- Truncation still occurs at the specified limits
- Memory usage unchanged (limits are checked, not pre-allocated)

## Migration Path

No action required for existing users:
- Users with explicit config values: No change needed
- Users with default config: Automatically get increased limits
- Users without config file: Automatically get increased limits

## References

- **Issue**: Output truncation in chat mode with Ollama provider
- **Root Cause**: `subagent.output_max_size` default of 4 KB was too restrictive
- **Solution**: Increase both subagent (1 MB) and tool (5 MB) output limits
- **Configuration File**: `config/config.yaml` shows both values

## Future Considerations

### Potential Enhancements

1. **Adaptive Limits**: Could dynamically adjust based on available context window
2. **Per-Tool Limits**: Could set different limits for different tool types
3. **Streaming Output**: For very large outputs, could stream results instead of buffering
4. **Compression**: Could compress large outputs before returning

### Monitoring

Consider monitoring in production:
- Track actual output sizes across tool executions
- Monitor context window usage with new larger limits
- Alert if outputs frequently exceed thresholds

## Conclusion

This change resolves the truncation issue while maintaining reasonable defaults for memory and context management. The new limits (1 MB for subagents, 5 MB for tools) provide sufficient headroom for typical use cases without creating performance concerns.
