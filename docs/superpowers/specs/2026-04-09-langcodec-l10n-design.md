# Numi `langcodec` Localization Parsing Design

## Summary

Numi should stop manually parsing localization formats.
Instead, `langcodec` should become the sole localization parser for `.strings`, `.xcstrings`, and future supported localization formats.

Numi should continue to own:

- config and input discovery
- adaptation from parsed localization data into Numi IR
- template context shaping
- rendering
- diagnostics presentation

The parser boundary itself should move to `langcodec`.

## Facts

- Numi currently parses localization inputs in custom code at [parse_l10n.rs](/Users/wendell/Developer/oops-rs/numi/crates/numi-core/src/parse_l10n.rs).
- That file currently contains:
  - a hand-written `.strings` parser
  - custom `.xcstrings` JSON structs
  - custom string-unit selection logic
  - custom escape handling
- The spec already says localization support should come from the Rust ecosystem, especially via `langcodec` where applicable.
- `langcodec` exists as a Rust library crate: `langcodec = "0.11.0"`.
- `langcodec` is also installed locally as a CLI, and direct checks against lama-ludo files show it can successfully read localization files that Numi currently rejects.
- Numi currently fails on real lama-ludo localization inputs because of its own parser behavior:
  - `.strings`: rejects `\\'` as an unsupported escape
  - `.xcstrings`: treats entries without a usable selected `stringUnit` as fatal
- Real-world validation showed the config model is working, but Task 4 is blocked by localization parser compatibility rather than config shape.

## Goals

- Make `langcodec` the only localization parser in Numi.
- Remove manual l10n parsing logic from Numi’s runtime path.
- Improve real-world compatibility against existing Apple localization files.
- Keep Numi’s IR and template system as the stable downstream surface.
- Allow additive metadata in template context when `langcodec` exposes richer information.

## Non-Goals

- Rewriting `langcodec` behavior inside Numi.
- Keeping the custom Numi parser as a fallback path.
- Shelling out to the `langcodec` CLI at runtime when a Rust library is available.
- Solving all future localization rendering semantics in the same change.
- Guaranteeing zero template-context drift when richer metadata becomes available.

## Core Rule

Numi should never parse localization formats manually.

That means:

- no hand-written `.strings` lexer/parser in Numi
- no custom `.xcstrings` JSON interpretation in Numi
- no manual escape compatibility tables in Numi
- no Numi-side reimplementation of Apple localization format semantics

Instead:

- `langcodec` parses
- Numi adapts

## Architecture

### Parser Boundary

`crates/numi-core/src/parse_l10n.rs` should become an adapter layer, not a format parser.

Its responsibilities should be:

- dispatch by input kind (`strings`, `xcstrings`, future localization kinds)
- call `langcodec`
- convert parsed localization resources into `LocalizationTable`
- shape additive metadata into `RawEntry.properties`
- translate `langcodec` parse issues into Numi diagnostics

It should no longer contain the source-of-truth parsing logic for Apple localization formats.

### Why Library Integration, Not CLI Shell-Out

Numi should depend on the `langcodec` Rust crate directly.

Reasons:

- lower runtime overhead
- cleaner error propagation
- stronger type integration
- easier testing
- no dependence on an external binary existing on the user’s machine

The `langcodec` CLI is useful for validation and debugging, but it should not be Numi’s runtime integration path.

## Data Flow

The intended localization flow becomes:

1. Numi resolves config and jobs.
2. Numi discovers localization input paths.
3. Numi invokes `langcodec` for parsing those inputs.
4. Numi converts `langcodec`’s parsed representation into `LocalizationTable`.
5. Numi normalizes that into IR/context.
6. Templates render from that context.

This keeps the parsing responsibility in one place while preserving Numi’s existing rendering architecture.

## IR Mapping

Numi should retain its own IR and module model.

Mapping rules:

- `.strings` inputs become `modules[].kind = "strings"`
- `.xcstrings` inputs become `modules[].kind = "xcstrings"`
- localization table naming remains Numi-owned
- `LocalizationTable.entries` still contain `RawEntry` values suitable for current normalization and rendering

Each localization entry should still carry, at minimum:

- `properties.key`
- `properties.translation`

Numi should continue to shape these into the existing stable surface where practical so built-in templates do not need a full redesign just to consume parsed values.

## Additive Metadata

If `langcodec` exposes richer localization data cleanly, Numi may add it to template context.

Examples:

- placeholders
- comments
- translation status
- source language
- richer value metadata that may matter later for plural or variant-aware templates

The design intent is:

- preserve useful metadata rather than hiding it
- avoid changing existing field meanings unnecessarily
- allow built-in templates to ignore what they do not use yet

This is an additive policy, not a commitment that built-in templates will immediately consume every new metadata field.

## Error And Warning Policy

### Parsing Errors

Parser errors should mostly mean:

- `langcodec` could not parse the localization input at all

Numi should not keep introducing format-validity errors that come from its own manual parsing logic.

### Adaptation Warnings

If `langcodec` can parse an entry but the current Numi adapter or built-in template surface cannot fully express it, Numi should prefer warnings plus skipped entries where possible over fatal job failure.

Examples of acceptable warning cases:

- parsed entries that exist but have no currently renderable singular value for the built-in template path
- richer variant structures that current templates ignore

The adapter should distinguish between:

- parser failure
- successful parse with partial adaptation

Those are not the same class of problem.

## Real-World Impact

This migration is intended to resolve the concrete lama-ludo validation blockers already observed:

- `.strings` files containing `\\'` should stop failing because Numi no longer owns escape parsing
- `.xcstrings` entries without Numi’s currently expected shape should be handled according to `langcodec`’s parsed model, then adapted or skipped with warnings
- representative validation should move from parser rejection toward actual output comparison

The goal is not “make these three files pass by special case.”
The goal is “stop owning localization parsing logic in Numi at all.”

## Compatibility Model

Numi should preserve the existing stable downstream ideas where reasonable:

- `modules[].kind`
- `properties.key`
- `properties.translation`

But small context-shape changes are acceptable if `langcodec` provides richer metadata and the adapter exposes it additively.

This means the migration should optimize for:

- correctness
- parser delegation clarity
- real-world compatibility

and not for preserving every accidental detail of the current custom parser.

## Implementation Direction

### Replace, Don’t Fallback

The migration should replace Numi’s manual localization parser rather than introducing a dual-path system.

Reasons:

- fallback logic weakens the architecture boundary
- dual parsing paths are harder to test and reason about
- it becomes easy for custom parsing logic to linger indefinitely
- user behavior becomes harder to explain

The clean contract is:

- localization parsing comes from `langcodec`
- Numi owns adaptation and rendering

### Adapter Responsibilities

The new adapter layer should:

- convert `langcodec` parsed resources into `LocalizationTable`
- preserve source path and table name information
- select the current best translation field for built-in `l10n` rendering
- attach warnings for partially usable records
- preserve additive metadata when available

## Testing Strategy

Testing should shift from “does Numi’s parser accept this syntax?” to “does Numi adapt `langcodec` output correctly?”

Coverage should include:

- `.strings` fixture parsing through `langcodec`
- `.xcstrings` fixture parsing through `langcodec`
- existing Numi fixture behavior staying correct at the output level
- real-world compatibility cases derived from lama-ludo inputs
- warnings for entries that parse successfully but cannot be fully adapted
- context serialization tests proving the adapter still emits the expected module kinds and core fields

The most important regression tests should come from the real-world failure shapes already identified.

## Risks

### Adapter Shape Drift

If the adapter tries to preserve too much of Numi’s old parser behavior, it may reintroduce custom parsing logic by another name.
The adapter must stay thin.

### Overexposure Of Rich Metadata

If every `langcodec` detail is dumped into template context without thought, the context can become noisy and unstable.
Metadata should be additive but intentional.

### Built-In Template Assumptions

Current built-in templates assume a fairly simple localization entry model.
If the adapter changes what counts as a “usable entry,” we need tests that prove the built-in `l10n` output stays sensible.

## Acceptance Criteria

- Numi no longer manually parses `.strings` or `.xcstrings` in the runtime path.
- Numi depends on the `langcodec` Rust crate for localization parsing.
- `parse_l10n.rs` becomes an adapter layer rather than a source-format parser.
- Real lama-ludo localization cases that currently fail due to Numi parser behavior are re-evaluated through `langcodec`.
- Numi still emits usable localization IR/context for built-in rendering.
- Additive metadata from `langcodec` may appear in template context without breaking the core `key` / `translation` surface.
- Warnings vs fatal failures are decided at the adaptation layer, not by leftover manual format parsing logic inside Numi.
