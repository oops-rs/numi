# Broad Maintainability Refactor Design

## Summary

Refactor the Numi workspace into smaller, responsibility-focused modules so the codebase is easier to understand, extend, and test.

The primary targets are large files that currently combine unrelated concerns and large inline test suites, especially:

- `crates/numi-core/src/pipeline.rs`
- `crates/numi-config/src/lib.rs`
- `crates/numi-cli/src/lib.rs`

The refactor should preserve user-facing behavior where practical, but API reshaping across crates is allowed when it produces a materially cleaner and more robust architecture.

## Current Facts

- The workspace currently contains five crates: `numi-cli`, `numi-core`, `numi-config`, `numi-ir`, and `numi-diagnostics`.
- The largest source files are `crates/numi-core/src/pipeline.rs` at about 3.4k lines and `crates/numi-config/src/lib.rs` at about 2.7k lines.
- `crates/numi-cli/src/lib.rs` is also large enough to be carrying multiple concerns at once.
- `numi-cli` already uses crate-level integration tests under `crates/numi-cli/tests`.
- `numi-core`, `numi-config`, and some smaller crates still keep a large amount of behavior-heavy test code inline inside source files.
- `numi-core::pipeline` currently mixes public API types, generation orchestration, context construction, input loading, parse-cache handling, generation fingerprinting, hook execution, rendering setup, utility helpers, and a large inline test module.
- `numi-config::lib` currently mixes manifest loading, TOML sniffing, parsing, legacy-syntax detection, job selection, workspace member config resolution, path rebasing helpers, and a large inline test module.
- `numi-cli::lib` currently mixes command dispatch, manifest discovery, workspace execution, command-specific reporting, terminal formatting, and support helpers.

## Goals

- reduce the number of long, mixed-responsibility Rust files
- move the workspace toward clear domain boundaries within each crate
- make future feature work land in an obvious module instead of extending monolithic files
- keep tests close to the right abstraction level instead of burying large suites inside implementation files
- improve robustness by replacing implicit coupling with explicit module boundaries and smaller internal APIs
- leave the workspace easier to navigate for contributors who did not write the original code

## Non-Goals

- do not add new end-user features as part of this refactor
- do not split the existing workspace into many new crates unless a boundary problem proves impossible to solve inside the current crate layout
- do not rewrite stable behavior merely because the current code can be expressed differently
- do not chase unrelated cleanup in files that are not materially involved in the maintainability problems

## Options Considered

### Option 1: Conservative decomposition

Keep crate boundaries and most public APIs as-is, and only split the largest files into smaller internal modules.

Pros:
- lowest behavior risk
- smallest diff
- straightforward to review

Cons:
- leaves several awkward APIs and duplicated helpers in place
- improves file size more than architecture

### Option 2: Domain-oriented refactor

Keep the current workspace layout, but reorganize each crate around responsibilities and allow targeted public API cleanup where it clarifies ownership.

Pros:
- significantly improves maintainability without exploding workspace complexity
- creates clearer extension points for future inputs, manifest features, and CLI commands
- lets tests move to the right level as part of the same redesign

Cons:
- broader churn across the workspace
- requires disciplined staging to avoid accidental regressions

### Option 3: Aggressive architecture rewrite

Use the refactor as a reason to introduce new crates and substantially reshape cross-crate APIs.

Pros:
- maximum long-term freedom
- could produce the cleanest theoretical architecture

Cons:
- highest review and regression risk
- more likely to mix cleanup with redesign
- unnecessary unless current crate boundaries are fundamentally wrong

## Decision

Use Option 2.

The workspace already has sensible top-level crate boundaries. The maintainability problem is mainly that several crates have grown monolithic implementation files and large inline test modules. A domain-oriented refactor within the existing workspace gives the biggest practical win with lower risk than a full architecture rewrite.

## Design

### Cross-cutting principles

- Each file should have one dominant reason to change.
- Public entrypoints should stay easy to find.
- Private helpers should live with the subsystem that owns them.
- Large behavior tests should target public workflows or subsystem boundaries rather than private implementation details.
- Extraction should happen before deep simplification so tests can confirm the structure change did not alter behavior.

### `numi-core`

`numi-core` should stop using `pipeline.rs` as a catch-all implementation file.

Recommended structure:

- `src/pipeline/mod.rs`: shared exports and module wiring
- `src/pipeline/api.rs`: public reports, options, and error types
- `src/pipeline/generate.rs`: generation entrypoints and job execution flow
- `src/pipeline/check.rs`: check entrypoints and stale-output detection flow
- `src/pipeline/context.rs`: context-building orchestration and module assembly
- `src/pipeline/inputs.rs`: input parsing dispatch, parse-cache coordination, and related helpers
- `src/pipeline/fingerprint.rs`: generation fingerprint computation and dependency enumeration
- `src/pipeline/hooks.rs`: hook environment setup and hook execution
- `src/pipeline/sort.rs`: resource ordering helpers currently embedded in the pipeline
- `src/pipeline/tests/`: extracted tests organized by behavior area

This keeps `numi-core` centered on generation orchestration while making new inputs, hook rules, or caching behaviors easier to extend without reopening a giant file.

Adjacent files in `numi-core` should also be cleaned where beneficial:

- move heavy inline tests into sibling `tests.rs` modules or `src/<module>/tests.rs`
- extract private helper clusters when a file is still carrying more than one responsibility after test removal

### `numi-config`

`numi-config` should stop using `lib.rs` as the implementation home for nearly the entire crate.

Recommended structure:

- `src/lib.rs`: crate exports only
- `src/error.rs`: config and manifest error types
- `src/load.rs`: file loading entrypoints
- `src/manifest.rs`: `Manifest`, `LoadedManifest`, and related public manifest-level types
- `src/parse.rs`: config parsing entrypoints and legacy-syntax detection
- `src/sniff.rs`: manifest-kind sniffing and lossy TOML shape detection
- `src/resolve.rs`: selected-job resolution and config default materialization
- `src/workspace_merge.rs`: workspace-member config resolution and rebasing helpers
- existing `workspace.rs`, `validate.rs`, `model.rs`, and `discovery.rs` should either remain focused or be trimmed if responsibilities move out
- `src/tests/` or sibling module tests organized by parsing, sniffing, resolution, and workspace merge behavior

This separation gives future manifest features a clearer home and reduces the current coupling between parsing, classification, and workspace resolution.

### `numi-cli`

`numi-cli` should be reorganized so `src/lib.rs` becomes an entrypoint rather than the place where most command behavior lives.

Recommended structure:

- `src/lib.rs`: high-level `run` entrypoint and module exports
- `src/commands/`: one module per top-level command group or execution path
- `src/manifest.rs`: manifest-loading and workspace/single-config resolution for CLI use
- `src/ui.rs` and, when the code volume warrants it, focused submodules for status lines, summaries, warnings, and display helpers
- `src/error.rs` if `CliError` and related formatting grow beyond trivial size
- keep `src/cli.rs` focused on Clap definitions rather than execution details

This makes it easier to add or change commands without repeatedly reopening the same long file.

### `numi-ir`

`numi-ir` is comparatively small, so the refactor should stay light:

- keep the crate small unless a real new boundary emerges
- move tests into sibling modules where that reduces noise
- leave the crate simple if the current structure is already easy to maintain

### `numi-diagnostics`

`numi-diagnostics` is already small and focused. Only make lightweight improvements:

- keep diagnostics types compact
- extract tests only if they start obscuring the main implementation
- avoid unnecessary abstraction

## Testing Strategy

Use a hybrid test placement rule:

- keep small, pure, local unit tests near the implementation they protect
- move large `mod tests` blocks out of large source files into sibling test modules
- prefer crate `tests/` integration suites for public workflows, cross-module behavior, and command-level behavior

Examples:

- `numi-core` pipeline workflow tests should move toward subsystem or integration-level suites
- `numi-config` manifest parsing and workspace resolution tests can be grouped by behavior area instead of living in one giant inline block
- `numi-cli` should continue to use crate-level integration tests for user-facing behavior

## Refactor Rules

To keep the large sweep safe, apply these rules throughout implementation:

1. Move code first, simplify second.
2. Keep edits behavior-preserving until a subsystem has clear boundaries and passing tests.
3. Only reshape public APIs when the new boundary is clearer than the old one.
4. Avoid introducing a utility module that becomes a new dumping ground.
5. Prefer names that describe responsibility rather than technical mechanism.

## Verification

The refactor should be validated in stages, not only at the end.

Minimum verification:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

During implementation, targeted test runs for the crate or subsystem being moved should be used before full-workspace verification.

## Risks

### Risk: structural churn hides regressions

Large file moves can make logic changes harder to spot.

Mitigation:

- stage extraction before simplification
- keep tests passing after each major subsystem move

### Risk: over-normalizing small crates

Applying the same decomposition pattern everywhere could make small crates worse.

Mitigation:

- use light cleanup in `numi-ir` and `numi-diagnostics`
- only split where there is real mixed responsibility

### Risk: test relocation weakens coverage

Moving tests out of implementation files can accidentally drop scenarios or make them harder to run.

Mitigation:

- move tests with explicit behavior grouping
- preserve or improve scenario coverage during relocation

### Risk: new module boundaries become artificial

If modules are split by file length instead of ownership, maintenance improves only superficially.

Mitigation:

- derive module boundaries from responsibilities already visible in the current code
- keep each module’s public surface narrow

## Recommendation

Refactor broadly across the existing workspace with `numi-core`, `numi-config`, and `numi-cli` as the primary targets, and use smaller, responsibility-focused modules plus a hybrid test strategy to improve extensibility, maintainability, and robustness without turning the effort into a full crate-architecture rewrite.
