# AppUI Performance Improvement Design

## Summary

Build a local-only performance runner for the `AppUI` workload, then use it to drive first-run performance improvements in `numi` until cold generate is competitive on that real project shape.

## Current Facts

- A local comparison on `AppUI` showed mixed results:
  - `numi` was much faster on repeated unchanged-input runs
  - SwiftGen was faster on the first successful generate
- The user wants to improve `numi` rather than weaken the positioning claim.
- The user wants the comparison workflow to target `AppUI`, not a smaller fixture.
- The comparison runner does not need to be published as a public benchmark harness.
- The current evidence suggests first-run latency is the main problem, not repeated-run latency.

## Goal

Create a fast local feedback loop for `AppUI` performance work and use it to improve `numi`’s first-run generate time on that workload.

## Non-Goals

- creating a general-purpose public benchmarking framework
- proving universal performance leadership across all project shapes
- changing product scope outside performance-related work
- optimizing repeated-run performance unless a first-run fix regresses it

## Problem Statement

The current “Rust should be faster” intuition is not enough. The observed `AppUI` data says `numi` loses on cold generate and wins on repeated generate. That means the optimization work must be driven by measured first-run costs in the actual code path, not by assumptions about implementation language.

## Options Considered

### Option 1: Optimize directly without a runner

Start changing code immediately based on intuition or small ad-hoc timings.

Pros:
- fastest start

Cons:
- easy to optimize the wrong thing
- hard to prove improvement
- weak regression detection

### Option 2: Build a local `AppUI` comparison runner first, then optimize

Create a local-only script that prepares `AppUI` temp workdirs, runs both tools, and prints comparable timing summaries. Use that script after each optimization pass.

Pros:
- creates a stable before/after measurement loop
- keeps optimization grounded in real workload evidence
- avoids public benchmark maintenance

Cons:
- requires a small upfront investment before optimization begins

### Option 3: Profile first, compare later

Use profiling tools on `numi` first and defer the comparison runner.

Pros:
- may expose hotspots quickly

Cons:
- harder to tell whether fixes actually improve the user-visible comparison
- weaker guardrail for regressions

## Decision

Use Option 2.

The correct sequence is:

1. build the local `AppUI` comparison runner
2. profile and improve first-run generate
3. rerun the same `AppUI` comparison after each change

## Design

### 1. Local `AppUI` Comparison Runner

Create a local-only script that:

- targets the real `AppUI` workload only
- prepares temporary working copies under `/tmp`
- runs `numi` and SwiftGen against equivalent asset and `.strings` inputs
- measures:
  - first successful generate
  - repeated unchanged-input generate
- records:
  - tool versions
  - exact commands
  - raw timings
  - median real time per scenario

This runner should optimize for speed of repeated local use, not for generality.

### 2. Fairness Contract

The runner should keep the comparison honest:

- same source workload for both tools
- same machine
- successful runs only
- similar output surface
- clear disclosure if built-in templates are comparable rather than identical

The runner does not need to hide caveats. It should surface them explicitly.

### 3. Optimization Focus

The first optimization target is cold generate latency on `AppUI`.

Likely cold-path cost centers to investigate:

- config loading
- directory traversal
- resource parsing
- normalization and IR building
- template environment setup
- rendering
- filesystem writes

The work should follow the largest measured bottleneck first rather than broad cleanup.

### 4. Validation Loop

Each optimization pass should use the same loop:

1. run the local `AppUI` comparison runner
2. profile or instrument the slow cold path
3. change the minimum code needed
4. rerun correctness tests
5. rerun the same local `AppUI` comparison

### 5. Success Criteria

This project is successful when all of the following are true:

- the `AppUI` comparison can be rerun locally with one command
- the measurement output is stable enough to compare before/after changes
- `numi`’s first-run `AppUI` performance materially improves
- repeated-run performance does not regress meaningfully

## Testing And Verification

The work needs two kinds of verification:

### Correctness

- existing `cargo test` coverage must stay green

### Performance

- before/after `AppUI` runner output must show the change

Performance claims should be phrased narrowly and only from the measured `AppUI` workload unless later evidence broadens them.
