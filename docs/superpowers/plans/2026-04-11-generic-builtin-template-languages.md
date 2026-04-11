# Generic Built-in Template Languages Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Swift-only built-in template selector with a generic `language + name` model, add workspace default built-in language support, and ship Objective-C built-ins for assets, localization, and files.

**Architecture:** Keep the current template context and generation pipeline intact, but change built-in template identity from a single Swift string to a typed `(language, name)` pair. Make `numi-config` own the new schema and validation rules, make `numi-core` resolve embedded templates through a static registry, and migrate all tests, fixtures, and docs to the new config shape in the same change.

**Tech Stack:** Rust workspace, Serde/TOML config parsing, Minijinja rendering, Cargo test suite, embedded `.jinja` templates

---

## File Map

### Existing Files To Modify

- `crates/numi-config/src/model.rs`
- `crates/numi-config/src/validate.rs`
- `crates/numi-config/src/workspace.rs`
- `crates/numi-config/src/lib.rs`
- `crates/numi-core/src/render.rs`
- `crates/numi-core/src/pipeline.rs`
- `crates/numi-cli/tests/config_commands.rs`
- `crates/numi-cli/tests/generate_l10n.rs`
- `README.md`
- `docs/examples/starter-numi.toml`
- `docs/migration-from-swiftgen.md`
- `docs/spec.md`
- `fixtures/bench-mixed-large/numi.toml`
- `fixtures/files-basic/numi.toml`
- `fixtures/l10n-basic/numi.toml`
- `fixtures/multimodule-repo/apps/assets/numi.toml`
- `fixtures/multimodule-repo/packages/files/numi.toml`
- `fixtures/xcassets-basic/numi.toml`
- `fixtures/xcstrings-basic/numi.toml`

### New Files To Create

- `templates/objc/assets.jinja`
- `templates/objc/l10n.jinja`
- `templates/objc/files.jinja`

### Verification Commands

- `cargo test -p numi-config`
- `cargo test -p numi-core render::tests --lib`
- `cargo test -p numi-core pipeline::tests --lib`
- `cargo test -p numi-cli --test config_commands`
- `cargo test -p numi-cli --test generate_l10n`
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`

### Task 1: Replace The Built-in Config Shape In `numi-config`

**Files:**
- Modify: `crates/numi-config/src/model.rs`
- Test: `crates/numi-config/src/lib.rs`

- [ ] **Step 1: Write failing config-model tests for `language + name`**

Add tests in `crates/numi-config/src/lib.rs` that parse the new shape and reject the old shape:

```rust
#[test]
fn parses_builtin_template_language_and_name() {
    let config = parse_config_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
    )
    .expect("config should parse");

    let builtin = config.jobs[0].template.builtin.as_ref().expect("builtin should exist");
    assert_eq!(builtin.language.as_deref(), Some("objc"));
    assert_eq!(builtin.name.as_deref(), Some("assets"));
}

#[test]
fn rejects_legacy_swift_builtin_namespace_shape() {
    let error = parse_config_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"
"#,
    )
    .expect_err("legacy builtin namespace shape should fail");

    let message = error.to_string();
    assert!(message.contains("unknown field `swift`"));
}
```

- [ ] **Step 2: Run the targeted config tests to verify they fail for the right reason**

Run: `cargo test -p numi-config parses_builtin_template_language_and_name rejects_legacy_swift_builtin_namespace_shape -- --nocapture`
Expected: FAIL because `BuiltinTemplateConfig` still exposes `swift` instead of `language` and `name`.

- [ ] **Step 3: Replace the Swift-only built-in config model with a generic one**

Update `crates/numi-config/src/model.rs`:

```rust
pub const BUILTIN_TEMPLATE_LANGUAGES: &[&str] = &["swift", "objc"];
pub const SWIFT_BUILTIN_TEMPLATE_NAMES: &[&str] = &["swiftui-assets", "l10n", "files"];
pub const OBJC_BUILTIN_TEMPLATE_NAMES: &[&str] = &["assets", "l10n", "files"];

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinTemplateConfig {
    pub language: Option<String>,
    pub name: Option<String>,
}

impl BuiltinTemplateConfig {
    pub fn is_empty(&self) -> bool {
        self.language.is_none() && self.name.is_none()
    }
}
```

- [ ] **Step 4: Update config serialization and parsing assertions to the new field names**

Update existing `crates/numi-config/src/lib.rs` assertions from:

```rust
.and_then(|builtin| builtin.swift.as_deref())
```

to:

```rust
.and_then(|builtin| builtin.language.as_deref())
```

and add the matching `name` assertions:

```rust
assert_eq!(
    job.template
        .builtin
        .as_ref()
        .and_then(|builtin| builtin.name.as_deref()),
    Some("swiftui-assets")
);
```

- [ ] **Step 5: Run the targeted config tests to verify the new shape passes**

Run: `cargo test -p numi-config parses_builtin_template_language_and_name rejects_legacy_swift_builtin_namespace_shape -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit the config-shape change**

```bash
git add crates/numi-config/src/model.rs crates/numi-config/src/lib.rs
git commit -m "refactor(config): use generic builtin template selectors"
```

### Task 2: Add Validation For Built-in Languages And Names

**Files:**
- Modify: `crates/numi-config/src/validate.rs`
- Test: `crates/numi-config/src/lib.rs`

- [ ] **Step 1: Write failing validation tests for missing and unknown built-ins**

Add tests in `crates/numi-config/src/lib.rs`:

```rust
#[test]
fn rejects_builtin_template_without_name() {
    let error = parse_config_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
"#,
    )
    .expect_err("builtin without name should fail");

    assert!(error.to_string().contains("jobs.assets.template.builtin.name"));
}

#[test]
fn rejects_unknown_builtin_language() {
    let error = parse_config_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "kotlin"
name = "assets"
"#,
    )
    .expect_err("unknown builtin language should fail");

    assert!(error.to_string().contains("jobs.assets.template.builtin.language must be one of"));
}

#[test]
fn rejects_unknown_builtin_name_for_language() {
    let error = parse_config_str(
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "swiftui-assets"
"#,
    )
    .expect_err("objc builtin name should be validated against objc names");

    assert!(error.to_string().contains("jobs.assets.template.builtin.name must be one of"));
}
```

- [ ] **Step 2: Run the targeted validation tests to verify they fail before implementation**

Run: `cargo test -p numi-config rejects_builtin_template_without_name rejects_unknown_builtin_language rejects_unknown_builtin_name_for_language -- --nocapture`
Expected: FAIL because validation still assumes `builtin.swift`.

- [ ] **Step 3: Update template validation to use effective language and name**

Refactor `crates/numi-config/src/validate.rs` so `validate_template` checks the new fields:

```rust
if let Some(builtin) = &template.builtin {
    let Some(language) = builtin.language.as_deref() else {
        diagnostics.push(
            Diagnostic::error(format!("{label} builtin must set `language`"))
                .with_hint(format!("set `[{field_path}.builtin] language = \"swift\"`")),
        );
        return;
    };

    validate_allowed_value(
        diagnostics,
        &format!("{field_path}.builtin.language"),
        language,
        BUILTIN_TEMPLATE_LANGUAGES,
        job,
    );

    let Some(name) = builtin.name.as_deref() else {
        diagnostics.push(
            Diagnostic::error(format!("{label} builtin must set `name`"))
                .with_hint(format!("set `[{field_path}.builtin] name = \"l10n\"`")),
        );
        return;
    };

    validate_allowed_value(
        diagnostics,
        &format!("{field_path}.builtin.name"),
        name,
        builtin_template_names_for_language(language),
        job,
    );
}
```

- [ ] **Step 4: Add a helper that maps languages to shipped built-in names**

In `crates/numi-config/src/model.rs` or `validate.rs`, add:

```rust
pub fn builtin_template_names_for_language(language: &str) -> &'static [&'static str] {
    match language {
        "swift" => SWIFT_BUILTIN_TEMPLATE_NAMES,
        "objc" => OBJC_BUILTIN_TEMPLATE_NAMES,
        _ => &[],
    }
}
```

- [ ] **Step 5: Update validation error-message assertions to mention `language` and `name`**

Replace old expectation strings like:

```rust
assert!(message.contains("set `[jobs.assets.template.builtin] swift = \"...\"`"));
```

with:

```rust
assert!(message.contains("set `[jobs.assets.template.builtin] language = \"swift\"`"));
assert!(message.contains("set `[jobs.assets.template.builtin] name = \"swiftui-assets\"`"));
```

- [ ] **Step 6: Run the full `numi-config` suite**

Run: `cargo test -p numi-config`
Expected: PASS

- [ ] **Step 7: Commit the validation change**

```bash
git add crates/numi-config/src/validate.rs crates/numi-config/src/lib.rs crates/numi-config/src/model.rs
git commit -m "feat(config): validate builtin template languages and names"
```

### Task 3: Add Workspace Default Built-in Language Support

**Files:**
- Modify: `crates/numi-config/src/workspace.rs`
- Test: `crates/numi-config/src/lib.rs`

- [ ] **Step 1: Write failing workspace-default inheritance tests**

Add tests in `crates/numi-config/src/lib.rs`:

```rust
#[test]
fn workspace_defaults_can_supply_builtin_language() {
    let workspace = parse_workspace_str(
        r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.assets.template.builtin]
language = "objc"
"#,
    )
    .expect("workspace should parse");

    assert_eq!(
        workspace.workspace.defaults.jobs["assets"]
            .template
            .builtin
            .as_ref()
            .and_then(|builtin| builtin.language.as_deref()),
        Some("objc")
    );
}
```

and an integration-style config merge test where a member job sets only `name` and inherits `language`.

- [ ] **Step 2: Run the targeted workspace tests to verify they fail before merge logic changes**

Run: `cargo test -p numi-config workspace_defaults_can_supply_builtin_language -- --nocapture`
Expected: FAIL if old assertions and merge logic still assume `builtin.swift`.

- [ ] **Step 3: Update workspace merging so template defaults only fill in missing built-in language**

In `crates/numi-config/src/workspace.rs`, keep `name` explicit and merge only `language`:

```rust
if job.template.path.is_none() {
    if let (Some(job_builtin), Some(default_builtin)) = (
        job.template.builtin.as_mut(),
        defaults.template.builtin.as_ref(),
    ) {
        if job_builtin.language.is_none() {
            job_builtin.language = default_builtin.language.clone();
        }
    }
}
```

If a job has no built-in block at all, do not synthesize one from defaults alone.

- [ ] **Step 4: Add a regression test proving defaults never invent a built-in name**

Add a test that loads a workspace default with `language = "objc"` and a member job with no `template.builtin.name`, then assert validation still fails with a missing-name error.

- [ ] **Step 5: Run the full workspace-related `numi-config` tests**

Run: `cargo test -p numi-config workspace -- --nocapture`
Expected: PASS

- [ ] **Step 6: Commit the workspace-default behavior**

```bash
git add crates/numi-config/src/workspace.rs crates/numi-config/src/lib.rs
git commit -m "feat(workspace): support default builtin template language"
```

### Task 4: Replace The Built-in Render Registry And Add ObjC Templates

**Files:**
- Modify: `crates/numi-core/src/render.rs`
- Test: `crates/numi-core/src/render.rs`
- Create: `templates/objc/assets.jinja`
- Create: `templates/objc/l10n.jinja`
- Create: `templates/objc/files.jinja`

- [ ] **Step 1: Write failing render tests for language-qualified built-ins**

Add tests in `crates/numi-core/src/render.rs`:

```rust
#[test]
fn renders_builtin_objc_l10n_template() {
    let rendered = render_builtin(("objc", "l10n"), &l10n_context()).expect("template should render");
    assert!(rendered.contains("@interface"));
}

#[test]
fn rejects_unknown_builtin_language_and_name_pair() {
    let error = render_builtin(("objc", "swiftui-assets"), &l10n_context())
        .expect_err("unknown builtin pair should fail");

    assert!(matches!(
        error,
        RenderError::UnknownBuiltin { language, name }
        if language == "objc" && name == "swiftui-assets"
    ));
}
```

- [ ] **Step 2: Run the targeted render tests to verify they fail before registry changes**

Run: `cargo test -p numi-core render::tests::renders_builtin_objc_l10n_template render::tests::rejects_unknown_builtin_language_and_name_pair -- --nocapture`
Expected: FAIL because `render_builtin` still accepts a single built-in name.

- [ ] **Step 3: Change render errors and registry lookup to use `(language, name)`**

Refactor `crates/numi-core/src/render.rs`:

```rust
pub enum RenderError {
    UnknownBuiltin { language: String, name: String },
    // existing variants...
}

pub fn render_builtin(
    builtin: (&str, &str),
    context: &AssetTemplateContext,
) -> Result<String, RenderError> {
    let template_source = builtin_template_source(builtin)?;
    let template_id = format!("{}/{}", builtin.0, builtin.1);
    render_template_source(&template_id, template_source, context)
}

pub fn builtin_template_source(builtin: (&str, &str)) -> Result<&'static str, RenderError> {
    match builtin {
        ("swift", "swiftui-assets") => Ok(SWIFTUI_ASSETS_TEMPLATE),
        ("swift", "l10n") => Ok(L10N_TEMPLATE),
        ("swift", "files") => Ok(FILES_TEMPLATE),
        ("objc", "assets") => Ok(OBJC_ASSETS_TEMPLATE),
        ("objc", "l10n") => Ok(OBJC_L10N_TEMPLATE),
        ("objc", "files") => Ok(OBJC_FILES_TEMPLATE),
        (language, name) => Err(RenderError::UnknownBuiltin {
            language: language.to_owned(),
            name: name.to_owned(),
        }),
    }
}
```

- [ ] **Step 4: Add embedded Objective-C built-in templates**

Create `templates/objc/assets.jinja`, `templates/objc/l10n.jinja`, and `templates/objc/files.jinja` with simple deterministic ObjC output. For example, `templates/objc/l10n.jinja` should follow a minimal shape like:

```jinja
NS_ASSUME_NONNULL_BEGIN

@interface {{ job.name | upper_first }} : NSObject
{% for module in modules %}
+ (NSString *){{ module.name | lower_first }};
{% endfor %}
@end

NS_ASSUME_NONNULL_END
```

Keep these templates intentionally narrow and compatible with the existing context.

- [ ] **Step 5: Add include constants for the new ObjC templates**

At the top of `crates/numi-core/src/render.rs`, add:

```rust
const OBJC_ASSETS_TEMPLATE: &str = include_str!("../../../templates/objc/assets.jinja");
const OBJC_L10N_TEMPLATE: &str = include_str!("../../../templates/objc/l10n.jinja");
const OBJC_FILES_TEMPLATE: &str = include_str!("../../../templates/objc/files.jinja");
```

- [ ] **Step 6: Run the full render test module**

Run: `cargo test -p numi-core render::tests --lib`
Expected: PASS

- [ ] **Step 7: Commit the render registry and templates**

```bash
git add crates/numi-core/src/render.rs templates/objc/assets.jinja templates/objc/l10n.jinja templates/objc/files.jinja
git commit -m "feat(render): add generic builtin language registry"
```

### Task 5: Update Pipeline Dispatch And Generation Fingerprints

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Test: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Write failing pipeline tests for generic built-in dispatch**

Update or add pipeline tests that use:

```toml
[jobs.files.template.builtin]
language = "objc"
name = "files"
```

and assert generation succeeds and contains ObjC output markers like `@interface`.

Also add a regression test that changing built-in `language` invalidates the generation fingerprint:

```rust
assert_ne!(swift_record, objc_record);
```

- [ ] **Step 2: Run the targeted pipeline tests to verify they fail before dispatch changes**

Run: `cargo test -p numi-core pipeline::tests::generate_writes_builtin_files_accessors --lib -- --nocapture`
Expected: FAIL after test migration because pipeline still reads `builtin.swift`.

- [ ] **Step 3: Update pipeline built-in lookup to use `language` and `name`**

In `crates/numi-core/src/pipeline.rs`, replace:

```rust
.and_then(|builtin| builtin.swift.as_deref())
```

with logic that reads both fields:

```rust
let builtin = job.template.builtin.as_ref();
let builtin_language = builtin.and_then(|builtin| builtin.language.as_deref());
let builtin_name = builtin.and_then(|builtin| builtin.name.as_deref());

if let (Some(language), Some(name)) = (builtin_language, builtin_name) {
    return render_builtin((language, name), context).map_err(|source| GenerateError::Render {
        job: job.name.clone(),
        source,
    });
}
```

- [ ] **Step 4: Include both language and name in generation fingerprint records**

Change `GenerationTemplateFingerprintRecord::Builtin` from:

```rust
Builtin { name: String, source_hash: String }
```

to:

```rust
Builtin {
    language: String,
    name: String,
    source_hash: String,
}
```

and populate it from `builtin_template_source((language, name))`.

- [ ] **Step 5: Migrate pipeline test helper configs to the new built-in shape**

Update all in-file TOML snippets in `crates/numi-core/src/pipeline.rs` from:

```toml
[jobs.l10n.template.builtin]
swift = "l10n"
```

to:

```toml
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
```

- [ ] **Step 6: Run the full pipeline test module**

Run: `cargo test -p numi-core pipeline::tests --lib`
Expected: PASS

- [ ] **Step 7: Commit the pipeline update**

```bash
git add crates/numi-core/src/pipeline.rs
git commit -m "refactor(pipeline): dispatch builtin templates by language and name"
```

### Task 6: Migrate CLI Tests, Fixtures, And User-Facing Docs

**Files:**
- Modify: `crates/numi-cli/tests/config_commands.rs`
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
- Modify: `README.md`
- Modify: `docs/examples/starter-numi.toml`
- Modify: `docs/migration-from-swiftgen.md`
- Modify: `docs/spec.md`
- Modify: `fixtures/bench-mixed-large/numi.toml`
- Modify: `fixtures/files-basic/numi.toml`
- Modify: `fixtures/l10n-basic/numi.toml`
- Modify: `fixtures/multimodule-repo/apps/assets/numi.toml`
- Modify: `fixtures/multimodule-repo/packages/files/numi.toml`
- Modify: `fixtures/xcassets-basic/numi.toml`
- Modify: `fixtures/xcstrings-basic/numi.toml`

- [ ] **Step 1: Write or update one CLI integration test that uses an ObjC built-in**

Add a focused test in `crates/numi-cli/tests/config_commands.rs` or a nearby integration test file:

```rust
#[test]
fn config_print_preserves_builtin_language_and_name() {
    let config = r#"
version = 1

[jobs.assets]
output = "Generated/Assets.h"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#;

    // write config, run `numi config print`, assert both lines appear
}
```

- [ ] **Step 2: Run the targeted CLI test to verify it fails before the fixture/doc sweep is complete**

Run: `cargo test -p numi-cli --test config_commands config_print_preserves_builtin_language_and_name -- --nocapture`
Expected: FAIL until CLI-facing snapshots and assertions are updated.

- [ ] **Step 3: Migrate all built-in-using fixtures and CLI test TOML snippets**

Replace every built-in block that currently looks like:

```toml
[jobs.assets.template.builtin]
swift = "swiftui-assets"
```

with:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

Do the same for `l10n` and `files`, and for workspace defaults:

```toml
[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
```

- [ ] **Step 4: Update README and starter docs to show the new config shape**

Use the new style in examples:

```toml
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
```

and add a short ObjC example:

```toml
[jobs.assets.template.builtin]
language = "objc"
name = "assets"
```

- [ ] **Step 5: Update migration docs to describe the generic built-in model**

Revise `docs/migration-from-swiftgen.md` to say built-ins use `language` and `name`, not language-specific namespace keys.

- [ ] **Step 6: Run the focused CLI test suites**

Run:

```bash
cargo test -p numi-cli --test config_commands
cargo test -p numi-cli --test generate_l10n
```

Expected: PASS

- [ ] **Step 7: Commit the fixture and doc migration**

```bash
git add crates/numi-cli/tests/config_commands.rs crates/numi-cli/tests/generate_l10n.rs README.md docs/examples/starter-numi.toml docs/migration-from-swiftgen.md docs/spec.md fixtures
git commit -m "docs: migrate builtin templates to language and name selectors"
```

### Task 7: Run Full Verification

**Files:**
- Modify: none

- [ ] **Step 1: Run formatting**

Run: `cargo fmt --all --check`
Expected: PASS

- [ ] **Step 2: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS

- [ ] **Step 3: Run the full workspace test suite**

Run: `cargo test --workspace`
Expected: PASS

- [ ] **Step 4: Review changed files before handoff**

Run: `git status --short`
Expected: only files related to generic built-in languages, ObjC templates, migrated fixtures, and docs are changed.

- [ ] **Step 5: Create the final implementation commit**

```bash
git add crates/numi-config/src/model.rs crates/numi-config/src/validate.rs crates/numi-config/src/workspace.rs crates/numi-config/src/lib.rs crates/numi-core/src/render.rs crates/numi-core/src/pipeline.rs crates/numi-cli/tests/config_commands.rs crates/numi-cli/tests/generate_l10n.rs templates/objc/assets.jinja templates/objc/l10n.jinja templates/objc/files.jinja README.md docs/examples/starter-numi.toml docs/migration-from-swiftgen.md docs/spec.md fixtures
git commit -m "feat: add generic builtin template languages"
```
