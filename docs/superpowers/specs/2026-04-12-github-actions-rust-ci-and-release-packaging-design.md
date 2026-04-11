# GitHub Actions Rust CI, Release Packaging, And Homebrew Design

## Goal

Add GitHub Actions workflows and Homebrew tap support that:

- run Rust CI for normal development changes
- package release binaries for GitHub Releases
- update the Homebrew tap for tagged releases
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
- The user also wants Homebrew updates automated through `/Users/wendell/developer/oops-rs/homebrew-tap`.
- `langcodec` already uses a workable pattern for this:
  - `.github/workflows/release.yml` builds cross-target release archives and uploads them to GitHub Releases.
  - `homebrew-tap/scripts/update-formula.sh` updates a formula from a tagged GitHub source tarball.
  - its Homebrew formula installs from source with Cargo instead of consuming attached release binaries.

## Constraints

- The CI workflow should be simple and predictable.
- The release workflow should attach built binaries to the GitHub Release and should not publish crates to crates.io.
- The release workflow should target these binary platforms:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-apple-darwin`
  - `aarch64-apple-darwin`
  - `x86_64-pc-windows-msvc`
- The release workflow should produce archives that are easy to download and install.
- The Homebrew path should stay simple and should not require bottle generation in this pass.
- The tap update should be gated by a GitHub secret token and should no-op cleanly when that token is absent.
- The implementation should avoid unnecessary release automation complexity such as changelog generation, signing, or Homebrew integration in this pass.

## Non-Goals

- Automating crates.io publication.
- Adding release signing, notarization, SBOM generation, provenance attestations, or checksum files.
- Supporting extra targets beyond the four approved ones.
- Refactoring the repository layout or Cargo manifests for distribution features.
- Adding Homebrew bottles or binary-install formulas.

## Recommended Approach

Add two workflows and one tap update path:

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

### 2. Release Binary Packaging And Homebrew Update

Create a workflow triggered by `release.published` that:

- builds `numi` in release mode for the approved targets
- packages the built binary into one archive per target
- uploads those archives to the triggering GitHub Release
- optionally updates the Homebrew tap after the release assets are published

Packaging shape:

- `.tar.gz` for Linux and macOS targets
- `.zip` for Windows

Archive naming:

- `numi-v<release-tag>-<target>.tar.gz`
- `numi-v<release-tag>-<target>.zip`

The workflow should derive the version string from the GitHub Release tag so the artifact names line up with the public release record.

Homebrew behavior:

- The workflow should check out `oops-rs/homebrew-tap` using a `HOMEBREW_TAP_TOKEN` secret.
- The workflow should run `scripts/update-formula.sh numi <tag>` in the tap repository.
- The workflow should commit and push the formula update when the tap contents changed.
- When the token is absent, the workflow should skip the tap update without failing the release packaging job.

### Homebrew Formula Strategy

Follow `langcodec`’s model rather than building bottles from the GitHub release binaries:

- Add `Formula/numi.rb` to the tap.
- Point the formula at `https://github.com/oops-rs/numi/archive/refs/tags/<tag>.tar.gz`.
- Build with Cargo from source inside Homebrew:
  - `system "cargo", "install", "--locked", *std_cargo_args(path: "crates/numi-cli")`

This is simpler than binary/bottle distribution and matches the current tap conventions.

## Alternatives Considered

### 1. Run CI on a multi-OS matrix

Rejected for this pass. It increases cost and maintenance without being required for the user’s stated goal. The main need is a reliable Rust correctness gate, not broad OS compatibility testing yet.

### 2. Publish to crates.io from the release workflow

Rejected by requirement. The user wants crates.io publication to stay manual.

### 3. Use attached GitHub release binaries as the Homebrew install source

Rejected for now. That would push the tap toward bottle-style or custom binary-install logic, add more checksum and platform management, and depart from the existing `homebrew-tap` pattern already used by `langcodec`.

### 4. Use a third-party release automation tool

Rejected for now. A direct GitHub Actions workflow is simpler, easier to audit, and sufficient for the current needs.

## Planned Changes

### Workflow Files

- Add `.github/workflows/rust-ci.yml`
- Add `.github/workflows/release.yml`

### Documentation

- Update `docs/crates-io-release.md` with a short section that clarifies:
  - GitHub Releases attach packaged binaries automatically
  - Homebrew tap updates automatically from tagged source releases when the tap token is configured
  - crates.io publication remains manual

### Homebrew Tap

- Add `/Users/wendell/developer/oops-rs/homebrew-tap/Formula/numi.rb`
- Update `/Users/wendell/developer/oops-rs/homebrew-tap/scripts/update-formula.sh` to support `numi`

## Verification

After implementation:

- validate workflow YAML locally by reading it carefully for syntax and action compatibility
- validate the new Homebrew formula shape against the existing tap conventions
- run the normal repository verification:
  - `cargo fmt --all --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- confirm the new workflow files are present and readable
- confirm the release doc clearly states the split between GitHub binary packaging and manual crates.io publication
- confirm the tap update script can render a `numi` formula from a tag without disturbing existing formulas

## Risks

- Cross-target release builds can fail if target-specific toolchain assumptions are wrong in Actions.
- A release workflow that packages binaries but is not exercised until the first published GitHub Release has some latent risk.
- The Homebrew formula can drift from the repository packaging model if the install path or crate layout changes later.
- The tap update job can fail because of missing token permissions or an unexpected formula-generation edge case.

## Mitigation

- Keep the release workflow straightforward and explicit.
- Use widely adopted GitHub Actions and standard Rust target installation.
- Keep the first version focused on building and uploading archives only, with no additional release logic.
- Reuse the existing `homebrew-tap` update script pattern instead of inventing a new tap workflow.
- Keep the `numi` formula source-based and aligned with the repository’s actual Cargo entrypoint.
