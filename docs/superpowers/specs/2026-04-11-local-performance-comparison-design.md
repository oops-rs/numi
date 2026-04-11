# Local Performance Comparison Design

## Summary

Run a one-time local comparison between `numi` and SwiftGen to validate whether the current "blazingly fast" positioning is supported by evidence. This is a private measurement workflow only; no benchmark harness or result artifacts need to be committed beyond the design/plan docs created by the workflow.

## Current Facts

- The README currently describes Numi as "blazingly fast".
- The repo already contains internal Criterion benchmarks for Numi in `crates/numi-core/benches/pipeline.rs`.
- The repo does not currently contain a checked-in SwiftGen comparison harness.
- The user wants a one-time local comparison first, not a published or permanent benchmark suite.
- A fair comparison requires both tools to run against the same workload and finish successfully.

## Goal

Produce an evidence-based local answer to this question:

Can we credibly describe `numi` as "blazingly fast" relative to SwiftGen on a comparable workload?

## Non-Goals

- adding committed benchmark code
- publishing benchmark results in the repo
- optimizing performance as part of this step
- proving universal superiority across every project shape

## Options Considered

### Option 1: Ad-hoc single-run timing

Use `time` once for each tool on one command.

Pros:
- fastest path

Cons:
- too noisy
- weak evidence

### Option 2: Controlled local comparison

Use the same workload, the same machine, multiple timed runs, and summarize medians.

Pros:
- good signal quality
- still lightweight
- fits the one-time local request

Cons:
- needs a bit of setup

### Option 3: Full reproducible benchmark project

Build a dedicated harness and preserve scripts/results in the repo.

Pros:
- strongest long-term reproducibility

Cons:
- explicitly out of scope for this request

## Decision

Use Option 2.

This gives us a credible answer with minimal scope and no permanent benchmark maintenance burden.

## Measurement Design

### Workload

Use a workload where both tools can run successfully and where the generated output shape is reasonably comparable.

Preferred order:

1. an existing local migrated project or fixture where both `numi` and SwiftGen can generate from the same resources
2. a repo fixture adapted locally for one-time comparison if that is faster to stand up

The workload should include at least assets and/or localization, since those are core supported surfaces for Numi today.

### Tooling

Preferred timing tool:

1. `hyperfine`, if installed
2. repeated `/usr/bin/time` measurements otherwise

Record:

- `numi` invocation used
- SwiftGen invocation used
- tool versions
- whether `numi` is run from a release build or installed binary

### Scenarios

Measure at least two cases:

1. first successful generate for each tool
2. repeated generate on unchanged inputs

This gives us both a general generation comparison and a practical repeated-run comparison.

### Fairness Rules

- same machine
- same workload inputs
- both commands must succeed
- avoid background changes to the workload between runs
- use release-mode `numi`
- keep output destinations comparable enough that one tool is not doing materially more work than the other

### Interpretation

Treat the result conservatively:

- if `numi` is clearly faster in both scenarios, the "blazingly fast" wording is supported
- if results are mixed, the wording is not yet strongly supported
- if SwiftGen is faster on the tested workload, we should not lean on that claim

## Output

The final output should be a local summary in chat only:

- workload used
- exact commands
- measured timings
- short conclusion on whether the wording is justified
