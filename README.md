<p align="center">
  <img src="docs/assets/numi-logo.png" alt="Numi logo" width="320">
</p>

# Numi

Numi is a deterministic resource code generator for Apple projects.
It turns asset catalogs, localization resources, and file lists into generated code using built-in or custom templates, with first-class support for multi-module repositories and CI verification.

## Why Numi

- Generates code from `.xcassets`, `.strings`, `.xcstrings`, and file-based inputs
- Supports built-in templates and custom Minijinja templates
- Works well in modular repos through workspace manifests and shared defaults
- Avoids rewriting unchanged outputs
- Verifies checked-in generated files with `numi check`
- Supports per-job and workspace-level generation hooks for tasks like formatting

Numi started as a modern SwiftGen replacement path, but its core model is broader: parse resources into a stable context, then render the output shape your project actually wants.

## Install

```bash
cargo install numi
```

The installed binary is `numi`.

## Quick Start

Initialize a starter config:

```bash
numi init
```

Generate outputs:

```bash
numi generate
```

Check whether generated files are up to date:

```bash
numi check
```

If your repo has a root workspace manifest, plain `numi generate` and `numi check` already do the right thing:

- from the repo root, they use the nearest workspace manifest
- from a workspace member directory, they auto-prefer the nearest ancestor workspace

Use `--config` when you want to force a specific manifest, and `--workspace` when you want to explicitly require workspace execution.

## A Minimal Config

Numi uses `numi.toml`.

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

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
```

The starter config shipped with `numi init` lives in [docs/examples/starter-numi.toml](docs/examples/starter-numi.toml).

## Typical Workflows

### Single module

```bash
numi generate
numi check
```

### Generate only selected jobs

```bash
numi generate --job assets --job l10n
```

### Monorepo or modular app

From the repo root:

```bash
numi generate
numi check
```

From a member directory:

```bash
numi generate
numi check
```

Force a specific manifest when needed:

```bash
numi generate --config AppUI/numi.toml
```

### Template authoring

Inspect the exact template context for one job:

```bash
numi dump-context --job l10n
```

That is the fastest way to build or debug a custom template.

## Supported Inputs

| Input type | What it reads | Notes |
| --- | --- | --- |
| `xcassets` | Asset catalogs | Built-in Swift and Objective-C templates available |
| `strings` | `.strings` files or directories | Localization helpers |
| `xcstrings` | `.xcstrings` files or directories | Placeholder metadata is preserved when available |
| `files` | Files or directories | Good for bundle/resource accessors |
| `fonts` | Font files or directories | Supported in template context and custom templates |

## Built-In Templates

| Language | Built-in name | Purpose |
| --- | --- | --- |
| `swift` | `swiftui-assets` | SwiftUI-friendly asset accessors |
| `swift` | `l10n` | Localization accessors for `.strings` and `.xcstrings` |
| `swift` | `files` | File-oriented helpers |
| `objc` | `assets` | Objective-C asset accessors |
| `objc` | `l10n` | Objective-C localization accessors |
| `objc` | `files` | Objective-C file-oriented helpers |

Example:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

Fonts are supported in the stable template context, but Numi does not currently ship a dedicated built-in Swift fonts template.

## Custom Templates

Custom templates use Minijinja:

```toml
[jobs.l10n.template]
path = "Templates/l10n.jinja"
```

Numi resolves templates and includes carefully:

- the configured template path is resolved from the manifest that declared it
- `{% include %}` can resolve from the including template directory
- `{% include %}` can also resolve from the config-root search path
- if the same include exists in both places, Numi fails instead of guessing

The stable template context is documented in [docs/context-schema.md](docs/context-schema.md).

## Workspace Manifests

For multi-module repositories, a repo-root `numi.toml` can orchestrate member configs:

```toml
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults]
access_level = "internal"

[workspace.defaults.bundle]
mode = "module"

[workspace.defaults.jobs.assets.template.builtin]
language = "swift"

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n.jinja"
```

Then a member can stay lean:

```toml
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
name = "swiftui-assets"
```

Workspace rules:

- workspace members are directory roots, not config file paths
- workspace defaults can provide shared defaults, built-in languages, template paths, and hooks
- workspace-default template paths are resolved relative to the workspace manifest
- plain `generate` and `check` auto-prefer the nearest ancestor workspace when run from a member directory

## Generation Hooks

Hooks let you run tools before or after generation, which is especially useful for formatting generated files.

### Per-job hooks

```toml
[jobs.l10n.hooks.pre_generate]
command = ["Scripts/prepare-generated.sh"]

[jobs.l10n.hooks.post_generate]
command = ["swiftformat"]
```

### Shared workspace hooks

```toml
[workspace.defaults.hooks.post_generate]
command = ["Scripts/format-generated.sh"]

[workspace.defaults.jobs.l10n.hooks.post_generate]
command = ["Scripts/format-generated-localization.sh"]
```

Hook behavior:

- hooks run only during `numi generate`
- `pre_generate` runs before rendering and writing
- `post_generate` runs only after a job creates or updates its output
- `workspace.defaults.hooks` applies to every workspace job
- `workspace.defaults.jobs.<job>.hooks` overrides shared workspace hooks for that job and phase
- job-level hooks replace inherited hooks for the same phase
- hook failures fail the command

Numi passes target metadata through environment variables:

- `NUMI_JOB_NAME`
- `NUMI_OUTPUT_PATH`
- `NUMI_OUTPUT_DIR`
- `NUMI_CONFIG_PATH`
- `NUMI_WORKSPACE_MANIFEST_PATH` when running through a workspace
- `NUMI_WRITE_OUTCOME` for post hooks, set to `created` or `updated`

If `command[0]` looks like a filesystem path, Numi resolves it relative to the manifest that declared it.

## Incremental Generation Modes

`numi generate` supports one incremental mode flag:

```bash
numi generate --incremental auto
numi generate --incremental always
numi generate --incremental never
numi generate --incremental refresh
```

Mode meanings:

- `auto`: use the config and default behavior
- `always`: force incremental generation behavior on for this run
- `never`: disable incremental parsing and generation-cache reuse for this run
- `refresh`: rerender now even if the job would otherwise be skipped, while still allowing parser cache reuse

## Command Reference

| Command | Purpose |
| --- | --- |
| `numi generate` | Generate outputs for one config or workspace |
| `numi check` | Check whether generated outputs are up to date |
| `numi init` | Write a starter `numi.toml` in the current directory |
| `numi config locate` | Print the resolved config path |
| `numi config print` | Print the resolved single-config manifest with defaults materialized |
| `numi dump-context --job <name>` | Print the template context for one job |

Top-level help:

```bash
numi --help
numi generate --help
```

## CI

Numi is designed for checked-in outputs.
A typical CI step is:

```bash
numi check
```

For modular repos with a root workspace manifest:

```bash
numi check
```

`numi check` exits:

- `0` when outputs are current
- `2` when outputs are stale

It never runs generation hooks.

## Diagnostics and UX

`numi generate` and `numi check` emit clearer interactive status output in a terminal, including which manifest was selected, per-job outcomes, warnings, and a final summary. Non-interactive output stays plain and script-friendly.

Warnings are surfaced without silently changing behavior. When Numi cannot safely guess, it prefers a clear failure over an implicit fallback.

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

## Development

Useful local commands:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
