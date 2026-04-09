# Unified `numi.toml` Manifest Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the split `numi.toml` and `numi-workspace.toml` model with a unified `numi.toml` manifest that supports either single-config or workspace mode, plus workspace job-template defaults and extensionless `template.path` resolution.

**Architecture:** Keep the current single-config `Config` model intact for job execution, but add a unified manifest layer in `numi-config` that can parse either a job manifest or a workspace manifest from `numi.toml`. Make CLI discovery local-first: default commands resolve the nearest `numi.toml` and dispatch by manifest mode, while `--workspace` explicitly searches for an ancestor workspace manifest. Merge workspace defaults into each member's loaded config before it reaches `numi-core`, and implement extensionless `template.path` resolution in the render path so the behavior stays uniform for both member and workspace-inherited templates.

**Tech Stack:** Rust 2024, Clap 4, Serde, existing `numi-config` discovery and validation, existing `numi-cli` command dispatch, existing `numi-core` pipeline and render path, fixture-backed CLI integration tests.

---

### Task 1: Introduce A Unified Manifest Model In `numi-config`

**Files:**
- Modify: `crates/numi-config/src/model.rs`
- Modify: `crates/numi-config/src/workspace.rs`
- Modify: `crates/numi-config/src/lib.rs`
- Test: `crates/numi-config/src/lib.rs`

- [ ] **Step 1: Add workspace-side defaults and overrides to the workspace model**

Extend `crates/numi-config/src/workspace.rs` so the workspace schema matches the approved spec:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceConfig {
    pub version: u32,
    pub workspace: WorkspaceSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceSettings {
    pub members: Vec<String>,
    #[serde(default, skip_serializing_if = "WorkspaceDefaults::is_empty")]
    pub defaults: WorkspaceDefaults,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub member_overrides: BTreeMap<String, WorkspaceMemberOverride>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceDefaults {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub jobs: BTreeMap<String, WorkspaceJobDefaults>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceJobDefaults {
    #[serde(default, skip_serializing_if = "crate::model::TemplateConfig::is_empty")]
    pub template: crate::model::TemplateConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspaceMemberOverride {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jobs: Vec<String>,
}
```

- [ ] **Step 2: Add a unified manifest enum and parser entrypoints**

In `crates/numi-config/src/lib.rs`, add a manifest-level API rather than forcing callers to guess the file mode:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Manifest {
    Config(Config),
    Workspace(WorkspaceConfig),
}

#[derive(Debug)]
pub struct LoadedManifest {
    pub path: PathBuf,
    pub manifest: Manifest,
}

pub fn parse_manifest_str(input: &str) -> Result<Manifest, ConfigError> {
    let value: toml::Value = toml::from_str(input).map_err(ConfigError::ParseToml)?;
    let has_jobs = value.get("jobs").is_some();
    let has_workspace = value.get("workspace").is_some();

    match (has_jobs, has_workspace) {
        (true, false) => parse_str(input).map(Manifest::Config),
        (false, true) => load_workspace_config_from_value(value).map(Manifest::Workspace),
        (true, true) => Err(ConfigError::Invalid(vec![
            Diagnostic::error("manifest must not define both `jobs` and `workspace`")
                .with_hint("use `jobs` for a single-config manifest or `workspace` for a workspace manifest"),
        ])),
        (false, false) => Err(ConfigError::Invalid(vec![
            Diagnostic::error("manifest must define either `jobs` or `workspace`")
                .with_hint("add `[jobs.<name>]` for a single-config manifest or `[workspace]` for a workspace manifest"),
        ])),
    }
}

pub fn load_manifest_from_path(path: &Path) -> Result<LoadedManifest, ConfigError> { /* ... */ }
```

- [ ] **Step 3: Tighten workspace validation around the new shape**

Update `validate_workspace` in `crates/numi-config/src/workspace.rs` to enforce the agreed invariants:

```rust
if config.workspace.members.is_empty() {
    diagnostics.push(
        Diagnostic::error("workspace must declare at least one member")
            .with_hint("add at least one entry to `workspace.members`"),
    );
}

for member in &config.workspace.members {
    if Path::new(member).is_absolute() || member.ends_with("/numi.toml") {
        diagnostics.push(
            Diagnostic::error("workspace.members entries must be relative member roots")
                .with_hint("use values like `AppUI` or `Core`, not config-file paths"),
        );
    }
}
```

Also validate:

- `workspace.member_overrides` keys must refer to declared members
- each override `jobs` list must be non-empty and unique when present
- each `workspace.defaults.jobs.<name>.template` still resolves to exactly one template source after normal config validation

- [ ] **Step 4: Add focused parsing tests before changing callers**

Add tests in `crates/numi-config/src/lib.rs` covering:

```rust
#[test]
fn parses_unified_single_config_manifest() { /* parse_manifest_str with jobs */ }

#[test]
fn parses_unified_workspace_manifest() { /* parse_manifest_str with [workspace] */ }

#[test]
fn rejects_manifest_that_mixes_jobs_and_workspace() { /* diagnostic text */ }

#[test]
fn rejects_workspace_members_that_look_like_config_paths() { /* AppUI/numi.toml */ }

#[test]
fn parses_workspace_defaults_job_template_shape() { /* workspace.defaults.jobs.l10n.template */ }
```

Run:

```bash
cargo test -p numi-config
```

Expected: PASS, with all old single-config tests still green and the new manifest-mode tests passing.

- [ ] **Step 5: Commit the unified manifest schema layer**

```bash
git add crates/numi-config/src/model.rs crates/numi-config/src/workspace.rs crates/numi-config/src/lib.rs
git commit -m "feat(config): unify config and workspace manifests"
```

### Task 2: Replace File Discovery With Local-First Unified `numi.toml` Resolution

**Files:**
- Modify: `crates/numi-config/src/discovery.rs`
- Modify: `crates/numi-config/src/lib.rs`
- Modify: `crates/numi-cli/src/cli.rs`
- Modify: `crates/numi-cli/src/lib.rs`
- Test: `crates/numi-cli/tests/config_commands.rs`
- Test: `crates/numi-cli/tests/cli_help.rs`

- [ ] **Step 1: Remove the separate workspace filename contract**

In `crates/numi-config/src/discovery.rs`, keep only:

```rust
pub const CONFIG_FILE_NAME: &str = "numi.toml";
```

Add an ancestor-only helper for explicit workspace resolution:

```rust
pub fn discover_workspace_ancestor(start_dir: &Path, explicit_path: Option<&Path>) -> Result<PathBuf, DiscoveryError> {
    if let Some(explicit_path) = explicit_path {
        return resolve_explicit_path(start_dir, explicit_path);
    }

    let canonical_start = start_dir.canonicalize()?;
    find_in_ancestors(&canonical_start, CONFIG_FILE_NAME).ok_or(DiscoveryError::NotFound {
        start_dir: canonical_start,
    })
}
```

Do not scan descendants for `--workspace`; the spec requires explicit ancestor lookup only.

- [ ] **Step 2: Replace the `workspace` subcommand surface with flags on the default commands**

Update `crates/numi-cli/src/cli.rs`:

```rust
#[derive(Debug, Args)]
pub struct GenerateArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
    #[arg(long = "workspace", action = ArgAction::SetTrue)]
    pub workspace: bool,
    #[arg(long = "job")]
    pub jobs: Vec<String>,
    #[command(flatten)]
    pub incremental_override: IncrementalOverrideArgs,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    #[arg(long = "config")]
    pub config: Option<PathBuf>,
    #[arg(long = "workspace", action = ArgAction::SetTrue)]
    pub workspace: bool,
    #[arg(long = "job")]
    pub jobs: Vec<String>,
}
```

Remove:

```rust
Command::Workspace(...)
WorkspaceCommand
WorkspaceSubcommand
WorkspaceGenerateArgs
WorkspaceCheckArgs
```

- [ ] **Step 3: Dispatch `generate` and `check` by manifest mode**

Refactor `crates/numi-cli/src/lib.rs` so default commands load a manifest first and then branch:

```rust
fn run_generate(args: &GenerateArgs) -> Result<(), CliError> {
    let loaded = load_cli_manifest(args.config.as_deref(), args.workspace)?;
    match loaded.manifest {
        Manifest::Config(config) => run_generate_config(&loaded.path, &config, args),
        Manifest::Workspace(workspace) => run_generate_workspace(&loaded.path, &workspace, args),
    }
}

fn run_check(args: &CheckArgs) -> Result<(), CliError> {
    let loaded = load_cli_manifest(args.config.as_deref(), args.workspace)?;
    match loaded.manifest {
        Manifest::Config(config) => run_check_config(&loaded.path, &config, args),
        Manifest::Workspace(workspace) => run_check_workspace(&loaded.path, &workspace, args),
    }
}
```

`dump-context` should remain single-config only. If it hits a workspace manifest, return:

```text
`dump-context` only supports single-config manifests; run it from a member directory or pass `--config <member>/numi.toml`
```

- [ ] **Step 4: Replace CLI integration tests with unified-manifest coverage**

Update `crates/numi-cli/tests/config_commands.rs` and `crates/numi-cli/tests/cli_help.rs` to cover:

- `numi generate` from a member directory resolves the nearest member `numi.toml`
- `numi generate --workspace` from a member directory resolves the ancestor workspace `numi.toml`
- `numi check --workspace` aggregates stale paths across members
- CLI help no longer lists `workspace` as a subcommand

Example command checks:

```bash
cargo test -p numi-cli --test config_commands
cargo test -p numi-cli --test cli_help
```

Expected: PASS, with the old explicit-workspace command coverage replaced by unified-manifest behavior.

- [ ] **Step 5: Commit the discovery and CLI dispatch change**

```bash
git add crates/numi-config/src/discovery.rs crates/numi-config/src/lib.rs crates/numi-cli/src/cli.rs crates/numi-cli/src/lib.rs crates/numi-cli/tests/config_commands.rs crates/numi-cli/tests/cli_help.rs
git commit -m "feat(cli): resolve unified numi manifests locally"
```

### Task 3: Merge Workspace Job Defaults Into Member Configs

**Files:**
- Modify: `crates/numi-config/src/lib.rs`
- Modify: `crates/numi-config/src/workspace.rs`
- Modify: `crates/numi-cli/src/lib.rs`
- Test: `crates/numi-config/src/lib.rs`
- Test: `crates/numi-cli/tests/config_commands.rs`

- [ ] **Step 1: Add a resolver that loads member configs from workspace member roots**

In `crates/numi-config/src/lib.rs`, add a helper that converts a workspace member path like `AppUI` into `AppUI/numi.toml`:

```rust
pub fn workspace_member_config_path(workspace_root: &Path, member_root: &str) -> PathBuf {
    workspace_root.join(member_root).join(CONFIG_FILE_NAME)
}
```

Use that helper everywhere instead of raw string concatenation.

- [ ] **Step 2: Merge workspace job defaults and member overrides into each loaded config**

Add a resolver like:

```rust
pub fn resolve_workspace_member_config(
    workspace: &WorkspaceConfig,
    member_root: &str,
    member_config: &Config,
) -> Result<Config, Vec<Diagnostic>> {
    let mut resolved = member_config.clone();

    for job in &mut resolved.jobs {
        if job.template.is_empty() {
            if let Some(defaults) = workspace
                .workspace
                .defaults
                .jobs
                .get(&job.name)
            {
                job.template = defaults.template.clone();
            }
        }
    }

    if let Some(override_config) = workspace.workspace.member_overrides.get(member_root) {
        // job-selection stays workspace-side; do not mutate the Config job list here
        let _ = override_config;
    }

    let diagnostics = validate::validate_config(&resolved);
    if diagnostics.is_empty() { Ok(resolved) } else { Err(diagnostics) }
}
```

The important behavior is:

- local member `job.template` wins
- workspace default `job.template` fills only missing member templates
- validation happens after merge, not before execution

- [ ] **Step 3: Make workspace execution use the merged config**

In `crates/numi-cli/src/lib.rs`, update workspace-mode execution to load each member config, merge workspace defaults, and then execute against the merged config:

```rust
let member_manifest_path = workspace_member_config_path(workspace_dir, member_root);
let loaded_member = numi_config::load_from_path(&member_manifest_path)?;
let merged_config = numi_config::resolve_workspace_member_config(&workspace, member_root, &loaded_member.config)?;

let report = numi_core::generate_loaded_config(
    &member_manifest_path,
    &merged_config,
    selected_jobs,
    options,
)?;
```

If `numi_core` does not yet expose a loaded-config entrypoint, add one rather than serializing the merged config back to disk.

- [ ] **Step 4: Add inheritance-focused tests**

Add tests that prove:

- a member `l10n` job inherits `[workspace.defaults.jobs.l10n.template]`
- a member `assets` job with its own local template does not inherit the workspace template
- a member override `jobs = ["l10n"]` still narrows workspace execution to one job

Run:

```bash
cargo test -p numi-config
cargo test -p numi-cli --test config_commands
```

Expected: PASS, with explicit coverage for inherited and overridden workspace templates.

- [ ] **Step 5: Commit the workspace-default merge layer**

```bash
git add crates/numi-config/src/lib.rs crates/numi-config/src/workspace.rs crates/numi-cli/src/lib.rs crates/numi-cli/tests/config_commands.rs
git commit -m "feat(config): inherit workspace job templates"
```

### Task 4: Add Extensionless `template.path` Resolution

**Files:**
- Modify: `crates/numi-core/src/render.rs`
- Modify: `crates/numi-core/src/pipeline.rs`
- Test: `crates/numi-core/src/render.rs`
- Test: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Centralize `template.path` resolution in the render layer**

In `crates/numi-core/src/render.rs`, add a resolver used by both render execution and dependency fingerprinting:

```rust
pub fn resolve_template_entry_path(config_root: &Path, configured_path: &str) -> Result<PathBuf, RenderError> {
    let direct = config_root.join(configured_path);
    let with_jinja = config_root.join(format!("{configured_path}.jinja"));

    match (direct.is_file(), with_jinja.is_file()) {
        (true, false) => Ok(direct),
        (false, true) => Ok(with_jinja),
        (false, false) => Err(RenderError::ReadTemplate {
            path: direct,
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "template file was not found"),
        }),
        (true, true) => Err(RenderError::Render(
            minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("ambiguous template path `{configured_path}` matched both extensionless and `.jinja` files"),
            ),
        )),
    }
}
```

- [ ] **Step 2: Use the same resolution path for rendering and generation-cache fingerprinting**

Update `crates/numi-core/src/pipeline.rs` so both of these code paths use the new resolver:

```rust
let resolved_path = resolve_template_entry_path(config_dir, template_path)?;
render_path(&resolved_path, config_dir, context)
```

and:

```rust
let resolved_path = resolve_template_entry_path(config_dir, template_path).ok()?;
let dependencies = collect_custom_template_dependencies(&resolved_path, config_dir).ok()??;
```

This keeps workspace-inherited templates and local member templates on the same behavior path.

- [ ] **Step 3: Add render-layer tests for all four outcomes**

Add tests in `crates/numi-core/src/render.rs` covering:

- configured path exists directly
- configured path resolves via `.jinja`
- neither file exists
- both extensionless and `.jinja` files exist

Add one pipeline test that uses a config with:

```toml
[jobs.l10n.template]
path = "Templates/l10n"
```

and verifies generation succeeds with `Templates/l10n.jinja`.

- [ ] **Step 4: Run the focused core verification**

Run:

```bash
cargo test -p numi-core --lib render::tests::
cargo test -p numi-core --lib pipeline::tests::
```

Expected: PASS, including the new extensionless template resolution cases.

- [ ] **Step 5: Commit the template-path resolution change**

```bash
git add crates/numi-core/src/render.rs crates/numi-core/src/pipeline.rs
git commit -m "feat(core): resolve extensionless template paths"
```

### Task 5: Migrate Fixtures, Docs, And User-Facing Messages To The Unified Model

**Files:**
- Modify: `README.md`
- Modify: `docs/spec.md`
- Modify: `docs/migration-from-swiftgen.md`
- Modify: `docs/examples/starter-numi.toml`
- Modify: `fixtures/multimodule-repo/AppUI/numi.toml`
- Modify: `fixtures/multimodule-repo/Core/numi.toml`
- Modify: `crates/numi-cli/tests/config_commands.rs`
- Modify: `crates/numi-config/src/workspace.rs`

- [ ] **Step 1: Replace all `numi-workspace.toml` examples with unified `numi.toml` examples**

Update docs to show:

```toml
version = 1

[workspace]
members = ["AppUI", "Core"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"
```

Remove any remaining examples like:

```toml
[[members]]
config = "AppUI/numi.toml"
```

- [ ] **Step 2: Rewrite CLI help and diagnostics to match the new command surface**

Update user-facing strings so they refer to:

- `numi.toml`
- `--workspace`
- nearest-manifest local-first behavior

and do not mention:

- `numi-workspace.toml`
- `numi workspace generate`
- `numi workspace check`

- [ ] **Step 3: Update fixtures to exercise the new workspace syntax**

Add or update a workspace fixture rooted at one `numi.toml` with:

```toml
version = 1

[workspace]
members = ["apps/assets", "packages/files"]

[workspace.member_overrides.packages/files]
jobs = ["files"]
```

Keep the member manifests in their own directories as normal `numi.toml` files.

- [ ] **Step 4: Run the cross-crate verification**

Run:

```bash
cargo test -p numi-config
cargo test -p numi-cli
cargo test -p numi-core --lib pipeline::tests::
```

Expected: PASS, with docs, fixtures, parsing, CLI dispatch, inheritance, and extensionless template resolution aligned.

- [ ] **Step 5: Commit the migration and docs pass**

```bash
git add README.md docs/spec.md docs/migration-from-swiftgen.md docs/examples/starter-numi.toml fixtures crates/numi-cli/tests/config_commands.rs crates/numi-config/src/workspace.rs
git commit -m "docs: migrate workspace examples to unified manifests"
```

### Task 6: Final Verification And Compatibility Sweep

**Files:**
- Modify: `crates/numi-cli/src/lib.rs`
- Modify: `crates/numi-config/src/lib.rs`
- Modify: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Run the full verification suite relevant to the feature**

Run:

```bash
cargo test -p numi-config
cargo test -p numi-cli
cargo test -p numi-core --lib pipeline::tests::
cargo test -p numi-core --lib render::tests::
```

Expected: PASS, except for any already-known unrelated pre-existing failures that were present before this feature branch. If anything new fails, fix it before proceeding.

- [ ] **Step 2: Sanity-check the final UX manually with representative commands**

Run these manual checks from fixture directories:

```bash
cargo run -p numi-cli -- generate --config fixtures/l10n-basic/numi.toml
cargo run -p numi-cli -- check --config fixtures/l10n-basic/numi.toml
cargo run -p numi-cli -- generate --workspace --config fixtures/multimodule-repo/numi.toml
```

Expected:

- the single-config commands target one manifest only
- the workspace-forced command targets the workspace manifest
- diagnostics mention `numi.toml` and `--workspace`, not `numi-workspace.toml` or `numi workspace`

- [ ] **Step 3: Search for stale old-surface references**

Run:

```bash
rg -n "numi-workspace.toml|numi workspace generate|numi workspace check|\\[\\[members\\]\\][[:space:][:print:]]*config =" README.md docs crates fixtures
```

Expected: no matches in active docs, code, or fixtures, unless a test intentionally checks migration or legacy rejection behavior.

- [ ] **Step 4: Commit the final cleanup if needed**

```bash
git add README.md docs crates fixtures
git commit -m "refactor: finish unified numi manifest migration"
```
