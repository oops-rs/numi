# Generic Built-in Template Languages Design

## Summary

Replace the current Swift-shaped built-in template selector with a generic built-in language model so Numi can ship built-ins for multiple languages. Ship Objective-C as the first non-Swift built-in language and organize embedded templates under `templates/<language>/<name>.jinja`.

## Goal

Let users select built-in templates in `numi.toml` using a generic language-plus-name shape, support workspace-level default built-in languages, and ship a new Objective-C built-in template family for assets, localization, and files.

## Current Facts

- Built-in template config is currently modeled around a single `swift` namespace.
- Validation only knows about Swift built-in template names.
- Rendering resolves built-ins through hard-coded names in `crates/numi-core/src/render.rs`.
- Embedded template files currently live only under `templates/swift/`.
- The current built-in Swift templates are `swiftui-assets`, `l10n`, and `files`.
- The user wants true built-in Objective-C templates, not example custom templates.
- The user also wants the model to be language-generic rather than adding another one-off sibling namespace.

## Non-Goals

- Backward compatibility for the old `[jobs.*.template.builtin] swift = "..."` shape.
- A plugin or runtime template-loading system for built-ins.
- More languages beyond Swift and Objective-C in this change.
- Expanding the template context model beyond what the current Swift built-ins already receive.

## Config Model

Built-in templates should be selected with a generic shape:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

Objective-C built-ins use the same shape:

```toml
[jobs.assets.template.builtin]
language = "objc"
name = "assets"
```

The built-in selector now identifies a template by the tuple `(language, name)`.

## Workspace Defaults

Workspace defaults should be able to provide a default built-in language for jobs that use built-ins. The effective rules are:

1. A job-level `template.builtin.language` wins when present.
2. Otherwise, inherit the workspace default built-in language if present.
3. A job must still provide a built-in `name`; the workspace default language never invents a template name.
4. If no effective language exists, validation fails.

This keeps job configs concise when many workspace members target the same language.

## Template Registry And Resolution

Built-in templates should be embedded and resolved from a static registry keyed by `(language, name)`. The registry should not depend on runtime filesystem discovery.

The on-disk layout for embedded source files becomes:

```text
templates/swift/<name>.jinja
templates/objc/<name>.jinja
```

The renderer should resolve built-ins by looking up the exact `(language, name)` pair in the embedded registry. Unknown languages or unknown names for a known language should fail validation and rendering with clear diagnostics.

## Shipped Built-ins

This change should ship these built-in templates:

- `swift/swiftui-assets`
- `swift/l10n`
- `swift/files`
- `objc/assets`
- `objc/l10n`
- `objc/files`

Swift keeps its current built-in names. Objective-C can use names that match its output family without forcing Swift naming onto it.

## Validation Rules

Validation must enforce:

- `builtin.language` is required when no workspace default provides it.
- `builtin.name` is always required.
- `builtin.language` must be one of the shipped built-in languages.
- `builtin.name` must be valid for the effective language.
- `template.path` and `template.builtin` remain mutually exclusive.

Diagnostics should point to the exact config path and list the allowed values for the active language when a built-in name is invalid.

## Rendering Rules

Rendering should stay otherwise unchanged:

- custom templates still use `template.path`
- built-in templates still receive the current stable template context
- generation fingerprints should include both built-in language and built-in name
- changing either language or name should invalidate the cached generation contract

## Documentation

Update the public docs and examples to use the new config shape. At minimum:

- README examples for built-ins
- any starter or example config that uses built-ins
- docs that describe built-in template selection

The docs should also mention that built-ins now live under language families and that Objective-C built-ins are available for assets, localization, and files.

## Testing

The change should include:

- config parsing tests for the new built-in shape
- validation tests for missing and unknown languages and names
- workspace inheritance tests for default built-in language behavior
- rendering tests for at least one Objective-C built-in
- CLI or pipeline integration tests showing an Objective-C built-in can be selected in `numi.toml`
- updates to existing tests and fixtures that still use the old Swift-shaped config

## Migration

This is a deliberate config-shape change. Existing configs and tests in the repo should be migrated in the same change from:

```toml
[jobs.assets.template.builtin]
swift = "swiftui-assets"
```

to:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

The implementation should remove the old schema rather than carrying both forms.

## Risks

### Risk: The schema change breaks more tests and fixtures than expected

Mitigation:

- update all built-in-using fixtures in the same change
- add focused parsing and integration coverage before changing implementation

### Risk: Workspace default language behavior becomes implicit or confusing

Mitigation:

- limit defaulting to `language` only
- require `name` to stay explicit at the job level
- document precedence rules clearly

### Risk: Objective-C built-ins overpromise output quality

Mitigation:

- keep the first templates narrow and deterministic
- prefer simple, practical Objective-C output over trying to cover every style preference

## Success Criteria

The feature is complete when:

- built-in config is expressed as `language + name`
- workspace defaults can provide a built-in language
- Swift built-ins still work after migration
- Objective-C built-ins exist for assets, localization, and files
- rendering and validation resolve built-ins by `(language, name)`
- docs and tests reflect the new model
