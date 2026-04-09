# Coordinated Issues 2 3 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land deterministic larger benchmark fixtures, parser-boundary caching, and explicit workspace orchestration on one coordinated branch without changing existing single-config behavior.

**Architecture:** Expand the fixture and benchmark bed first so the branch has realistic multimodule inputs. Then add a disk-backed parsed-input cache below normalization and rendering, keyed by input kind, canonical input path, and a deterministic content fingerprint. Finally add a separate `numi-workspace.toml` manifest plus `numi workspace generate/check` commands that compose the existing single-config execution path rather than replacing it.

**Tech Stack:** Rust 2024, Clap 4, Serde, Criterion, existing fixture-backed CLI integration tests, existing `numi-config` discovery and validation, existing `numi-core` pipeline and render path.

---

### Task 1: Expand Deterministic Fixture Coverage For Multimodule And Mixed Benchmarks

**Files:**
- Modify: `fixtures/multimodule-repo/AppUI/numi.toml`
- Modify: `fixtures/multimodule-repo/Core/numi.toml`
- Create: `fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Contents.json`
- Create: `fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Brand.colorset/Contents.json`
- Create: `fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Illustration.imageset/Contents.json`
- Create: `fixtures/multimodule-repo/Core/Resources/Localization/en.lproj/Localizable.strings`
- Create: `fixtures/multimodule-repo/Core/Resources/Localization/fr.lproj/Localizable.strings`
- Create: `fixtures/bench-mixed-large/numi.toml`
- Create: `fixtures/bench-mixed-large/Resources/Assets.xcassets/Contents.json`
- Create: `fixtures/bench-mixed-large/Resources/Localization/en.lproj/Localizable.strings`
- Create: `fixtures/bench-mixed-large/Resources/Localization/fr.lproj/Localizable.strings`

- [ ] **Step 1: Populate the multimodule fixture with real AppUI asset data**

Use committed JSON payloads rather than generated-at-runtime files. Update `fixtures/multimodule-repo/AppUI/numi.toml` only if needed to keep the existing job shape pointing at `Resources/Assets.xcassets`.

Create the asset catalog files with contents like:

```json
// fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Contents.json
{
  "info": {
    "author": "xcode",
    "version": 1
  }
}
```

```json
// fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Brand.colorset/Contents.json
{
  "colors": [
    {
      "idiom": "universal",
      "color": {
        "color-space": "srgb",
        "components": {
          "red": "0.129",
          "green": "0.435",
          "blue": "0.914",
          "alpha": "1.000"
        }
      }
    }
  ],
  "info": {
    "author": "xcode",
    "version": 1
  }
}
```

```json
// fixtures/multimodule-repo/AppUI/Resources/Assets.xcassets/Illustration.imageset/Contents.json
{
  "images": [
    {
      "idiom": "universal",
      "filename": "illustration.pdf"
    }
  ],
  "info": {
    "author": "xcode",
    "version": 1
  },
  "properties": {
    "preserves-vector-representation": true
  }
}
```

- [ ] **Step 2: Populate the multimodule fixture with real Core localization data**

Keep the existing `fixtures/multimodule-repo/Core/numi.toml` job shape, but add real localized resources under `Resources/Localization`.

Create `.strings` files like:

```text
// fixtures/multimodule-repo/Core/Resources/Localization/en.lproj/Localizable.strings
"profile.title" = "Profile";
"profile.subtitle" = "Multimodule fixture";
"settings.about" = "About";
```

```text
// fixtures/multimodule-repo/Core/Resources/Localization/fr.lproj/Localizable.strings
"profile.title" = "Profil";
"profile.subtitle" = "Fixture multimodule";
"settings.about" = "A propos";
```

- [ ] **Step 3: Add a larger mixed benchmark fixture with both assets and localization**

Create `fixtures/bench-mixed-large/numi.toml` with both an asset job and a localization job:

```toml
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template.builtin]
swift = "swiftui-assets"

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template.builtin]
swift = "l10n"
```

Use committed resource files under `Resources/Assets.xcassets` and `Resources/Localization`. Keep the contents static and sorted so repeated copies stay byte-stable.

- [ ] **Step 4: Verify the new fixtures are valid inputs before touching benchmarks**

Run:

```bash
cargo test -p numi-cli --test generate_assets repeated_generate_is_byte_stable -- --exact
cargo test -p numi-cli --test generate_l10n repeated_l10n_generate_is_byte_stable -- --exact
cargo test -p numi-cli --test config_commands config_locate_reports_ambiguous_descendant_configs -- --exact
```

Expected: PASS on the existing tests, proving the new fixture payloads did not regress current behavior.

- [ ] **Step 5: Commit the fixture expansion**

```bash
git add fixtures/multimodule-repo fixtures/bench-mixed-large
git commit -m "test: add deterministic multimodule benchmark fixtures"
```

### Task 2: Extend The Benchmark Harness And Document Scenario Coverage

**Files:**
- Modify: `crates/numi-core/benches/pipeline.rs`
- Modify: `README.md`
- Modify: `docs/spec.md`

- [ ] **Step 1: Refactor the benchmark harness into reusable fixture-preparation helpers**

Restructure `crates/numi-core/benches/pipeline.rs` so each benchmark can copy a named fixture into a temp directory and return the working root plus resolved config paths.

Add helpers with shapes like:

```rust
fn prepare_fixture(fixture_name: &str, bench_name: &str) -> (PathBuf, PathBuf) {
    let temp_root = make_temp_dir(bench_name);
    let fixture_root = repo_root().join("fixtures").join(fixture_name);
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);
    (temp_root, working_root)
}

fn config_path(working_root: &Path) -> PathBuf {
    working_root.join("numi.toml")
}
```

- [ ] **Step 2: Add explicit benchmark cases for mixed generation and multimodule discovery**

Extend `criterion_group!` to include benchmark functions like:

```rust
fn benchmark_generate_assets_fixture(c: &mut Criterion) { /* existing xcassets-basic path */ }

fn benchmark_generate_mixed_large_fixture(c: &mut Criterion) {
    let (temp_root, working_root) = prepare_fixture("bench-mixed-large", "pipeline-mixed-large");
    let config_path = config_path(&working_root);

    numi_core::generate(&config_path, None).expect("fixture warm-up generate should succeed");

    c.bench_function("generate_mixed_large_fixture", |b| {
        b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
    });

    std::fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

fn benchmark_discover_multimodule_root(c: &mut Criterion) {
    let (temp_root, working_root) = prepare_fixture("multimodule-repo", "discover-multimodule");

    c.bench_function("discover_multimodule_root_fixture", |b| {
        b.iter(|| {
            let result = numi_config::discover_config(&working_root, None);
            assert!(result.is_err(), "fixture root should remain ambiguous");
        });
    });

    std::fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
```

- [ ] **Step 3: Update the README fixture and benchmark section**

Add a short benchmark-coverage section to `README.md` that explicitly lists:

```md
Useful fixtures:

- `fixtures/xcassets-basic`
- `fixtures/l10n-basic`
- `fixtures/xcstrings-basic`
- `fixtures/multimodule-repo`
- `fixtures/bench-mixed-large`

Benchmark scenarios currently measured:

- unchanged repeated generation for a single asset fixture
- unchanged repeated generation for a mixed assets + localization fixture
- multimodule config discovery from an ambiguous repo root
```

- [ ] **Step 4: Align the spec measurement bullets with the implemented scenarios**

In `docs/spec.md`, keep the existing measurement section but make the wording concrete enough to match the new benchmark names:

```md
The project should include benchmark fixtures for:
- single asset catalog repeated generation
- ambiguous multi-module repo config discovery
- mixed assets + localization repeated generation
- unchanged re-run performance with parser-cache hits once caching lands
```

- [ ] **Step 5: Verify benchmark compilation and docs consistency**

Run:

```bash
cargo bench -p numi-core --bench pipeline --no-run
cargo test -p numi-cli --test cli_help -v
```

Expected: PASS, and the benchmark target compiles with the new scenarios.

- [ ] **Step 6: Commit the benchmark harness updates**

```bash
git add crates/numi-core/benches/pipeline.rs README.md docs/spec.md
git commit -m "bench: add multimodule and mixed fixture coverage"
```

### Task 3: Add A Disk-Backed Parsed-Input Cache Module

**Files:**
- Create: `crates/numi-core/src/parse_cache.rs`
- Modify: `crates/numi-core/src/lib.rs`
- Modify: `crates/numi-core/Cargo.toml`
- Modify: `crates/numi-core/src/parse_l10n.rs`
- Modify: `crates/numi-core/src/parse_xcassets.rs`
- Modify: `crates/numi-ir/src/lib.rs`
- Modify: `crates/numi-ir/src/normalize.rs`
- Modify: `crates/numi-diagnostics/src/lib.rs`

- [ ] **Step 1: Add serde round-trip support for the types that cached parser payloads need**

Update the existing derives so cached parser outputs can be serialized and deserialized. Use `serde::{Serialize, Deserialize}` symmetry on the concrete types used in cache payloads.

Examples:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalizationTable {
    pub table_name: String,
    pub source_path: Utf8PathBuf,
    pub module_kind: ModuleKind,
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}
```

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
    pub hint: Option<String>,
    pub job: Option<String>,
    pub path: Option<PathBuf>,
}
```

Apply the same `Deserialize` addition to:

- `Severity`
- `ModuleKind`
- `EntryKind`
- `RawEntry`
- `XcassetsReport`

- [ ] **Step 2: Add the cache module and its on-disk record types**

Create `crates/numi-core/src/parse_cache.rs` with:

```rust
use crate::parse_l10n::LocalizationTable;
use crate::parse_xcassets::XcassetsReport;
use numi_ir::RawEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CACHE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CachedParseData {
    Xcassets(XcassetsReport),
    Strings(Vec<LocalizationTable>),
    Xcstrings(Vec<LocalizationTable>),
    Files(Vec<RawEntry>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheRecord {
    schema_version: u32,
    fingerprint: String,
    data: CachedParseData,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheKind {
    Xcassets,
    Strings,
    Xcstrings,
    Files,
}
```

- [ ] **Step 3: Implement deterministic cache-root and fingerprint helpers**

Keep the cache outside the repo working tree. Use the system temp dir plus a Numi-specific folder:

```rust
fn cache_root() -> PathBuf {
    std::env::temp_dir().join("numi-cache").join(format!("parsed-v{}", CACHE_SCHEMA_VERSION))
}
```

Add a deterministic fingerprint builder that:

- canonicalizes the input path
- walks only files relevant to the parser kind
- sorts all relative file paths before hashing
- hashes both relative path and file contents

Implement helper shapes like:

```rust
pub fn fingerprint_input(kind: CacheKind, path: &Path) -> Result<String, CacheError> { /* ... */ }

fn relevant_files(kind: CacheKind, path: &Path) -> Result<Vec<PathBuf>, CacheError> { /* ... */ }
```

Use parser-aligned relevance rules:

- `Xcassets`: all regular files below the catalog root
- `Strings`: only `.strings` files
- `Xcstrings`: only `.xcstrings` files
- `Files`: all regular files except `.DS_Store`

- [ ] **Step 4: Implement load/store helpers that do not hide parse errors**

Add cache helpers that either return a verified cached payload or fall through to the caller’s parser closure:

```rust
pub fn load(kind: CacheKind, path: &Path) -> Result<Option<CachedParseData>, CacheError> { /* ... */ }

pub fn store(
    kind: CacheKind,
    path: &Path,
    fingerprint: &str,
    data: &CachedParseData,
) -> Result<(), CacheError> { /* ... */ }
```

Do not swallow parser failures. Cache misses and unreadable cache files should behave like misses; actual parser failures should still bubble up from the parser closure.

- [ ] **Step 5: Add focused unit tests for cache key and record behavior**

In `parse_cache.rs`, add tests like:

```rust
#[test]
fn fingerprint_changes_when_matching_file_contents_change() { /* write file, hash, mutate, hash */ }

#[test]
fn fingerprint_ignores_non_matching_files_for_strings_directory() { /* add png beside .strings */ }

#[test]
fn cache_record_round_trips_xcassets_payload() { /* store/load Xcassets report */ }
```

- [ ] **Step 6: Verify the cache module in isolation**

Run:

```bash
cargo test -p numi-core parse_cache -- --nocapture
cargo test -p numi-ir --lib
cargo test -p numi-diagnostics --lib
```

Expected: PASS on the new cache unit tests and the serde-derived support types.

- [ ] **Step 7: Commit the cache foundation**

```bash
git add crates/numi-core/src/parse_cache.rs crates/numi-core/src/lib.rs crates/numi-core/Cargo.toml crates/numi-core/src/parse_l10n.rs crates/numi-core/src/parse_xcassets.rs crates/numi-ir/src/lib.rs crates/numi-ir/src/normalize.rs crates/numi-diagnostics/src/lib.rs
git commit -m "feat: add parsed input cache foundation"
```

### Task 4: Wire The Parsed-Input Cache Through Generate, Check, And Dump-Context

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Modify: `crates/numi-core/benches/pipeline.rs`
- Modify: `crates/numi-cli/tests/generate_assets.rs`
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
- Modify: `crates/numi-cli/tests/generate_files.rs`

- [ ] **Step 1: Add parser-cache entrypoints in `build_modules`**

In `crates/numi-core/src/pipeline.rs`, replace direct parser calls with cache-aware helpers that still return the same parser payloads to the rest of the pipeline.

The new shape should look like:

```rust
match input.kind.as_str() {
    "xcassets" => {
        let report = load_or_parse_xcassets(&input_path, &job.name)?;
        warnings.extend(report.warnings.iter().cloned().map(|w| w.with_job(job.name.clone())));
        asset_entries.extend(report.entries);
    }
    "strings" => {
        let tables = load_or_parse_strings(&input_path, &job.name)?;
        // existing duplicate-table and normalize_scope logic stays here
    }
    "xcstrings" => {
        let tables = load_or_parse_xcstrings(&input_path, &job.name)?;
        // existing duplicate-table and normalize_scope logic stays here
    }
    "files" => {
        let raw_entries = load_or_parse_files(&input_path, &job.name)?;
        // existing normalize_scope logic stays here
    }
    other => { /* unchanged unsupported-kind path */ }
}
```

- [ ] **Step 2: Keep normalization, duplicate checks, rendering, and stale-output logic unchanged**

Do not move `normalize_scope`, duplicate localization table detection, bundle resolution, `AssetTemplateContext::new`, or `render_job` into the cache path. Preserve the current control flow after parser payload retrieval:

```rust
let entries = normalize_scope(&job.name, raw_entries).map_err(GenerateError::Diagnostics)?;
```

and:

```rust
let rendered = render_job(config_dir, job, &context)?;
```

- [ ] **Step 3: Add pipeline-level tests that prove correctness under cache hits**

Add or extend unit tests in `pipeline.rs` to cover cache-hit correctness with temp directories:

```rust
#[test]
fn generate_reuses_cached_strings_parse_without_changing_output() { /* generate twice, assert bytes equal */ }

#[test]
fn check_reuses_cached_files_parse_and_still_reports_stale_outputs() { /* generate, mutate output only, check == stale */ }

#[test]
fn dump_context_reuses_cached_xcstrings_parse_and_keeps_json_stable() { /* dump twice, compare JSON */ }
```

The assertions should focus on observable correctness:

- same output bytes for repeated `generate`
- same JSON for repeated `dump-context`
- same stale detection for `check`

- [ ] **Step 4: Turn the warmed benchmark into an explicit cache-hit scenario**

Update the benchmark names in `crates/numi-core/benches/pipeline.rs` so the warmed rerun is clearly the cache-hit measurement:

```rust
c.bench_function("generate_assets_cached_rerun_fixture", |b| {
    b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
});

c.bench_function("generate_mixed_large_cached_rerun_fixture", |b| {
    b.iter(|| numi_core::generate(&config_path, None).expect("fixture generate should work"));
});
```

Keep the one-time warm-up call before each `b.iter(...)` loop so the repeated iterations measure cache-hit behavior.

- [ ] **Step 5: Re-run the fixture-backed CLI tests plus the benchmark compile gate**

Run:

```bash
cargo test -p numi-core pipeline::tests -- --nocapture
cargo test -p numi-cli --test generate_assets -v
cargo test -p numi-cli --test generate_l10n -v
cargo test -p numi-cli --test generate_files -v
cargo bench -p numi-core --bench pipeline --no-run
```

Expected: PASS, with no behavior regressions and the benchmark target still compiling.

- [ ] **Step 6: Commit the cache integration**

```bash
git add crates/numi-core/src/pipeline.rs crates/numi-core/benches/pipeline.rs crates/numi-cli/tests/generate_assets.rs crates/numi-cli/tests/generate_l10n.rs crates/numi-cli/tests/generate_files.rs
git commit -m "feat: cache parsed inputs across invocations"
```

### Task 5: Add A Separate Workspace Manifest Schema And Discovery Path

**Files:**
- Create: `crates/numi-config/src/workspace.rs`
- Modify: `crates/numi-config/src/lib.rs`
- Modify: `crates/numi-config/src/model.rs`

- [ ] **Step 1: Define a dedicated `numi-workspace.toml` schema instead of extending `Config`**

Create `crates/numi-config/src/workspace.rs` with a separate model:

```rust
use serde::{Deserialize, Serialize};

pub const WORKSPACE_FILE_NAME: &str = "numi-workspace.toml";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub members: Vec<WorkspaceMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMember {
    pub config: String,
    #[serde(default)]
    pub jobs: Vec<String>,
}
```

- [ ] **Step 2: Add parsing, validation, and discovery helpers for workspace manifests**

In the same file, add:

```rust
pub fn load_workspace_from_path(path: &Path) -> Result<LoadedWorkspace, ConfigError> { /* ... */ }

pub fn discover_workspace(start_dir: &Path, explicit_path: Option<&Path>) -> Result<PathBuf, DiscoveryError> {
    discover_named_file(start_dir, explicit_path, WORKSPACE_FILE_NAME)
}
```

Validate:

- `version == 1`
- at least one member
- no duplicate `members[].config`
- if `jobs` is present, it is not empty and does not contain duplicates

- [ ] **Step 3: Reuse the existing discovery algorithm rather than cloning it**

Refactor `crates/numi-config/src/lib.rs` or `discovery.rs` so both config and workspace discovery share a named-file helper:

```rust
pub fn discover_config(start_dir: &Path, explicit_path: Option<&Path>) -> Result<PathBuf, DiscoveryError> {
    discover_named_file(start_dir, explicit_path, CONFIG_FILE_NAME)
}

pub fn discover_workspace(start_dir: &Path, explicit_path: Option<&Path>) -> Result<PathBuf, DiscoveryError> {
    discover_named_file(start_dir, explicit_path, WORKSPACE_FILE_NAME)
}
```

Keep the existing `numi.toml` behavior unchanged.

- [ ] **Step 4: Export the workspace API without breaking current config callers**

Add exports in `crates/numi-config/src/lib.rs` like:

```rust
pub use workspace::{
    WORKSPACE_FILE_NAME, WorkspaceConfig, WorkspaceMember, discover_workspace,
    load_workspace_from_path,
};
```

Do not modify the existing `Config` schema or `validate_config` logic for `numi.toml`.

- [ ] **Step 5: Add focused workspace-model tests in `numi-config`**

Add tests covering:

```rust
#[test]
fn parses_workspace_manifest() { /* one AppUI member, one Core member */ }

#[test]
fn rejects_duplicate_workspace_members() { /* same config twice */ }

#[test]
fn discovers_workspace_manifest_with_same_rules_as_single_config() { /* ancestor, descendant, ambiguous */ }
```

- [ ] **Step 6: Verify the workspace schema layer**

Run:

```bash
cargo test -p numi-config --lib -v
```

Expected: PASS, with the current `numi.toml` tests still green and the new workspace-model tests passing.

- [ ] **Step 7: Commit the workspace schema layer**

```bash
git add crates/numi-config/src/workspace.rs crates/numi-config/src/lib.rs crates/numi-config/src/model.rs
git commit -m "feat: add workspace manifest schema"
```

### Task 6: Add `numi workspace generate` And `numi workspace check`

**Files:**
- Modify: `crates/numi-cli/src/cli.rs`
- Modify: `crates/numi-cli/src/lib.rs`
- Modify: `crates/numi-cli/tests/cli_help.rs`
- Modify: `crates/numi-cli/tests/config_commands.rs`

- [ ] **Step 1: Extend the CLI with a nested `workspace` command group**

In `crates/numi-cli/src/cli.rs`, add:

```rust
#[derive(Debug, Subcommand)]
pub enum Command {
    Generate(GenerateArgs),
    Check(CheckArgs),
    Init(InitArgs),
    Config(ConfigCommand),
    Workspace(WorkspaceCommand),
    #[command(name = "dump-context")]
    DumpContext(DumpContextArgs),
}

#[derive(Debug, Args)]
pub struct WorkspaceCommand {
    #[command(subcommand)]
    pub command: WorkspaceSubcommand,
}

#[derive(Debug, Subcommand)]
pub enum WorkspaceSubcommand {
    Generate(WorkspaceGenerateArgs),
    Check(WorkspaceCheckArgs),
}

#[derive(Debug, Args)]
pub struct WorkspaceGenerateArgs {
    #[arg(long = "workspace")]
    pub workspace: Option<PathBuf>,
    #[arg(long = "member")]
    pub members: Vec<String>,
}
```

Mirror the same shape for `WorkspaceCheckArgs`.

- [ ] **Step 2: Implement workspace execution in `crates/numi-cli/src/lib.rs`**

Add runner functions that:

- resolve the workspace manifest path
- load and validate the workspace manifest
- filter members if `--member` is repeated
- invoke existing `numi_core::generate` or `numi_core::check` for each selected member config

Use code shaped like:

```rust
fn run_workspace_generate(args: &WorkspaceGenerateArgs) -> Result<(), CliError> {
    let workspace_path = discover_workspace_path(args.workspace.as_deref())?;
    let loaded = numi_config::load_workspace_from_path(&workspace_path)
        .map_err(|error| CliError::new(error.to_string()))?;
    let workspace_dir = loaded.path.parent().unwrap_or_else(|| Path::new("."));

    for member in selected_workspace_members(&loaded.config, &args.members)? {
        let config_path = workspace_dir.join(&member.config);
        let selected_jobs = (!member.jobs.is_empty()).then_some(member.jobs.as_slice());
        let report = numi_core::generate(&config_path, selected_jobs)
            .map_err(|error| CliError::new(error.to_string()))?;
        print_warnings(&report.warnings);
    }

    Ok(())
}
```

For `workspace check`, aggregate stale paths across members and return exit code `2` if any member is stale.

- [ ] **Step 3: Add member-selection and stale-path aggregation rules**

Implement member selection by manifest `config` path string, not by inferred module names. Use predictable filtering:

```rust
fn selected_workspace_members<'a>(
    workspace: &'a WorkspaceConfig,
    selected_members: &[String],
) -> Result<Vec<&'a WorkspaceMember>, CliError> { /* ... */ }
```

If no `--member` flags are present, run all members in manifest order. If one or more selected members are missing, return a `CliError` listing the valid member config paths.

- [ ] **Step 4: Add CLI help and integration tests for workspace commands**

Extend `crates/numi-cli/tests/cli_help.rs` to include the new top-level command:

```rust
assert_eq!(
    names,
    ["generate", "check", "init", "config", "workspace", "dump-context"]
);
```

Add integration tests in `crates/numi-cli/tests/config_commands.rs` like:

```rust
#[test]
fn workspace_generate_runs_multiple_member_configs() { /* copied multimodule fixture + workspace manifest */ }

#[test]
fn workspace_check_returns_exit_code_2_when_any_member_is_stale() { /* mutate one output after generate */ }

#[test]
fn workspace_generate_can_select_one_member() { /* --member Core/numi.toml */ }
```

- [ ] **Step 5: Verify the CLI workspace surface end-to-end**

Run:

```bash
cargo test -p numi-cli --test cli_help -v
cargo test -p numi-cli --test config_commands -v
```

Expected: PASS, with both existing single-config behavior and new workspace behavior covered.

- [ ] **Step 6: Commit the workspace CLI execution layer**

```bash
git add crates/numi-cli/src/cli.rs crates/numi-cli/src/lib.rs crates/numi-cli/tests/cli_help.rs crates/numi-cli/tests/config_commands.rs
git commit -m "feat: add workspace generate and check commands"
```

### Task 7: Finish Documentation And Run The Full Verification Matrix

**Files:**
- Modify: `README.md`
- Modify: `docs/spec.md`
- Modify: `docs/migration-from-swiftgen.md`

- [ ] **Step 1: Document the workspace manifest and command surface**

Add a dedicated README section with an example workspace manifest:

```toml
version = 1

[[members]]
config = "AppUI/numi.toml"

[[members]]
config = "Core/numi.toml"
jobs = ["l10n"]
```

Document commands:

```bash
numi workspace generate
numi workspace generate --workspace numi-workspace.toml --member Core/numi.toml
numi workspace check
```

State explicitly that `numi generate` and `numi check` still resolve exactly one `numi.toml`.

- [ ] **Step 2: Document parsed-input caching without promising output-cache behavior**

Update README and `docs/spec.md` to state:

- repeated runs may reuse cached parser outputs when inputs are unchanged
- cache invalidation happens on relevant file add, remove, rename, or content change
- normalization, rendering, and output checks still run every time

Do not document any output-hash cache, because this branch is not implementing that optimization.

- [ ] **Step 3: Update migration guidance for monorepo users**

In `docs/migration-from-swiftgen.md`, add a short monorepo note that users can:

- keep per-module `numi.toml` files
- add a repo-level `numi-workspace.toml` to orchestrate them
- continue using `numi check` in CI either per-config or at the workspace level

- [ ] **Step 4: Run the full branch verification matrix**

Run:

```bash
cargo fmt --check
cargo test
cargo bench -p numi-core --bench pipeline --no-run
```

Expected: PASS across formatting, all unit and integration tests, and benchmark compilation.

- [ ] **Step 5: Inspect the final worktree diff for scope**

Run:

```bash
git status --short
git diff --stat main...HEAD
```

Expected: only fixture, benchmark, cache, workspace, and docs changes related to issues `#2`, `#3`, and `#4`.

- [ ] **Step 6: Commit the final documentation and verification pass**

```bash
git add README.md docs/spec.md docs/migration-from-swiftgen.md
git commit -m "docs: describe workspace and parser cache behavior"
```
