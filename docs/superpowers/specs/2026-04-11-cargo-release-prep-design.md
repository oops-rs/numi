# Cargo Release Prep Design

## Summary

Prepare Numi for an initial crates.io release that is installable with Cargo and documented for outside users, without expanding product scope or refactoring the current crate structure.

## Goal

Make the current Rust workspace publishable as a coordinated `0.1.x` family on crates.io, with `numi`/`numi-cli` presented as the user-facing install path.

## Current Facts

- The workspace currently contains five crates: `numi-cli`, `numi-core`, `numi-config`, `numi-ir`, and `numi-diagnostics`.
- `numi-cli` depends on the other four crates via local path dependencies.
- All five crates are currently marked `publish = false`.
- `cargo publish --dry-run -p numi-cli` currently fails because the package is not publishable.
- `cargo package -p numi-cli --allow-dirty --no-verify` currently fails because internal path dependencies do not declare versions and the package metadata is incomplete.
- Fresh local verification is green: formatting, clippy, and workspace tests pass.
- The current README is aimed at local developers, not public users.

## Constraints

- The first release target is crates.io-installable and documented.
- Broader release automation is out of scope for this pass.
- Full SwiftGen parity is not required for this release.
- Avoid large architectural refactors solely to change the publishing model.

## Scope

### In Scope

- Publish all workspace crates to satisfy Cargo packaging rules.
- Keep `numi`/`numi-cli` as the only user-facing product in documentation.
- Add the package metadata required for packaging and public consumption.
- Update internal crate dependency declarations so packaging and dry-run publishing succeed.
- Add a project license file.
- Rewrite the README so it explains installation, supported features, current limitations, and the intended migration posture clearly.
- Verify the publish path locally with packaging and dry-run commands.

### Out of Scope

- New parsers, templates, or resource-format support.
- SwiftGen feature-parity work.
- GitHub Actions, release automation, tagging flow, or changelog process.
- Collapsing internal crates into `numi-cli`.

## Approach Options Considered

### Option 1: Publish All Workspace Crates

Publish all five crates as a coordinated release family, but frame only `numi` as the public product.

Why this is recommended:

- It satisfies Cargo's packaging model with the smallest code change.
- It preserves the current architecture.
- It keeps release-prep work focused on packaging and docs instead of refactoring.

### Option 2: Publish Only `numi-cli`

Keep internal crates private by folding them into the CLI package or otherwise removing package boundaries.

Why this is not recommended now:

- It requires structural refactoring unrelated to release readiness.
- It introduces avoidable risk into a release-prep pass.

### Option 3: Ship Source-Only First

Release only through GitHub source installation and defer crates.io.

Why this is rejected:

- It does not meet the selected release target.

## Design

### 1. Packaging Metadata

Update each crate manifest with the metadata expected for a public crate release:

- `description`
- `license` or `license-file`
- `repository`
- `homepage` and `documentation` for `numi-cli`, and at least `repository` plus license metadata for the internal crates
- publishability enabled for the release set

Update inter-crate dependencies from path-only declarations to path-plus-version declarations so Cargo can package and dry-run publish the workspace correctly.

The release should remain version-aligned across all workspace crates for the first launch to reduce coordination complexity.

### 2. Public Documentation

Rewrite the README around public adoption rather than local development.

The README should:

- explain what Numi is in user-facing language
- show the crates.io install command
- document the current supported inputs and shipped built-in templates
- clearly describe current limitations, especially `.xcstrings` variation handling
- explain the migration posture relative to SwiftGen without overstating parity
- keep advanced internal/developer notes secondary

The docs should also resolve current drift between implementation and public messaging. If fonts are supported today only through custom-template workflows and not shipped built-ins, that distinction should be made explicit rather than omitted.

### 3. Verification

Release readiness claims must be backed by fresh local evidence. The release-prep pass should verify:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo package` for the publishable crates
- `cargo publish --dry-run` for the release set, or at minimum the end-user crate plus any required internal crates in dependency order

## User-Facing Positioning

The public release should describe Numi as an early but usable Cargo-installable code generator for Apple project resources, not as a full SwiftGen replacement.

The release should be explicit about the current supported surface:

- `.xcassets`
- `.strings`
- `.xcstrings` plain-string support with warnings for unsupported variations
- `files`
- existing template-driven workflows

If fonts remain custom-template-oriented rather than backed by a shipped built-in template, that should be documented carefully to avoid overpromising.

## Risks and Mitigations

### Risk: Cargo packaging still fails after metadata updates

Mitigation:

- Validate with `cargo package` before any release claim.
- Fix missing version constraints or package metadata iteratively until packaging is clean.

### Risk: Public docs overstate support compared with SwiftGen

Mitigation:

- Keep feature statements narrow and evidence-backed.
- Explicitly call out current limitations and non-goals.

### Risk: Internal crates create public API expectations unintentionally

Mitigation:

- Document `numi` as the intended entrypoint.
- Keep crate descriptions for internal crates minimal and accurate.

## Success Criteria

The release-prep work is complete when:

- all workspace crates are packageable for crates.io
- dry-run publishing succeeds for the release set
- the README supports a public Cargo install flow
- public docs accurately describe current support and limitations
- all existing Rust verification gates remain green
