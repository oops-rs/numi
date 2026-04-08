# Template Includes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add include support for file-based custom templates with local-directory resolution, shared config-root resolution, and deterministic ambiguity errors.

**Architecture:** Keep include resolution entirely inside `numi-core`'s render layer. Custom template rendering will move from a single `read_to_string` call to a MiniJinja loader-backed render session that carries both the entry template location and the config-root search path. The pipeline will only pass the config directory into custom-template rendering and will stay unaware of include lookup rules.

**Tech Stack:** Rust, MiniJinja 2.19 loader and path-join callbacks, filesystem-backed unit tests in `numi-core`

---

## File Structure

- Modify: `crates/numi-core/src/render.rs`
  - Extend custom-template rendering to support includes.
  - Add deterministic path-resolution helpers and path-rich error messages.
  - Add render-layer tests for local, shared-root, nested, missing, and ambiguous includes.
- Modify: `crates/numi-core/src/pipeline.rs`
  - Pass `config_dir` into custom-template rendering.
  - Add an end-to-end pipeline test proving includes work through the actual `generate(...)` path.

### Task 1: Add Successful Include Resolution In The Render Layer

**Files:**
- Modify: `crates/numi-core/src/render.rs`

- [ ] **Step 1: Write failing render tests for local, shared-root, and nested includes**

```rust
#[test]
fn renders_local_include_from_template_directory() {
    let temp_dir = make_temp_dir("render-local-include");
    let config_root = temp_dir.join("Config");
    let templates_dir = config_root.join("Templates");
    fs::create_dir_all(templates_dir.join("partials")).expect("templates dir should exist");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}\n",
    )
    .expect("main template should be written");
    fs::write(
        templates_dir.join("partials/header.jinja"),
        "LOCAL",
    )
    .expect("local partial should be written");

    let rendered = render_path(
        &templates_dir.join("main.jinja"),
        &config_root,
        &l10n_context(),
    )
    .expect("template should render");

    assert_eq!(rendered, "LOCAL|L10n\n");
    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn renders_include_from_shared_config_root() {
    let temp_dir = make_temp_dir("render-shared-include");
    let config_root = temp_dir.join("Config");
    let templates_dir = config_root.join("Templates");
    fs::create_dir_all(templates_dir).expect("templates dir should exist");
    fs::create_dir_all(config_root.join("partials")).expect("shared partial dir should exist");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/header.jinja\" %}|{{ modules[0].name }}\n",
    )
    .expect("main template should be written");
    fs::write(
        config_root.join("partials/header.jinja"),
        "SHARED",
    )
    .expect("shared partial should be written");

    let rendered = render_path(
        &templates_dir.join("main.jinja"),
        &config_root,
        &l10n_context(),
    )
    .expect("template should render");

    assert_eq!(rendered, "SHARED|Localizable\n");
    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn renders_nested_includes_from_mixed_roots() {
    let temp_dir = make_temp_dir("render-nested-includes");
    let config_root = temp_dir.join("Config");
    let templates_dir = config_root.join("Templates");
    fs::create_dir_all(templates_dir.join("partials")).expect("templates dir should exist");
    fs::create_dir_all(config_root.join("shared")).expect("shared include dir should exist");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/outer.jinja\" %}\n",
    )
    .expect("main template should be written");
    fs::write(
        templates_dir.join("partials/outer.jinja"),
        "OUTER[{% include \"shared/inner.jinja\" %}]",
    )
    .expect("outer partial should be written");
    fs::write(
        config_root.join("shared/inner.jinja"),
        "{{ job.swiftIdentifier }}",
    )
    .expect("shared nested partial should be written");

    let rendered = render_path(
        &templates_dir.join("main.jinja"),
        &config_root,
        &l10n_context(),
    )
    .expect("template should render");

    assert_eq!(rendered, "OUTER[L10n]\n");
    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the focused render tests to verify they fail first**

Run: `cargo test -p numi-core render::tests::renders_local_include_from_template_directory -v`

Expected: FAIL with `E0061` because `render_path` still takes only `(path, context)` and does not accept the shared config-root argument yet.

- [ ] **Step 3: Implement loader-backed custom template rendering**

```rust
use minijinja::{Environment, Error, ErrorKind};
use std::{borrow::Cow, fs, path::{Path, PathBuf}};

const ENTRY_TEMPLATE_NAME: &str = "__numi_entry__";
const FILE_TEMPLATE_PREFIX: &str = "file:";
const INCLUDE_REQUEST_PREFIX: &str = "include:";

pub fn render_path(
    path: &Path,
    config_root: &Path,
    context: &AssetTemplateContext,
) -> Result<String, RenderError> {
    let template_source = fs::read_to_string(path).map_err(|source| RenderError::ReadTemplate {
        path: path.to_path_buf(),
        source,
    })?;
    let mut environment = build_custom_environment(path, config_root);
    environment
        .add_template_owned(ENTRY_TEMPLATE_NAME.to_string(), template_source)
        .map_err(RenderError::RegisterTemplate)?;

    let rendered = environment
        .get_template(ENTRY_TEMPLATE_NAME)
        .map_err(RenderError::Render)?
        .render(context)
        .map_err(RenderError::Render)?;

    Ok(normalize_blank_lines(&rendered))
}

fn build_custom_environment(entry_path: &Path, config_root: &Path) -> Environment<'static> {
    let mut environment = build_environment();
    let entry_dir = entry_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let config_root = config_root.to_path_buf();

    environment.set_path_join_callback(move |name, parent| {
        Cow::Owned(encode_include_request(parent, name))
    });

    environment.set_loader(move |name| {
        load_custom_template(name, &entry_dir, &config_root)
    });

    environment
}

fn encode_include_request(parent: &str, include_name: &str) -> String {
    format!("{INCLUDE_REQUEST_PREFIX}{parent}|{include_name}")
}
```

Add the matching helpers in the same file:

```rust
fn load_custom_template(
    name: &str,
    entry_dir: &Path,
    config_root: &Path,
) -> Result<Option<String>, Error> {
    let Some((parent_name, include_name)) = decode_include_request(name) else {
        return Ok(None);
    };

    let local_root = parent_local_root(parent_name, entry_dir);
    let resolved_path = match resolve_include(include_name, &local_root, config_root) {
        Ok(path) => path,
        Err(error) => return Err(error),
    };

    fs::read_to_string(&resolved_path)
        .map(Some)
        .map_err(|source| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("failed to read included template {}", resolved_path.display()),
            )
            .with_source(source)
        })
}

fn parent_local_root(parent_name: &str, entry_dir: &Path) -> PathBuf {
    if parent_name == ENTRY_TEMPLATE_NAME {
        return entry_dir.to_path_buf();
    }

    decode_loaded_template_path(parent_name)
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| entry_dir.to_path_buf())
}
```

- [ ] **Step 4: Run the render success-path tests**

Run: `cargo test -p numi-core render::tests::renders_ -v`

Expected: PASS for:
- `renders_local_include_from_template_directory`
- `renders_include_from_shared_config_root`
- `renders_nested_includes_from_mixed_roots`
- existing `renders_builtin_l10n_template`
- existing `renders_custom_template_from_disk`

- [ ] **Step 5: Commit the success-path render work**

```bash
git add crates/numi-core/src/render.rs
git commit -m "feat: add custom template include resolution"
```

### Task 2: Add Explicit Missing And Ambiguous Include Errors

**Files:**
- Modify: `crates/numi-core/src/render.rs`

- [ ] **Step 1: Write failing render tests for missing and ambiguous includes**

```rust
#[test]
fn missing_include_reports_local_and_shared_roots() {
    let temp_dir = make_temp_dir("render-missing-include");
    let config_root = temp_dir.join("Config");
    let templates_dir = config_root.join("Templates");
    fs::create_dir_all(&templates_dir).expect("templates dir should exist");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/missing.jinja\" %}\n",
    )
    .expect("main template should be written");

    let error = render_path(
        &templates_dir.join("main.jinja"),
        &config_root,
        &l10n_context(),
    )
    .expect_err("missing include should fail");

    let message = error.to_string();
    assert!(message.contains("missing included template `partials/missing.jinja`"));
    assert!(message.contains("Templates"));
    assert!(message.contains("Config"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn ambiguous_include_reports_both_candidate_paths() {
    let temp_dir = make_temp_dir("render-ambiguous-include");
    let config_root = temp_dir.join("Config");
    let templates_dir = config_root.join("Templates");
    fs::create_dir_all(templates_dir.join("partials")).expect("local partial dir should exist");
    fs::create_dir_all(config_root.join("partials")).expect("shared partial dir should exist");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/header.jinja\" %}\n",
    )
    .expect("main template should be written");
    fs::write(templates_dir.join("partials/header.jinja"), "LOCAL").expect("local partial should exist");
    fs::write(config_root.join("partials/header.jinja"), "SHARED").expect("shared partial should exist");

    let error = render_path(
        &templates_dir.join("main.jinja"),
        &config_root,
        &l10n_context(),
    )
    .expect_err("ambiguous include should fail");

    let message = error.to_string();
    assert!(message.contains("ambiguous included template `partials/header.jinja`"));
    assert!(message.contains("Templates/partials/header.jinja"));
    assert!(message.contains("Config/partials/header.jinja"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the failing error-path tests**

Run: `cargo test -p numi-core render::tests::missing_include_reports_local_and_shared_roots -v`

Expected: FAIL because the loader still returns a generic MiniJinja failure that does not include both searched roots or the requested include path.

- [ ] **Step 3: Implement deterministic resolution and path-rich loader errors**

Add the resolution helpers in `crates/numi-core/src/render.rs`:

```rust
fn resolve_include(
    include_name: &str,
    local_root: &Path,
    config_root: &Path,
) -> Result<PathBuf, Error> {
    let local_candidate = safe_template_join(local_root, include_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("invalid include path `{include_name}`"),
        )
    })?;
    let shared_candidate = safe_template_join(config_root, include_name).ok_or_else(|| {
        Error::new(
            ErrorKind::InvalidOperation,
            format!("invalid include path `{include_name}`"),
        )
    })?;

    let local_exists = local_candidate.exists();
    let shared_exists = shared_candidate.exists();

    match (local_exists, shared_exists) {
        (true, false) => Ok(local_candidate),
        (false, true) => Ok(shared_candidate),
        (false, false) => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "missing included template `{include_name}`; searched local root {} and shared root {}",
                local_root.display(),
                config_root.display()
            ),
        )),
        (true, true) => Err(Error::new(
            ErrorKind::InvalidOperation,
            format!(
                "ambiguous included template `{include_name}`; matched {} and {}",
                local_candidate.display(),
                shared_candidate.display()
            ),
        )),
    }
}

fn safe_template_join(base: &Path, include_name: &str) -> Option<PathBuf> {
    minijinja::loader::safe_join(base, include_name)
}
```

Also finish the name-encoding helpers so nested includes keep the current local root:

```rust
fn decode_include_request(name: &str) -> Option<(&str, &str)> {
    let payload = name.strip_prefix(INCLUDE_REQUEST_PREFIX)?;
    payload.split_once('|')
}

fn decode_loaded_template_path(name: &str) -> Option<PathBuf> {
    name.strip_prefix(FILE_TEMPLATE_PREFIX).map(PathBuf::from)
}
```

Update `load_custom_template(...)` so successful loads are re-registered under a path-backed template name before being returned on later nested includes:

```rust
let loaded_name = format!("{FILE_TEMPLATE_PREFIX}{}", resolved_path.display());
// use loaded_name as the next parent template name
```

- [ ] **Step 4: Run the render error-path tests and full render module tests**

Run: `cargo test -p numi-core render::tests::ambiguous_include_reports_both_candidate_paths -v`
Expected: PASS

Run: `cargo test -p numi-core render::tests -v`
Expected: PASS for all render tests, including success and failure paths.

- [ ] **Step 5: Commit the error-path resolution work**

```bash
git add crates/numi-core/src/render.rs
git commit -m "feat: add deterministic include error reporting"
```

### Task 3: Wire Config-Root Includes Through The Pipeline

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Modify: `crates/numi-core/src/render.rs`

- [ ] **Step 1: Write a failing pipeline test for custom-template includes**

```rust
#[test]
fn generate_renders_custom_template_with_includes_from_config_root() {
    let temp_dir = make_temp_dir("pipeline-template-includes");
    let config_path = temp_dir.join("swiftgen.toml");
    let localization_root = temp_dir.join("Resources/Localization");
    let templates_dir = temp_dir.join("Templates");
    let partials_dir = temp_dir.join("partials");
    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(&templates_dir).expect("templates dir should exist");
    fs::create_dir_all(&partials_dir).expect("partials dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings should be written");
    fs::write(
        templates_dir.join("L10n.jinja"),
        "{% include \"partials/header.jinja\" %}\n{{ modules[0].entries[0].properties.translation }}\n",
    )
    .expect("main template should be written");
    fs::write(
        partials_dir.join("header.jinja"),
        "HEADER",
    )
    .expect("shared partial should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
path = "Templates/L10n.jinja"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generate should succeed");
    assert_eq!(report.jobs.len(), 1);

    let generated = fs::read_to_string(temp_dir.join("Generated/L10n.swift"))
        .expect("generated output should exist");
    assert_eq!(generated, "HEADER\nProfile\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the focused pipeline test to verify it fails**

Run: `cargo test -p numi-core pipeline::tests::generate_renders_custom_template_with_includes_from_config_root -v`

Expected: FAIL because `render_job(...)` still calls `render_path(&resolved_path, context)` and never passes the config directory as the shared search root.

- [ ] **Step 3: Update the pipeline to pass the config root into custom-template rendering**

```rust
fn render_job(
    config_dir: &Path,
    job: &JobConfig,
    context: &AssetTemplateContext,
) -> Result<String, GenerateError> {
    if let Some(builtin_name) = job.template.builtin.as_deref() {
        return render_builtin(builtin_name, context).map_err(|source| GenerateError::Render {
            job: job.name.clone(),
            source,
        });
    }

    if let Some(template_path) = job.template.path.as_deref() {
        let resolved_path = config_dir.join(template_path);
        return render_path(&resolved_path, config_dir, context).map_err(|source| {
            GenerateError::Render {
                job: job.name.clone(),
                source,
            }
        });
    }

    Err(GenerateError::UnsupportedJob {
        job: job.name.clone(),
        detail: "job template must set a built-in or custom template path".to_string(),
    })
}
```

Keep the existing custom render regression in `render.rs` by updating it to pass the same directory for both `path.parent()` and `config_root`:

```rust
let rendered = render_path(&template_path, &temp_dir, &l10n_context())
    .expect("template should render");
```

- [ ] **Step 4: Run focused and full verification**

Run: `cargo test -p numi-core pipeline::tests::generate_renders_custom_template_with_includes_from_config_root -v`
Expected: PASS

Run: `cargo test -p numi-core -v`
Expected: PASS for render tests, pipeline tests, and existing parsing/output regressions.

- [ ] **Step 5: Commit the pipeline wiring**

```bash
git add crates/numi-core/src/pipeline.rs crates/numi-core/src/render.rs
git commit -m "feat: wire custom template includes through pipeline"
```

## Self-Review

### Spec Coverage

- Local-directory includes: covered by Task 1 success-path tests and loader implementation.
- Shared config-root includes: covered by Task 1 success-path tests and Task 3 pipeline test.
- Nested include behavior: covered by Task 1 nested include test plus the path-join callback in Task 1.
- Missing and ambiguous failure behavior: covered by Task 2 tests and `resolve_include(...)`.
- Deterministic, debuggable CLI-visible errors: covered by Task 2's path-rich error messages and Task 3's pipeline integration path.

### Placeholder Scan

- No `TODO`, `TBD`, or “similar to previous task” instructions remain.
- Every code-changing step includes concrete code blocks.
- Every verification step includes an exact command and expected result.

### Type Consistency

- `render_path(path, config_root, context)` is introduced once in Task 1 and used consistently afterward.
- `render_job(config_dir, job, context)` remains the pipeline boundary and only changes in how it calls `render_path`.
- Include resolution consistently uses the two roots named in the approved spec: local root and shared config root.
