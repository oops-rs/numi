use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
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

fn write_workspace_manifest(root: &Path, contents: &str) {
    fs::write(root.join("numi-workspace.toml"), contents).expect("workspace manifest should exist");
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
swift = "swiftui-assets"
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
fn config_locate_reports_ambiguous_descendant_configs() {
    let root = make_temp_dir("ambiguous-descendants");
    let app_ui = root.join("AppUI");
    let core = root.join("Core");
    fs::create_dir_all(&app_ui).expect("AppUI dir should exist");
    fs::create_dir_all(&core).expect("Core dir should exist");
    fs::write(app_ui.join("numi.toml"), "version = 1\njobs = []\n")
        .expect("AppUI config should be written");
    fs::write(core.join("numi.toml"), "version = 1\njobs = []\n")
        .expect("Core config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "locate"])
        .current_dir(&root)
        .output()
        .expect("numi config locate should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("Multiple configuration files found"));
    assert!(stderr.contains("AppUI/numi.toml"));
    assert!(stderr.contains("Core/numi.toml"));

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
swift = "swiftui-assets"
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
swift = "l10n"
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
fn config_locate_finds_single_descendant_when_no_ancestor_exists() {
    let root = make_temp_dir("single-descendant");
    let search_dir = root.join("Repo");
    let config_dir = search_dir.join("AppUI");
    fs::create_dir_all(&config_dir).expect("config dir should exist");

    let config_path = config_dir.join("numi.toml");
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
swift = "swiftui-assets"
"#,
    )
    .expect("descendant config should be written");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "locate"])
        .current_dir(&search_dir)
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
        .expect("descendant config should canonicalize");
    assert_eq!(stdout.trim(), expected_path.to_string_lossy());

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
swift = "l10n"
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
    let root = make_temp_dir("init-success");

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
    assert_eq!(
        created,
        include_str!("../../../docs/examples/starter-numi.toml")
    );
    assert!(
        created.contains("[jobs.l10n.template.builtin]"),
        "starter config was: {created}"
    );
    assert!(
        created.contains("swift = \"l10n\""),
        "starter config was: {created}"
    );
    assert!(
        !created.contains("path = \"Templates/l10n.stencil\""),
        "starter config was: {created}"
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
swift = "swiftui-assets"
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
swift = "l10n"
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
    assert!(stdout.contains("swift = \"l10n\""), "stdout was: {stdout}");

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
    assert!(stdout.contains("swift = \"files\""), "stdout was: {stdout}");
    assert!(stdout.contains("mode = \"module\""), "stdout was: {stdout}");

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn workspace_generate_runs_multiple_member_configs() {
    let temp_root = make_temp_dir("workspace-generate-multiple");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_workspace_manifest(
        &workspace_root,
        r#"
version = 1

[[members]]
config = "apps/assets/numi.toml"

[[members]]
config = "packages/files/numi.toml"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "workspace",
            "generate",
            "--workspace",
            "numi-workspace.toml",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("numi workspace generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        assets_root.join("Generated/Assets.swift").exists(),
        "assets output was not generated"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "files output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn workspace_generate_reports_missing_manifest_with_real_guidance() {
    let root = make_temp_dir("workspace-generate-missing-manifest");

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["workspace", "generate"])
        .current_dir(&root)
        .output()
        .expect("numi workspace generate should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("No workspace manifest found"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("numi workspace generate --workspace <path>"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("numi workspace locate"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn workspace_check_reports_ambiguous_manifests_with_real_guidance() {
    let root = make_temp_dir("workspace-check-ambiguous-manifests");
    let app_dir = root.join("apps/App");
    let core_dir = root.join("packages/Core");
    fs::create_dir_all(&app_dir).expect("app dir should exist");
    fs::create_dir_all(&core_dir).expect("core dir should exist");
    write_workspace_manifest(
        &app_dir,
        "version = 1\n[[members]]\nconfig = \"apps/App/numi.toml\"\n",
    );
    write_workspace_manifest(
        &core_dir,
        "version = 1\n[[members]]\nconfig = \"packages/Core/numi.toml\"\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["workspace", "check"])
        .current_dir(&root)
        .output()
        .expect("numi workspace check should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(
        stderr.contains("Multiple workspace manifests found"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("apps/App/numi-workspace.toml"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("packages/Core/numi-workspace.toml"),
        "stderr was: {stderr}"
    );
    assert!(
        stderr.contains("numi workspace check --workspace <path>"),
        "stderr was: {stderr}"
    );
    assert!(
        !stderr.contains("numi workspace locate"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn workspace_generate_can_select_one_member() {
    let temp_root = make_temp_dir("workspace-generate-select-one");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_workspace_manifest(
        &workspace_root,
        r#"
version = 1

[[members]]
config = "apps/assets/numi.toml"

[[members]]
config = "packages/files/numi.toml"
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "workspace",
            "generate",
            "--workspace",
            "numi-workspace.toml",
            "--member",
            "packages/files/numi.toml",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("numi workspace generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !assets_root.join("Generated/Assets.swift").exists(),
        "unselected member should not have generated output"
    );
    assert!(
        files_root.join("Generated/Files.swift").exists(),
        "selected member output was not generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn workspace_generate_honors_member_jobs_selection() {
    let temp_root = make_temp_dir("workspace-generate-member-jobs");
    let workspace_root = temp_root.join("workspace");
    let member_root = workspace_root.join("apps/mixed");
    fs::create_dir_all(member_root.join("Resources")).expect("resources dir should exist");

    copy_dir_all(
        &repo_root().join("fixtures/xcassets-basic/Resources/Assets.xcassets"),
        &member_root.join("Resources/Assets.xcassets"),
    );
    copy_dir_all(
        &repo_root().join("fixtures/files-basic/Resources/Fixtures"),
        &member_root.join("Resources/Fixtures"),
    );
    fs::write(
        member_root.join("numi.toml"),
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template.builtin]
swift = "swiftui-assets"

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template.builtin]
swift = "files"
"#,
    )
    .expect("member config should be written");
    write_workspace_manifest(
        &workspace_root,
        r#"
version = 1

[[members]]
config = "apps/mixed/numi.toml"
jobs = ["assets"]
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "workspace",
            "generate",
            "--workspace",
            "numi-workspace.toml",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("numi workspace generate should run");

    assert!(
        output.status.success(),
        "command failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        member_root.join("Generated/Assets.swift").exists(),
        "selected assets output was not generated"
    );
    assert!(
        !member_root.join("Generated/Files.swift").exists(),
        "unselected files output should not be generated"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}

#[test]
fn workspace_check_returns_exit_code_2_when_any_member_is_stale() {
    let temp_root = make_temp_dir("workspace-check-stale");
    let workspace_root = temp_root.join("workspace");
    let assets_root = workspace_root.join("apps/assets");
    let files_root = workspace_root.join("packages/files");

    copy_dir_all(&repo_root().join("fixtures/xcassets-basic"), &assets_root);
    copy_dir_all(&repo_root().join("fixtures/files-basic"), &files_root);
    write_workspace_manifest(
        &workspace_root,
        r#"
version = 1

[[members]]
config = "apps/assets/numi.toml"

[[members]]
config = "packages/files/numi.toml"
"#,
    );

    let stale_assets_output = assets_root.join("Generated/Assets.swift");
    fs::create_dir_all(
        stale_assets_output
            .parent()
            .expect("generated file should have parent"),
    )
    .expect("generated directory should exist");
    fs::write(&stale_assets_output, "// stale output\n").expect("stale output should be written");

    let generate_output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args([
            "workspace",
            "generate",
            "--workspace",
            "numi-workspace.toml",
            "--member",
            "packages/files/numi.toml",
        ])
        .current_dir(&workspace_root)
        .output()
        .expect("numi workspace generate should run");
    assert!(
        generate_output.status.success(),
        "setup generate failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&generate_output.stdout),
        String::from_utf8_lossy(&generate_output.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["workspace", "check", "--workspace", "numi-workspace.toml"])
        .current_dir(&workspace_root)
        .output()
        .expect("numi workspace check should run");

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
        !stderr.contains("packages/files/Generated/Files.swift"),
        "stderr was: {stderr}"
    );

    fs::remove_dir_all(temp_root).expect("temp dir should be removed");
}
