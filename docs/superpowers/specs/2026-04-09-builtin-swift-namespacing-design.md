# Built-In Swift Namespacing Design

## Summary

Numi should be described and structured as a general-purpose code generator, even though the current shipped templates target the Apple ecosystem.

The current built-in template model uses a flat string:

```toml
[jobs.template]
builtin = "l10n"
```

That shape implies a single global built-in namespace and is awkward for a product that should eventually support multiple output languages. The first step is to namespace built-ins by language family and make the currently shipped family explicit:

```toml
[jobs.template.builtin]
swift = "l10n"
```

At the same time, the repository layout should stop using a `builtin` folder for shipped templates. The current folder should be renamed to `templates/swift`, because today those templates are Swift-specific, not generic built-ins.

## Facts And Constraints

- Numi currently renders templates through `TemplateConfig` in `crates/numi-config`.
- Render dispatch currently resolves a flat built-in string in `crates/numi-core/src/render.rs`.
- The shipped templates are Swift-only:
  - `swiftui-assets`
  - `l10n`
  - `files`
- The built-in template sources are compile-time `include_str!` assets, so renaming their folder is low risk as long as all include paths are updated consistently.
- Numi has not been published yet, so backward compatibility with the old `builtin = "..."` shape is not required.
- The current implementation stage is Apple-focused, but the product framing should not claim Numi is inherently Swift-only.

## Goals

- Make built-in template selection explicitly language-scoped.
- Keep the current shipped behavior intact for Swift jobs.
- Rename the internal template directory to reflect reality.
- Update product-facing wording so Numi is framed as a general-purpose generator with current Swift/Apple support.

## Non-Goals

- Adding non-Swift built-ins in this change.
- Changing the runtime template engine or context schema.
- Designing a full multi-language plugin system.
- Preserving compatibility with the old flat `builtin = "..."` config shape.

## Recommended Design

### Config Shape

Replace the flat built-in string field with a namespaced built-in table:

```toml
[jobs.template.builtin]
swift = "l10n"
```

`TemplateConfig` should represent `builtin` as a structured table rather than `Option<String>`.

For this phase, the valid built-in source is:

- `template.builtin.swift`

The existing `template.path` custom-template source remains unchanged.

The invariant stays the same at a higher level:

- each job template must select exactly one source
- that source is either a built-in namespace table or a custom template path

### Validation Rules

Validation should enforce:

- exactly one template source is set: `template.builtin` or `template.path`
- when `template.builtin` is set, exactly one built-in namespace is selected
- for this phase, the only valid namespace is `swift`
- `template.builtin.swift` must be one of the shipped Swift built-ins

Invalid examples should fail clearly:

- both `template.path` and `template.builtin` set
- empty `[jobs.template.builtin]`
- unsupported namespace such as `kotlin = "resources"`
- unknown Swift built-in such as `swift = "foobar"`

### Render Dispatch

Render dispatch should become two-step:

1. resolve the built-in namespace from config
2. resolve the template name within that namespace

For this change, that means:

- if `template.builtin.swift = "l10n"`, render the Swift `l10n` template
- if `template.builtin.swift = "swiftui-assets"`, render the Swift `swiftui-assets` template
- if `template.builtin.swift = "files"`, render the Swift `files` template

This keeps the future extension point obvious without introducing speculative abstractions.

### Filesystem Layout

Rename:

- `templates/builtin/` -> `templates/swift/`

The shipped files remain:

- `templates/swift/swiftui-assets.jinja`
- `templates/swift/l10n.jinja`
- `templates/swift/files.jinja`

Code should stop referring to these as “built-in folder” assets and instead treat them as shipped Swift templates.

### Product Wording

Product-facing documentation should shift from:

- “Rust CLI for generating Swift code from resource files”

to wording in this direction:

- “Rust CLI for generating code from structured project resources”
- “Today it ships Swift templates for the Apple ecosystem”

This change should be applied where it affects the primary user understanding:

- `README.md`
- starter config examples
- migration docs where built-ins are described

The docs should still be honest that current built-ins are Swift-oriented and Apple-oriented.

## Data Model Direction

The simplest data model for this phase is:

- `TemplateConfig.path: Option<String>`
- `TemplateConfig.builtin: Option<BuiltinTemplateConfig>`
- `BuiltinTemplateConfig.swift: Option<String>`

This shape is preferable to encoding the namespace into a single string because:

- validation stays explicit
- TOML mirrors the conceptual model
- future namespaces can be added additively
- error messages can point to concrete config fields

## Testing Strategy

Add or update tests to cover:

- config parsing of `[jobs.template.builtin] swift = "..."`
- config validation for exactly-one-source behavior
- rejection of invalid built-in namespace combinations
- generator success for existing Swift fixtures after migrating them to the new config shape
- init/starter-config output using the new namespaced built-in syntax
- docs/examples aligned with the new syntax

The existing generate/check/dump-context tests should continue to prove runtime behavior after fixture migration.

## Implementation Outline

1. Change `TemplateConfig` to parse structured built-ins.
2. Update validation to enforce the new template-source invariants.
3. Update render dispatch to resolve `builtin.swift`.
4. Rename `templates/builtin` to `templates/swift` and fix all `include_str!` paths.
5. Migrate fixtures, starter config, and tests to `[jobs.template.builtin] swift = "..."`.
6. Update README and migration docs to describe Numi as general-purpose with current Swift/Apple support.

## Open Questions Resolved

- Should Numi stay described as Swift-only?
  - No. It should be framed as a general-purpose generator whose current shipped templates target the Apple ecosystem.
- Should we preserve the old flat `builtin = "..."` syntax?
  - No. Numi has not been published, so compatibility is not required.
- Should the built-in folder remain named `builtin`?
  - No. It should be renamed to `swift` because that is the actual scope of the shipped templates today.
