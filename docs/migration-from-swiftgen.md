# Migration From SwiftGen

Numi is intentionally close to SwiftGen for the MVP path, but it makes a few contracts more explicit.
Numi is a general-purpose code generator that currently ships Swift templates for the Apple ecosystem.

## What Stays Familiar

- The config file name is `numi.toml`
- Jobs still describe inputs, templates, and one output file
- Asset catalogs and `.strings` localization inputs remain config-driven
- `.xcstrings` localization inputs are supported config-driven inputs too
- Generated Swift can still be checked into the repository or validated in CI with `numi check`

## What Changes

- Numi renders from a stable template context instead of hard-coded generators
- Config discovery is explicit: nearest ancestor config wins, then a single descendant match is allowed
- Output writing is deterministic and no-op stable, so unchanged files are not rewritten
- Diagnostics are designed to fail loudly with actionable messages instead of silently picking a fallback
- `.xcstrings` records are parsed into the same stable localization surface as `.strings`, with placeholder metadata preserved when present

## Config Mapping

The SwiftGen MVP concepts map directly onto Numi's current config surface:

- `[jobs.<name>]` describes one named generation unit
- `[[jobs.<name>.inputs]]` declares each resource input for that named job
- `[jobs.<name>.template]` contains either `[jobs.<name>.template.builtin]` for a shipped Swift template or a custom template path
- `[jobs.<name>.template.builtin]` contains built-in template namespace keys; today `swift` is the supported namespace key, and its value selects the shipped template, for example `swift = "l10n"`
- `[defaults]` and `[defaults.bundle]` provide shared defaults across jobs

## Built-In Templates

Current shipped Swift templates in `templates/swift` cover the MVP resource types:

- `swiftui-assets` for SwiftUI-friendly asset accessors
- `l10n` for `.strings` and `.xcstrings` localization accessors
- `files` for file-oriented helpers

Example:

```toml
[jobs.l10n.template.builtin]
swift = "l10n"
```

If a SwiftGen setup relied on a custom Stencil template, the closest Numi migration path is to move that output shape into a custom Minijinja template and validate it with `numi dump-context`.

## Migration Notes

- `.strings` is supported in v1
- `.xcstrings` is supported in v1, but plural and device-specific variations are skipped with warnings in the current release
- Bundle handling is explicit in the template context through `bundle.mode` and `bundle.identifier`
- The stable context contract is documented in [context-schema.md](context-schema.md)
- In monorepos, you can keep per-module `numi.toml` files and add a repo-level `numi-workspace.toml` to orchestrate them
- CI can keep using `numi check` either once per config or through a workspace-level `numi workspace check`

## Suggested Migration Flow

1. Copy the existing SwiftGen config into `numi.toml`.
2. Replace the generator-specific template reference with either a Numi built-in or a custom Minijinja template.
3. Run `numi dump-context --job <name>` to inspect the exact context your template receives.
4. Run `numi generate` and compare the generated Swift against the previous SwiftGen output.
5. Add `numi check` to CI once the generated output is accepted.
