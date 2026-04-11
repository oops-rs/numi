# Cargo Release Prep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Numi installable from crates.io as a coordinated Rust workspace release and update the repository docs so outside users can install and understand the current supported surface.

**Architecture:** Keep the current five-crate workspace intact and make it publishable rather than refactoring crate boundaries. Treat Cargo packaging as the primary release contract: manifests, license, dependency versions, and README wording all need to line up with what `cargo package` and `cargo publish --dry-run` expect.

**Tech Stack:** Rust workspace, Cargo packaging/publishing, Markdown docs

---

## File Map

### Existing Files To Modify

- `crates/numi-cli/Cargo.toml`
- `crates/numi-core/Cargo.toml`
- `crates/numi-config/Cargo.toml`
- `crates/numi-ir/Cargo.toml`
- `crates/numi-diagnostics/Cargo.toml`
- `README.md`

### New Files To Create

- `LICENSE`

### Verification Commands

- `cargo publish --dry-run -p numi-cli`
- `cargo package -p numi-cli --allow-dirty --no-verify`
- `cargo package -p numi-diagnostics --allow-dirty`
- `cargo package -p numi-ir --allow-dirty`
- `cargo package -p numi-config --allow-dirty`
- `cargo package -p numi-core --allow-dirty`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

### Task 1: Make The Workspace Cargo-Publishable

**Files:**
- Modify: `crates/numi-cli/Cargo.toml`
- Modify: `crates/numi-core/Cargo.toml`
- Modify: `crates/numi-config/Cargo.toml`
- Modify: `crates/numi-ir/Cargo.toml`
- Modify: `crates/numi-diagnostics/Cargo.toml`

- [ ] **Step 1: Reproduce the current packaging failure**

Run: `cargo publish --dry-run -p numi-cli`

Expected: FAIL with the current message that `numi-cli` cannot be published because `package.publish` is `false`.

- [ ] **Step 2: Reproduce the current package-manifest failure**

Run: `cargo package -p numi-cli --allow-dirty --no-verify`

Expected: FAIL with the current messages about missing package metadata and missing version requirements for internal path dependencies.

- [ ] **Step 3: Update `crates/numi-diagnostics/Cargo.toml` with public package metadata**

Replace the file contents with:

```toml
[package]
name = "numi-diagnostics"
version = "0.1.0"
edition = "2024"
description = "Diagnostics types for the Numi resource code generation workspace."
license = "MIT"
repository = "https://github.com/oops-rs/numi"

[dependencies]
serde = { version = "1", features = ["derive"] }
```

- [ ] **Step 4: Update `crates/numi-ir/Cargo.toml` with metadata and versioned internal dependencies**

Replace the file contents with:

```toml
[package]
name = "numi-ir"
version = "0.1.0"
edition = "2024"
description = "Intermediate representation types for the Numi resource code generation workspace."
license = "MIT"
repository = "https://github.com/oops-rs/numi"

[dependencies]
camino = { version = "1", features = ["serde1"] }
numi-diagnostics = { version = "0.1.0", path = "../numi-diagnostics" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 5: Update `crates/numi-config/Cargo.toml` with metadata and versioned internal dependencies**

Replace the file contents with:

```toml
[package]
name = "numi-config"
version = "0.1.0"
edition = "2024"
description = "Config parsing, validation, and discovery for the Numi resource code generation workspace."
license = "MIT"
repository = "https://github.com/oops-rs/numi"

[dependencies]
numi-diagnostics = { version = "0.1.0", path = "../numi-diagnostics" }
serde = { version = "1", features = ["derive"] }
toml = "0.8"
```

- [ ] **Step 6: Update `crates/numi-core/Cargo.toml` with metadata and versioned internal dependencies**

Replace the file contents with:

```toml
[package]
name = "numi-core"
version = "0.1.0"
edition = "2024"
description = "Core parsing, normalization, rendering, and output orchestration for Numi."
license = "MIT"
repository = "https://github.com/oops-rs/numi"

[dependencies]
atomic-write-file = "0.3"
blake3 = "1"
camino = { version = "1", features = ["serde1"] }
minijinja = "2"
numi-config = { version = "0.1.0", path = "../numi-config" }
numi-diagnostics = { version = "0.1.0", path = "../numi-diagnostics" }
numi-ir = { version = "0.1.0", path = "../numi-ir" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
langcodec = "0.11.0"
sha2 = "0.10"
xcassets = "0.1.0"

[dev-dependencies]
criterion = { version = "0.5", default-features = false }

[[bench]]
name = "pipeline"
harness = false
```

- [ ] **Step 7: Update `crates/numi-cli/Cargo.toml` with end-user package metadata and versioned internal dependencies**

Replace the file contents with:

```toml
[package]
name = "numi-cli"
version = "0.1.0"
edition = "2024"
description = "CLI for generating Swift code from Apple project resources."
license = "MIT"
repository = "https://github.com/oops-rs/numi"
homepage = "https://github.com/oops-rs/numi"
documentation = "https://docs.rs/numi-cli"

[[bin]]
name = "numi"
path = "src/main.rs"

[dependencies]
clap = { version = "4", features = ["derive"] }
numi-config = { version = "0.1.0", path = "../numi-config" }
numi-core = { version = "0.1.0", path = "../numi-core" }
toml = "0.8"

[dev-dependencies]
serde_json = "1"
```

- [ ] **Step 8: Run focused packaging checks for the internal crates**

Run:

```bash
cargo package -p numi-diagnostics --allow-dirty
cargo package -p numi-ir --allow-dirty
cargo package -p numi-config --allow-dirty
cargo package -p numi-core --allow-dirty
```

Expected: PASS for all four crates.

- [ ] **Step 9: Run the end-user crate packaging check again**

Run: `cargo package -p numi-cli --allow-dirty --no-verify`

Expected: PASS, with no manifest-level packaging errors about metadata or internal dependency versions.

- [ ] **Step 10: Run the publish dry-run again**

Run: `cargo publish --dry-run -p numi-cli`

Expected: PASS for the CLI package, or if Cargo now blocks on unpublished internal crates on the registry path, fail with a next actionable registry-ordering message instead of manifest-structure errors.

- [ ] **Step 11: Commit the packaging metadata changes**

```bash
git add crates/numi-cli/Cargo.toml crates/numi-core/Cargo.toml crates/numi-config/Cargo.toml crates/numi-ir/Cargo.toml crates/numi-diagnostics/Cargo.toml
git commit -m "chore: prepare workspace crates for cargo publishing"
```

### Task 2: Add The MIT License

**Files:**
- Create: `LICENSE`

- [ ] **Step 1: Add the MIT license text**

Create `LICENSE` with:

```text
MIT License

Copyright (c) 2026 oops-rs

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

- [ ] **Step 2: Verify the license file exists exactly where Cargo and GitHub expect**

Run: `test -f LICENSE`

Expected: PASS with exit code `0`.

- [ ] **Step 3: Commit the license file**

```bash
git add LICENSE
git commit -m "docs: add mit license"
```

### Task 3: Rewrite The README For Public Cargo Installation

**Files:**
- Modify: `README.md`

- [ ] **Step 1: Capture the current README’s public-release mismatch**

Run: `rg -n "local developers|cargo install --path|Today it supports|Current Status" README.md`

Expected: PASS and show the current local-developer framing plus local-path installation wording that needs to change.

- [ ] **Step 2: Replace `README.md` with a public release README**

Replace the file contents with:

```md
# Numi

Numi is a Rust CLI for generating Swift code from Apple project resources.
It is an early, template-driven alternative to SwiftGen for teams that want deterministic code generation and custom template workflows.

## Install

```bash
cargo install numi-cli
```

The installed binary is named `numi`.

## What Numi Supports Today

- `.xcassets` inputs for image and color asset accessors
- `.strings` inputs for localization generation
- `.xcstrings` plain-string inputs for localization generation
- `files` inputs for file-oriented helper generation
- custom Minijinja templates, including `{% include %}` support
- built-in Swift templates for `swiftui-assets`, `l10n`, and `files`

Numi is intentionally narrower than SwiftGen today. The first public release focuses on the currently proven resource types rather than full SwiftGen feature parity.

## Quick Start

Initialize a starter config in your project:

```bash
numi init
```

Generate code:

```bash
numi generate
```

Check whether committed generated files are up to date:

```bash
numi check
```

Workspace orchestration is also available when a repo has multiple `numi.toml` files:

```bash
numi generate --workspace
numi check --workspace
```

## Minimal Config

Numi uses `numi.toml` as its config filename.

```toml
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
swift = "l10n"
```

You can also point localization generation at `.xcstrings`:

```toml
[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
swift = "l10n"
```

The starter config shipped with `numi init` lives in [docs/examples/starter-numi.toml](docs/examples/starter-numi.toml).

## Commands

`numi generate`

- discovers the nearest manifest unless `--config` is passed
- uses the nearest local `numi.toml` first
- runs one config for `[jobs]` manifests and the whole workspace for `[workspace]` manifests
- generates outputs for all named jobs, or only selected jobs when `--job` is repeated
- prints non-fatal warnings to stderr
- may reuse cached parser outputs when inputs are unchanged

`numi check`

- computes what `generate` would write without modifying files
- exits `0` when outputs are current
- exits `2` when outputs are stale
- prints warnings to stderr without turning the run into a failure

`numi dump-context`

- prints the exact JSON context a job template receives
- is the fastest way to debug or author custom templates

`numi config locate`

- prints the resolved config path

`numi config print`

- prints the resolved config with defaults materialized

## Built-In Templates

Numi currently ships these Swift templates:

- `swiftui-assets`
- `l10n`
- `files`

Fonts are supported in the template context and in custom-template workflows, but the first public release does not ship a dedicated built-in Swift template for fonts.

## Current Limitations

- `.xcstrings` plural and device-specific variations are skipped with warnings
- the shipped `l10n` template currently emits simple no-argument accessors even when placeholder metadata is present in template context
- full SwiftGen feature parity is out of scope for this release

## Workspace Manifests

Repos with more than one `numi.toml` can orchestrate them from a repo-level `numi.toml`:

```toml
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"

[workspace.member_overrides.Core]
jobs = ["l10n"]
```

Workspace members are directory roots, not config-file paths. From the repo root, plain `numi generate` and `numi check` use that nearest workspace `numi.toml` automatically. From inside a member directory, add `--workspace` when you want the ancestor workspace instead of the local member manifest.

## Custom Templates

Custom templates use Minijinja:

```toml
[jobs.l10n.template]
path = "Templates/l10n.jinja"
```

Numi supports `{% include %}` from:

- the including template's local directory
- the config-root search path

If the same include path exists in both places, Numi errors instead of guessing.

Start custom-template work with:

```bash
numi dump-context --job l10n
```

The stable context contract is documented in [docs/context-schema.md](docs/context-schema.md).

## Migration Notes

If you are migrating from SwiftGen, start with [docs/migration-from-swiftgen.md](docs/migration-from-swiftgen.md). Numi is closest to SwiftGen in the asset, localization, and template-driven generation workflows covered in this repository today.

## Development

Useful local commands:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
```

- [ ] **Step 3: Verify the README now presents a crates.io install path**

Run: `rg -n "cargo install numi-cli|The installed binary is named \`numi\`|first public release" README.md`

Expected: PASS and show those public-release strings in the rewritten README.

- [ ] **Step 4: Commit the README rewrite**

```bash
git add README.md
git commit -m "docs: rewrite readme for cargo release"
```

### Task 4: Run Full Release Verification

**Files:**
- Modify: `crates/numi-cli/Cargo.toml` if `documentation`, `homepage`, or description strings need final correction
- Modify: `crates/numi-core/Cargo.toml` if packaging feedback requires metadata fixes
- Modify: `crates/numi-config/Cargo.toml` if packaging feedback requires metadata fixes
- Modify: `crates/numi-ir/Cargo.toml` if packaging feedback requires metadata fixes
- Modify: `crates/numi-diagnostics/Cargo.toml` if packaging feedback requires metadata fixes
- Modify: `README.md` if command/package naming needs correction after verification

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all --check`

Expected: PASS.

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 3: Run the full test suite**

Run: `cargo test --workspace`

Expected: PASS.

- [ ] **Step 4: Run the full packaging set**

Run:

```bash
cargo package -p numi-diagnostics --allow-dirty
cargo package -p numi-ir --allow-dirty
cargo package -p numi-config --allow-dirty
cargo package -p numi-core --allow-dirty
cargo package -p numi-cli --allow-dirty
```

Expected: PASS for all five crates.

- [ ] **Step 5: Run dry-run publishing for the crates in dependency order**

Run:

```bash
cargo publish --dry-run -p numi-diagnostics
cargo publish --dry-run -p numi-ir
cargo publish --dry-run -p numi-config
cargo publish --dry-run -p numi-core
cargo publish --dry-run -p numi-cli
```

Expected: PASS for each crate. If the dry-run reveals new metadata or packaging issues, fix those exact manifest problems before rerunning the full set.

- [ ] **Step 6: Commit any final metadata or docs corrections from verification**

```bash
git add crates/numi-cli/Cargo.toml crates/numi-core/Cargo.toml crates/numi-config/Cargo.toml crates/numi-ir/Cargo.toml crates/numi-diagnostics/Cargo.toml README.md LICENSE
git commit -m "chore: finalize cargo release verification"
```

## Self-Review

### Spec Coverage

- Packaging metadata and versioned internal dependencies are covered in Task 1.
- MIT license adoption is covered in Task 2.
- Public Cargo-facing documentation is covered in Task 3.
- Fresh release verification is covered in Task 4.

### Placeholder Scan

- No `TODO`, `TBD`, or deferred “fill this in later” language remains.
- Commands and file contents are concrete.

### Type Consistency

- The plan uses the existing crate names consistently: `numi-cli`, `numi-core`, `numi-config`, `numi-ir`, `numi-diagnostics`.
- The public binary name stays `numi`.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-04-11-cargo-release-prep.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**
