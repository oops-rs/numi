# Unified `numi.toml` Manifest Design

## Summary

Numi should unify single-config and workspace configuration under one filename: `numi.toml`.

This change should simplify the mental model without changing the core locality rule:

- the nearest `numi.toml` remains authoritative for `numi generate` and `numi check`
- a workspace manifest should not silently override a nearer module manifest
- users should still have an explicit way to target workspace execution

## Goals

- remove the extra `numi-workspace.toml` concept
- make Numi feel more consistent with tools like Cargo
- preserve predictable local execution semantics
- avoid ambiguous discovery rules

## Non-Goals

- do not make ancestor workspace manifests override a nearer module manifest
- do not support mixing workspace and single-config schemas in one file
- do not change the meaning of `numi generate` or `numi check` from local-first execution

## Recommended Shape

Use `numi.toml` for both modes.

### Single-config mode

This keeps the current job-oriented schema:

```toml
version = 1

[defaults]
access_level = "internal"

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"
```

### Workspace mode

Workspace manifests should use a dedicated top-level `workspace` block plus members:

```toml
version = 1

[workspace]

[[members]]
config = "AppUI/numi.toml"

[[members]]
config = "Core/numi.toml"
jobs = ["l10n"]
```

The `workspace` table acts as the mode marker. Its presence means the file is a workspace manifest rather than a single-config manifest.

## Invariants

The parser should enforce these rules:

- a manifest is either single-config mode or workspace mode
- single-config mode requires `jobs`
- workspace mode requires `[workspace]` and `members`
- a manifest must not define both `jobs` and `workspace`
- validation errors should explain the selected mode and the conflicting keys

## Discovery Semantics

### Default commands

`numi generate` and `numi check` should continue to resolve the nearest `numi.toml`.

That means:

- if the nearest manifest is a module config, run single-config mode
- if the nearest manifest is a workspace config, run workspace mode

The key rule is locality, not workspace precedence.

### Why not prefer ancestor workspace first

Ancestor-first workspace discovery would make command behavior depend on more distant state than the current directory, which weakens predictability.

Example:

- `/repo/numi.toml` is a workspace manifest
- `/repo/AppUI/numi.toml` is a module manifest
- running `numi generate` inside `/repo/AppUI/Sources` should resolve `/repo/AppUI/numi.toml`, not `/repo/numi.toml`

This preserves the intuition that local work should target the local module unless the user explicitly asks otherwise.

## Explicit Workspace Execution

Even with unified filenames, users should still be able to force workspace execution.

Recommended options:

- `numi generate --workspace`
- `numi check --workspace`

Meaning:

- search upward for the nearest ancestor `numi.toml` that is a workspace manifest
- fail with a clear error if no workspace manifest is found
- do not infer workspace mode from descendants

This gives users a convenient repo-level shortcut without making default behavior surprising.

## CLI Contract

After this change, the command model should be:

- `numi generate`: run the nearest manifest, whatever mode it is
- `numi check`: run the nearest manifest, whatever mode it is
- `numi generate --workspace`: force workspace resolution
- `numi check --workspace`: force workspace resolution

The dedicated `numi workspace ...` command group can be removed after migration, or kept temporarily as a compatibility alias during internal development.

## Migration

Migration should be straightforward because Numi is not published yet.

Recommended path:

1. Change workspace schema filename from `numi-workspace.toml` to `numi.toml`.
2. Extend config parsing to detect mode from top-level keys.
3. Update discovery so default commands resolve the nearest `numi.toml` and then dispatch by mode.
4. Add explicit `--workspace` resolution for `generate` and `check`.
5. Remove or deprecate the `workspace` subcommand surface.
6. Update fixtures, docs, and CLI help together.

## Risks

### Risk: mixed manifests become confusing

If one file is allowed to contain both `jobs` and `workspace`, users will not know which fields apply in which mode.

Mitigation:

- reject mixed mode completely

### Risk: default commands become less predictable

If ancestor workspace manifests override nearer module manifests, local commands become harder to reason about.

Mitigation:

- keep nearest-manifest semantics
- make workspace execution explicit with `--workspace`

### Risk: Cargo-style expectations expand scope

Users may expect Cargo-like package-plus-workspace cohabitation in one manifest.

Mitigation:

- explicitly document that Numi intentionally supports a simpler exclusive-mode model
- only add mixed-mode support if a concrete use case requires it later

## Recommendation

Adopt unified `numi.toml` manifests, but keep discovery local-first.

This gives Numi the ergonomic win of one filename without paying the complexity cost of workspace-first command resolution.
