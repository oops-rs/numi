# xcassets Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Numi's manual `.xcassets` runtime parser with an adapter over the `xcassets` crate while preserving image/color generation and surfacing unsupported node kinds as warnings.

**Architecture:** Keep the pipeline and render layers unchanged at their boundary. `crates/numi-core/src/parse_xcassets.rs` becomes a thin adapter from `xcassets::ParseReport` into Numi `RawEntry` values plus `Diagnostic` warnings, and all direct filesystem / JSON parsing of asset catalogs is removed from the runtime path.

**Tech Stack:** Rust, `xcassets`, `numi-core`, `numi-diagnostics`, cargo test

---

### File Map

**Files:**
- Modify: `crates/numi-core/Cargo.toml`
- Modify: `crates/numi-core/src/parse_xcassets.rs`
- Modify: `crates/numi-core/src/pipeline.rs`
- Test: `crates/numi-cli/tests/generate_assets.rs`
- Test: `fixtures/xcassets-basic/`
- Validate: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`
- Validate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`

`parse_xcassets.rs` is the main boundary. It should own only adaptation, warning conversion, and deterministic tree walking. `pipeline.rs` should need at most a signature adjustment if the parser now returns warnings alongside entries. The generated output contract should stay stable, so most verification should come from existing asset fixture tests plus one new unsupported-node warning test.

### Task 1: Lock Adapter Expectations With Failing Tests

**Files:**
- Modify: `crates/numi-core/src/parse_xcassets.rs`
- Test: `crates/numi-cli/tests/generate_assets.rs`

- [ ] **Step 1: Add a focused parser test for unsupported asset nodes becoming warnings**

Add a new test module in `crates/numi-core/src/parse_xcassets.rs` that creates a temp `.xcassets` catalog containing one supported `.imageset` and one unsupported typed folder such as `.appiconset`, then asserts:

```rust
let report = parse_catalog(temp_catalog.path()).expect("catalog should parse");
assert_eq!(report.entries.len(), 1);
assert_eq!(report.entries[0].kind, EntryKind::Image);
assert!(report
    .warnings
    .iter()
    .any(|warning| warning.message.contains("unsupported asset node kind")));
```

- [ ] **Step 2: Run the focused parser test and confirm it fails on the current manual parser**

Run: `cargo test -p numi-core parse_xcassets -- --nocapture`

Expected: FAIL because the current parser does not return warning-aware reports for unsupported node kinds.

- [ ] **Step 3: Add a fixture-level stability test for generated asset output**

Extend `crates/numi-cli/tests/generate_assets.rs` with an assertion that generation still produces expected asset names from `fixtures/xcassets-basic`, for example:

```rust
assert!(generated.contains("ImageAsset(name: \"Common/icon_close\")"));
assert!(generated.contains("ColorAsset(name: \"Brand/primary\")"));
```

Use names that already exist in the fixture rather than inventing new fixture content.

- [ ] **Step 4: Run the asset generation test and capture the current baseline**

Run: `cargo test -p numi-cli --test generate_assets -v`

Expected: PASS on the existing fixture test, with the new unsupported-node parser test still failing.

- [ ] **Step 5: Commit the test lock-in**

```bash
git add crates/numi-core/src/parse_xcassets.rs crates/numi-cli/tests/generate_assets.rs
git commit -m "test: lock xcassets adapter expectations"
```

### Task 2: Replace Manual Parsing With an xcassets Adapter

**Files:**
- Modify: `crates/numi-core/Cargo.toml`
- Modify: `crates/numi-core/src/parse_xcassets.rs`
- Modify: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Add the `xcassets` dependency to `numi-core`**

Update `crates/numi-core/Cargo.toml` to include the crate dependency alongside the existing parser dependencies:

```toml
[dependencies]
xcassets = "0.1.0"
```

- [ ] **Step 2: Replace the parser return shape so warnings can flow out of the adapter**

Refactor `crates/numi-core/src/parse_xcassets.rs` so the public entrypoint returns both entries and warnings, for example:

```rust
pub struct XcassetsReport {
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}

pub fn parse_catalog(catalog_path: &Path) -> Result<XcassetsReport, ParseXcassetsError> {
    // adapter implementation
}
```

Keep `ParseXcassetsError` for fatal failures only.

- [ ] **Step 3: Remove manual directory and JSON parsing from the runtime path**

Delete the `fs::read_dir`, `serde_json` asset contents structs, and suffix-classification logic from `parse_xcassets.rs`. Replace it with:

```rust
let report = xcassets::parse_catalog(catalog_path).map_err(ParseXcassetsError::from)?;
let mut entries = Vec::new();
let mut warnings = map_xcassets_diagnostics(&report.diagnostics, catalog_path);
walk_nodes(&report.catalog.children, &mut entries, &mut warnings)?;
entries.sort_by(|left, right| left.path.cmp(&right.path));
Ok(XcassetsReport { entries, warnings })
```

- [ ] **Step 4: Implement deterministic node walking for supported image/color nodes**

Add helpers in `parse_xcassets.rs` that recurse through `xcassets::Node` and convert only supported nodes:

```rust
fn walk_nodes(
    nodes: &[xcassets::Node],
    entries: &mut Vec<RawEntry>,
    warnings: &mut Vec<Diagnostic>,
) -> Result<(), ParseXcassetsError> {
    for node in nodes {
        match node {
            xcassets::Node::Group(group) => walk_nodes(&group.children, entries, warnings)?,
            xcassets::Node::ImageSet(node) => entries.push(image_entry(node)?),
            xcassets::Node::ColorSet(node) => entries.push(color_entry(node)?),
            xcassets::Node::AppIconSet(node) => warnings.push(unsupported_node_warning(
                &node.relative_path,
                "appiconset",
            )),
            xcassets::Node::Opaque(node) => warnings.push(unsupported_node_warning(
                &node.relative_path,
                &node.folder_type,
            )),
        }
    }
    Ok(())
}
```

Use the crate's `relative_path` as the source of truth for deriving `assetName`.

- [ ] **Step 5: Map supported nodes into the existing `RawEntry` shape**

Keep the current IR contract stable:

```rust
RawEntry {
    path: asset_name.clone(),
    source_path: utf8_path(catalog_root.join(&node.relative_path))?,
    kind: EntryKind::Image, // or Color
    properties: Metadata::from([(
        "assetName".to_string(),
        Value::String(asset_name),
    )]),
}
```

Derive `asset_name` by stripping only the terminal typed-folder suffix and joining path segments with `/`.

- [ ] **Step 6: Convert `xcassets` diagnostics into Numi warnings**

Add a helper that projects crate diagnostics into `numi_diagnostics::Diagnostic` warnings while preserving path-rich messages. The warnings should remain non-fatal and deterministic:

```rust
fn map_xcassets_diagnostics(
    diagnostics: &[xcassets::Diagnostic],
    catalog_path: &Path,
) -> Vec<Diagnostic> {
    // severity -> warning, path preserved when available
}
```

- [ ] **Step 7: Adjust `pipeline.rs` to collect parser warnings**

Update the asset-input branch in `crates/numi-core/src/pipeline.rs` so it handles the new report shape, for example:

```rust
let report = parse_catalog(&input_path).map_err(|source| GenerateError::ParseXcassets {
    job: job.name.clone(),
    source,
})?;
warnings.extend(report.warnings);
raw_entries.extend(report.entries);
```

- [ ] **Step 8: Run focused tests for the adapter path**

Run:

```bash
cargo test -p numi-core parse_xcassets -- --nocapture
cargo test -p numi-cli --test generate_assets -v
```

Expected: PASS for both commands.

- [ ] **Step 9: Commit the adapter migration**

```bash
git add crates/numi-core/Cargo.toml crates/numi-core/src/parse_xcassets.rs crates/numi-core/src/pipeline.rs crates/numi-cli/tests/generate_assets.rs
git commit -m "refactor: use xcassets for asset catalog parsing"
```

### Task 3: Validate Real-World Catalogs And Warning Behavior

**Files:**
- Validate: `/Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`
- Validate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`
- Validate: `/Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml`

- [ ] **Step 1: Build the validation binary from the integrated branch**

Run: `cargo build -p numi-cli -q`

Expected: PASS with an updated `target/debug/numi`.

- [ ] **Step 2: Run the asset-focused lama-ludo validation configs**

Run:

```bash
/Users/wendell/Developer/oops-rs/numi/.worktrees/numi-toml/target/debug/numi generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml
/Users/wendell/Developer/oops-rs/numi/.worktrees/numi-toml/target/debug/numi generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml
/Users/wendell/Developer/oops-rs/numi/.worktrees/numi-toml/target/debug/numi generate --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml
```

Expected:
- `AppUI` assets still generate
- `Game` assets still generate and any unsupported asset node kinds appear as warnings rather than failures
- `Profile` assets still generate

- [ ] **Step 3: Run the core pipeline regression suite**

Run: `cargo test -p numi-core pipeline::tests -- --nocapture`

Expected: PASS

- [ ] **Step 4: Run the asset + l10n + files CLI regressions**

Run:

```bash
cargo test -p numi-cli --test config_commands -v
cargo test -p numi-cli --test generate_assets -v
cargo test -p numi-cli --test generate_l10n -v
cargo test -p numi-cli --test generate_files -v
```

Expected: PASS

- [ ] **Step 5: Commit the validation checkpoint**

```bash
git add .
git commit -m "test: validate xcassets adapter on real catalogs"
```

### Task 4: Final Review And Handoff

**Files:**
- Review: `crates/numi-core/src/parse_xcassets.rs`
- Review: `crates/numi-core/src/pipeline.rs`
- Review: `crates/numi-cli/tests/generate_assets.rs`

- [ ] **Step 1: Review the runtime path for leftover manual parsing logic**

Search for removed parsing primitives:

```bash
rg -n "read_dir|read_to_string|\\.imageset|\\.colorset|serde_json" crates/numi-core/src/parse_xcassets.rs
```

Expected: only remaining references are those needed for adapting paths or tests, not runtime directory / JSON parsing logic.

- [ ] **Step 2: Run formatting and the full Rust verification set**

Run:

```bash
cargo fmt --check
cargo test -v
```

Expected: PASS

- [ ] **Step 3: Summarize validation evidence in the handoff**

Record:
- which commands passed
- whether any unsupported asset node warnings appeared in lama-ludo
- whether generated asset output changed
- whether any `xcassets` crate gap was discovered

- [ ] **Step 4: Commit the final polish if needed**

```bash
git add .
git commit -m "chore: finalize xcassets adapter migration"
```

### Spec Coverage Check

- Parser boundary: covered in Task 2 Steps 2-4.
- Supported image/color-only projection: covered in Task 2 Steps 4-5.
- Unsupported node warnings: covered in Task 1 Step 1 and Task 2 Steps 4 and 6.
- Deterministic ordering: covered in Task 2 Steps 4 and 8.
- Real-world lama-ludo validation: covered in Task 3 Step 2.
- Removal of manual runtime parsing: covered in Task 2 Step 3 and Task 4 Step 1.

### Placeholder Scan

- No `TODO` or `TBD` markers remain.
- All verification commands and touched files are concrete.
