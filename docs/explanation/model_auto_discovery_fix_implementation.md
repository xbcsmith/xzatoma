# Model Auto-Discovery Fix

## Problem

Running `xzatoma chat` with an OpenAI provider (or any provider) that had no
model explicitly configured produced an immediate startup error:

```text
Error: Config("provider.openai.model must be set for execution")
```

The error appeared before the agent loop started, meaning the user had no
opportunity to benefit from the provider's model-listing API, which already
existed and was designed to handle exactly this case.

## Root Cause

`Config::validate_for_execution` contained a model-emptiness check:

```rust
let model_is_empty = match self.provider.provider_type.as_str() {
    "copilot" => self.provider.copilot.model.trim().is_empty(),
    "ollama"  => self.provider.ollama.model.trim().is_empty(),
    "openai"  => self.provider.openai.model.trim().is_empty(),
    _         => false,
};
if model_is_empty {
    return Err(XzatomaError::Config(format!(
        "provider.{}.model must be set for execution",
        self.provider.provider_type
    )));
}
```

This check ran before the provider factory was invoked, preventing
`resolve_effective_model` (in `src/providers/factory.rs`) from ever querying the
provider's model list.

`resolve_effective_model` already handled empty models correctly:

- Empty `configured` + API returns models: picks the latest model by timestamp.
- Empty `configured` + API returns no models: returns a clear error.
- Empty `configured` + API unreachable (transient): returns a clear error.
- Empty `configured` + API missing (404/405/501): returns a hard error.

The upfront check in `validate_for_execution` was therefore both redundant and
harmful: it blocked the auto-discovery path that the factory provided.

## Fix

Removed the model-emptiness check from `Config::validate_for_execution`. The
function now only verifies that `provider.provider_type` is non-empty, which is
the minimum required to construct a provider instance at all.

The factory's `resolve_effective_model` is solely responsible for model
selection, whether configured or auto-discovered.

### Files Changed

- `src/config.rs`
  - `validate_for_execution`: removed model check, updated doc comment and
    inline example.
  - `test_validate_for_execution_fails_with_empty_model`: renamed to
    `test_validate_for_execution_passes_with_empty_model` and asserted
    `is_ok()`, reflecting the new contract.

## Behavior After Fix

| Scenario                                          | Before           | After                        |
| ------------------------------------------------- | ---------------- | ---------------------------- |
| Provider configured, model configured             | OK               | OK                           |
| Provider configured, model empty, API reachable   | Error at startup | Auto-selects latest model    |
| Provider configured, model empty, API unreachable | Error at startup | Clear error from factory     |
| Provider type empty                               | Error at startup | Error at startup (unchanged) |

All three providers (copilot, ollama, openai) default to an empty model string,
so this fix enables zero-config usage for any provider whose API is reachable.
