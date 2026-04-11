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
- only supports single-config (`[jobs]`) manifests and rejects workspace manifests
- is the fastest way to debug or author custom templates

`numi config locate`

- prints the resolved config path

`numi config print`

- prints the resolved config with defaults materialized
- only supports single-config (`[jobs]`) manifests and rejects workspace manifests

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
