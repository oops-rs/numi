# `.xcstrings` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `.xcstrings` localization input support, expose additive placeholder metadata, and emit warnings for skipped plural or device-specific catalog records without failing generation.

**Architecture:** Keep localization parsing inside `crates/numi-core/src/parse_l10n.rs`, but promote the parser output to a shared table/result shape that can carry module kind and warnings. The pipeline stays responsible for building `ResourceModule` values and hard-failing true errors, while CLI command handlers become responsible for printing non-fatal warnings to stderr for both `generate` and `check`.

**Tech Stack:** Rust, serde_json, existing `numi-diagnostics` diagnostics model, fixture-backed CLI tests, deterministic JSON and Swift output assertions

---

## File Structure

- Modify: `crates/numi-core/src/parse_l10n.rs`
  - Add `.xcstrings` parsing.
  - Introduce a shared localization-table result that can carry warnings.
  - Keep `.strings` support working on the same path.
- Modify: `crates/numi-core/src/pipeline.rs`
  - Add the `xcstrings` input branch.
  - Preserve `ModuleKind::Xcstrings`.
  - Separate warnings from hard diagnostics.
- Modify: `crates/numi-core/src/context.rs`
  - Add context coverage for `xcstrings` modules and optional placeholders.
- Modify: `crates/numi-core/src/lib.rs`
  - Re-export any report shape changes needed by CLI.
- Modify: `crates/numi-cli/src/lib.rs`
  - Print warnings to stderr during `generate` and `check`.
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
  - Add fixture-backed `.xcstrings` generate and `dump-context` coverage.
  - Add stderr assertions for warning cases.
- Create: `fixtures/xcstrings-basic/swiftgen.toml`
  - Minimal config for an `.xcstrings` localization job.
- Create: `fixtures/xcstrings-basic/Resources/Localization/Localizable.xcstrings`
  - Plain-string catalog with placeholder metadata and one unsupported variation entry.
- Modify: `docs/context-schema.md`
  - Document `xcstrings` as a stable module kind and `placeholders` as additive metadata.
- Modify: `docs/migration-from-swiftgen.md`
  - Remove the statement that `.xcstrings` is deferred.

### Task 1: Refactor Localization Parsing To A Shared Table Result

**Files:**
- Modify: `crates/numi-core/src/parse_l10n.rs`
- Modify: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Write the failing unit test that locks the shared table shape for `.strings`**

Add this test near the existing `parse_strings(...)` tests in `crates/numi-core/src/parse_l10n.rs`:

```rust
#[test]
fn parses_strings_into_shared_localization_tables() {
    let temp_dir = make_temp_dir("parse-strings-shared-table");
    let strings_path = temp_dir.join("Localizable.strings");
    fs::write(
        &strings_path,
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");

    let tables = parse_strings(&strings_path).expect("strings should parse");

    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].table_name, "Localizable");
    assert_eq!(tables[0].module_kind, ModuleKind::Strings);
    assert!(tables[0].warnings.is_empty());
    assert_eq!(tables[0].entries[0].properties["key"], "profile.title");
    assert_eq!(tables[0].entries[0].properties["translation"], "Profile");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the focused parser test and verify it fails first**

Run: `cargo test -p numi-core parse_l10n::tests::parses_strings_into_shared_localization_tables -v`

Expected: FAIL with an unknown field or missing field error because `StringsTable` does not yet expose `module_kind` or `warnings`.

- [ ] **Step 3: Replace `StringsTable` with a shared `LocalizationTable` result**

Update `crates/numi-core/src/parse_l10n.rs` so the parser returns a shared table shape:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizationTable {
    pub table_name: String,
    pub module_kind: ModuleKind,
    pub source_path: Utf8PathBuf,
    pub entries: Vec<RawEntry>,
    pub warnings: Vec<Diagnostic>,
}
```

Update the imports at the top of the file:

```rust
use numi_diagnostics::{Diagnostic, Severity};
use numi_ir::{EntryKind, Metadata, ModuleKind, RawEntry};
```

Change `parse_strings(...)` and `parse_strings_file(...)` to return `Vec<LocalizationTable>` and `LocalizationTable` respectively, with the `.strings` branch constructing:

```rust
Ok(LocalizationTable {
    table_name,
    module_kind: ModuleKind::Strings,
    source_path,
    entries,
    warnings: Vec::new(),
})
```

Update `crates/numi-core/src/pipeline.rs` to consume `table.module_kind` instead of hardcoding `ModuleKind::Strings` for the existing strings path:

```rust
modules.push(ResourceModule {
    id: table_name.clone(),
    kind: table.module_kind.clone(),
    name: swift_identifier(&table_name),
    entries,
    metadata: Metadata::from([(
        "tableName".to_string(),
        Value::String(table_name),
    )]),
});
```

- [ ] **Step 4: Run parser and pipeline tests to verify the refactor passes without behavior changes**

Run: `cargo test -p numi-core parse_l10n::tests::parses_strings_into_shared_localization_tables -v`

Expected: PASS

Run: `cargo test -p numi-core parse_l10n::tests::parses_strings_files_from_directory -v`

Expected: PASS

Run: `cargo test -p numi-core pipeline::tests::generate_rejects_duplicate_strings_table_names_from_directory_inputs -v`

Expected: PASS

- [ ] **Step 5: Commit the parser refactor**

```bash
git add crates/numi-core/src/parse_l10n.rs crates/numi-core/src/pipeline.rs
git commit -m "refactor: share localization parser table results"
```

### Task 2: Parse `.xcstrings` Catalogs And Surface Skip Warnings

**Files:**
- Modify: `crates/numi-core/src/parse_l10n.rs`
- Modify: `crates/numi-core/src/pipeline.rs`

- [ ] **Step 1: Write failing `.xcstrings` parser tests for plain strings, placeholders, and skipped variations**

Add these tests in `crates/numi-core/src/parse_l10n.rs`:

```rust
#[test]
fn parses_xcstrings_plain_string_and_placeholders() {
    let temp_dir = make_temp_dir("parse-xcstrings-placeholders");
    let catalog_path = temp_dir.join("Localizable.xcstrings");
    fs::write(
        &catalog_path,
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "profile.title" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "Profile"
          }
        }
      }
    },
    "files.remaining" : {
      "localizations" : {
        "en" : {
          "stringUnit" : {
            "state" : "translated",
            "value" : "%#@files@"
          },
          "substitutions" : {
            "files" : {
              "argNum" : 1,
              "formatSpecifier" : "ld"
            }
          }
        }
      }
    }
  }
}"#,
    )
    .expect("catalog should be written");

    let tables = parse_xcstrings(&catalog_path).expect("catalog should parse");

    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].module_kind, ModuleKind::Xcstrings);
    assert!(tables[0].warnings.is_empty());
    assert_eq!(tables[0].entries.len(), 2);
    assert_eq!(tables[0].entries[0].properties["key"], "files.remaining");
    assert_eq!(
        tables[0].entries[0].properties["placeholders"][0]["name"],
        "files"
    );
    assert_eq!(
        tables[0].entries[0].properties["placeholders"][0]["format"],
        "ld"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn skips_xcstrings_plural_variations_with_warning() {
    let temp_dir = make_temp_dir("parse-xcstrings-plural-warning");
    let catalog_path = temp_dir.join("Localizable.xcstrings");
    fs::write(
        &catalog_path,
        r#"{
  "sourceLanguage" : "en",
  "strings" : {
    "files.remaining" : {
      "localizations" : {
        "en" : {
          "variations" : {
            "plural" : {
              "one" : {
                "stringUnit" : { "state" : "translated", "value" : "%d file remaining" }
              },
              "other" : {
                "stringUnit" : { "state" : "translated", "value" : "%d files remaining" }
              }
            }
          }
        }
      }
    }
  }
}"#,
    )
    .expect("catalog should be written");

    let tables = parse_xcstrings(&catalog_path).expect("catalog should parse");

    assert_eq!(tables[0].entries.len(), 0);
    assert_eq!(tables[0].warnings.len(), 1);
    assert_eq!(tables[0].warnings[0].severity, Severity::Warning);
    assert!(tables[0].warnings[0].message.contains("files.remaining"));
    assert!(tables[0].warnings[0].message.contains("plural"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the focused `.xcstrings` parser test and verify it fails**

Run: `cargo test -p numi-core parse_l10n::tests::parses_xcstrings_plain_string_and_placeholders -v`

Expected: FAIL with `cannot find function 'parse_xcstrings'` or equivalent missing-symbol error.

- [ ] **Step 3: Implement `.xcstrings` file and directory parsing**

Add these public parser entrypoints to `crates/numi-core/src/parse_l10n.rs`:

```rust
pub fn parse_xcstrings(path: &Path) -> Result<Vec<LocalizationTable>, ParseL10nError> {
    if path.is_file() {
        return parse_xcstrings_file(path).map(|table| vec![table]);
    }

    if path.is_dir() {
        let mut files = Vec::new();
        collect_xcstrings_files(path, &mut files)?;
        files.sort();

        return files
            .into_iter()
            .map(|file| parse_xcstrings_file(&file))
            .collect();
    }

    Err(ParseL10nError::InvalidPath {
        path: path.to_path_buf(),
    })
}
```

Add the catalog walker helpers in the same file:

```rust
fn collect_xcstrings_files(
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), ParseL10nError> {
    let read_dir = fs::read_dir(directory).map_err(|source| ParseL10nError::ReadDirectory {
        path: directory.to_path_buf(),
        source,
    })?;

    for entry in read_dir {
        let entry = entry.map_err(|source| ParseL10nError::ReadDirectory {
            path: directory.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| ParseL10nError::ReadDirectory {
            path: path.clone(),
            source,
        })?;

        if file_type.is_dir() {
            collect_xcstrings_files(&path, files)?;
            continue;
        }

        if path.extension().and_then(|ext| ext.to_str()) == Some("xcstrings") {
            files.push(path);
        }
    }

    Ok(())
}
```

Use a JSON walker for supported records:

```rust
fn parse_xcstrings_file(path: &Path) -> Result<LocalizationTable, ParseL10nError> {
    let bytes = fs::read(path).map_err(|source| ParseL10nError::ReadFile {
        path: path.to_path_buf(),
        source,
    })?;
    let root: Value = serde_json::from_slice(&bytes).map_err(|source| ParseL10nError::ParseFile {
        path: path.to_path_buf(),
        message: format!("invalid xcstrings JSON: {source}"),
    })?;

    let table_name = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| ParseL10nError::InvalidUtf8Path {
            path: path.to_path_buf(),
        })?
        .to_owned();
    let source_path = Utf8PathBuf::from_path_buf(path.to_path_buf())
        .map_err(|path| ParseL10nError::InvalidUtf8Path { path })?;

    let strings = root
        .get("strings")
        .and_then(Value::as_object)
        .ok_or_else(|| ParseL10nError::ParseFile {
            path: path.to_path_buf(),
            message: "xcstrings catalog is missing a top-level `strings` object".to_string(),
        })?;

    let mut entries = Vec::new();
    let mut warnings = Vec::new();

    for (key, record) in strings.iter() {
        match parse_xcstrings_record(key, record, &source_path) {
            Ok(Some(entry)) => entries.push(entry),
            Ok(None) => warnings.push(
                Diagnostic {
                    severity: Severity::Warning,
                    message: format!(
                        "skipped unsupported `.xcstrings` variations for `{key}`"
                    ),
                    hint: Some(
                        "plural and device-specific variations are not supported in this Numi version"
                            .to_string(),
                    ),
                    job: None,
                    path: Some(source_path.as_std_path().to_path_buf()),
                },
            ),
            Err(message) => {
                return Err(ParseL10nError::ParseFile {
                    path: path.to_path_buf(),
                    message,
                })
            }
        }
    }

    entries.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(LocalizationTable {
        table_name,
        module_kind: ModuleKind::Xcstrings,
        source_path,
        entries,
        warnings,
    })
}
```

Use additive placeholder metadata only when present:

```rust
fn parse_xcstrings_record(
    key: &str,
    record: &Value,
    source_path: &Utf8PathBuf,
) -> Result<Option<RawEntry>, String> {
    let localizations = record
        .get("localizations")
        .and_then(Value::as_object)
        .ok_or_else(|| format!("catalog key `{key}` is missing `localizations`"))?;
    let localization = localizations
        .values()
        .next()
        .ok_or_else(|| format!("catalog key `{key}` has no localizations"))?;

    if localization.get("variations").is_some() {
        return Ok(None);
    }

    let translation = localization
        .get("stringUnit")
        .and_then(|value| value.get("value"))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("catalog key `{key}` is missing `stringUnit.value`"))?;

    let mut properties = Metadata::from([
        ("key".to_string(), Value::String(key.to_string())),
        (
            "translation".to_string(),
            Value::String(translation.to_string()),
        ),
    ]);

    if let Some(substitutions) = localization.get("substitutions").and_then(Value::as_object) {
        let mut placeholders = Vec::new();
        for (name, substitution) in substitutions {
            let mut placeholder = serde_json::Map::new();
            placeholder.insert("name".to_string(), Value::String(name.clone()));
            if let Some(format) = substitution.get("formatSpecifier").and_then(Value::as_str) {
                placeholder.insert("format".to_string(), Value::String(format.to_string()));
            }
            if let Some(swift_type) = infer_swift_type(substitution) {
                placeholder.insert("swiftType".to_string(), Value::String(swift_type));
            }
            placeholders.push(Value::Object(placeholder));
        }
        placeholders.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        if !placeholders.is_empty() {
            properties.insert("placeholders".to_string(), Value::Array(placeholders));
        }
    }

    Ok(Some(RawEntry {
        path: key.to_string(),
        source_path: source_path.clone(),
        kind: EntryKind::StringKey,
        properties,
    }))
}
```

Implement the format-to-type helper in the same file:

```rust
fn infer_swift_type(substitution: &Value) -> Option<String> {
    match substitution.get("formatSpecifier").and_then(Value::as_str) {
        Some("ld") | Some("li") | Some("lld") => Some("Int".to_string()),
        Some("f") | Some("lf") => Some("Double".to_string()),
        Some("@") => Some("String".to_string()),
        _ => None,
    }
}
```

- [ ] **Step 4: Teach the pipeline about `xcstrings` input kind**

Update the imports at the top of `crates/numi-core/src/pipeline.rs`:

```rust
use crate::{
    context::{AssetTemplateContext, ContextError},
    output::{OutputError, WriteOutcome, output_is_stale, write_if_changed_atomic},
    parse_l10n::{LocalizationTable, ParseL10nError, parse_strings, parse_xcstrings},
    parse_xcassets::{ParseXcassetsError, parse_catalog},
    render::{RenderError, render_builtin, render_path},
};
```

Add the `xcstrings` input branch in `build_modules(...)`:

```rust
"xcstrings" => {
    let tables =
        parse_xcstrings(&input_path).map_err(|source| GenerateError::ParseStrings {
            job: job.name.clone(),
            source,
        })?;

    for table in tables {
        warnings.extend(
            table
                .warnings
                .into_iter()
                .map(|diagnostic| diagnostic.with_job(job.name.clone())),
        );

        let table_name = table.table_name.clone();
        if let Some(first_source) = duplicate_table_sources
            .insert(table_name.clone(), table.source_path.clone())
        {
            diagnostics.push(
                Diagnostic::error(format!(
                    "duplicate localization table `{table_name}` from localization inputs"
                ))
                .with_job(job.name.clone())
                .with_path(table.source_path.as_std_path())
                .with_hint(format!(
                    "found `{}` and `{}`; merge these inputs before generation or select a single source",
                    first_source,
                    table.source_path
                )),
            );
            continue;
        }

        let entries = normalize_scope(&job.name, table.entries)
            .map_err(GenerateError::Diagnostics)?;
        modules.push(ResourceModule {
            id: table_name.clone(),
            kind: table.module_kind.clone(),
            name: swift_identifier(&table_name),
            entries,
            metadata: Metadata::from([(
                "tableName".to_string(),
                Value::String(table_name),
            )]),
        });
    }
}
```

- [ ] **Step 5: Run the parser and core pipeline tests**

Run: `cargo test -p numi-core parse_l10n::tests::parses_xcstrings_plain_string_and_placeholders -v`

Expected: PASS

Run: `cargo test -p numi-core parse_l10n::tests::skips_xcstrings_plural_variations_with_warning -v`

Expected: PASS

Run: `cargo test -p numi-core -v`

Expected: PASS with existing strings and asset tests still green.

- [ ] **Step 6: Commit the `.xcstrings` parser and pipeline support**

```bash
git add crates/numi-core/src/parse_l10n.rs crates/numi-core/src/pipeline.rs
git commit -m "feat: add xcstrings parser support"
```

### Task 3: Propagate Warnings Through Reports And Print Them In The CLI

**Files:**
- Modify: `crates/numi-core/src/pipeline.rs`
- Modify: `crates/numi-core/src/lib.rs`
- Modify: `crates/numi-cli/src/lib.rs`

- [ ] **Step 1: Write the failing CLI warning test**

Add this test to `crates/numi-cli/tests/generate_l10n.rs`:

```rust
#[test]
fn generate_warns_and_succeeds_for_skipped_xcstrings_variations() {
    let temp_root = make_temp_dir("generate-xcstrings-warning");
    let fixture_root = repo_root().join("fixtures/xcstrings-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("warning: skipped unsupported `.xcstrings` variations"));
    assert!(stderr.contains("files.remaining"));

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
```

- [ ] **Step 2: Run the CLI warning test and verify it fails**

Run: `cargo test -p numi-cli --test generate_l10n generate_warns_and_succeeds_for_skipped_xcstrings_variations -v`

Expected: FAIL because warnings are not yet propagated through `numi_core::generate(...)` and nothing is printed to stderr.

- [ ] **Step 3: Add warnings to `GenerateReport` and `CheckReport`**

Update the report types in `crates/numi-core/src/pipeline.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerateReport {
    pub jobs: Vec<JobReport>,
    pub warnings: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckReport {
    pub stale_paths: Vec<Utf8PathBuf>,
    pub warnings: Vec<Diagnostic>,
}
```

Update `generate(...)` and `check(...)` to collect warnings across jobs:

```rust
let mut reports = Vec::with_capacity(jobs.len());
let mut warnings = Vec::new();

for job in jobs {
    let job_result = generate_job(&loaded.path, config_dir, &loaded.config.defaults, job)?;
    warnings.extend(job_result.warnings.iter().cloned());
    reports.push(JobReport {
        job_name: job_result.job_name,
        output_path: job_result.output_path,
        outcome: job_result.outcome,
    });
}

Ok(GenerateReport { jobs: reports, warnings })
```

Add an internal job-scoped helper struct in the same file:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
struct JobExecution {
    job_name: String,
    output_path: Utf8PathBuf,
    outcome: WriteOutcome,
    warnings: Vec<Diagnostic>,
}
```

Change `build_modules(...)` to return warnings alongside modules:

```rust
struct BuildModulesResult {
    modules: Vec<ResourceModule>,
    warnings: Vec<Diagnostic>,
}
```

and make `build_context(...)` return `(AssetTemplateContext, Vec<Diagnostic>)`.

- [ ] **Step 4: Print warnings in the CLI without changing exit success**

Update `crates/numi-cli/src/lib.rs`:

```rust
fn print_warnings(warnings: &[numi_diagnostics::Diagnostic]) {
    for warning in warnings {
        eprintln!("{warning}");
    }
}
```

Change `run_generate(...)`:

```rust
fn run_generate(args: &GenerateArgs) -> Result<(), CliError> {
    let config_path = discover_config_path(args.config.as_deref())?;
    let selected_jobs = selected_jobs(&args.jobs);
    let report = numi_core::generate(&config_path, selected_jobs)
        .map_err(|error| CliError::new(error.to_string()))?;
    print_warnings(&report.warnings);
    Ok(())
}
```

Change `run_check(...)`:

```rust
let report = numi_core::check(&config_path, selected_jobs)
    .map_err(|error| CliError::new(error.to_string()))?;
print_warnings(&report.warnings);

if report.stale_paths.is_empty() {
    Ok(())
} else {
    let lines = report
        .stale_paths
        .iter()
        .map(display_path)
        .collect::<Vec<_>>()
        .join("\n");
    Err(CliError::with_exit_code(
        format!("stale generated outputs:\n{lines}"),
        2,
    ))
}
```

- [ ] **Step 5: Run focused warning-path verification**

Run: `cargo test -p numi-cli --test generate_l10n generate_warns_and_succeeds_for_skipped_xcstrings_variations -v`

Expected: PASS

Run: `cargo test -p numi-cli --test config_commands check_returns_exit_code_2_for_stale_output_without_rewriting_file -v`

Expected: PASS, proving the check exit contract still works after the report shape change.

- [ ] **Step 6: Commit warning propagation and CLI printing**

```bash
git add crates/numi-core/src/pipeline.rs crates/numi-core/src/lib.rs crates/numi-cli/src/lib.rs crates/numi-cli/tests/generate_l10n.rs
git commit -m "feat: print xcstrings skip warnings"
```

### Task 4: Add Fixture-Backed `.xcstrings` Generation, Context, And Docs Coverage

**Files:**
- Create: `fixtures/xcstrings-basic/swiftgen.toml`
- Create: `fixtures/xcstrings-basic/Resources/Localization/Localizable.xcstrings`
- Modify: `crates/numi-cli/tests/generate_l10n.rs`
- Modify: `crates/numi-core/src/context.rs`
- Modify: `docs/context-schema.md`
- Modify: `docs/migration-from-swiftgen.md`

- [ ] **Step 1: Create the `.xcstrings` fixture and config**

Create `fixtures/xcstrings-basic/swiftgen.toml`:

```toml
version = 1

[[jobs]]
name = "l10n"
output = "Generated/L10n.swift"

[[jobs.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.template]
builtin = "l10n"
```

Create `fixtures/xcstrings-basic/Resources/Localization/Localizable.xcstrings`:

```json
{
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
    "user.greeting": {
      "localizations": {
        "en": {
          "stringUnit": {
            "state": "translated",
            "value": "Hello %@"
          },
          "substitutions": {
            "username": {
              "argNum": 1,
              "formatSpecifier": "@"
            }
          }
        }
      }
    },
    "files.remaining": {
      "localizations": {
        "en": {
          "variations": {
            "plural": {
              "one": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%d file remaining"
                }
              },
              "other": {
                "stringUnit": {
                  "state": "translated",
                  "value": "%d files remaining"
                }
              }
            }
          }
        }
      }
    }
  }
}
```

- [ ] **Step 2: Add fixture-backed generate and dump-context tests**

Add these tests to `crates/numi-cli/tests/generate_l10n.rs`:

```rust
#[test]
fn generate_writes_l10n_accessors_from_xcstrings() {
    let temp_root = make_temp_dir("generate-xcstrings");
    let fixture_root = repo_root().join("fixtures/xcstrings-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(working_root.join("Generated/L10n.swift"))
        .expect("generated l10n file should exist");

    assert_eq!(
        generated,
        r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let profileTitle = tr("Localizable", "profile.title")
        internal static let userGreeting = tr("Localizable", "user.greeting")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_emits_xcstrings_module_kind_and_placeholders() {
    let fixture_root = repo_root().join("fixtures/xcstrings-basic");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--config", "swiftgen.toml", "--job", "l10n"])
        .current_dir(&fixture_root)
        .output()
        .expect("numi dump-context should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be json");

    assert_eq!(json["modules"][0]["kind"], "xcstrings");
    assert_eq!(json["modules"][0]["properties"]["tableName"], "Localizable");
    assert_eq!(
        json["modules"][0]["entries"][1]["properties"]["placeholders"][0]["name"],
        "username"
    );
    assert_eq!(
        json["modules"][0]["entries"][1]["properties"]["placeholders"][0]["format"],
        "@"
    );
    assert_eq!(
        json["modules"][0]["entries"][1]["properties"]["placeholders"][0]["swiftType"],
        "String"
    );
    assert!(
        json["modules"][0]["entries"][0]["properties"]
            .get("placeholders")
            .is_none()
    );
}
```

- [ ] **Step 3: Add context coverage for `.xcstrings` serialization**

Add this unit test to `crates/numi-core/src/context.rs`:

```rust
#[test]
fn builds_stable_template_surface_for_xcstrings_localization() {
    let module = ResourceModule {
        id: "Localizable".to_string(),
        kind: ModuleKind::Xcstrings,
        name: "Localizable".to_string(),
        entries: vec![ResourceEntry {
            id: "user.greeting".to_string(),
            name: "user.greeting".to_string(),
            source_path: Utf8PathBuf::from("fixture"),
            swift_identifier: "UserGreeting".to_string(),
            kind: EntryKind::StringKey,
            children: Vec::new(),
            properties: Metadata::from([
                ("key".to_string(), json!("user.greeting")),
                ("translation".to_string(), json!("Hello %@")),
                (
                    "placeholders".to_string(),
                    json!([
                        {
                            "name": "username",
                            "format": "@",
                            "swiftType": "String"
                        }
                    ]),
                ),
            ]),
            metadata: Metadata::new(),
        }],
        metadata: Metadata::from([("tableName".to_string(), json!("Localizable"))]),
    };

    let context = AssetTemplateContext::new(
        "l10n",
        "Generated/L10n.swift",
        "internal",
        "module",
        None,
        &[module],
    )
    .expect("context should build");
    let serialized = serde_json::to_value(&context).expect("context should serialize");

    assert_eq!(serialized["modules"][0]["kind"], "xcstrings");
    assert_eq!(
        serialized["modules"][0]["entries"][0]["properties"]["placeholders"][0]["name"],
        "username"
    );
}
```

- [ ] **Step 4: Update the stable docs**

Update `docs/context-schema.md`:

```md
Current v1 module kinds:

- `xcassets`
- `strings`
- `xcstrings`

Current stable entry property keys:

- `assetName` for asset entries
- `key` for localization string entries
- `translation` for localization string entries
- `placeholders` for localization entries when placeholder metadata exists
```

Replace the deferred-language in `docs/migration-from-swiftgen.md` with:

```md
- `.strings` and `.xcstrings` localization inputs are supported in v1
- plural and device-specific `.xcstrings` variations are skipped with warnings in the current release
```

- [ ] **Step 5: Run the full verification suite**

Run: `cargo test -p numi-core context::tests::builds_stable_template_surface_for_xcstrings_localization -v`

Expected: PASS

Run: `cargo test -p numi-cli --test generate_l10n generate_writes_l10n_accessors_from_xcstrings -v`

Expected: PASS

Run: `cargo test -p numi-cli --test generate_l10n dump_context_emits_xcstrings_module_kind_and_placeholders -v`

Expected: PASS

Run: `cargo test -v`

Expected: PASS

Run: `cargo fmt --check`

Expected: PASS

- [ ] **Step 6: Commit fixtures, docs, and end-to-end coverage**

```bash
git add fixtures/xcstrings-basic crates/numi-cli/tests/generate_l10n.rs crates/numi-core/src/context.rs docs/context-schema.md docs/migration-from-swiftgen.md
git commit -m "feat: document and test xcstrings support"
```

## Self-Review

- Spec coverage: the parser, `xcstrings` module kind preservation, additive placeholder metadata, warning emission, fixture coverage, CLI behavior, and docs updates each map to an explicit task above.
- Placeholder scan: no `TODO`, `TBD`, or deferred implementation language remains in the task steps.
- Type consistency: the plan consistently uses `LocalizationTable`, `ModuleKind::Xcstrings`, `entry.properties.placeholders`, and `warnings` report fields across parser, pipeline, CLI, and docs tasks.
