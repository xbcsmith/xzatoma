# Plan Extractor Trait Send+Sync Implementation

## Overview

This change fixes thread-safety compile errors and clippy diagnostics that occurred when using `Arc<dyn PlanExtractorTrait>` and passing it across async/thread boundaries (errors like `E0277` and futures not being `Send`). The root cause was that `PlanExtractorTrait` did not require `Send + Sync`, so trait objects like `dyn PlanExtractorTrait` were not guaranteed to be thread-safe.

Resolution:
- Add `Send + Sync` bounds to the trait: `pub trait PlanExtractorTrait: Send + Sync { ... }`
- Add a small compile-time/test assertion ensuring the trait object can be used behind `Arc` (i.e., is `Send + Sync`)
- Verify formatting, compilation, clippy, and tests pass

## Components Delivered

- `src/watcher/plan_extractor.rs` (modified)
  - Trait signature updated to require `Send + Sync`
  - Added `#[test]` asserting the trait object is `Send + Sync`
- `docs/explanation/plan_extractor_trait_send_sync_implementation.md` (this document)

## Implementation Details

Problem:
- The watcher code stores a `PlanExtractorTrait` as `Arc<dyn PlanExtractorTrait>` and uses it from within async handlers and thread-capable contexts. Without requiring `Send + Sync` on the trait, `Arc<dyn PlanExtractorTrait>` may not be `Send + Sync`, resulting in errors like:
  - `error[E0277]: dyn PlanExtractorTrait cannot be shared between threads safely`
  - `future cannot be sent between threads safely` in async handlers

Fix:
- Require implementors of the trait to be thread-safe by changing the trait declaration:

    pub trait PlanExtractorTrait: Send + Sync {
        /// Extract plan YAML/text from a CloudEvent message.
        fn extract(&self, event: &CloudEventMessage) -> Result<String>;
    }

- Add a compile-time test to ensure the trait object is `Send + Sync` when behind common smart pointers:

    #[test]
    fn test_plan_extractor_trait_is_send_sync() {
        // Compile-time assertion that the trait object is Send + Sync.
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<PlanExtractor>();
        _assert_send_sync::<std::sync::Arc<dyn PlanExtractorTrait>>();
    }

Rationale:
- The `MessageHandler` trait and async handler contexts expect `Send + Sync` (or are used in contexts where futures must be `Send`). Requiring the bound on the trait itself is the cleanest and most explicit solution — it documents the expectation for implementors and ensures trait objects can be shared safely.

## Testing

What I ran locally to validate the change:

- `cargo fmt --all`  
  - Result: formatted / no changes required
- `cargo check --all-targets --all-features`  
  - Result: compilation succeeds
- `cargo clippy --all-targets --all-features -- -D warnings`  
  - Result: no warnings (passes with `-D warnings`)
- `cargo test --all-features`  
  - Result: all tests pass (no failing tests)

Notes:
- The fix is small and confined to the trait contract and tests; no behavioral changes in extraction logic were required.
- The added unit test is a compile-time assertion (it will fail to compile if trait is not `Send + Sync`), which makes regressions unlikely.

## Usage Example

Below is a minimal example demonstrating that a `PlanExtractor` instance can now be used as `Arc<dyn PlanExtractorTrait>` and moved across thread boundaries:

    use std::sync::Arc;
    use std::thread;

    // Create the extractor as a trait object behind Arc
    let extractor: Arc<dyn PlanExtractorTrait> = Arc::new(PlanExtractor::new());

    // Clone and move into a thread
    let extractor_clone = Arc::clone(&extractor);
    let handle = thread::spawn(move || {
        // extractor_clone can be used here safely
        // (e.g., extractor_clone.extract(&event).unwrap();)
    });

    handle.join().unwrap();

## References

- Modified file: `src/watcher/plan_extractor.rs`
- Compiler error observed: `E0277` (trait object not `Send/Sync`)
- Context: watcher async message handlers and `MessageHandler` trait requiring `Send + Sync`

## Validation Results

- [x] `cargo fmt --all` (code formatted)
- [x] `cargo check --all-targets --all-features` (compiles)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` (no warnings)
- [x] `cargo test --all-features` (all tests pass)

---

If you want, I can:
- Add a short note in the `CHANGELOG` (if you maintain one) to record this behavioral/contract change, or
- Add an integration test that exercises the watcher message handler with a mocked `PlanExtractorTrait` to ensure it works end-to-end in the async handler context.

Let me know which follow-up you'd prefer and I can prepare it next.
