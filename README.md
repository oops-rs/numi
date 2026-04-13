<p align="center">
  <img src="docs/assets/numi-logo.png" alt="Numi logo" width="320">
</p>

# Numi

Numi is a deterministic resource code generator for Apple projects.
It turns asset catalogs, localization resources, fonts, and file lists into generated code using built-in or custom templates, with first-class support for multi-module repositories and CI verification.

## Why Numi

- Generates code from `.xcassets`, `.strings`, `.xcstrings`, fonts, and file-based inputs
- Supports built-in templates and custom Minijinja templates
- Works well in modular repos through workspace manifests and shared defaults
- Avoids rewriting unchanged outputs — deterministic, byte-stable generation
- Verifies checked-in generated files with `numi check`
- Supports per-job and workspace-level generation hooks for tasks like formatting
- Incremental caching — asset catalogs and strings files are parsed once and skipped when untouched

Numi started as a modern SwiftGen replacement path, but its core model is broader: parse resources into a stable context, then render the output shape your project actually wants.

## Install

With Homebrew:

```bash
brew install oops-rs/tap/numi
```

With Cargo:

```bash
cargo install numi
```

## Quick Start

```bash
numi init          # scaffold a starter numi.toml
numi generate      # parse resources and write generated files
numi check         # verify generated files are up to date (CI-safe)
```

If your repo has a root workspace manifest, `numi generate` and `numi check` auto-detect it — from the repo root or any member directory.

## A Minimal Config

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
language = "swift"
name = "swiftui-assets"
```

The starter config shipped with `numi init` lives in [docs/examples/starter-numi.toml](docs/examples/starter-numi.toml).

## Command Reference

| Command | Purpose |
| --- | --- |
| `numi generate` | Generate outputs for one config or workspace |
| `numi check` | Check whether generated outputs are up to date |
| `numi init` | Write a starter `numi.toml` in the current directory |
| `numi config locate` | Print the resolved config path |
| `numi config print` | Print the resolved manifest with defaults materialized |
| `numi dump-context --job <name>` | Print the template context for one job |

## Current Limitations

- `.xcstrings` plural and device-specific variations are currently skipped with warnings
- the shipped Swift `l10n` built-in is still conservative and does not yet expose every placeholder-aware output shape a custom template can implement
- `dump-context` and `config print` are single-config tools and reject workspace manifests

## Docs

- [docs/context-schema.md](docs/context-schema.md): stable template context reference
- [docs/migration-from-swiftgen.md](docs/migration-from-swiftgen.md): migration notes from SwiftGen
- [docs/spec.md](docs/spec.md): full product and technical specification
- [docs/examples/starter-numi.toml](docs/examples/starter-numi.toml): starter manifest
- [docs/crates-io-release.md](docs/crates-io-release.md): release workflow notes

For detailed guides on workspace manifests, generation hooks, custom templates, supported inputs, built-in templates, and CI integration, see the [numi website](https://numi.oops.rs).

## Development

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
