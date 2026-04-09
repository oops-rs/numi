# Langcodec L10n Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace Numi's manual `.strings` and `.xcstrings` parsing with a `langcodec`-backed adapter while preserving Numi's rendering pipeline, stable core context fields, and warning behavior.

**Architecture:** Keep `numi-core`'s localization boundary at `parse_l10n.rs`, but turn that file into an adapter around `langcodec`'s `Resource`/`Entry`/`Translation` model instead of a handwritten parser. `pipeline.rs`, `context.rs`, and CLI command behavior should continue to consume `LocalizationTable` and diagnostics the same way they do today, with only additive metadata changes where `langcodec` provides richer information.

**Tech Stack:** Rust 2024, `langcodec`, `serde_json`, `numi-ir`, `numi-diagnostics`, existing CLI integration tests and fixture-based temp-dir tests.

---

## File Map

- Modify: `crates/numi-core/Cargo.toml`
  - Add the `langcodec` dependency and keep all localization parsing inside `numi-core`.
- Modify: `crates/numi-core/src/parse_l10n.rs`
  - Remove handwritten `.strings` / `.xcstrings` parsing from the runtime path.
  - Add adapter helpers that read files through `langcodec`, map `Resource`/`Entry`/`Translation` into `LocalizationTable`, and emit Numi warnings for partially adaptable records.
- Modify: `crates/numi-core/src/pipeline.rs`
  - Keep the parser call sites stable, but update tests to prove `generate`, `check`, and `dump-context` still behave correctly through the new adapter.
- Modify: `crates/numi-core/src/context.rs`
  - Extend or adjust serialization tests only if the new adapter exposes additive metadata fields such as status or comment.
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
  - Keep output-contract coverage for `.strings` and `.xcstrings`, and add representative warning/success cases that currently fail in lama-ludo.
- Modify: `crates/numi-cli/tests/config_commands.rs`
  - Preserve `check` warning behavior and stale-output exit codes while the parser backend changes.
- Modify: `README.md`
  - Update developer usage notes to mention that Apple localization parsing is delegated to `langcodec`.
- Modify: `docs/context-schema.md`
  - Document any additive localization metadata exposed via the adapter.
- Modify: `docs/spec.md`
  - Replace “especially via `langcodec` where applicable” language with the now-true parser contract.

### Task 1: Add `langcodec` and lock adapter-facing regression tests

**Files:**
- Modify: `crates/numi-core/Cargo.toml`
- Modify: `crates/numi-core/src/parse_l10n.rs`
- Test: `crates/numi-core/src/parse_l10n.rs`

- [ ] **Step 1: Add failing tests for the real parser regressions and the new adapter contract**

```rust
#[test]
fn parses_strings_with_escaped_apostrophes_via_langcodec() {
    let temp_dir = make_temp_dir("parse-strings-apostrophe");
    let strings_path = temp_dir.join("Localizable.strings");
    fs::write(
        &strings_path,
        "\"invite.accept\" = \"Can\\'t accept the invitation\";\n",
    )
    .expect("strings file should be written");

    let tables = parse_strings(&strings_path).expect("strings should parse");

    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].module_kind, ModuleKind::Strings);
    assert_eq!(
        tables[0].entries[0].properties["translation"],
        Value::String("Can't accept the invitation".to_string())
    );
}

#[test]
fn skips_xcstrings_entries_without_renderable_singular_value_with_warning() {
    let temp_dir = make_temp_dir("parse-xcstrings-empty-entry");
    let xcstrings_path = temp_dir.join("Localizable.xcstrings");
    fs::write(
        &xcstrings_path,
        r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "Lv.%lld": {
      "comment": "header only"
    },
    "": {
      "shouldTranslate": false
    }
  }
}
"#,
    )
    .expect("xcstrings file should be written");

    let tables = parse_xcstrings(&xcstrings_path).expect("xcstrings should parse");

    assert!(tables[0].entries.is_empty());
    assert_eq!(tables[0].warnings.len(), 2);
    assert!(tables[0].warnings[0].message.contains("Lv.%lld"));
    assert!(tables[0].warnings[1].message.contains("do_not_translate"));
}
```

- [ ] **Step 2: Run the focused parser tests to confirm they fail with the current handwritten parser**

Run: `cargo test -p numi-core parse_strings_with_escaped_apostrophes_via_langcodec skips_xcstrings_entries_without_renderable_singular_value_with_warning -- --nocapture`

Expected: FAIL because `parse_strings` still rejects `\\'` and `parse_xcstrings` still treats unsupported records as hard parse errors.

- [ ] **Step 3: Add the `langcodec` dependency in `numi-core`**

```toml
[dependencies]
atomic-write-file = "0.3"
camino = "1"
langcodec = "0.11.0"
minijinja = "2"
numi-config = { path = "../numi-config" }
numi-diagnostics = { path = "../numi-diagnostics" }
numi-ir = { path = "../numi-ir" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 4: Run the focused parser tests again to confirm the dependency-only change still fails**

Run: `cargo test -p numi-core parse_strings_with_escaped_apostrophes_via_langcodec skips_xcstrings_entries_without_renderable_singular_value_with_warning -- --nocapture`

Expected: FAIL, but compile succeeds with `langcodec` available for the next task.

- [ ] **Step 5: Commit the dependency and failing tests**

```bash
git add crates/numi-core/Cargo.toml crates/numi-core/src/parse_l10n.rs
git commit -m "test: lock l10n adapter regressions"
```

### Task 2: Replace handwritten localization parsing with a `langcodec` adapter

**Files:**
- Modify: `crates/numi-core/src/parse_l10n.rs`
- Test: `crates/numi-core/src/parse_l10n.rs`

- [ ] **Step 1: Replace the handwritten parser types with adapter helpers**

```rust
use langcodec::{
    Codec,
    infer_language_from_path,
    types::{Entry, EntryStatus, Resource, Translation},
};

fn parse_strings_file(path: &Path) -> Result<LocalizationTable, ParseL10nError> {
    parse_langcodec_file(path, ModuleKind::Strings)
}

fn parse_xcstrings_file(path: &Path) -> Result<LocalizationTable, ParseL10nError> {
    parse_langcodec_file(path, ModuleKind::Xcstrings)
}

fn parse_langcodec_file(
    path: &Path,
    module_kind: ModuleKind,
) -> Result<LocalizationTable, ParseL10nError> {
    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();
    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;

    let resource = read_langcodec_resource(path)?;
    let (entries, warnings) = adapt_resource_entries(&source_path, module_kind, resource);

    Ok(LocalizationTable {
        table_name,
        source_path,
        module_kind,
        entries,
        warnings,
    })
}
```

- [ ] **Step 2: Implement the `langcodec` read helper and error translation**

```rust
fn read_langcodec_resource(path: &Path) -> Result<Resource, ParseL10nError> {
    let mut codec = Codec::new();
    let language = infer_language_from_path(path)
        .ok()
        .flatten()
        .map(|value| value.to_string());

    codec
        .read_file_by_extension(path, language)
        .map_err(|error| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;

    codec
        .resources
        .into_iter()
        .next()
        .ok_or_else(|| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: "langcodec returned no resources".to_string(),
        })
}
```

- [ ] **Step 3: Implement entry adaptation and warning policy**

```rust
fn adapt_resource_entries(
    source_path: &Utf8PathBuf,
    module_kind: ModuleKind,
    resource: Resource,
) -> (Vec<RawEntry>, Vec<Diagnostic>) {
    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for entry in resource.entries {
        match adapt_entry(source_path, module_kind, &entry) {
            Ok(Some(raw_entry)) => entries.push(raw_entry),
            Ok(None) => warnings.push(langcodec_skip_warning(source_path, &entry)),
            Err(message) => warnings.push(Diagnostic {
                severity: Severity::Warning,
                path: Some(source_path.as_std_path().to_path_buf()),
                message,
            }),
        }
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    warnings.sort_by(|left, right| left.message.cmp(&right.message));

    (entries, warnings)
}

fn adapt_entry(
    source_path: &Utf8PathBuf,
    module_kind: ModuleKind,
    entry: &Entry,
) -> Result<Option<RawEntry>, String> {
    let translation = match &entry.value {
        Translation::Singular(value) => value.clone(),
        Translation::Empty => return Ok(None),
        Translation::Plural(_) => return Ok(None),
    };

    let mut properties = Metadata::from([
        ("key".to_string(), Value::String(entry.id.clone())),
        ("translation".to_string(), Value::String(translation)),
    ]);

    if let Some(comment) = &entry.comment {
        properties.insert("comment".to_string(), Value::String(comment.clone()));
    }

    if let Some(status) = entry_status_value(entry.status) {
        properties.insert("status".to_string(), Value::String(status.to_string()));
    }

    if module_kind == ModuleKind::Xcstrings {
        if let Some(placeholders) = build_placeholder_metadata_from_translation(&entry.value) {
            properties.insert("placeholders".to_string(), Value::Array(placeholders));
        }
    }

    Ok(Some(RawEntry {
        path: entry.id.clone(),
        source_path: source_path.clone(),
        kind: EntryKind::StringKey,
        properties,
    }))
}

fn entry_status_value(status: EntryStatus) -> Option<&'static str> {
    match status {
        EntryStatus::Translated => Some("translated"),
        EntryStatus::NeedsReview => Some("needs_review"),
        EntryStatus::DoNotTranslate => Some("do_not_translate"),
        EntryStatus::New => Some("new"),
        EntryStatus::Stale => Some("stale"),
        _ => None,
    }
}
```

- [ ] **Step 4: Delete the dead handwritten parser code from the runtime path**

```rust
// Remove:
// - StringsParser and its tokenization helpers
// - decode_strings_bytes
// - XcstringsCatalog / XcstringsRecord / XcstringsLocalization / XcstringsVariations
// - custom string-unit selection logic
//
// Keep:
// - ParseL10nError
// - LocalizationTable
// - directory traversal helpers
// - adapter-oriented tests
```

- [ ] **Step 5: Run the `parse_l10n` test module and make it pass**

Run: `cargo test -p numi-core parse_l10n -- --nocapture`

Expected: PASS, including the new escaped-apostrophe and skipped-entry warning tests.

- [ ] **Step 6: Commit the adapter migration**

```bash
git add crates/numi-core/src/parse_l10n.rs
git commit -m "refactor: use langcodec for l10n parsing"
```

### Task 3: Prove pipeline and CLI behavior stay stable through the new parser

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Modify: `crates/numi-core/src/context.rs`
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
- Modify: `crates/numi-cli/tests/config_commands.rs`

- [ ] **Step 1: Add a focused pipeline regression for a real-world `.strings` escape case**

```rust
#[test]
fn generate_accepts_strings_with_escaped_apostrophes_via_langcodec() {
    let temp_dir = make_temp_dir("pipeline-strings-apostrophe");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"invite.accept\" = \"Can\\'t accept the invitation\";\n",
    )
    .expect("strings file should be written");
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
builtin = "l10n"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");

    assert!(report.warnings.is_empty());
}
```

- [ ] **Step 2: Extend CLI tests so warning-based `.xcstrings` adaptation still succeeds**

```rust
#[test]
fn generate_warns_and_succeeds_for_langcodec_skipped_xcstrings_entries() {
    let temp_root = make_temp_dir("generate-xcstrings-langcodec-warning");
    let working_root = temp_root.join("fixture");
    let localization_root = working_root.join("Resources/Localization");
    fs::create_dir_all(&localization_root).expect("localization directory should exist");
    fs::write(
        working_root.join("numi.toml"),
        r#"
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.template]
builtin = "l10n"
"#,
    )
    .expect("config should be written");
    fs::write(
        localization_root.join("Localizable.xcstrings"),
        r#"{
  "version": "1.0",
  "sourceLanguage": "en",
  "strings": {
    "profile.title": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Profile"
          }
        }
      }
    },
    "Lv.%lld": {
      "comment": "header only"
    }
  }
}
"#,
    )
    .expect("xcstrings file should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(output.status.success(), "command failed: {output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("warning: skipping xcstrings key `Lv.%lld`"),
        "stderr was: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
```

- [ ] **Step 3: Update context tests only for additive metadata**

```rust
assert_eq!(serialized["modules"][0]["kind"], "xcstrings");
assert_eq!(
    serialized["modules"][0]["entries"][0]["properties"]["status"],
    "translated"
);
assert_eq!(
    serialized["modules"][0]["entries"][0]["properties"]["comment"],
    "Greeting"
);
```

- [ ] **Step 4: Run the cross-layer regression suite**

Run: `cargo test -p numi-core pipeline::tests -- --nocapture`

Run: `cargo test -p numi-cli --test generate_l10n -v`

Run: `cargo test -p numi-cli --test config_commands -v`

Expected: PASS, with warning cases still surfacing on stderr and stale-output checks still exiting with code `2`.

- [ ] **Step 5: Commit the compatibility harness updates**

```bash
git add crates/numi-core/src/pipeline.rs crates/numi-core/src/context.rs crates/numi-cli/tests/generate_l10n.rs crates/numi-cli/tests/config_commands.rs
git commit -m "test: preserve l10n pipeline behavior through langcodec"
```

### Task 4: Document the new parser contract and verify against representative real-world data

**Files:**
- Modify: `README.md`
- Modify: `docs/context-schema.md`
- Modify: `docs/spec.md`

- [ ] **Step 1: Update the docs to reflect the new parser boundary**

```md
## Localization Parsing

Numi delegates Apple localization parsing to `langcodec`.
That means `.strings` and `.xcstrings` syntax compatibility is owned by `langcodec`, while Numi owns:

- config and input discovery
- adaptation into the render context
- template rendering
- warning and error presentation
```

- [ ] **Step 2: Document additive metadata in the context schema**

```md
For localization entries, these properties remain stable:

- `key`
- `translation`

The adapter may also expose additive metadata when available:

- `comment`
- `status`
- `placeholders`
```

- [ ] **Step 3: Run the full repo verification suite**

Run: `cargo test -v`

Run: `cargo fmt --check`

Expected: PASS across the workspace.

- [ ] **Step 4: Re-run representative lama-ludo validation commands**

Run: `cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/AppUI/numi.toml`

Run: `cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Game/numi.toml`

Run: `cargo run -p numi-cli -- check --config /Users/wendell/developer/WeNext/lama-ludo-ios/Modules/Profile/numi.toml`

Expected:

- `AppUI`: no parser failure on `\\'`; success or stale-output exit depending on generated-file state
- `Game`: warning for skipped non-renderable entry, not fatal parse failure
- `Profile`: warning for skipped do-not-translate/empty entry, not fatal parse failure

- [ ] **Step 5: Commit the docs and final verification pass**

```bash
git add README.md docs/context-schema.md docs/spec.md
git commit -m "docs: document langcodec parser boundary"
```
