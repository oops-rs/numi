# Open-Source Readiness Blockers Design

## Goal

Remove the hard blockers that would make the first public `numi` release fail or publish incomplete crates.

## Facts

- `numi-core` embeds built-in templates at compile time with `include_str!`, and those files currently live outside the crate root in the repository-level `templates/` directory.
- `numi-cli` embeds the starter manifest at compile time with `include_str!`, and that file currently lives outside the crate root in `docs/examples/`.
- `cargo package --list -p numi-core` showed only crate-local files and did not list the repository-level templates.
- `cargo package --list -p numi` showed only crate-local files plus `README.md` and did not list `docs/examples/starter-numi.toml`.
- `cargo package -p numi-diagnostics --allow-dirty` and `cargo publish --dry-run -p numi-diagnostics --allow-dirty` both succeed, so the Cargo manifest shape is valid for at least one leaf crate.
- `cargo package -p numi --allow-dirty` currently fails because internal workspace crates are not yet on crates.io. That is expected for the first multi-crate publish and is documented already.

## Constraints

- A published crate must be self-contained relative to the files Cargo includes in that crate archive.
- We should not change runtime behavior, template semantics, or the public CLI contract in this pass.
- We should preserve the existing top-level developer-facing template and starter-config files unless there is a clear reason to remove them.
- Verification must rely on fresh package and test commands, not assumptions from local builds.

## Non-Goals

- Adding contributor/community files such as `CONTRIBUTING.md`, `SECURITY.md`, or CI workflows.
- Reworking crate metadata, docs.rs content, or release automation.
- Changing template output or adding new built-in templates.

## Recommended Approach

Make each publishing crate own the compile-time assets it embeds:

- Copy the built-in template sources into a crate-local directory under `crates/numi-core/`.
- Copy the starter `numi.toml` into a crate-local directory under `crates/numi-cli/`.
- Update `include_str!` call sites to reference only crate-local files.
- Add regression coverage that asserts the crate source references remain crate-local, so future moves back outside the crate root are caught in tests.

This is the simplest design that satisfies Cargo's packaging model. It removes the mismatch between compile-time file lookups and publish-time crate boundaries.

## Alternatives Considered

### 1. Keep shared top-level files and rely on package include rules

Rejected. The embedded files live above the crate roots, so even if inclusion could be made to work, the ownership model would remain fragile and easy to break.

### 2. Generate embedded assets into Rust source at build time

Rejected. This adds a build-step and extra moving parts for no benefit on this narrow blocker-fix pass.

## Planned Changes

### `numi-core`

- Add a crate-local directory for built-in template assets.
- Copy the current Swift and Objective-C built-in templates into that directory without changing contents.
- Update `src/render.rs` to embed the crate-local copies.

### `numi-cli`

- Add a crate-local directory for starter config assets.
- Copy the current starter config into that directory without changing contents.
- Update `src/lib.rs` to embed the crate-local copy.

### Tests

- Add a regression test in `crates/numi-core` that inspects `src/render.rs` and asserts the embedded paths are crate-local.
- Add a regression test in `crates/numi-cli` that inspects `src/lib.rs` and asserts the starter config embed path is crate-local.

These tests do not prove Cargo archive contents directly, but they protect the invariant that all compile-time embedded files stay under each crate root.

## Verification

Run fresh verification after the code changes:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo package --list -p numi-core`
- `cargo package --list -p numi`
- `cargo package -p numi-diagnostics --allow-dirty`
- `cargo publish --dry-run -p numi-diagnostics --allow-dirty`

Expected outcomes:

- The test and lint suite remains green.
- The package listings for `numi-core` and `numi` now include the crate-local embedded assets they compile against.
- The leaf-crate publish dry-run remains green.

## Risks

- Copying assets can create drift between the repository-level files and the crate-local copies if future edits update only one location.

## Mitigation

- Keep this pass narrowly focused on unblockers.
- In a later pass, decide whether the crate-local files should become the canonical copies or whether shared-source generation is worth introducing.
