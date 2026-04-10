# Numi

Numi is a Rust CLI for generating code from structured project resources.
Today it ships Swift templates for the Apple ecosystem.

Today it supports:

- `.xcassets` inputs
- `.strings` inputs
- `.xcstrings` inputs
- `type = "files"` inputs for file-oriented helpers
- shipped Swift templates for SwiftUI assets, localization, and file helpers
- custom Minijinja templates, including `{% include %}` support

This README is aimed at developers working on or integrating Numi locally.

## Quick Start

Build the CLI:

```bash
cargo build -p numi-cli
```

Run it directly through Cargo:

```bash
cargo run -p numi-cli -- --help
```

Or install the local binary into your Cargo bin directory:

```bash
cargo install --path crates/numi-cli
```

Then initialize a starter config in your project:

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

## Config File

Numi uses `numi.toml` as its config filename.

The current discovery behavior is:

- use `--config <path>` when provided
- otherwise prefer the nearest ancestor `numi.toml`
- if no ancestor exists, allow a single unambiguous descendant match
- fail loudly if discovery is ambiguous
- `numi generate` and `numi check` dispatch from that nearest manifest first:
  - `[jobs]` runs one local config
  - `[workspace]` runs the local workspace
- `--workspace` keeps the search local-first but requires an ancestor workspace `numi.toml`

A minimal config looks like this:

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

You can also point a localization job at `.xcstrings`:

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

## Workspace Manifest

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

## Commands

`numi generate`

- discovers the nearest manifest unless `--config` is passed
- uses the nearest local `numi.toml` first
- runs one config for `[jobs]` manifests and the whole workspace for `[workspace]` manifests
- generates outputs for all named jobs, or only selected jobs when `--job` is repeated
- prints non-fatal warnings to stderr
- repeated runs may reuse cached parser outputs when inputs are unchanged
- cache invalidation happens on relevant file add, remove, rename, or content change
- generation also skips unchanged jobs by default when the full job contract and committed output are still fresh

Examples:

```bash
numi generate
numi generate --config AppUI/numi.toml
numi generate --workspace
numi generate --job assets --job l10n
```

`numi check`

- computes what `generate` would write
- discovers the nearest manifest unless `--config` is passed
- uses the nearest local `numi.toml` first
- checks one config for `[jobs]` manifests and the whole workspace for `[workspace]` manifests
- exits `0` when outputs are current
- exits `2` when outputs are stale
- prints warnings to stderr without turning the run into a failure
- repeated runs may reuse cached parser outputs when inputs are unchanged
- cache invalidation happens on relevant file add, remove, rename, or content change
- still renders and compares output every time because `check` needs to prove freshness rather than trust the generate cache

Example:

```bash
numi check --job l10n
numi check --workspace
```

`--workspace`

- forces `generate` or `check` to use a workspace `numi.toml`
- ignores a nearer member `numi.toml` and searches ancestors for the nearest workspace
- keeps each member in its own normal `numi.toml`

Examples:

```bash
numi generate --workspace
numi check --workspace
numi generate --workspace --config numi.toml
```

`numi dump-context`

- prints the exact JSON context a job template receives
- prints warnings to stderr
- is the fastest way to debug or author custom templates

Example:

```bash
numi dump-context --job l10n
```

`numi config locate`

- prints the resolved config path

`numi config print`

- prints the resolved config with defaults materialized
- always prints the named-job TOML shape, which is also the only supported config syntax

## Built-In Templates

Current shipped Swift templates in `templates/swift` for the Apple ecosystem:

- `swiftui-assets` for SwiftUI-friendly asset accessors
- `l10n` for simple localization accessors from `.strings` or supported `.xcstrings` records
- `files` for file-oriented helpers

Current `.xcstrings` limitation:

- plural and device-specific variations are skipped with warnings
- supported plain-string records still generate normally
- placeholder metadata is preserved in template context, but the shipped `l10n` Swift template currently emits simple no-argument accessors

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

When building custom templates, start with:

```bash
numi dump-context --job l10n
```

The stable context contract is documented in [docs/context-schema.md](docs/context-schema.md).

## Supported Input Semantics

`.xcassets`

- images and colors are supported

`.strings`

- parsed into localization entries with stable `key` and `translation` properties

`.xcstrings`

- plain string records are supported
- `modules[].kind` remains `xcstrings` in template context
- `entry.properties.placeholders` is included only when placeholder metadata exists
- unsupported variation-bearing records are skipped with warnings

`files`

- file-oriented inputs are supported with the shipped `files` Swift template
- templates receive file metadata through Numi's current template context surface

## Developer Workflow

Useful local commands:

```bash
cargo test -v
cargo fmt --check
cargo test -p numi-cli --test generate_l10n -v
cargo test -p numi-core -v
```

If you are changing template or parsing behavior, `dump-context` tests and repeated-generate stability tests are usually the most important ones to keep green.

## Repo Guide

Useful docs:

- [docs/context-schema.md](docs/context-schema.md)
- [docs/migration-from-swiftgen.md](docs/migration-from-swiftgen.md)
- [docs/spec.md](docs/spec.md)

Useful fixtures:

- `fixtures/xcassets-basic`
- `fixtures/l10n-basic`
- `fixtures/xcstrings-basic`
- `fixtures/multimodule-repo`
- `fixtures/bench-mixed-large`

Benchmark scenarios currently measured:

- repeated generation for a single asset fixture
- repeated generation for a mixed assets + localization fixture
- workspace discovery from a member directory

## Current Status

Numi is usable today for:

- SwiftUI asset generation
- `.strings` localization generation
- `.xcstrings` localization generation for supported plain-string records
- file-oriented generation via the shipped `files` Swift template
- custom-template workflows driven by `dump-context`

The main current gap in `.xcstrings` support is variation handling: plural and device-specific branches are intentionally not generated yet.
