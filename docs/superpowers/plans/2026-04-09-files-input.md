# Files Input Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a new `files` input kind that scans bundled files and generates SwiftGen-style `URL` accessors through a built-in `files` template.

**Architecture:** Extend config validation and pipeline dispatch with a new `files` kind, parse regular files into `RawEntry` values using a dedicated filesystem adapter, then reuse existing normalization/context/rendering to produce a tree of `Data` entries under `modules[].kind = "files"`. Ship a built-in `files` template that respects existing bundle modes and emits deterministic nested enums plus `URL` lookup helpers.

**Tech Stack:** Rust 2024, existing `numi-config`/`numi-core`/`numi-ir` crates, Minijinja built-in templates, filesystem-backed temp-dir tests, CLI integration tests.

---

## File Map

- Modify: `crates/numi-config/src/model.rs`
  - Add `files` to `INPUT_KIND_VALUES`.
- Modify: `crates/numi-config/src/lib.rs`
  - Update config parsing tests/examples to include `files`.
- Modify: `crates/numi-ir/src/lib.rs`
  - Add `ModuleKind::Files` and ensure serialization remains stable.
- Create: `crates/numi-core/src/parse_files.rs`
  - Implement filesystem scanning for single-file and directory inputs, `.DS_Store` skipping, and `RawEntry` construction.
- Modify: `crates/numi-core/src/lib.rs`
  - Export the new parser module.
- Modify: `crates/numi-core/src/pipeline.rs`
  - Dispatch `files` inputs, build `ResourceModule` values, and add parser/integration tests.
- Modify: `crates/numi-core/src/context.rs`
  - Expose `modules[].kind = "files"` and `EntryKind::Data` as template-friendly strings.
- Create: `templates/builtin/files.jinja`
  - Render nested enums and bundle-aware `URL` accessors.
- Create: `fixtures/files-basic/`
  - Add a minimal file-tree fixture plus `numi.toml`.
- Modify: `crates/numi-cli/tests/config_commands.rs`
  - Validate config/print and stale-output behavior for `files`.
- Create: `crates/numi-cli/tests/generate_files.rs`
  - Add CLI-level `generate`/`dump-context` tests for the new input kind.
- Modify: `README.md`
  - Document `type = "files"` and the new built-in template.
- Modify: `docs/context-schema.md`
  - Document `modules[].kind = "files"` and file entry properties.
- Modify: `docs/spec.md`
  - Add `files` to supported input kinds and built-ins.

### Task 1: Add `files` to config and template/context contracts

**Files:**
- Modify: `crates/numi-config/src/model.rs`
- Modify: `crates/numi-config/src/lib.rs`
- Modify: `crates/numi-ir/src/lib.rs`
- Modify: `crates/numi-core/src/context.rs`

- [ ] **Step 1: Write failing config and context tests for the new kind**

```rust
#[test]
fn accepts_files_as_a_supported_input_kind() {
    let config = parse_str(
        r#"
version = 1

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
builtin = "files"
"#,
    )
    .expect("config should parse");

    assert_eq!(config.jobs[0].inputs[0].kind, "files");
    assert_eq!(config.jobs[0].template.builtin.as_deref(), Some("files"));
}
```

```rust
#[test]
fn builds_stable_template_surface_for_files_modules() {
    let module = ResourceModule {
        id: "Fixtures".to_string(),
        kind: ModuleKind::Files,
        name: "Fixtures".to_string(),
        entries: vec![ResourceEntry {
            id: "Onboarding/welcome-video.mp4".to_string(),
            name: "welcome-video.mp4".to_string(),
            source_path: Utf8PathBuf::from("fixture"),
            swift_identifier: "WelcomeVideoMp4".to_string(),
            kind: EntryKind::Data,
            children: Vec::new(),
            properties: Metadata::from([
                ("relativePath".to_string(), json!("Onboarding/welcome-video.mp4")),
                ("fileName".to_string(), json!("welcome-video.mp4")),
                ("pathExtension".to_string(), json!("mp4")),
            ]),
            metadata: Metadata::new(),
        }],
        metadata: Metadata::new(),
    };

    let context = AssetTemplateContext::new(
        "files",
        "Generated/Files.swift",
        "internal",
        "module",
        None,
        &[module],
    )
    .expect("context should build");
    let serialized = serde_json::to_value(&context).expect("context should serialize");

    assert_eq!(serialized["modules"][0]["kind"], "files");
    assert_eq!(serialized["modules"][0]["entries"][0]["kind"], "data");
}
```
- [ ] **Step 2: Run the targeted tests to confirm they fail before implementation**

Run: `cargo test -p numi-config accepts_files_as_a_supported_input_kind -- --nocapture`

Run: `cargo test -p numi-core builds_stable_template_surface_for_files_modules -- --nocapture`

Expected: FAIL because `files` is not a supported input kind, `ModuleKind::Files` does not exist, and `EntryKind::Data` is not exposed in context.

- [ ] **Step 3: Add the new config and IR enum support**

```rust
pub const INPUT_KIND_VALUES: &[&str] = &["xcassets", "strings", "xcstrings", "files"];
```

```rust
pub enum ModuleKind {
    Xcassets,
    Strings,
    Xcstrings,
    Files,
    Custom(String),
}
```

- [ ] **Step 4: Extend context for the new module and entry kinds**

```rust
kind: match &module.kind {
    ModuleKind::Xcassets => "xcassets".to_string(),
    ModuleKind::Strings => "strings".to_string(),
    ModuleKind::Xcstrings => "xcstrings".to_string(),
    ModuleKind::Files => "files".to_string(),
    other => return Err(ContextError::unsupported_module(other)),
},
```

```rust
let kind = match entry.kind {
    EntryKind::Namespace => "namespace".to_string(),
    EntryKind::Image => "image".to_string(),
    EntryKind::Color => "color".to_string(),
    EntryKind::StringKey => "string".to_string(),
    EntryKind::Data => "data".to_string(),
    other => return Err(ContextError::unsupported_entry(other, &entry.id)),
};
```

- [ ] **Step 5: Run the focused tests and make them pass**

Run: `cargo test -p numi-config accepts_files_as_a_supported_input_kind -- --nocapture`

Run: `cargo test -p numi-core builds_stable_template_surface_for_files_modules -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit the contract changes**

```bash
git add crates/numi-config/src/model.rs crates/numi-config/src/lib.rs crates/numi-ir/src/lib.rs crates/numi-core/src/context.rs
git commit -m "feat: add files input contract"
```

### Task 2: Implement filesystem parsing for `files`

**Files:**
- Create: `crates/numi-core/src/parse_files.rs`
- Modify: `crates/numi-core/src/lib.rs`
- Test: `crates/numi-core/src/parse_files.rs`

- [ ] **Step 1: Write failing parser tests for single-file and directory inputs**

```rust
#[test]
fn parses_single_file_input_into_data_entry() {
    let temp_dir = make_temp_dir("parse-single-file");
    let file_path = temp_dir.join("Fixtures/welcome-video.mp4");
    fs::create_dir_all(file_path.parent().expect("parent should exist")).expect("dir should exist");
    fs::write(&file_path, b"video").expect("file should be written");

    let entries = parse_files(&file_path).expect("files input should parse");

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "welcome-video.mp4");
    assert_eq!(entries[0].kind, EntryKind::Data);
    assert_eq!(entries[0].properties["pathExtension"], "mp4");
}
```

```rust
#[test]
fn parses_directory_input_recursively_and_skips_ds_store() {
    let temp_dir = make_temp_dir("parse-files-directory");
    let root = temp_dir.join("Fixtures");
    fs::create_dir_all(root.join("Onboarding")).expect("nested dir should exist");
    fs::write(root.join(".DS_Store"), b"noise").expect("noise file should be written");
    fs::write(root.join("Onboarding/welcome-video.mp4"), b"video").expect("video file should be written");
    fs::write(root.join("faq.pdf"), b"pdf").expect("pdf file should be written");

    let entries = parse_files(&root).expect("files input should parse");

    assert_eq!(entries.iter().map(|entry| entry.path.as_str()).collect::<Vec<_>>(), vec![
        "Onboarding/welcome-video.mp4",
        "faq.pdf",
    ]);
}
```

- [ ] **Step 2: Run the parser tests to confirm they fail before the parser exists**

Run: `cargo test -p numi-core parse_files -- --nocapture`

Expected: FAIL because `parse_files` does not exist yet.

- [ ] **Step 3: Implement `parse_files.rs`**

```rust
#[derive(Debug)]
pub enum ParseFilesError {
    ReadDirectory { path: PathBuf, source: io::Error },
    InvalidInputPath { path: PathBuf },
    InvalidUtf8Path { path: PathBuf },
}

pub fn parse_files(path: &Path) -> Result<Vec<RawEntry>, ParseFilesError> {
    if path.is_file() {
        return Ok(vec![parse_file_entry(path, path.parent().unwrap_or_else(|| Path::new(".")))?]);
    }

    if path.is_dir() {
        let mut entries = Vec::new();
        collect_file_entries(path, path, &mut entries)?;
        entries.sort_by(|left, right| left.path.cmp(&right.path));
        return Ok(entries);
    }

    Err(ParseFilesError::InvalidInputPath {
        path: path.to_path_buf(),
    })
}
```

```rust
fn collect_file_entries(
    root: &Path,
    current: &Path,
    entries: &mut Vec<RawEntry>,
) -> Result<(), ParseFilesError> {
    let read_dir = fs::read_dir(current).map_err(|source| ParseFilesError::ReadDirectory {
        path: current.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseFilesError::ReadDirectory {
            path: current.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| ParseFilesError::ReadDirectory {
            path: path.clone(),
            source,
        })?;

        if file_type.is_dir() {
            collect_file_entries(root, &path, entries)?;
            continue;
        }

        if path.file_name().and_then(|name| name.to_str()) == Some(".DS_Store") {
            continue;
        }

        if file_type.is_file() {
            entries.push(parse_file_entry(&path, root)?);
        }
    }

    Ok(())
}
```

```rust
fn parse_file_entry(path: &Path, root: &Path) -> Result<RawEntry, ParseFilesError> {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let relative_path = relative
        .iter()
        .map(|component| component.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join("/");

    Ok(RawEntry {
        path: relative_path.clone(),
        source_path: Utf8PathBuf::from_path_buf(path.to_path_buf())
            .map_err(|path| ParseFilesError::InvalidUtf8Path { path })?,
        kind: EntryKind::Data,
        properties: Metadata::from([
            ("relativePath".to_string(), Value::String(relative_path)),
            (
                "fileName".to_string(),
                Value::String(path.file_name().unwrap().to_string_lossy().into_owned()),
            ),
            (
                "pathExtension".to_string(),
                Value::String(
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .unwrap_or("")
                        .to_string(),
                ),
            ),
        ]),
    })
}
```

- [ ] **Step 4: Export the parser module**

```rust
mod parse_files;
```

- [ ] **Step 5: Run the parser test module and make it pass**

Run: `cargo test -p numi-core parse_files -- --nocapture`

Expected: PASS.

- [ ] **Step 6: Commit the parser adapter**

```bash
git add crates/numi-core/src/parse_files.rs crates/numi-core/src/lib.rs
git commit -m "feat: parse bundled files inputs"
```

### Task 3: Wire `files` through pipeline, fixture coverage, and the built-in template

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Modify: `crates/numi-core/src/render.rs`
- Create: `templates/builtin/files.jinja`
- Create: `fixtures/files-basic/numi.toml`
- Create: `fixtures/files-basic/Resources/Fixtures/faq.pdf`
- Create: `fixtures/files-basic/Resources/Fixtures/Onboarding/welcome-video.mp4`
- Create: `crates/numi-cli/tests/generate_files.rs`
- Modify: `crates/numi-cli/tests/config_commands.rs`

- [ ] **Step 1: Write failing integration tests for generate and dump-context**

```rust
#[test]
fn generate_writes_files_accessors_from_fixture() {
    let temp_root = make_temp_dir("generate-files");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(output.status.success(), "command failed: {output:?}");
    let generated = fs::read_to_string(working_root.join("Generated/Files.swift"))
        .expect("generated files output should exist");
    assert!(generated.contains("enum Files"));
    assert!(generated.contains("static let faqPdf"));
}
```

```rust
#[test]
fn dump_context_emits_files_module_kind_and_properties() {
    let temp_root = make_temp_dir("dump-context-files");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("numi dump-context should run");

    assert!(output.status.success(), "command failed: {output:?}");
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("stdout should be JSON");
    assert_eq!(json["modules"][0]["kind"], "files");
    assert_eq!(json["modules"][0]["entries"][0]["kind"], "namespace");
    assert_eq!(json["modules"][0]["entries"][0]["children"][0]["kind"], "data");
    assert!(
        json["modules"][0]["entries"][0]["children"][0]["properties"]["relativePath"].is_string()
    );
}
```

- [ ] **Step 2: Run the new CLI tests to confirm they fail before wiring**

Run: `cargo test -p numi-cli --test generate_files -v`

Expected: FAIL because there is no `files` pipeline branch, fixture, or built-in template yet.

- [ ] **Step 3: Add the pipeline branch and fixture**

```rust
ParseFiles {
    job: String,
    source: ParseFilesError,
},
```

```rust
"files" => {
    let raw_entries = crate::parse_files::parse_files(&input_path)
        .map_err(|source| GenerateError::ParseFiles {
            job: job.name.clone(),
            source,
        })?;
    let entries = normalize_scope(&job.name, raw_entries).map_err(GenerateError::Diagnostics)?;
    let module_id = input_path
        .file_stem()
        .or_else(|| input_path.file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("Files")
        .to_string();
    modules.push(ResourceModule {
        id: module_id.clone(),
        kind: ModuleKind::Files,
        name: swift_identifier(&module_id),
        entries,
        metadata: Metadata::new(),
    });
}
```

```toml
version = 1

[[jobs]]
name = "files"
output = "Generated/Files.swift"

[[jobs.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.template]
builtin = "files"
```

```text
fixtures/files-basic/Resources/Fixtures/
fixtures/files-basic/Resources/Fixtures/faq.pdf
fixtures/files-basic/Resources/Fixtures/Onboarding/welcome-video.mp4
```

```rust
const FILES_TEMPLATE: &str = include_str!("../../../templates/builtin/files.jinja");

let template_source = match builtin_name {
    "swiftui-assets" => SWIFTUI_ASSETS_TEMPLATE,
    "l10n" => L10N_TEMPLATE,
    "files" => FILES_TEMPLATE,
    other => return Err(RenderError::UnknownBuiltin(other.to_owned())),
};
```

- [ ] **Step 4: Implement the built-in template**

```jinja
import Foundation

{% macro render_entries(entries, indent) -%}
{% for entry in entries -%}
{% if entry.kind == "namespace" -%}
{{ indent }}{{ access_level }} enum {{ entry.swiftIdentifier }} {
{{ render_entries(entry.children, indent ~ "    ") }}{{ indent }}}
{% elif entry.kind == "data" -%}
{{ indent }}{{ access_level }} static let {{ entry.swiftIdentifier | lower_first }} = file({{ entry.properties.relativePath | string_literal }})
{% endif -%}
{% endfor -%}
{%- endmacro %}

{{ access_level }} enum Files {
{{ render_entries(modules[0].entries, "    ") }}}

private func resourceBundle() -> Bundle {
{% if bundle.mode == "module" -%}
    Bundle.module
{% elif bundle.mode == "main" -%}
    .main
{% else -%}
    Bundle(identifier: {{ bundle.identifier | string_literal }})!
{% endif -%}
}

private func file(_ path: String) -> URL {
    guard let url = resourceBundle().url(forResource: path, withExtension: nil) else {
        fatalError("Missing file resource: \(path)")
    }
    return url
}
```

- [ ] **Step 5: Run the integration suite and make it pass**

Run: `cargo test -p numi-core pipeline::tests -- --nocapture`

Run: `cargo test -p numi-cli --test generate_files -v`

Run: `cargo test -p numi-cli --test config_commands -v`

Expected: PASS.

- [ ] **Step 6: Commit the end-to-end files feature**

```bash
git add crates/numi-core/src/pipeline.rs templates/builtin/files.jinja fixtures/files-basic crates/numi-cli/tests/generate_files.rs crates/numi-cli/tests/config_commands.rs
git commit -m "feat: generate bundled files accessors"
```

### Task 4: Document the new input kind and run workspace verification

**Files:**
- Modify: `README.md`
- Modify: `docs/context-schema.md`
- Modify: `docs/spec.md`

- [ ] **Step 1: Update the developer docs and schema**

```md
Supported input kinds:

- `xcassets`
- `strings`
- `xcstrings`
- `files`

Use `type = "files"` for bundled resources that should generate Swift-style `URL` accessors without parsing file contents.
```

```md
For `modules[].kind = "files"`:

- leaf entries use `kind = "data"`
- `properties.relativePath` is the bundle-relative lookup path
- `properties.fileName` is the original file name
- `properties.pathExtension` is the final extension segment, or `""`
```

- [ ] **Step 2: Run full verification**

Run: `cargo test -v`

Run: `cargo fmt --check`

Expected: PASS across the workspace.

- [ ] **Step 3: Commit the docs and verification pass**

```bash
git add README.md docs/context-schema.md docs/spec.md
git commit -m "docs: document files input support"
```
