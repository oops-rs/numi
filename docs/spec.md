# Numi: Full Product and Technical Specification

## 1. Overview

### 1.1 Project Name
- Project name: Numi
- Positioning: A blazingly fast Rust-based resource code generation tool for Apple projects
- Initial purpose: Modern replacement for SwiftGen with stronger performance, better extensibility, and template-driven output

Primary use cases:
- Generate strongly typed Swift accessors for asset catalogs
- Generate localization accessors from string resources
- Support configurable, template-driven output instead of hardcoded generators only
- Work well in multi-module Swift Package and Xcode-based repositories

### 1.2 Vision
Numi should evolve from a Rust reimplementation of SwiftGen into a resource compilation platform for Apple app development.

The system should:
- Parse multiple Apple resource formats
- Normalize them into a unified intermediate representation (IR)
- Expose a stable template context
- Render Swift source or other future outputs
- Integrate cleanly with CI, Xcode, and Swift Package Manager workflows

### 1.3 Goals
- Be fast enough to feel instantaneous on normal projects
- Be deterministic so CI outputs are stable
- Be extensible so new resource formats can be added later
- Be template-driven so users are not forced into one output shape
- Be safe by default with strong diagnostics and clear failures
- Support multi-module repos without awkward config ergonomics

### 1.4 Non-goals for v1
- Full SwiftGen feature parity
- Full plugin ecosystem
- Storyboard/XIB parsing
- Generated code formatting beyond reasonable built-in formatting
- Automatic Xcode project editing
- Runtime-only access APIs

## 2. Product Scope

### 2.1 v1 Scope
Numi v1 should support:
- Config discovery
- Config-driven generation jobs
- Parsing `.xcassets`
- Parsing file-oriented inputs from files or directories
- Parsing localization resources:
  - `.strings`
  - `.xcstrings`
- Unified IR
- Stable template context
- Built-in templates
- Custom user templates
- Swift and Objective-C code output
- `generate`, `check`, `init`, and config inspection commands
- Deterministic ordering and stable output
- Clear diagnostics for collisions, invalid inputs, and template failures

### 2.2 v1.5 Scope
- Broader `.xcstrings` coverage and diagnostics for additional variation kinds and edge cases
- More built-in templates
- Multiple output files from one resource graph
- Partial templates/includes
- Better incremental change detection
- More detailed context dump tooling

### 2.3 v2 Direction
- Fonts
- Symbols and additional asset metadata
- Plist-driven generation
- Plugin parser system
- Additional output languages beyond the current Swift and Objective-C built-ins

## 3. Target Users

### 3.1 Primary Users
- iOS/macOS developers using Xcode
- Swift Package Manager users
- Teams with modular Apple codebases
- Developers replacing or extending SwiftGen-like workflows

### 3.2 Secondary Users
- Infra engineers maintaining internal Apple tooling
- Teams wanting custom generation conventions
- Agent-driven coding workflows that need deterministic codegen

## 4. Core Design Principles

### 4.1 Typed Core, Flexible Edges
Internally, Numi should use a strongly typed Rust IR.
Externally, templates should consume a stable JSON-like context model.

### 4.2 Deterministic Output
Given the same input files, template version, and config, generated output must be byte-stable unless config explicitly requests nondeterministic values.

### 4.3 Fast by Default
Performance is a first-class requirement. The architecture should minimize unnecessary file reads, repeated parsing, and full-tree scans.

### 4.4 Clear Failure Modes
When generation fails, Numi should explain:
- What failed
- Where it failed
- What the user can do next

### 4.5 Configured, Not Magical
Discovery and defaults should reduce friction, but behavior must remain explainable.

## 5. Naming and Packaging

### 5.1 Product Naming
- User-facing tool: `numi`
- Workspace/package naming can use `numi-*` convention

### 5.2 Proposed Rust Workspace Crates
- `numi`: published CLI package and entrypoint (implemented in `crates/numi-cli`)
- `numi-config`: config parsing, validation, discovery
- `numi-core`: orchestration APIs
- `numi-scan`: file discovery and input collection
- `numi-ir`: typed intermediate representation
- `numi-parse-xcassets`: asset catalog parser adapter
- `numi-parse-l10n`: localization parser adapter
- `numi-normalize`: graph normalization and identifier derivation
- `numi-context`: template context builder
- `numi-render`: rendering abstraction
- `numi-render-minijinja`: Minijinja integration
- `numi-builtin-templates`: embedded built-in templates
- `numi-diagnostics`: error/warning infrastructure
- `numi-fs`: filesystem abstractions and caching utilities
- `numi-check`: up-to-date checking logic

For v1, this can be reduced if needed. A smaller initial workspace is acceptable as long as module boundaries stay conceptually clean.

## 6. Functional Requirements

### 6.1 Input Resource Support

#### 6.1.1 Asset Catalogs
Numi v1 must support parsing `.xcassets` through the existing Rust ecosystem, especially via `xcassets`.

Initial support target:
- Image sets
- Color sets
- Hierarchical groups/folders
- Original asset names and paths
- Namespacing derived from group structure

Nice-to-have metadata if available:
- Appearances
- Idioms
- Scales
- Preserve-vector-representation

#### 6.1.2 Localization Resources
Numi v1 must support localization generation through the existing Rust ecosystem, especially via `langcodec` where applicable.

Initial support target:
- `.strings`
- `.xcstrings`
- Localization table/source grouping
- String keys
- Placeholder metadata if parseable
- Developer comments if available

#### 6.1.3 File Resources
Numi v1 must support file-oriented generation from arbitrary files or directories without parsing file contents.

Initial support target:
- Single-file inputs
- Recursive directory inputs
- Bundle-relative lookup paths for each discovered file
- Original file names and path-extension metadata
- Deterministic ordering for repeated runs over unchanged inputs

#### 6.1.4 Future Resources
Architecture must allow future resource kinds without redesigning the system:
- Fonts
- Symbols
- Plist values
- Data assets
- Arbitrary custom inputs

### 6.2 Output Generation

#### 6.2.1 Built-in Templates
Numi v1 must ship with built-in templates for common Swift and Objective-C output styles.

Minimum built-ins:
- SwiftUI-friendly assets template
- UIKit/AppKit-compatible assets template, if low-cost
- Localization template
- File-helper template
- Objective-C assets template
- Objective-C localization template
- Objective-C file-helper template

#### 6.2.2 Custom Templates
Users must be able to provide template files from disk.
Custom templates should receive the same stable context model as built-ins.

#### 6.2.3 Output Files
Each job generates exactly one output file in v1.
One config may contain multiple jobs.

#### 6.2.4 Output Writing
Numi must:
- Create parent directories if needed
- Avoid rewriting output if content is unchanged
- Write atomically where practical

### 6.3 CLI Behavior

#### 6.3.1 Core Commands
Numi v1 should support:

```bash
numi generate
numi generate --config AppUI/numi.toml
numi generate --workspace
numi generate --job assets
numi check
numi check --workspace
numi init
numi config locate
numi config print
numi dump-context --job assets
```

#### 6.3.2 Command Definitions
`numi generate`:
- Discover config
- Resolve the nearest `numi.toml`
- Run a single config for `[jobs]` manifests
- Run a workspace for `[workspace]` manifests
- Resolve jobs
- Parse inputs
- Normalize IR
- Build template context
- Render outputs
- Write changed files
- Report warnings/errors
- May reuse cached parser outputs when inputs are unchanged
- Cache invalidation occurs on relevant file add, remove, rename, or content change
- Normalization, rendering, and output checks still run every time

`numi check`:
- Run generation logically without modifying files
- Exit non-zero if any output is stale, missing, or would change
- Resolve the nearest `numi.toml`
- Check a single config for `[jobs]` manifests
- Check a workspace for `[workspace]` manifests
- May reuse cached parser outputs when inputs are unchanged
- Cache invalidation occurs on relevant file add, remove, rename, or content change
- Normalization, rendering, and output checks still run every time

`--workspace` on `generate` and `check`:
- Require a workspace `numi.toml`
- Search ancestors for the nearest workspace `numi.toml`
- Ignore a nearer member manifest when repo-level orchestration is requested
- Support repo-level CI orchestration for multi-module repositories

`numi init`:
- Create a starter `numi.toml` in the current directory
- Refuse to overwrite unless `--force`

`numi config locate`:
- Print discovered config path
- Optionally explain discovery path

`numi config print`:
- Print resolved config after normalization/default application

`numi dump-context`:
- Emit the template context for one job as JSON
- Useful for debugging custom templates

## 7. Configuration Specification

### 7.1 Filename
Supported config filename in v1:
- `numi.toml`

The product is Numi, but using SwiftGen-compatible naming lowers migration friction.
A future alias like `numi.toml` can be considered later.

### 7.2 Discovery Algorithm

#### 7.2.1 Priority Order
1. If `--config` is provided, use it.
2. Otherwise use the nearest `numi.toml` in the current directory or an ancestor directory.
3. If no local or ancestor manifest exists, fail.
4. When `--workspace` is set, skip a nearer member manifest and search ancestors for the nearest workspace `numi.toml`.

#### 7.2.2 Discovery Rules
- Discovery is ancestor-only unless `--config` is explicit.
- Ancestor search uses nearest match.
- Descendant directories are not scanned for `numi.toml`.
- Relative paths in config are always resolved relative to the config file directory.

#### 7.2.3 Missing Manifest Error
Example:

```text
No configuration file found from /repo

Please specify one with:
  numi config locate --config <path>
```

### 7.3 Config Schema

#### 7.3.1 Top-level Structure (TOML)
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
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
```

#### 7.3.2 Top-level Keys
- `version`: schema version, required
- `defaults`: optional defaults applied to jobs
- `jobs`: required table of named generation jobs

#### 7.3.3 Defaults Block
Possible v1 defaults:
- `access_level`: `internal` or `public`
- `bundle.mode`: `module`, `main`, or `custom`
- `bundle.identifier`: optional when mode is `custom`
- `naming`: optional naming defaults
- `format`: future placeholder

#### 7.3.4 Job Schema
Each named job contains:
- `inputs`: non-empty list
- `template`: built-in or path-based template
- `output`: destination file path
- Optional job-level overrides for defaults

#### 7.3.5 Input Schema
Each input contains:
- `type`: required
- `path`: required
- Optional parser-specific options

Supported v1 input types:
- `xcassets`
- `files`
- `strings`
- `xcstrings`

#### 7.3.6 Template Schema
Built-in template:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

Objective-C built-in:

```toml
[jobs.assets.template.builtin]
language = "objc"
name = "assets"
```

Custom template:

```toml
[jobs.assets.template]
path = "Templates/assets.jinja"
```

Optional future fields:
- `partials_dir`
- `strict`

#### 7.3.7 Naming Settings
Possible v1 naming controls:
- Casing style per generated symbol family
- Keyword escaping strategy
- Duplicate resolution strategy
- Namespace flattening toggle

### 7.4 Workspace Manifest

Workspace orchestration uses the same `numi.toml` filename as single-config mode. A manifest is either a `[jobs]` config or a `[workspace]` config, never both.

Example:

```toml
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"

[workspace.member_overrides.Core]
jobs = ["l10n"]
```

Workspace manifest fields:
- `version`: schema version, required
- `workspace.members`: required list of relative member roots
- `workspace.defaults`: optional job defaults applied before member execution
- `workspace.member_overrides`: optional per-member overrides keyed by member root

Each workspace member contains:
- one directory root that resolves to `<member>/numi.toml`
- optional member overrides, including `jobs`, keyed by that member root

## 8. Intermediate Representation (IR)

### 8.1 Purpose
The IR is the typed internal model between parsing and rendering.
It must preserve enough structure and metadata to support:
- Built-in generators
- Custom templates
- Future resource kinds
- Diagnostics

### 8.2 Core Model
```rust
pub struct ResourceGraph {
    pub modules: Vec<ResourceModule>,
    pub diagnostics: Vec<Diagnostic>,
    pub metadata: GraphMetadata,
}

pub struct ResourceModule {
    pub id: String,
    pub kind: ModuleKind,
    pub name: String,
    pub entries: Vec<ResourceEntry>,
    pub metadata: Metadata,
}

pub enum ModuleKind {
    Xcassets,
    Strings,
    Xcstrings,
    Files,
    Custom(String),
}

pub struct ResourceEntry {
    pub id: String,
    pub name: String,
    pub source_path: std::path::PathBuf,
    pub swift_identifier: String,
    pub kind: EntryKind,
    pub children: Vec<ResourceEntry>,
    pub properties: std::collections::BTreeMap<String, serde_json::Value>,
    pub metadata: Metadata,
}

pub enum EntryKind {
    Namespace,
    Image,
    Color,
    StringKey,
    PluralKey,
    Font,
    Data,
    Unknown,
}
```

### 8.3 IR Requirements
- Must support hierarchical grouping
- Must preserve original names and canonical generated identifiers
- Must store parser-specific properties in extensible form
- Must carry source location metadata for diagnostics
- Must be serializable for debugging if needed

### 8.4 Why Hybrid Typed + Property Map
Use typed enums for high-level semantics and extensible properties for future growth.
This avoids both over-rigid enums and fully untyped internal maps.

## 9. Parsing Pipeline

### 9.1 Stages
1. Resolve config
2. Resolve jobs
3. Collect inputs
4. Parse inputs into raw typed structures
5. Normalize into unified IR
6. Validate graph
7. Build template context
8. Render
9. Write/check outputs

### 9.2 Input Scanning
Scanner behavior:
- Resolve input paths relative to config root
- Validate existence
- Detect basic file kind mismatches
- Collect relevant files efficiently

### 9.3 Parser Adapters
Each parser adapter should:
- Read input files
- Convert them to parser-specific models
- Map them into common IR constructs
- Emit warnings when data is ignored or unsupported

## 10. Normalization Rules

### 10.1 Naming Normalization
Normalization responsibilities:
- Derive Swift-safe identifiers
- Escape Swift keywords
- Transform unsupported characters
- Preserve stable naming across runs

### 10.2 Namespace Handling
For grouped resources, Numi should preserve hierarchy by default.

Example:
- Asset path: `Icons/Common/add`
- Generated path: `Assets.Icons.Common.add`

Optional future flattening can be added later.

### 10.3 Collision Handling
If two entries normalize to the same identifier in the same scope:
- Emit a hard error by default
- Error message must identify both source paths/names

Optional future strategies:
- Suffixing
- Source-name preservation mode

### 10.4 Ordering
All modules and entries must be sorted deterministically before rendering.
Suggested order:
- By kind group if necessary
- Then by canonical path/name

## 11. Template System

### 11.1 Engine Choice
Use Minijinja for v1.

Rationale:
- Good Rust integration
- Jinja-like syntax
- Mature enough for production use
- Easy serde context support
- Support for custom filters/functions

### 11.2 Template Philosophy
Templates are user-extensible output definitions over a stable context model.
Templates must not directly access internal Rust structs.

### 11.3 Built-in Template Packaging
Built-in templates should be embedded in the binary or bundled with the crate in a deterministic way.
Users should refer to them by logical name, not path.

### 11.4 Custom Filters and Functions
V1 should include Swift-aware helpers:
- `swift_identifier`
- `escape_swift_keyword`
- `camel_case`
- `pascal_case`
- `snake_case`
- `lower_first`
- `upper_first`
- `string_literal`
- `indent`
- `doc_comment`

Optional helper later:
- `swift_type_for_payload`

### 11.5 Template Failure Behavior
Template render errors must report:
- Template file or builtin name
- Line/column if available
- Missing field/filter/function information
- Job name

## 12. Template Context Schema

### 12.1 Context Design Goals
- Stable across minor internal refactors
- Serializable to JSON
- Easy for users and AI agents to inspect
- Rich enough to support custom code generation

### 12.2 Example Shape
```json
{
  "job": {
    "name": "assets",
    "output": "Generated/Assets.swift"
  },
  "bundle": {
    "mode": "module"
  },
  "modules": [
    {
      "kind": "xcassets",
      "name": "Assets",
      "entries": [
        {
          "name": "Icons",
          "swiftIdentifier": "Icons",
          "kind": "namespace",
          "children": [
            {
              "name": "add",
              "swiftIdentifier": "Add",
              "kind": "image",
              "children": [],
              "properties": {
                "assetName": "Icons/add"
              }
            }
          ]
        }
      ]
    }
  ]
}
```

### 12.3 Context Compatibility Policy
The template context schema is a public compatibility surface.
Breaking changes to field names or structure require a major version bump unless guarded by config/schema versioning.

## 13. Built-in Swift Output Design

### 13.1 Asset Output Target
Default built-in asset template should generate strongly typed wrappers.

Possible shape:

```swift
public enum Assets {
  public enum Icons {
    public static let add = ImageAsset(name: "Icons/add")
  }
}
```

Alternative built-in template later:

```swift
public enum Assets {
  public enum Icons {
    public static var add: Image { .init("Icons/add") }
  }
}
```

### 13.2 Localization Output Target
Default built-in `l10n` template shape in this branch emits simple string accessors with a shared translation helper. Placeholder-aware overloads are not part of the shipped API yet:

```swift
internal enum L10n {
  internal enum Localizable {
    internal static let welcome = tr("Localizable", "welcome")
  }
}

private func tr(_ table: String, _ key: String) -> String {
  NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
```

### 13.3 Generated Helper Types
If built-in templates need helper types or helper functions like `ImageAsset`, `ColorAsset`, or `tr`, those helpers should be generated into the same output file unless explicitly disabled later.

## 14. Diagnostics

### 14.1 Severity Levels
- `error`
- `warning`
- `note`

### 14.2 Error Categories
- Config discovery failure
- Config validation failure
- Missing input path
- Unsupported input kind
- Parser failure
- Normalization collision
- Template render failure
- Write failure
- Stale output in check mode

### 14.3 Diagnostic Requirements
Every diagnostic should include, when possible:
- Severity
- Message
- Related job name
- File path
- Source location or source entry name
- Actionable hint

Example:

```text
error: identifier collision in job `assets`
  resource `Icons/add` and `Icons/Add` both normalize to `add`
  hint: rename one asset or adjust naming rules
```

## 15. Performance Specification

### 15.1 Performance Goals
- Small/medium projects should feel near-instant
- Large projects should scale predictably
- Repeated runs with unchanged output should minimize writes
- Descendant config search should avoid pathological repo scans when possible

### 15.2 Performance Strategies
- Use fast directory walking with ignore support where appropriate
- Avoid reparsing unrelated inputs across jobs when possible
- Share parsed resource graphs across jobs if inputs overlap and templates differ
- Reuse cached parser outputs on repeated runs when inputs are unchanged
- Invalidate parser cache entries on relevant file add, remove, rename, or content change
- Always rerun normalization, rendering, and output checks even when parser outputs are reused
- Skip file writes when bytes are unchanged
- Prefer stable in-memory transformations over repeated serialization/deserialization

### 15.3 Optional v1 Optimizations
- Parallel parse stage where safe

### 15.4 Measurement
The project should include benchmark fixtures for:
- single asset catalog repeated generation
- mixed assets + localization repeated generation
- nearest-workspace discovery from a member directory

## 16. Filesystem and Repo Integration

### 16.1 Multi-module Repo Support
Numi must support repositories where config may live in a submodule directory rather than the repo root.
For repositories with multiple module configs, a repo-level `numi.toml` with `[workspace]` should orchestrate existing per-module `numi.toml` files without replacing them.

### 16.2 Relative Path Semantics
All relative paths are resolved relative to the config file directory.
This includes:
- Input paths
- Output paths
- Template paths

### 16.3 Ignore Rules
V1 may optionally skip obvious build directories for downward search, such as:
- `.git`
- `.build`
- `build`
- `DerivedData`

This should apply only to discovery traversal, not to explicit input paths.

## 17. Check Mode Semantics

### 17.1 Purpose
`numi check` is for CI and pre-commit validation.

### 17.2 Behavior
Check mode must fail if:
- Output file is missing
- Output file differs from generated result
- Config is invalid
- Any job fails

### 17.3 Exit Codes
Suggested:
- `0`: success, all outputs up to date
- `1`: operational/config/render/parser error
- `2`: stale outputs detected in check mode

## 18. Init Command Spec

### 18.1 Behavior
`numi init` creates a starter config in the current directory.

### 18.2 Output
Starter file should include:
- Schema version
- One assets job example
- One localization job example
- Clear structure showing built-in vs custom templates

## 19. Suggested Project Layout for Implementation

```text
numi/
├── Cargo.toml
├── crates/
│   ├── numi-cli/
│   ├── numi-config/
│   ├── numi-core/
│   ├── numi-ir/
│   ├── numi-diagnostics/
│   ├── numi-parse-xcassets/
│   ├── numi-parse-l10n/
│   ├── numi-normalize/
│   ├── numi-context/
│   ├── numi-render/
│   ├── numi-render-minijinja/
│   └── numi-builtin-templates/
├── templates/
│   └── swift/
├── fixtures/
│   ├── xcassets-basic/
│   ├── l10n-basic/
│   └── multimodule-repo/
└── docs/
```

For faster iteration, an initial simplified layout is acceptable:

```text
crates/
  numi-cli/
  numi-core/
  numi-config/
  numi-ir/
```

Parser/render modules can start inside `numi-core`, then split later.

## 20. Testing Strategy

### 20.1 Unit Tests
- Identifier normalization
- Config parsing
- Config discovery
- Collision detection
- Context builder
- Template helper filters

### 20.2 Fixture Tests
Use real fixture directories for:
- `.xcassets`
- `.strings`
- `.xcstrings`
- Multi-config repo discovery

### 20.3 Snapshot Tests
Use snapshot tests for:
- Generated context JSON
- Built-in generated Swift output
- Diagnostics text

### 20.4 Integration Tests
Run full CLI flows:
- `generate`
- `check`
- `init`
- `config locate`
- Ambiguous config failure

### 20.5 Performance Tests
Benchmark:
- Config discovery
- Parse throughput
- Unchanged regenerate
- Many-job config execution

## 21. Implementation Roadmap

### Phase 0: Bootstrap
- Create workspace
- Set up CLI skeleton
- Implement diagnostics foundation
- Implement config parsing and discovery

### Phase 1: v1 Minimal Pipeline
- Support one config with one or more jobs
- Add `.xcassets` parsing
- Build IR
- Add normalization
- Add Minijinja rendering
- Ship one built-in asset template
- Implement `generate` and `check`

### Phase 2: Localization
- Add `.strings`
- Add `.xcstrings`
- Add localization built-in template
- Add placeholder/argument support where available
- Add `dump-context`

### Phase 3: Refinement
- Improve `.xcstrings` coverage and diagnostics
- Improve diagnostics
- Improve no-op writes and performance
- Add `init`, `config locate`, and `config print`

### Phase 4: Stabilization
- Write docs
- Add migration guide from SwiftGen
- Add benchmarks
- Add comprehensive fixture coverage

## 22. MVP Definition
A release qualifies as MVP when all of the following are true:
- `numi generate` works with discovered config
- `numi check` works for CI
- `.xcassets` image and color generation works
- Localization generation works for at least one supported format
- Built-in templates work
- Custom templates work
- Deterministic output is verified
- Ambiguity in downward config discovery is handled safely
- Diagnostics are actionable

## 23. Open Questions
These should be resolved during implementation planning:
1. Should built-in templates generate helper wrapper types or depend on external runtime support?
2. Should custom templates allow includes/partials in v1?
3. Should config support multiple TOML filenames in v1, or is `numi.toml` the only supported filename?
4. Should jobs be allowed to share a global parsed graph explicitly?
5. Should bundle resolution be purely templated or partly built into built-ins?
6. How much Swift formatting logic should be in templates vs render helpers?

## 24. Recommended Initial Defaults
To reduce decision load, start with these defaults:
- Config filename: `numi.toml`
- Template engine: Minijinja
- Asset parser: `xcassets`
- Localization parser: `langcodec` plus lightweight adapters as needed
- Output naming: preserve hierarchy
- Collision strategy: hard error
- Check mode: strict
- Config discovery: nearest local or ancestor manifest only

## 25. AI-Agent Execution Notes
This section is intended for an implementation agent.

### 25.1 Priority Order
1. Config parsing + discovery
2. CLI skeleton
3. IR definition
4. `.xcassets` parsing adapter
5. Normalization rules
6. Template engine integration
7. Built-in asset template
8. Output writing + check mode
9. Localization support
10. Diagnostics refinement

### 25.2 Implementation Constraints
- Keep public interfaces small and explicit
- Prefer deterministic data structures like `BTreeMap` for order-sensitive surfaces
- Avoid premature plugin abstractions in v1
- Keep template context schema documented as code comments and examples
- Write fixture tests immediately when adding a parser or normalization rule

### 25.3 Acceptance Style
Each completed milestone should include:
- Code
- Tests
- Fixture coverage where relevant
- Example config
- Example generated output

## 26. Example Starter Config (TOML)

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
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
```

## 27. Example Template Snippet

```jinja
{{ access_level }} enum Assets {
{% for module in modules %}
{% if module.kind == "xcassets" %}
{% for entry in module.entries recursive %}
{% if entry.kind == "namespace" %}
{{ "  " * loop.depth0 }}{{ access_level }} enum {{ entry.swiftIdentifier }} {
{{ loop(entry.children) }}
{{ "  " * loop.depth0 }}}
{% elif entry.kind == "image" %}
{{ "  " * loop.depth0 }}{{ access_level }} static let {{ entry.swiftIdentifier }} = ImageAsset(name: {{ entry.properties.assetName | string_literal }})
{% endif %}
{% endfor %}
{% endif %}
{% endfor %}
}
```

## 28. Final Positioning Statement
Numi is a fast, template-driven resource code generation tool for Apple projects, built in Rust, designed to replace rigid legacy generators with a modern IR-based pipeline that scales from simple app assets to future resource compilation workflows.
