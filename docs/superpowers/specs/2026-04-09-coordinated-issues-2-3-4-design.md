# Coordinated Design For Issues #2, #3, And #4

## Goal

Implement GitHub issues `#2`, `#3`, and `#4` on a single coordinated branch, in an order that improves verification and keeps existing single-config behavior stable.

The issues are:

- `#2` Add larger benchmark fixtures for multimodule repositories
- `#3` Add workspace orchestration for monorepos
- `#4` Cache parsed inputs across invocations

## Current Facts

### CLI and config facts

- Numi currently resolves exactly one config for `generate`, `check`, `config locate`, `config print`, and `dump-context`.
- Discovery rules today are:
  - explicit `--config` wins
  - otherwise nearest ancestor `numi.toml`
  - otherwise one unambiguous descendant `numi.toml`
  - otherwise fail as ambiguous
- Relative input, output, and template paths are resolved from the config file directory.

### Pipeline facts

- `generate`, `check`, and `dump-context` all converge on the same context-building path in `numi-core`.
- Parser work happens before normalization, context construction, rendering, output writing, and stale-output checking.
- The current pipeline reparses all configured inputs every invocation.

### Benchmark and fixture facts

- The current benchmark suite measures repeated `generate` on a single copied `xcassets-basic` fixture.
- The existing `multimodule-repo` fixture only proves discovery ambiguity and does not contain realistic resource payloads.
- The spec already calls for benchmark coverage for:
  - a single asset catalog project
  - multi-module repo config discovery
  - mixed assets + l10n generation
  - unchanged re-run performance

## Constraints And Invariants

- Existing single-config behavior must remain stable.
- `generate`, `check`, and `dump-context` must remain correct under any cache hit or miss path.
- Cache invalidation rules must be explicit and testable.
- Workspace orchestration must not silently reinterpret current `numi generate` or `numi check` discovery behavior.
- Benchmark fixtures must be deterministic, committed in-tree, and executed against copied temp directories rather than the repo working tree.

## Recommended Approach

Implement the three issues in this order:

1. `#2` Expand deterministic benchmark fixtures
2. `#4` Add parsed-input caching at the parser boundary
3. `#3` Add explicit workspace orchestration on top of existing single-config execution

This ordering gives us stronger fixtures before we claim performance improvements, and it keeps the highest-risk correctness work (`#4`) below the CLI-orchestration layer introduced by `#3`.

## Alternatives Considered

### Option A: `#3` before `#2`

Pros:

- faster delivery of a visible end-user feature

Cons:

- weaker multimodule verification bed
- less confidence in discovery and execution behavior across representative repository layouts

### Option B: `#4` before `#2`

Pros:

- faster path to parser-cache implementation

Cons:

- benchmark evidence would still rely on too-small fixtures
- repeated-run performance claims would be under-supported

### Option C: One broad mixed implementation without phase boundaries

Pros:

- fewer short-term handoff points

Cons:

- harder to reason about regressions
- harder to attribute performance wins
- greater risk of overlapping changes across CLI, config, and pipeline layers

## Chosen Design

### Phase 1: Issue `#2` deterministic larger fixtures and benchmarks

Expand the benchmark and fixture layer first.

Scope:

- enrich `fixtures/multimodule-repo` with real resource payloads under multiple module-local `numi.toml` roots
- add at least one larger benchmark-oriented fixture that exercises mixed assets and localization data
- update the benchmark harness so benchmark names make scenario coverage clear
- document what each benchmark is measuring

Non-goals:

- do not add workspace orchestration in this phase
- do not change parser behavior in this phase

Acceptance targets:

- benchmark suite includes a multimodule scenario
- fixture setup remains deterministic and copied into temp directories
- docs explain which scenarios the benchmarks represent

### Phase 2: Issue `#4` parser-boundary cache

Add caching only for parser outputs, not for normalized modules, template context, or rendered outputs.

Why this boundary:

- parser output depends on input kind and filesystem state
- normalization depends on job name and collision rules
- rendering depends on template selection, bundle mode, access level, and output shape

Caching above the parser boundary would risk reusing state across different jobs or rendering configurations that should still execute independently.

Cache key requirements:

- input kind
- canonical input path
- cache schema version
- fingerprint of all relevant files for that parser input

Invalidation rules:

- any relevant file content change invalidates
- any relevant file add, remove, or rename invalidates
- cache schema version change invalidates

Correctness rule:

- on cache hit, the system may reuse parsed state only
- normalization, context construction, duplicate-table validation, rendering, output writing, and stale-output checking must still execute normally

Acceptance targets:

- unchanged inputs reuse cached parsed state across `generate`, `check`, and `dump-context`
- changed inputs invalidate correctly
- repeated-run benchmarks show measurable improvement

### Phase 3: Issue `#3` explicit workspace orchestration

Add workspace orchestration as a new, explicit surface, rather than changing how the current commands resolve a single config.

Recommended shape:

- add workspace-specific command entrypoints such as:
  - `numi workspace generate`
  - `numi workspace check`
- back those commands with a dedicated workspace manifest instead of overloading the current `numi.toml` schema

Why explicit workspace commands:

- preserves today’s single-config CLI contract
- avoids making discovery rules ambiguous
- lets the workspace layer compose the existing per-config pipeline instead of replacing it

Initial workspace responsibilities:

- select multiple config roots
- optionally narrow to specific jobs per config
- execute the existing single-config `generate` and `check` flows for each selected config

Initial failure behavior:

- manifest or selection errors fail before execution
- hard execution errors stop the workspace run
- workspace `check` returns stale status if any selected config is stale

Non-goals:

- do not redefine `dump-context` for workspace mode in the first pass
- do not alter single-config discovery semantics

Acceptance targets:

- a workspace-level command or manifest can orchestrate multiple configs
- ambiguous selection and failure rules are documented
- representative multimodule tests pass

## File Responsibility Split

### Fixtures and benchmark layer

Primary files:

- `fixtures/`
- `crates/numi-core/benches/pipeline.rs`
- `README.md`
- `docs/spec.md`

### Parser cache layer

Primary files:

- `crates/numi-core/src/pipeline.rs`
- a new `crates/numi-core/src/*cache*.rs` module
- `crates/numi-core/Cargo.toml`

Potential supporting files if cache serialization requires type derives or helpers:

- `crates/numi-core/src/parse_xcassets.rs`
- `crates/numi-core/src/parse_l10n.rs`
- `crates/numi-core/src/parse_files.rs`
- `crates/numi-ir/src/lib.rs`
- `crates/numi-diagnostics/src/lib.rs`

### Workspace orchestration layer

Primary files:

- `crates/numi-cli/src/cli.rs`
- `crates/numi-cli/src/lib.rs`
- `crates/numi-config/src/model.rs`
- `crates/numi-config/src/lib.rs`
- a new workspace schema or discovery file under `crates/numi-config/src/`

## Testing Strategy

### Phase 1

- benchmark compile check
- benchmark execution on expanded fixtures
- fixture-backed CLI tests proving repeated generate stability still holds
- docs update for benchmark scenario coverage

### Phase 2

- cache hit tests for unchanged repeated runs
- cache invalidation tests for content changes
- cache invalidation tests for file add, remove, and rename
- cross-command correctness tests covering:
  - `generate`
  - `check`
  - `dump-context`
- benchmark comparisons that emphasize unchanged re-run performance

### Phase 3

- CLI help coverage for new workspace commands
- workspace manifest parsing tests
- representative multimodule execution tests
- docs coverage for discovery, selection, and failure behavior

## Risks

### Risk: cache correctness regression

Mitigation:

- keep the cache below normalization and rendering
- make invalidation data explicit and test it directly
- prove identical outputs and context across cache hit and miss paths

### Risk: workspace feature destabilizes current CLI behavior

Mitigation:

- make workspace orchestration explicit through new commands and a separate manifest
- leave `generate`, `check`, and single-config discovery unchanged

### Risk: fixture growth slows regular tests

Mitigation:

- keep larger fixtures targeted to benchmarks and representative integration tests
- keep small existing fixtures for fast functional coverage

## Success Criteria

The coordinated branch is complete when:

- larger deterministic benchmark fixtures exist and are documented
- parsed-input caching improves repeated-run performance without changing correctness
- workspace orchestration can drive multiple configs without changing current single-config semantics
- the branch verifies all three outcomes together on the stronger multimodule fixture bed created in Phase 1
