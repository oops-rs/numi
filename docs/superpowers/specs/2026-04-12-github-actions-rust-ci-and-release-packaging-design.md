# GitHub Actions Rust CI And Release Packaging Design

## Goal

Add GitHub Actions workflows that:

- run Rust CI for normal development changes
- package release binaries for GitHub Releases
- keep crates.io publishing manual

## Facts

- The repository currently has no `.github/workflows/` directory.
- The verified local quality gates for this repository are:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- The end-user binary is `numi`.
- The current release documentation already separates crates.io publication order from local preflight checks in `docs/crates-io-release.md`.
- The user wants GitHub Releases to include packaged binaries, but wants crates.io publication to remain manual.

## Constraints

- The CI workflow should be simple and predictable.
- The release workflow should attach built binaries to the GitHub Release and should not publish crates to crates.io.
- The release workflow should target these binary platforms:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- The release workflow should produce archives that are easy to download and install.
- The implementation should avoid unnecessary release automation complexity such as changelog generation, signing, or Homebrew integration in this pass.

## Non-Goals

- Automating crates.io publication.
- Adding release signing, notarization, SBOM generation, provenance attestations, or checksum files.
- Supporting extra targets beyond the four approved ones.
- Refactoring the repository layout or Cargo manifests for distribution features.

## Recommended Approach

Add two workflows:

### 1. Rust CI

Create a workflow triggered by `push` and `pull_request` that:

- checks out the repository
- installs stable Rust
- optionally caches Cargo artifacts
- runs:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`

This should run on Ubuntu because these checks are platform-independent for this repository’s current scope and Ubuntu is the lowest-friction runner for standard Rust CI.

### 2. Release Binary Packaging

Create a workflow triggered by `release.published` that:

- builds `numi` in release mode for the approved targets
- packages the built binary into one archive per target
- uploads those archives to the triggering GitHub Release

Packaging shape:

- `.tar.gz` for Linux and macOS targets
- `.zip` for Windows

Archive naming:

- `numi-v<release-tag>-<target>.tar.gz`
- `numi-v<release-tag>-<target>.zip`

The workflow should derive the version string from the GitHub Release tag so the artifact names line up with the public release record.

## Alternatives Considered

### 1. Run CI on a multi-OS matrix

Rejected for this pass. It increases cost and maintenance without being required for the user’s stated goal. The main need is a reliable Rust correctness gate, not broad OS compatibility testing yet.

### 2. Publish to crates.io from the release workflow

Rejected by requirement. The user wants crates.io publication to stay manual.

### 3. Use a third-party release automation tool

Rejected for now. A direct GitHub Actions workflow is simpler, easier to audit, and sufficient for the current needs.

## Planned Changes

### Workflow Files

- Add `.github/workflows/rust-ci.yml`
- Add `.github/workflows/release-binaries.yml`

### Documentation

- Update `docs/crates-io-release.md` with a short section that clarifies:
  - GitHub Releases attach packaged binaries automatically
  - crates.io publication remains manual

## Verification

After implementation:

- validate workflow YAML locally by reading it carefully for syntax and action compatibility
- run the normal repository verification:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- confirm the new workflow files are present and readable
- confirm the release doc clearly states the split between GitHub binary packaging and manual crates.io publication

## Risks

- Cross-target release builds can fail if target-specific toolchain assumptions are wrong in Actions.
- A release workflow that packages binaries but is not exercised until the first published GitHub Release has some latent risk.

## Mitigation

- Keep the release workflow straightforward and explicit.
- Use widely adopted GitHub Actions and standard Rust target installation.
- Keep the first version focused on building and uploading archives only, with no additional release logic.
