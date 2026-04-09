# Numi Files Input Design

## Summary

Numi should support SwiftGen-style bundled file resources through a new `files` input kind.

This feature should let users point a job input at either:

- a single file
- a directory tree containing arbitrary bundled files

Numi should then generate typed resource accessors for those files in the same overall style as SwiftGen’s files parser:

- nested folders become nested namespaces
- files become leaf accessors
- generated accessors resolve resources from the configured bundle
- the built-in output returns `URL`

This is resource discovery and bundle lookup generation.
It is not file-content parsing.

## Facts

- Numi currently supports only three input kinds:
  - `xcassets`
  - `strings`
  - `xcstrings`
- This support is enforced in both the spec and config validation:
  - [spec.md](/Users/wendell/Developer/oops-rs/numi/docs/spec.md)
  - [model.rs](/Users/wendell/Developer/oops-rs/numi/crates/numi-config/src/model.rs)
- The pipeline dispatch in [pipeline.rs](/Users/wendell/Developer/oops-rs/numi/crates/numi-core/src/pipeline.rs) currently has parser branches only for those three kinds.
- The IR already has a reasonable fit for generic bundled files:
  - `ResourceModule.kind` can represent new kinds
  - `ResourceEntry.kind` already includes `Data`
  - nested entry trees are already supported
- The existing bundle model already supports:
  - `module`
  - `main`
  - `custom`
- The existing rendering flow already supports built-in templates and custom templates.

## Goals

- Add a new input kind: `files`
- Support both directory input and single-file input
- Generate SwiftGen-style typed bundle accessors for arbitrary files
- Reuse Numi’s existing tree normalization and identifier rules where practical
- Keep output deterministic and byte-stable
- Make `dump-context` expose file-resource metadata cleanly

## Non-Goals

- Parsing file contents
- MIME sniffing or type-specific parsing
- Image/font/string specialization through the `files` input
- Glob patterns in v1 unless added explicitly later
- Full SwiftGen files-parser parity in every template/detail in the first version

## Core Rule

The `files` input kind is for bundled file discovery, not content parsing.

That means:

- Numi scans the filesystem
- Numi builds a stable tree of files and folders
- Numi generates bundle lookup accessors
- Numi does not inspect file contents to infer schema or semantics

## User-Facing Config

### Input Kind

Add `files` as a supported input kind:

```toml
[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"
```

The `path` may refer to:

- a single file
- a directory

### Built-In Template

Add a dedicated built-in template for file accessors.

Recommended built-in name:

- `files`

Example:

```toml
[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
builtin = "files"
```

The first version should optimize for the built-in template experience.
Custom templates should still work through the normal context path.

## Filesystem Semantics

### Single File Input

If the configured path points to a file:

- create one module for that input root
- create one leaf entry representing that file

### Directory Input

If the configured path points to a directory:

- walk recursively
- collect regular files only
- ignore directories as leaf resources
- build a stable namespace tree from relative path components

### Ignored Noise

The first version should skip common filesystem noise that should never generate API surface.

At minimum:

- `.DS_Store`

If more ignore rules are needed later, they can be additive.

### Ordering

Collected file entries must be sorted deterministically by relative path before tree normalization.

This is required for:

- byte-stable output
- deterministic context JSON
- stable duplicate-resolution behavior

## IR And Context Shape

### Module Kind

Expose file inputs as:

- `modules[].kind = "files"`

The module should represent the configured input root.

### Entry Kinds

Use:

- `EntryKind::Namespace` for folders
- `EntryKind::Data` for actual files

That keeps the semantics aligned with the existing IR.

### Entry Properties

Each file leaf should expose additive properties including:

- `relativePath`
- `fileName`
- `pathExtension`

Example:

```json
{
  "kind": "data",
  "name": "welcome-video.mp4",
  "properties": {
    "relativePath": "Onboarding/welcome-video.mp4",
    "fileName": "welcome-video.mp4",
    "pathExtension": "mp4"
  }
}
```

These fields are enough for:

- the built-in template
- custom templates
- future additive metadata

The first version should not add speculative metadata such as MIME type or UTI.

## Naming

Swift identifiers for file entries should reuse Numi’s existing identifier normalization rules.

That means:

- file and folder names normalize through the same deterministic path already used elsewhere
- collisions after normalization should use the existing deterministic conflict-resolution behavior rather than a file-specific scheme

This keeps the feature consistent with the rest of Numi.

## Generated API Shape

The built-in `files` template should emit SwiftGen-style bundled resource accessors that return `URL`.

Example direction:

```swift
internal enum Files {
    internal enum Onboarding {
        internal static let welcomeVideo = file("Onboarding/welcome-video.mp4")
    }
}

private func file(_ path: String) -> URL {
    guard let url = Bundle.module.url(forResource: path, withExtension: nil) else {
        fatalError("Missing file resource: \\(path)")
    }
    return url
}
```

Important details:

- folders become nested enums
- files become static properties
- generated lookup uses the configured bundle mode
- the helper should match the existing bundle strategy already used by other built-ins

## Bundle Behavior

The `files` built-in should respect the existing bundle configuration contract:

- `module`
- `main`
- `custom`

This is not a new bundle system.
It should reuse the same merged default/job bundle handling already present in the pipeline and rendering context.

## Error Handling

### Invalid Input Path

If a `files` path does not exist or is neither a file nor a directory:

- fail the job with a parser/input error

### Empty Directory

If a directory contains no supported file entries after noise filtering:

- generation should still succeed
- the resulting module should contain no file leaf entries

This is preferable to inventing a special failure mode for an otherwise valid path.

### Duplicate Paths

Two distinct filesystem entries under the same root cannot share the same relative path.
So the meaningful collision case is identifier normalization, not raw path duplication.

Identifier collisions should follow the same deterministic handling used elsewhere in Numi.

## Rendering And Templates

### Built-In Template

Add a built-in template named `files`.

Its responsibility is:

- render nested enums for namespaces
- render `URL`-returning accessors for file entries
- use the bundle helper strategy already established by built-in templates

### Custom Templates

Custom templates should automatically benefit from the new context surface once the `files` input kind is parsed into the graph.

No special custom-template mechanism is needed for this feature.

## Testing Strategy

Coverage should include:

- config validation accepts `files`
- single-file input parses successfully
- directory input parses recursively
- `.DS_Store` is skipped
- nested tree structure is deterministic
- file-entry properties appear in `dump-context`
- built-in `files` output is byte-stable across repeated runs
- bundle mode behavior matches existing expectations
- identifier collisions are deterministic

The tests should favor filesystem-backed fixtures and temp directories over mocks.

## Risks

### Overreaching Into Content Parsing

If the implementation starts inferring semantics from extensions or file contents, the feature will sprawl quickly.
The first version should stay strictly at the resource-discovery layer.

### Template Naming Drift

If the built-in API shape tries to match every historical SwiftGen variant immediately, the feature will get too large.
The first version should capture the core SwiftGen-style experience rather than total parity.

### Identifier Surprises

File and folder names are often messier than asset names.
The implementation must lean on deterministic normalization and collision handling, or the generated API will become unstable.

## Recommended Implementation Direction

Implement this as:

1. config support for `type = "files"`
2. a filesystem parser adapter in `numi-core`
3. graph/context support exposing `modules[].kind = "files"` and `EntryKind::Data`
4. a built-in `files` template returning `URL`
5. CLI/context/output tests for deterministic generation

That keeps the feature aligned with Numi’s current architecture while directly matching the SwiftGen-style bundled-resource use case.
