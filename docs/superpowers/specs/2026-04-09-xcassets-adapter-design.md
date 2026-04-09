# 2026-04-09 xcassets Adapter Design

## Summary

Numi should stop manually parsing `.xcassets` catalogs and instead use the Rust `xcassets` crate as the sole parser for asset catalogs. Numi will become a thin adapter from `xcassets` parse results into Numi IR.

For the current version, Numi only generates entries for image sets and color sets. Other parsed asset node kinds are skipped with warnings.

## Goals

- Remove Numi-owned xcassets parsing logic from the runtime path.
- Make `xcassets` the source of truth for asset catalog parsing behavior.
- Preserve the current Numi generation contract for supported asset entries.
- Warn, rather than fail, when catalogs contain parsed but unsupported asset node kinds.
- Keep real-world lama-ludo asset generation working after the migration.

## Non-Goals

- Add generation support for app icon sets, data sets, symbol sets, or other asset node kinds.
- Expand the built-in asset template contract beyond current image/color behavior.
- Redesign asset identifier normalization.
- Replace Numi's IR or template pipeline.

## Current Facts

### Numi Today

Numi currently implements its own xcassets parser in `crates/numi-core/src/parse_xcassets.rs`.

That parser currently:
- walks the catalog directory recursively
- reads `Contents.json` files directly with `serde_json`
- recognizes `.imageset` and `.colorset` folders by suffix
- converts those folders into `RawEntry` values with `assetName`
- ignores all other asset folder types implicitly

This means Numi currently owns xcassets format parsing logic itself.

### xcassets Crate Today

The `xcassets` crate already provides:
- `xcassets::parse_catalog(path) -> Result<ParseReport, ParseError>`
- typed node modeling via `AssetCatalog` and `Node`
- non-fatal diagnostics alongside partial parse success
- node kinds including groups, image sets, color sets, app icon sets, and opaque unsupported folders

This is already sufficient for Numi's current generation scope.

## Design

### Parser Boundary

Numi must no longer manually parse `.xcassets` structure or `Contents.json` files in the runtime path.

Instead, `crates/numi-core/src/parse_xcassets.rs` becomes a thin adapter layer that:
- calls `xcassets::parse_catalog(catalog_path)`
- receives `ParseReport { catalog, diagnostics }`
- recursively walks the parsed catalog tree
- converts only supported node kinds into Numi IR entries
- returns both generated entries and warnings

The runtime path must not keep fallback manual parsing behavior.

### Supported Node Kinds

Numi will generate entries only for:
- `Node::ImageSet` -> `EntryKind::Image`
- `Node::ColorSet` -> `EntryKind::Color`

These entries continue to expose the existing `assetName` property so current templates remain stable.

### Unsupported Node Kinds

Parsed node kinds that are not currently supported for generation are skipped with warnings.

Examples include:
- `Node::AppIconSet`
- `Node::Opaque`
- any future node kind added by the `xcassets` crate that Numi does not yet adapt

These must not fail the job as long as `xcassets` returned a successful `ParseReport`.

### Diagnostics Model

Numi should distinguish between:

1. Fatal parser failures
   - These come from `xcassets::ParseError`.
   - They fail the job because no parse report could be produced.

2. Non-fatal warnings
   - Diagnostics emitted by the `xcassets` crate become Numi warnings.
   - Unsupported parsed node kinds also become Numi warnings.

This keeps parser health visible while allowing generation from partially supported catalogs.

### Warning Shape

Warnings emitted for unsupported node kinds should be explicit and path-rich.

Recommended wording pattern:
- `skipping asset node 'AppIcon.appiconset': unsupported asset node kind 'appiconset'`
- `skipping asset node 'Foo.dataset': unsupported asset node kind 'dataset'`

The exact wording may vary, but warnings must include:
- enough path information to identify the skipped node
- the unsupported node kind
- the fact that the node was skipped

### Asset Name Mapping

For supported image/color nodes, Numi should continue producing the same logical asset names used by current generation.

That means:
- asset names remain catalog-relative paths
- terminal typed-folder suffixes are stripped
- nested folders remain slash-separated path segments

Example:
- `Activity/activity_center_edit_btn.imageset` -> `Activity/activity_center_edit_btn`

The migration should preserve generated output for existing supported fixtures unless the `xcassets` crate reveals a more correct representation in edge cases.

### Determinism

Warning order and entry order must remain deterministic.

Numi should:
- walk parsed child nodes in the stable order provided by the `xcassets` parse tree
- preserve deterministic ordering when flattening entries
- preserve stable warning ordering for unsupported node warnings
- append crate diagnostics in their reported order

## Implementation Notes

### Numi Changes

In `numi`:
- add the `xcassets` crate dependency to `crates/numi-core/Cargo.toml`
- replace the manual directory and JSON parsing logic in `parse_xcassets.rs`
- keep the external `parse_catalog(...)`-style function contract stable for pipeline callers where practical
- adapt `xcassets` diagnostics into `numi_diagnostics::Diagnostic`
- adapt supported node kinds into `RawEntry`

### xcassets Crate Changes

No `xcassets` crate changes are required unless an actual integration gap is discovered while implementing the adapter.

If a real gap is discovered, it is acceptable to update `/Users/wendell/developer/oops-rs/xcassets`, but only for a concrete missing capability that blocks the adapter.

Numi should not extend its own parser instead of fixing the crate.

## Verification

Implementation is complete when all of the following are true:

- Numi no longer manually parses `.xcassets` JSON in the runtime path.
- Existing asset fixture tests still pass.
- New tests cover warnings for unsupported asset node kinds.
- Generated output for supported image/color catalogs remains stable or changes only for clearly justified correctness reasons.
- Real-world lama-ludo asset configs still generate successfully for supported assets.
- Fatal parse failures still fail jobs correctly.

## Risks

### Warning Churn

Real-world catalogs may contain many unsupported asset folder types, which could increase warning volume.

This is acceptable for now because the warnings are informative and non-fatal, but it may motivate warning grouping later.

### Output Drift

The `xcassets` crate may represent edge-case paths or malformed catalogs more accurately than Numi's current parser. That could cause some generated output differences.

This is acceptable if the differences are tied to correctness and verified by tests.

## Decision

Numi will drop its custom xcassets parser and use the `xcassets` crate as the sole xcassets parser.

Numi will only generate image and color entries for now.
All other parsed asset node kinds will be skipped with warnings.
