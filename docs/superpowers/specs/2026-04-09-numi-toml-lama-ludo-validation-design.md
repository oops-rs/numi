# Numi `numi.toml` Contract And lama-ludo Validation Design

## Summary

Numi should stop discovering `swiftgen.toml` and instead discover only `numi.toml`.
After that contract change, we should validate Numi in the real-world iOS project at `/Users/wendell/developer/WeNext/lama-ludo-ios` by adding module-local `numi.toml` files to resource-bearing Swift packages.

The lama-ludo configs are for validation, not replacement. They should generate Numi-owned comparison outputs and must not overwrite the project's existing generated files or SwiftGen flow.

## Facts

- Numi currently discovers only `swiftgen.toml`.
- Numi already supports:
  - `.xcassets`
  - `.strings`
  - `.xcstrings`
  - built-in `swiftui-assets`
  - built-in `l10n`
- Explicit `--config <path>` already accepts any path; discovery is the part tied to the default filename.
- The lama-ludo repo uses Swift Package modules and treats a folder containing `Package.swift` as a module boundary.
- lama-ludo resource declarations are already expressed in `Package.swift` through `.process(...)` entries.
- Resource filenames are not perfectly uniform across modules:
  - some modules use `Assets.xcassets`
  - some modules use `Asset.xcassets`
  - many modules use `Localizable.xcstrings`
- `AppUI` is special: its resources live under `Sources/AppResource/Resources` and are processed by the `AppResource` target instead of top-level package-root resource files.

## Goals

- Make the Numi product contract self-consistent by using `numi.toml` as the discovered config filename.
- Keep the config model simple: one discovered filename, no migration fallback logic.
- Validate Numi against a real modular iOS codebase without disturbing the existing generation pipeline.
- Make it easy to run `numi generate` or `numi check` inside individual lama-ludo modules.

## Non-Goals

- Supporting both `swiftgen.toml` and `numi.toml` during discovery.
- Automatically migrating old config files.
- Replacing lama-ludo's current SwiftGen setup in this change.
- Building a generator that emits Numi configs from `Package.swift`.
- Adding new parser support beyond what Numi already supports.

## Product Contract

### Config Discovery

Numi should discover only `numi.toml`.

This applies to:

- ancestor discovery
- single descendant fallback
- `numi init`
- diagnostics, hints, docs, examples, fixtures, and tests

Discovery must no longer look for `swiftgen.toml`.

### Explicit Config Paths

Explicit `--config <path>` should continue to work with any filename.

That means:

- `numi --config some/custom/path.toml generate` remains valid
- `numi --config swiftgen.toml generate` can still work if a user explicitly points at such a file
- only discovery changes; explicit path loading does not gain filename restrictions

This keeps the CLI predictable:

- discovery contract: `numi.toml`
- explicit path contract: any TOML file path the user passes

## lama-ludo Validation Strategy

### Module Rule

A folder is considered a module if it contains `Package.swift`.

For this validation pass, we should add `numi.toml` only to modules that actually contain Numi-supported resource inputs. We should not create empty configs for modules that have no relevant resources.

### Resource Selection Rule

Use each module's `Package.swift` and actual on-disk layout as the source of truth for supported inputs.

Relevant inputs for this pass:

- `Assets.xcassets`
- `Asset.xcassets`
- `Localizable.xcstrings`

If a module does not contain one of those supported inputs, it does not need a validation config yet.

### AppUI Special Case

`AppUI` should not be treated like the package-root modules.

Its config should target the `AppResource` target layout under `Sources/AppResource/Resources`, because that is where the package actually processes:

- asset catalogs
- localization inputs
- any generated outputs that belong to the shared app resource target

## Config Shape

Each lama-ludo module config should be minimal and explicit:

- filename: `numi.toml`
- one job per supported resource kind
- built-in templates only for the first validation pass
- outputs written to Numi-owned comparison files

Example module shape:

```toml
version = 1

[defaults]
access_level = "internal"

[defaults.bundle]
mode = "module"

[[jobs]]
name = "assets"
output = "Sources/<Target>/Generated/NumiAssets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Sources/<Target>/Generated/NumiL10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Localizable.xcstrings"

[jobs.template]
builtin = "l10n"
```

Adaptations:

- if the module uses `Asset.xcassets`, the input path should use that exact filename
- if a module has only one supported resource type, include only that job
- `AppUI` paths should be rooted under `Sources/AppResource/Resources/...` and its outputs should go to a separate Numi-owned generated location inside the package

## Output Isolation

lama-ludo validation outputs must not replace or overwrite the project's existing generated Swift.

Each config should write to separate Numi-owned comparison files, for example:

- `NumiAssets.swift`
- `NumiL10n.swift`
- or another clearly Numi-specific generated path

The essential requirement is isolation:

- existing generated files remain untouched
- existing build scripts remain valid
- Numi output can be reviewed side-by-side against the current generation result

## Error Handling And Warnings

- Unsupported `.xcstrings` variation-bearing entries should continue to be skipped with warnings.
- `numi generate` for lama-ludo modules should still succeed when only those warnings are encountered.
- `numi check` should also surface warnings while using its normal up-to-date/stale exit behavior.
- Config discovery errors and starter-config messages must mention `numi.toml`, not `swiftgen.toml`.

## Testing Strategy

### Numi Contract Change

Update and verify:

- config discovery tests
- `init` tests
- config print/locate tests
- fixture names and fixture references
- README and docs examples
- migration-focused docs that still mention `swiftgen.toml`

The result should be a repository-wide contract where the discovered filename is consistently `numi.toml`.

### lama-ludo Validation

For each resource-bearing module config:

- run `numi generate --config <module>/numi.toml`
- confirm outputs are produced in Numi-owned comparison files
- confirm existing generated files are unchanged

For representative modules:

- test one with `Assets.xcassets`
- test one with `Asset.xcassets`
- test one with both assets and `.xcstrings`
- test `AppUI` with its `AppResource` layout

## Implementation Sequence

1. Change Numi's discovered config filename from `swiftgen.toml` to `numi.toml`.
2. Update tests, fixtures, docs, examples, starter config behavior, and diagnostics to match.
3. Identify lama-ludo modules with supported resource inputs by inspecting `Package.swift` and on-disk files.
4. Add module-local `numi.toml` files only for those modules.
5. Point all lama-ludo validation outputs to Numi-owned comparison files.
6. Run Numi against those configs and record any real-world gaps discovered during validation.

## Risks

### Breaking Discovery Contract

Changing discovery from `swiftgen.toml` to `numi.toml` is intentionally breaking.
That is acceptable for this product direction, but the break must be complete and consistent. Partial renaming would be worse than either old or new behavior alone.

### AppUI Layout Drift

`AppUI` does not follow the same simple package-root resource layout as the modules.
If we guess its paths instead of deriving them from the package structure and existing resource folders, the validation config will be misleading.

### False Confidence From Empty Configs

Adding configs to every package regardless of resource presence would create noise and weak validation.
Only resource-bearing packages should participate in this first pass.

## Acceptance Criteria

- Numi discovers only `numi.toml`.
- `numi init` writes `numi.toml`.
- Docs, examples, fixtures, and diagnostics consistently refer to `numi.toml` as the discovered filename.
- lama-ludo receives module-local `numi.toml` files only in modules with supported resource inputs.
- Those configs generate Numi-owned comparison outputs without overwriting the repo's current generated files.
- At least one real `AppUI` config and representative module configs are runnable with `numi generate`.
