# `.xcstrings` Support And Placeholder Metadata

## Goal

Add first-class `.xcstrings` catalog support to Numi without expanding scope to plural or device-specific generation.
The current version should treat a catalog as a source of plain localized string records, preserve stable template compatibility, and surface unsupported catalog variation types as warnings instead of hard failures.

## Current Facts

- Config already reserves `xcstrings` as a valid input kind.
- The render context already supports `modules[].kind = "xcstrings"`.
- The pipeline currently parses only `.strings` inputs.
- The stable v1 context schema documents `key` and `translation` for localization entries and allows additive metadata in `properties`.
- CLI `generate` and `check` currently stop on diagnostics only when they are returned as errors.

## Scope

This change includes:

- Parsing `.xcstrings` files end-to-end
- Converting supported catalog records into the existing localization IR path
- Preserving `.xcstrings` as a distinct module kind in template context
- Exposing placeholder metadata as additive entry properties
- Skipping unsupported variation-bearing records with warnings
- Updating docs and tests for the new stable context surface

This change does not include:

- Plural generation
- Device-specific generation
- A new built-in localization template API
- Normalizing `.xcstrings` modules into `modules[].kind = "strings"`

## Supported Catalog Model

For this version, Numi treats an `.xcstrings` catalog as a collection of plain string records.

Supported records:

- A key whose localization resolves to a single string-unit value
- A key with placeholder metadata attached to that plain string value

Unsupported records:

- Plural variations
- Device-specific variations
- Any other variation tree that does not reduce to a single plain string value

Unsupported records are skipped, and each skipped record emits a warning diagnostic.

## Parsing Design

Add a dedicated `.xcstrings` parser in the localization pipeline.
It should:

1. Read the catalog JSON from a `.xcstrings` file.
2. Walk the string records in deterministic key order.
3. For each record:
   - extract the plain translation text when the record is a supported string-unit entry
   - extract placeholder metadata when present
   - skip the record with a warning when the record uses an unsupported variation form
4. Emit a table-shaped result that mirrors the existing `.strings` parser contract closely enough for shared normalization and context building.

The parser should not attempt partial flattening of plural or device-specific structures in this version.
If a record has unsupported variations, it is skipped entirely.

## IR And Context Contract

`.xcstrings` should reuse the existing localization entry semantics while preserving source identity at the module level.

For supported `.xcstrings` modules:

- `modules[].kind = "xcstrings"`
- `modules[].name` remains the Swift identifier for the table name
- `modules[].properties.tableName` remains present

For supported `.xcstrings` entries:

- `entry.kind = "string"`
- `entry.properties.key` is present
- `entry.properties.translation` is present
- `entry.properties.placeholders` is present only when placeholders exist

Existing `.strings` entries remain unchanged.
This means `.strings` entries do not gain an always-present empty `placeholders` array.

## Placeholder Metadata Shape

`entry.properties.placeholders` is a deterministic array.
It is omitted entirely when the source entry has no placeholders.

Each placeholder item may include:

- `name`: the placeholder identifier from the catalog
- `format`: the raw placeholder format/category string when available
- `swiftType`: only when Numi can derive it confidently and deterministically from the catalog metadata

The array form is intentional:

- it preserves deterministic ordering
- it supports future positional or repeated placeholder representations
- it remains additive within the existing `properties` contract

## Diagnostics Behavior

Skipped `.xcstrings` records should produce warnings, not errors.

Each warning should include:

- severity `warning`
- the job name
- the `.xcstrings` file path
- the skipped key when available
- a short reason such as unsupported plural or device-specific variations

Warnings must be deterministic in message content and ordering.

## CLI Behavior

`generate` behavior:

- generation continues when only skip warnings are present
- warnings print to stderr
- output files are still written

`check` behavior:

- warning emission matches `generate`
- warnings print to stderr
- exit status still depends only on stale-output detection or real failures

Warnings from `.xcstrings` parsing must not change success into failure.

## Pipeline Integration

The pipeline should add a dedicated `xcstrings` input branch beside the existing `strings` branch.

That branch should:

- parse one `.xcstrings` file or a directory containing `.xcstrings` files
- build `ResourceModule` values with `ModuleKind::Xcstrings`
- normalize supported entries through the existing scope-normalization path
- accumulate warning diagnostics separately from hard-error diagnostics

Hard failures remain reserved for cases like:

- unreadable files
- invalid JSON structure
- malformed catalog data that prevents reliable parsing
- duplicate table-name conflicts that violate current localization invariants

## Documentation

Update [context-schema.md](/Users/wendell/Developer/oops-rs/numi/docs/context-schema.md) to document:

- `xcstrings` as a stable module kind
- `placeholders` as additive localization entry metadata
- omission of `placeholders` when no placeholders exist

Update migration or usage docs as needed so `.xcstrings` is no longer described as deferred.

## Testing

Add coverage for:

- parsing a simple `.xcstrings` catalog into localization entries
- extracting placeholder metadata when present
- omitting `placeholders` when absent
- skipping plural/device-specific variation records with warnings
- end-to-end generation from an `.xcstrings` input
- `dump-context` output showing `modules[].kind = "xcstrings"`
- CLI warning behavior during `generate` and `check`

Fixtures should use real `.xcstrings` files and deterministic expected output.

## Non-Goals For This Version

- Generating plural accessors
- Emitting specialized APIs for device-specific variants
- Inferring placeholder semantics beyond clearly derivable metadata
- Unifying `.strings` and `.xcstrings` module kinds

## Acceptance Criteria

- `xcstrings` input kind is supported end-to-end
- Supported plain-string catalog records generate localization output successfully
- Unsupported variation-bearing records are skipped with warnings
- `.xcstrings` modules appear as `modules[].kind = "xcstrings"` in template context
- `entry.properties.placeholders` appears only when placeholders exist
- `generate`, `check`, and `dump-context` are covered by tests for `.xcstrings`
- Stable context docs are updated to include the new additive metadata
