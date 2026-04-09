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
members = ["AppUI", "Core"]
```

The `workspace` table acts as the mode marker. Its presence means the file is a workspace manifest rather than a single-config manifest.

Each workspace member path identifies a member root relative to the workspace root. The member manifest is always resolved as `<member>/numi.toml`.

## Invariants

The parser should enforce these rules:

- a manifest is either single-config mode or workspace mode
- single-config mode requires `jobs`
- workspace mode requires `[workspace]` and `workspace.members`
- a manifest must not define both `jobs` and `workspace`
- validation errors should explain the selected mode and the conflicting keys
- each `workspace.members` entry must be unique
- each `workspace.members` entry is a relative member root, not a config-file path

## Workspace Defaults

Workspace manifests may define default job configuration that member jobs inherit unless they override it locally.

The recommended shape is job-name keyed defaults:

```toml
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.assets.template.builtin]
swift = "swiftui-assets"

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"
```

This follows the same template structure as normal job configuration:

- `[...template.builtin]` for built-in templates
- `[...template] path = "..."` for custom templates

### Inheritance rules

- workspace defaults apply by job name
- a member job inherits a workspace default only when the member job omits that field
- a member job's local `template` always overrides the workspace default `template`
- validation still runs on the final resolved job config

This keeps the mental model simple:

- workspace config provides shared defaults
- member configs stay authoritative for local overrides

## Member Overrides

The workspace should keep the common case short with `workspace.members = [...]`.

When a specific member needs workspace-side overrides such as job selection, use a separate override table keyed by member path:

```toml
[workspace]
members = ["AppUI", "Core"]

[workspace.member_overrides.Core]
jobs = ["l10n"]
```

This keeps member declaration compact while still leaving room for per-member workspace behavior.

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

## Template Path Resolution

Custom template paths should allow extensionless configuration.

Example:

```toml
[jobs.l10n.template]
path = "Templates/l10n"
```

Resolution rules:

- if the configured path exists as a file, use it directly
- otherwise try the same path with `.jinja` appended
- if neither exists, error
- if both exist, error instead of guessing

This rule should apply uniformly anywhere `template.path` appears, including:

- single-config job templates
- workspace job-template defaults

Include statements should remain explicit for now. Only the top-level `template.path` should get extensionless resolution in this phase.

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
2. Replace explicit config-file member entries with `workspace.members = [...]`.
3. Add workspace job defaults under `workspace.defaults.jobs.<name>`.
4. Add optional per-member overrides under `workspace.member_overrides.<member>`.
5. Extend config parsing to detect mode from top-level keys.
6. Update discovery so default commands resolve the nearest `numi.toml` and then dispatch by mode.
7. Add explicit `--workspace` resolution for `generate` and `check`.
8. Add extensionless `template.path` resolution.
9. Remove or deprecate the `workspace` subcommand surface.
10. Update fixtures, docs, and CLI help together.

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
