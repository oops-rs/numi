# Migration From SwiftGen

Numi is intentionally close to SwiftGen for the MVP path, but it makes a few contracts more explicit.

## What Stays Familiar

- The config file name remains `swiftgen.toml`
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

- `[[jobs]]` describes one generation unit
- `[[jobs.inputs]]` declares each resource input
- `[jobs.template]` selects either a built-in template or a custom template path
- `[defaults]` and `[defaults.bundle]` provide shared defaults across jobs

## Built-In Templates

Current built-ins cover the MVP resource types:

- `swiftui-assets` for SwiftUI-friendly asset accessors
- `l10n` for `.strings` and `.xcstrings` localization accessors

If a SwiftGen setup relied on a custom Stencil template, the closest Numi migration path is to move that output shape into a custom Minijinja template and validate it with `numi dump-context`.

## Migration Notes

- `.strings` is supported in v1
- `.xcstrings` is supported in v1, but plural and device-specific variations are skipped with warnings in the current release
- Bundle handling is explicit in the template context through `bundle.mode` and `bundle.identifier`
- The stable context contract is documented in [context-schema.md](/Users/wendell/Developer/oops-rs/numi/docs/context-schema.md)

## Suggested Migration Flow

1. Copy the existing SwiftGen config into `swiftgen.toml`.
2. Replace the generator-specific template reference with either a Numi built-in or a custom Minijinja template.
3. Run `numi dump-context --job <name>` to inspect the exact context your template receives.
4. Run `numi generate` and compare the generated Swift against the previous SwiftGen output.
5. Add `numi check` to CI once the generated output is accepted.
