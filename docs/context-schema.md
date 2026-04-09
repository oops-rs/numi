# Context Schema

This document defines the stable template context contract for Numi v1.
Templates may rely on the field names and meanings documented here.

## Top-Level Shape

Every `dump-context` payload and every built-in or custom template receives a single object with these top-level fields:

- `job`
- `access_level`
- `bundle`
- `modules`

Top-level field names are stable for all v1 releases.

## Stable Fields

### `job`

- `job.name`: the configured job name from `swiftgen.toml`
- `job.swiftIdentifier`: the Swift type name derived from `job.name`
- `job.output`: the configured output path as written in config

### `access_level`

- `access_level`: the resolved Swift access level string consumed by templates such as `internal` or `public`

### `bundle`

- `bundle.mode`: the resolved bundle lookup mode
- `bundle.identifier`: the custom bundle identifier when `bundle.mode = "custom"`, otherwise `null`

### `modules[]`

Each module object includes these stable fields:

- `modules[].kind`
- `modules[].name`
- `modules[].properties`
- `modules[].entries`

Current v1 module kinds:

- `xcassets`
- `strings`
- `xcstrings`

Current stable module property keys:

- `tableName` for `strings` modules
- `tableName` for `xcstrings` modules

### `modules[].entries[]`

Each entry object includes these stable fields:

- `modules[].entries[].name`
- `modules[].entries[].swiftIdentifier`
- `modules[].entries[].kind`
- `modules[].entries[].children`
- `modules[].entries[].properties`

Current v1 entry kinds:

- `namespace`
- `image`
- `color`
- `string`

Current stable entry property keys:

- `assetName` for asset entries
- `key` for localization string entries
- `translation` for localization string entries
- `placeholders` for localization string entries when placeholder metadata exists

`placeholders` is additive metadata for localization entries. It is omitted entirely when a string has no placeholders.

## Determinism

Numi v1 keeps module and entry ordering deterministic for the same config, inputs, and template version.
Repeated `generate` and `dump-context` runs should therefore be byte-stable when the inputs do not change.

## Compatibility Policy

Numi v1 treats the documented field names, current kind strings, and field meanings in this document as stable API.
The following are considered non-breaking during v1:

- Adding new keys inside `properties`
- Adding new metadata for future templates while preserving existing keys and meanings
- Adding new documented fields that existing templates can ignore safely

The following require a major-version change:

- Removing a documented field
- Renaming a documented field
- Changing the meaning of a documented field or kind string
- Changing deterministic ordering guarantees for existing modules or entries

Custom templates should ignore unknown fields and unknown `properties` keys so they remain forward-compatible with additive metadata.
