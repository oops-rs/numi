#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{
    fs,
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn make_temp_dir(test_name: &str) -> PathBuf {
    let unique = format!(
        "numi-{test_name}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root should exist")
}

fn copy_dir_all(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).expect("destination directory should exist");

    for entry in fs::read_dir(source).expect("source directory should be readable") {
        let entry = entry.expect("directory entry should be readable");
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().expect("file type should be readable");

        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination_path);
        } else {
            fs::copy(entry.path(), destination_path).expect("fixture file should copy");
        }
    }
}

fn write_manifest(root: &Path, contents: &str) {
    fs::write(root.join("numi.toml"), contents).expect("manifest should exist");
}

fn toml_array(values: &[String]) -> String {
    let parts = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>();
    format!("[{}]", parts.join(", "))
}

fn write_hook_probe_script(root: &Path, name: &str, exit_code: i32) -> String {
    let scripts_root = root.join("Scripts");
    fs::create_dir_all(&scripts_root).expect("scripts dir should exist");
    let file_name = if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        format!("{name}.sh")
    };
    let script_path = scripts_root.join(&file_name);
    let script_body = if cfg!(windows) {
        format!(
            "@echo off\r\nsetlocal\r\n>> \"%~1\" echo %NUMI_HOOK_PHASE%^|%NUMI_HOOK_JOB_NAME%^|%NUMI_HOOK_WORKSPACE_CONFIG_PATH%\r\nexit /b {exit_code}\r\n"
        )
    } else {
        format!(
            "#!/bin/sh\nlog_path=\"$1\"\nprintf '%s|%s|%s\\n' \"$NUMI_HOOK_PHASE\" \"$NUMI_HOOK_JOB_NAME\" \"${{NUMI_HOOK_WORKSPACE_CONFIG_PATH-}}\" >> \"$log_path\"\nexit {exit_code}\n"
        )
    };
    fs::write(&script_path, script_body).expect("hook script should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("script should be executable");
    }

    PathBuf::from("Scripts")
        .join(file_name)
        .display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn starter_config_lock_path() -> PathBuf {
    std::env::temp_dir().join("numi-cli-starter-config.lock")
}

struct StarterConfigLock {
    path: PathBuf,
}

impl Drop for StarterConfigLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn acquire_starter_config_lock() -> StarterConfigLock {
    let path = starter_config_lock_path();

    loop {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return StarterConfigLock { path },
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("failed to acquire starter config lock: {error}"),
        }
    }
}

struct FileRestore {
    path: PathBuf,
    contents: String,
}

impl Drop for FileRestore {
    fn drop(&mut self) {
        fs::write(&self.path, &self.contents).expect("starter config docs file should be restored");
    }
}

#[test]
fn config_locate_finds_nearest_ancestor() {
    let root = make_temp_dir("nearest-ancestor");
    let nested = root.join("Sources/App");
    fs::create_dir_all(&nested).expect("nested dir should exist");
    let config_path = root.join("numi.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "locate"])
        .current_dir(&nested)
        .output()
        .expect("numi config locate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let expected_path = config_path
        .canonicalize()
        .expect("config path should canonicalize");
    assert_eq!(stdout.trim(), expected_path.to_string_lossy());

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_locate_prefers_explicit_path_over_ancestor() {
    let root = make_temp_dir("explicit-config");
    let nested = root.join("Sources/App");
    let explicit_dir = root.join("Configs");
    fs::create_dir_all(&nested).expect("nested dir should exist");
    fs::create_dir_all(&explicit_dir).expect("explicit dir should exist");

    let ancestor_config = root.join("numi.toml");
    fs::write(
        &ancestor_config,
        r#"
version = 1

[jobs.ancestor]
output = "Generated/Ancestor.swift"

[[jobs.ancestor.inputs]]
type = "xcassets"
path = "Resources/Ancestor.xcassets"

[jobs.ancestor.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("ancestor config should be written");

    let explicit_config = explicit_dir.join("custom.toml");
    fs::write(
        &explicit_config,
        r#"
version = 1

[jobs.explicit]
output = "Generated/Explicit.swift"

[[jobs.explicit.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.explicit.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("explicit config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "config",
            "locate",
            "--config",
            explicit_config
                .strip_prefix(&nested)
                .unwrap_or(&explicit_config)
                .to_string_lossy()
                .as_ref(),
        ])
        .current_dir(&nested)
        .output()
        .expect("numi config locate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    let expected_path = explicit_config
        .canonicalize()
        .expect("explicit config should canonicalize");
    assert_eq!(stdout.trim(), expected_path.to_string_lossy());

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_locate_does_not_scan_descendants() {
    let root = make_temp_dir("descendants-not-searched");
    let search_dir = root.join("Repo");
    let config_dir = search_dir.join("AppUI");
    fs::create_dir_all(&config_dir).expect("config dir should exist");
    fs::write(config_dir.join("numi.toml"), "version = 1\njobs = []\n")
        .expect("descendant config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "locate"])
        .current_dir(&search_dir)
        .output()
        .expect("numi config locate should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("No configuration file found from"));

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn check_returns_exit_code_2_for_stale_output_without_rewriting_file() {
    let temp_root = make_temp_dir("check-stale");
    let fixture_root = repo_root().join("fixtures/xcassets-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let generated_path = working_root.join("Generated/Assets.swift");
    fs::create_dir_all(
        generated_path
            .parent()
            .expect("generated file should have parent"),
    )
    .expect("generated directory should exist");
    fs::write(&generated_path, "// stale output\n").expect("stale output should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check", "--config", "numi.toml"])
        .current_dir(&working_root)
        .output()
        .expect("numi check should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected status: {output:?}"
    );
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("Generated/Assets.swift"),
        "stderr was: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should still exist"),
        "// stale output\n"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn check_warns_and_returns_exit_code_2_for_stale_xcstrings_output() {
    let temp_root = make_temp_dir("check-xcstrings-warning-stale");
    let working_root = temp_root.join("fixture");
    let localization_root = working_root.join("Resources/Localization");
    fs::create_dir_all(&localization_root).expect("localization directory should exist");
    fs::write(
        working_root.join("numi.toml"),
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "xcstrings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
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

    let generated_path = working_root.join("Generated/L10n.swift");
    fs::create_dir_all(
        generated_path
            .parent()
            .expect("generated file should have parent"),
    )
    .expect("generated directory should exist");
    fs::write(&generated_path, "// stale output\n").expect("stale output should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check", "--config", "numi.toml", "--job", "l10n"])
        .current_dir(&working_root)
        .output()
        .expect("numi check should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected status: {output:?}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("warning: skipping xcstrings key `Lv.%lld`"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("does not contain a supported string unit"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("stale generated outputs:"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("Generated/L10n.swift"),
        "stderr was: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should still exist"),
        "// stale output\n"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn check_returns_exit_code_2_for_stale_files_output_without_rewriting_file() {
    let temp_root = make_temp_dir("check-files-stale");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let generated_path = working_root.join("Generated/Files.swift");
    fs::create_dir_all(
        generated_path
            .parent()
            .expect("generated file should have parent"),
    )
    .expect("generated directory should exist");
    fs::write(&generated_path, "// stale files output\n")
        .expect("stale files output should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check", "--config", "numi.toml", "--job", "files"])
        .current_dir(&working_root)
        .output()
        .expect("numi check should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected status: {output:?}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("Generated/Files.swift"),
        "stderr was: {stderr}"
    );
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should still exist"),
        "// stale files output\n"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn init_refuses_to_overwrite_existing_config_without_force() {
    let root = make_temp_dir("init-refuse-overwrite");
    let existing = root.join("numi.toml");
    fs::write(&existing, "version = 1\njobs = []\n").expect("existing config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .arg("init")
        .current_dir(&root)
        .output()
        .expect("numi init should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("--force"), "stderr was: {stderr}");
    assert_eq!(
        fs::read_to_string(&existing).expect("existing config should still exist"),
        "version = 1\njobs = []\n"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn init_creates_starter_numi_toml() {
    let _lock = acquire_starter_config_lock();
    let root = make_temp_dir("init-success");
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let docs_starter_path = manifest_dir.join("../../docs/examples/starter-numi.toml");
    let original_docs =
        fs::read_to_string(&docs_starter_path).expect("docs starter config should be readable");
    let _restore_docs = FileRestore {
        path: docs_starter_path.clone(),
        contents: original_docs,
    };
    let sentinel = "# sentinel content used to prove numi init reads the crate-local asset\n";
    fs::write(&docs_starter_path, sentinel).expect("docs starter config should be overridden");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .arg("init")
        .current_dir(&root)
        .output()
        .expect("numi init should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let created = fs::read_to_string(root.join("numi.toml")).expect("starter config should exist");
    let expected = fs::read_to_string(manifest_dir.join("assets/starter-numi.toml"))
        .expect("crate-local starter config should be readable");
    assert_eq!(
        created, expected,
        "starter config should be sourced from the crate-local asset"
    );
    assert!(
        created.contains("[jobs.l10n.template.builtin]"),
        "starter config was: {created}"
    );
    assert!(
        created.contains("language = \"swift\""),
        "starter config was: {created}"
    );
    assert!(
        created.contains("name = \"l10n\""),
        "starter config was: {created}"
    );
    assert!(
        !created.contains("path = \"Templates/l10n.stencil\""),
        "starter config was: {created}"
    );
    assert!(
        !created.contains("sentinel content used to prove numi init reads the crate-local asset"),
        "starter config unexpectedly came from the docs copy: {created}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_validation_hints_reference_numi_toml() {
    let root = make_temp_dir("config-print-validation-hints");
    let config_path = root.join("numi.toml");
    fs::write(&config_path, "version = 2\n[jobs]\n").expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "numi.toml"])
        .current_dir(&root)
        .output()
        .expect("numi config print should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("set `version = 1` in numi.toml"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("add one `[jobs.<name>]` table to numi.toml"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_emits_objc_builtin_language_and_name() {
    let root = make_temp_dir("config-print-objc-builtin");
    let config_path = root.join("numi.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "objc"
name = "assets"
"#,
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "numi.toml"])
        .current_dir(&root)
        .output()
        .expect("numi config print should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(
        stdout.contains("[jobs.assets.template.builtin]"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("language = \"objc\""),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("name = \"assets\""), "stdout was: {stdout}");

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_rejects_workspace_manifests() {
    let root = make_temp_dir("config-print-workspace-manifest");
    fs::create_dir_all(root.join("apps/assets")).expect("workspace member dir should exist");
    fs::write(
        root.join("numi.toml"),
        r#"
version = 1

[workspace]
members = ["apps/assets"]
"#,
    )
    .expect("workspace manifest should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "numi.toml"])
        .current_dir(&root)
        .output()
        .expect("numi config print should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert_eq!(
        stderr.trim(),
        "`config print` only supports single-config manifests; run it from a member directory or pass `--config <member>/numi.toml`"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn generate_missing_job_hint_references_numi_toml() {
    let root = make_temp_dir("generate-missing-job-hint");
    let config_path = root.join("numi.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--config", "numi.toml", "--job", "missing"])
        .current_dir(&root)
        .output()
        .expect("numi generate should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("select one of the job names declared in numi.toml"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_emits_the_resolved_config_with_effective_defaults() {
    let root = make_temp_dir("config-print-defaults");
    let config_path = root.join("numi.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "numi.toml"])
        .current_dir(&root)
        .output()
        .expect("numi config print should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("version = 1"), "stdout was: {stdout}");
    assert!(
        stdout.contains("access_level = \"internal\""),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("mode = \"module\""), "stdout was: {stdout}");
    assert!(stdout.contains("[jobs.l10n]"), "stdout was: {stdout}");
    assert!(
        stdout.contains("[jobs.l10n.template.builtin]"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("language = \"swift\""),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("name = \"l10n\""), "stdout was: {stdout}");

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_emits_files_builtin_and_input_kind() {
    let temp_root = make_temp_dir("config-print-files");
    let fixture_root = repo_root().join("fixtures/files-basic");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "numi.toml"])
        .current_dir(&working_root)
        .output()
        .expect("numi config print should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be utf8");
    assert!(stdout.contains("[jobs.files]"), "stdout was: {stdout}");
    assert!(stdout.contains("type = \"files\""), "stdout was: {stdout}");
    assert!(
        stdout.contains("[jobs.files.template.builtin]"),
        "stdout was: {stdout}"
    );
    assert!(
        stdout.contains("language = \"swift\""),
        "stdout was: {stdout}"
    );
    assert!(stdout.contains("name = \"files\""), "stdout was: {stdout}");
    assert!(stdout.contains("mode = \"module\""), "stdout was: {stdout}");

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_from_member_directory_auto_prefers_workspace_manifest() {
    let temp_root = make_temp_dir("generate-member-manifest");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .arg("generate")
        .current_dir(&assets_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "member output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "workspace sibling output should be generated when auto-preferring the ancestor workspace"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_from_workspace_root_uses_nearest_workspace_manifest() {
    let temp_root = make_temp_dir("generate-workspace-root-manifest");
    let fixture_root = repo_root().join("fixtures/multimodule-repo");
    let working_root = temp_root.join("fixture");
    copy_dir_all(&fixture_root, &working_root);

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .arg("generate")
        .current_dir(&working_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        working_root
            .join("apps/assets/Generated/Assets.swift")
            .exists(),
        "workspace assets output was not generated"
    );
    assert!(
        working_root
            .join("packages/files/Generated/Files.swift")
            .exists(),
        "workspace files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_from_member_directory_uses_ancestor_workspace_manifest() {
    let temp_root = make_temp_dir("generate-workspace-ancestor");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );
    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");
    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "workspace assets output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "workspace files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_skips_malformed_non_workspace_ancestors() {
    let temp_root = make_temp_dir("generate-workspace-skips-malformed-ancestors");
    let workspace_root = temp_root.join("workspace");
    let app_parent = workspace_root.join("apps");
    let assets_root = app_parent.join("assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );
    fs::write(app_parent.join("numi.toml"), "version = [\n")
        .expect("malformed non-workspace manifest should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "workspace assets output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "workspace files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_reports_nearer_mixed_manifest() {
    let temp_root = make_temp_dir("generate-workspace-reports-nearer-mixed-manifest");
    let workspace_root = temp_root.join("workspace");
    let app_parent = workspace_root.join("apps");
    let assets_root = app_parent.join("assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );
    fs::write(
        app_parent.join("numi.toml"),
        "version = 1\njobs = {}\nmembers = [\n",
    )
    .expect("malformed mixed manifest should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("manifest must not define both `jobs` and `workspace`"),
        "stderr was: {stderr}"
    );
    assert!(
        !assets_root.join("Generated/Assets.swift").exists(),
        "member output should not be generated when nearer mixed manifest is invalid"
    );
    assert!(
        !files_root.join("Generated/Files.swift").exists(),
        "higher workspace output should not be generated when nearer mixed manifest is invalid"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_reports_nearer_broken_workspace_manifest() {
    let temp_root = make_temp_dir("generate-workspace-broken-nearer-workspace-manifest");
    let workspace_root = temp_root.join("workspace");
    let app_parent = workspace_root.join("apps");
    let assets_root = app_parent.join("assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );
    fs::write(
        app_parent.join("numi.toml"),
        "version = 1\n[workspace]\nmembers = [\n",
    )
    .expect("broken nearer workspace manifest should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("failed to parse"), "stderr was: {stderr}");
    assert!(
        !assets_root.join("Generated/Assets.swift").exists(),
        "member output should not be generated when nearer workspace manifest is broken"
    );
    assert!(
        !files_root.join("Generated/Files.swift").exists(),
        "higher workspace output should not be generated when nearer workspace manifest is broken"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_detects_inline_table_workspace_manifests() {
    let temp_root = make_temp_dir("generate-workspace-inline-table-manifest");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1
workspace={members=["apps/assets","packages/files"]}
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "workspace assets output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "workspace files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_detects_legacy_top_level_members_assignment() {
    let temp_root = make_temp_dir("generate-workspace-legacy-members-assignment");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1
members = [{ config = "apps/assets/numi.toml" }, { config = "packages/files/numi.toml" }]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "workspace assets output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "workspace files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_rejects_explicit_member_manifest() {
    let temp_root = make_temp_dir("generate-workspace-explicit-member-manifest");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "generate",
            "--workspace",
            "--config",
            "apps/assets/numi.toml",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("numi generate --workspace --config should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("expected a workspace manifest"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("apps/assets/numi.toml"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("pass --config <workspace>/numi.toml or remove --workspace"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_reports_workspace_specific_guidance_when_missing() {
    let root = make_temp_dir("generate-workspace-missing-guidance");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("No workspace manifest found from"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("pass --config <workspace>/numi.toml"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("numi config locate --config <path>"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("numi-workspace.toml"),
        "stderr was: {stderr}"
    );
    assert!(!stderr.contains("numi workspace"), "stderr was: {stderr}");

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn check_workspace_aggregates_stale_paths_across_members() {
    let temp_root = make_temp_dir("check-workspace-stale");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );

    let generate_output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");
    assert!(
        generate_output.status.success(),
        "setup generate failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&generate_output.stdout),
        String::from_utf8_lossy(&generate_output.stderr)
    );

    fs::write(
        assets_root.join("Generated/Assets.swift"),
        "// stale assets output\n",
    )
    .expect("stale assets output should be written");
    fs::write(
        files_root.join("Generated/Files.swift"),
        "// stale files output\n",
    )
    .expect("stale files output should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi check --workspace should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected status: {output:?}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("apps/assets/Generated/Assets.swift"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("packages/files/Generated/Files.swift"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_cli_jobs_narrow_member_overrides() {
    let temp_root = make_temp_dir("generate-workspace-cli-jobs-narrow-member-overrides");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/mixed");

    fs::create_dir_all(member_root.join("Resources")).expect("member resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/xcassets-basic/Resources/Assets.xcassets"),
        &member_root.join("Resources/Assets.xcassets"),
    );
    copy_dir_all(
        &repo_root().join("fixtures/l10n-basic/Resources/Localization"),
        &member_root.join("Resources/Localization"),
    );
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/mixed"]

[workspace.member_overrides."apps/mixed"]
jobs = ["assets"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "generate",
            "--workspace",
            "--job",
            "assets",
            "--job",
            "l10n",
        ])
        .current_dir(&member_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        member_root.join("Generated/Assets.swift").exists(),
        "allowed member job should be generated"
    );
    assert!(
        !member_root.join("Generated/L10n.swift").exists(),
        "cli-selected jobs should be narrowed by the member override"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_inherits_templates_before_applying_member_job_overrides() {
    let temp_root = make_temp_dir("generate-workspace-inherits-templates");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/mixed");

    fs::create_dir_all(member_root.join("Resources")).expect("member resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/xcassets-basic/Resources/Assets.xcassets"),
        &member_root.join("Resources/Assets.xcassets"),
    );
    copy_dir_all(
        &repo_root().join("fixtures/l10n-basic/Resources/Localization"),
        &member_root.join("Resources/Localization"),
    );
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template.builtin]
name = "l10n"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/mixed"]

[workspace.defaults.jobs.l10n.template.builtin]
language = "swift"

[workspace.defaults.jobs.assets.template.builtin]
language = "swift"

[workspace.member_overrides."apps/mixed"]
jobs = ["l10n"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&member_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        member_root.join("Generated/L10n.swift").exists(),
        "workspace default template should enable l10n generation"
    );
    assert!(
        !member_root.join("Generated/Assets.swift").exists(),
        "member override jobs should still narrow workspace execution"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_uses_root_relative_custom_template_defaults() {
    let temp_root = make_temp_dir("generate-workspace-root-relative-template-default");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/l10n");
    let templates_root = workspace_root.join("Templates");

    fs::create_dir_all(member_root.join("Resources/Localization"))
        .expect("member localization dir should exist");
    fs::create_dir_all(&templates_root).expect("workspace templates dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/l10n-basic/Resources/Localization"),
        &member_root.join("Resources/Localization"),
    );
    fs::write(
        templates_root.join("l10n.jinja"),
        "ROOT|{{ job.name }}|{{ modules[0].name }}\n",
    )
    .expect("workspace template should be written");
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/l10n"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&member_root)
        .output()
        .expect("numi generate --workspace should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(member_root.join("Generated/L10n.swift"))
        .expect("generated workspace l10n file should exist");
    assert_eq!(generated, "ROOT|l10n|Localizable\n");

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_prefers_workspace_manifest_from_member_directory() {
    let temp_root = make_temp_dir("generate-prefers-workspace-manifest");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    fs::create_dir_all(assets_root.join("Resources")).expect("assets resources dir should exist");
    fs::create_dir_all(files_root.join("Resources")).expect("files resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/xcassets-basic/Resources/Assets.xcassets"),
        &assets_root.join("Resources/Assets.xcassets"),
    );
    copy_dir_all(
        &repo_root().join("fixtures/files-basic/Resources/Fixtures"),
        &files_root.join("Resources/Fixtures"),
    );
    write_manifest(
        &assets_root,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    );
    write_manifest(
        &files_root,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "current member output should be generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "other workspace member output should be generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_prefers_workspace_manifest_before_validating_member_config() {
    let temp_root = make_temp_dir("generate-prefers-workspace-before-validating-member");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/l10n");
    let templates_root = workspace_root.join("Templates");

    fs::create_dir_all(member_root.join("Resources/Localization"))
        .expect("member localization dir should exist");
    fs::create_dir_all(&templates_root).expect("workspace templates dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/l10n-basic/Resources/Localization"),
        &member_root.join("Resources/Localization"),
    );
    fs::write(
        templates_root.join("l10n.jinja"),
        "AUTO|{{ job.name }}|{{ modules[0].name }}\n",
    )
    .expect("workspace template should be written");
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/l10n"]

[workspace.defaults.jobs.l10n.template]
path = "Templates/l10n"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate"])
        .current_dir(&member_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let generated = fs::read_to_string(member_root.join("Generated/L10n.swift"))
        .expect("generated l10n file should exist");
    assert_eq!(generated, "AUTO|l10n|Localizable\n");

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_runs_inherited_hooks_and_passes_workspace_manifest_path() {
    let temp_root = make_temp_dir("generate-workspace-inherited-hooks");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/files");
    let hook_log = workspace_root.join("hook.log");
    let hook_script = write_hook_probe_script(&workspace_root, "workspace-post-hook", 0);

    fs::create_dir_all(member_root.join("Resources")).expect("member resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/files-basic/Resources/Fixtures"),
        &member_root.join("Resources/Fixtures"),
    );
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"
incremental = false

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    );
    write_manifest(
        &workspace_root,
        &format!(
            r#"
version = 1

[workspace]
members = ["apps/files"]

[workspace.defaults.jobs.files.hooks.post_generate]
command = {}
"#,
            toml_array(&[hook_script, hook_log.display().to_string()])
        ),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate"])
        .current_dir(&member_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let log = fs::read_to_string(&hook_log).expect("hook log should exist");
    let line = log.lines().next().expect("hook line should exist");
    let workspace_manifest = workspace_root
        .join("numi.toml")
        .canonicalize()
        .expect("workspace manifest should canonicalize");
    assert_eq!(
        line,
        format!("post_generate|files|{}", workspace_manifest.display())
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn generate_workspace_runs_shared_hooks_and_passes_workspace_manifest_path() {
    let temp_root = make_temp_dir("generate-workspace-shared-hooks");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/files");
    let hook_log = workspace_root.join("hook.log");
    let hook_script = write_hook_probe_script(&workspace_root, "workspace-post-hook", 0);

    fs::create_dir_all(member_root.join("Resources")).expect("member resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/files-basic/Resources/Fixtures"),
        &member_root.join("Resources/Fixtures"),
    );
    write_manifest(
        &member_root,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"
incremental = false

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    );
    write_manifest(
        &workspace_root,
        &format!(
            r#"
version = 1

[workspace]
members = ["apps/files"]

[workspace.defaults.hooks.post_generate]
command = {}
"#,
            toml_array(&[hook_script, hook_log.display().to_string()])
        ),
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate"])
        .current_dir(&member_root)
        .output()
        .expect("numi generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let log = fs::read_to_string(&hook_log).expect("hook log should exist");
    let line = log.lines().next().expect("hook line should exist");
    let workspace_manifest = workspace_root
        .join("numi.toml")
        .canonicalize()
        .expect("workspace manifest should canonicalize");
    assert_eq!(
        line,
        format!("post_generate|files|{}", workspace_manifest.display())
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn check_prefers_workspace_manifest_from_member_directory() {
    let temp_root = make_temp_dir("check-prefers-workspace-manifest");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    fs::create_dir_all(assets_root.join("Resources")).expect("assets resources dir should exist");
    fs::create_dir_all(files_root.join("Resources")).expect("files resources dir should exist");
    copy_dir_all(
        &repo_root().join("fixtures/xcassets-basic/Resources/Assets.xcassets"),
        &assets_root.join("Resources/Assets.xcassets"),
    );
    copy_dir_all(
        &repo_root().join("fixtures/files-basic/Resources/Fixtures"),
        &files_root.join("Resources/Fixtures"),
    );
    write_manifest(
        &assets_root,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    );
    write_manifest(
        &files_root,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    );
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets", "packages/files"]
"#,
    );

    let setup_output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["generate", "--workspace"])
        .current_dir(&assets_root)
        .output()
        .expect("numi generate --workspace should run");
    assert!(
        setup_output.status.success(),
        "setup generate failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&setup_output.stdout),
        String::from_utf8_lossy(&setup_output.stderr)
    );

    fs::write(
        files_root.join("Generated/Files.swift"),
        "// stale files output\n",
    )
    .expect("stale files output should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["check"])
        .current_dir(&assets_root)
        .output()
        .expect("numi check should run");

    assert_eq!(
        output.status.code(),
        Some(2),
        "unexpected status: {output:?}"
    );

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("packages/files/Generated/Files.swift"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn dump_context_rejects_workspace_manifests() {
    let temp_root = make_temp_dir("dump-context-workspace-manifest");
    let workspace_root = temp_root.join("workspace");
    fs::create_dir_all(workspace_root.join("apps/assets"))
        .expect("workspace member dir should exist");
    write_manifest(
        &workspace_root,
        r#"
version = 1

[workspace]
members = ["apps/assets"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["dump-context", "--job", "assets"])
        .current_dir(&workspace_root)
        .output()
        .expect("numi dump-context should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert_eq!(
        stderr.trim(),
        "`dump-context` only supports single-config manifests; run it from a member directory or pass `--config <member>/numi.toml`"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
