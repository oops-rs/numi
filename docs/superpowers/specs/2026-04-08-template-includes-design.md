# Template Includes Design

## Goal

Add include support for file-based custom templates so a template can reuse
partials without flattening everything into a single file.

This change applies only to custom templates loaded from disk. Built-in
templates keep their current behavior.

## Requirements

- File-based custom templates may include other templates.
- Includes must support a shared search root anchored at the config directory.
- Includes must also support local resolution relative to the including
  template's directory.
- Resolution must be deterministic.
- Ambiguous matches must fail instead of picking one implicitly.
- Errors must be explicit enough to debug from the CLI output.

## Resolution Model

Each include request is resolved against two roots:

1. The local root: the directory of the template that issued the include.
2. The shared root: the config directory for the active generation job.

For an include path such as `partials/header.jinja`, Numi builds two candidate
paths:

- `<including-template-dir>/partials/header.jinja`
- `<config-dir>/partials/header.jinja`

Resolution rules:

1. If neither candidate exists, rendering fails with a missing-include error.
2. If exactly one candidate exists, that file is loaded.
3. If both candidates exist and resolve to different files, rendering fails
   with an ambiguity error.
4. Nested includes re-anchor the local root to the directory of the nested
   template that issued the next include.
5. The shared config-root search path remains available for every nested
   include.

Numi does not introduce include prefixes or extra config for this feature.

## Implementation Boundary

The change stays in the render layer.

- The pipeline remains responsible for loading config, building context, and
  providing the config directory to the render layer.
- Custom-template rendering becomes a render session that knows the entry
  template path, the config-root search path, and the current include origin.
- Built-in rendering keeps the existing fast path and does not participate in
  filesystem include resolution.

This keeps template path logic in one place and avoids leaking resolution rules
into parsing or pipeline orchestration.

## Errors

The render layer must surface path-rich errors:

- Missing include:
  - name the requested include path
  - show the local root and shared root that were searched
- Ambiguous include:
  - name the requested include path
  - print both matching absolute paths
  - explain that rendering stopped because both local and shared roots matched
- Read failure:
  - print the resolved path
  - preserve the underlying filesystem error

## Testing

Add filesystem-backed render tests for:

- include resolved from the including template's directory
- include resolved from the shared config-root directory
- nested include resolution
- missing include failure
- ambiguous include failure when both roots match

Regression coverage must keep proving that:

- built-in template rendering still works unchanged
- single-file custom template rendering still works unchanged

## Non-Goals

- Built-in templates using includes
- Include prefixes such as `local:` or `root:`
- Template package manifests or configurable search-path lists

## Success Criteria

Issue `#1` is complete when:

- file-based custom templates can include reusable partials
- resolution behaves exactly as specified above
- ambiguous includes fail instead of being guessed
- errors are actionable from the CLI
- render-layer tests cover the supported resolution paths and failure modes
