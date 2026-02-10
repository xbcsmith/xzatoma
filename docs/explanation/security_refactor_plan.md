# Security And Refactor Implementation Plan

## Overview

This plan addresses the top five priority issues from the analysis: terminal execution hardening, SSRF validation, runtime panic removal, error handling consistency, and unused exports. The approach is phased to reduce risk, improve safety, and align error boundaries while keeping the codebase simple and compliant with project standards.

## Current State Analysis

### Existing Infrastructure

The agent executes tools via structured tool calls and includes file path validation, provider abstractions, and a dedicated terminal tool with safety modes. Fetch tooling includes SSRF checks, and error handling is split between crate errors and anyhow-based results.

### Identified Issues

1. Terminal tool executes shell commands with limited parsing safeguards, enabling injection risks in permissive modes; see [src/tools/terminal.rs](src/tools/terminal.rs#L243) and [src/tools/terminal.rs](src/tools/terminal.rs#L434).
2. SSRF validation does not resolve hostnames to IPs, leaving DNS rebinding and private-range access gaps; see [src/tools/fetch.rs](src/tools/fetch.rs#L139).
3. Runtime code uses unwrap on poisoned mutex locks in quota tracking; see [src/agent/quota.rs](src/agent/quota.rs#L152).
4. Error handling mixes anyhow and crate error types and drops context by stringifying errors; see [src/main.rs](src/main.rs#L6) and [src/storage/mod.rs](src/storage/mod.rs#L37).
5. Multiple exports appear unused within the crate and tests; see [src/tools/grep.rs](src/tools/grep.rs#L92) and [src/tools/plan_format.rs](src/tools/plan_format.rs#L17).

## Implementation Phases

### Phase 1: Core Implementation

#### Task 1.1 Foundation Work

Define a terminal execution policy that avoids shell invocation and limits command surfaces, with a clear error boundary using crate error types.

#### Task 1.2 Add Foundation Functionality

Implement a structured command runner for terminal tools and add hostname resolution to SSRF validation in fetch tooling.

#### Task 1.3 Integrate Foundation Work

Wire the new terminal execution path into tool execution and enforce updated SSRF checks in fetch, preserving existing safety mode behaviors.

#### Task 1.4 Testing Requirements

Add unit tests for command parsing and SSRF resolution, plus regression tests for previous safe-path behaviors.

#### Task 1.5 Deliverables

Updated terminal tool execution path, SSRF validation with DNS resolution, and tests covering new behavior.

#### Task 1.6 Success Criteria

Terminal tool execution no longer relies on shell parsing, SSRF checks block private-range resolutions, and all tests pass with required coverage.

### Phase 2: Feature Implementation

#### Task 2.1 Feature Work

Replace runtime unwraps in quota tracking with recoverable errors and consistent propagation using crate error types.

#### Task 2.2 Integrate Feature

Standardize error boundaries across binaries and internal modules, and preserve error context using structured error variants.

#### Task 2.3 Configuration Updates

Add any required configuration flags or safety defaults for the terminal tool and SSRF enforcement, ensuring configuration sources remain consistent.

#### Task 2.4 Testing Requirements

Add error handling tests for quota paths and updated error conversions, plus tests for configuration defaults.

#### Task 2.5 Deliverables

Quota runtime paths free of unwrap, consistent error boundary usage, and documented behavior for error propagation.

#### Task 2.6 Success Criteria

No runtime unwraps in quota tracking, consistent error types at boundaries, and passing tests with required coverage.

### Phase 3: Cleanup And Consolidation

#### Task 3.1 Foundation Work

Audit unused exports and adjust visibility or remove unused items with a clear public API boundary.

#### Task 3.2 Add Foundation Functionality

Consolidate duplicate test helpers into shared utilities in existing test utility modules.

#### Task 3.3 Integrate Foundation Work

Update tests and modules to use shared helpers and remove duplicated setups.

#### Task 3.4 Testing Requirements

Run affected unit and integration tests to confirm no behavioral regressions.

#### Task 3.5 Deliverables

Reduced unused exports, shared test utilities, and updated tests referencing consolidated helpers.

#### Task 3.6 Success Criteria

No unused exports within crate scope, fewer duplicate test setups, and stable test outcomes.
