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

#[test]
fn config_locate_finds_nearest_ancestor() {
    let root = make_temp_dir("nearest-ancestor");
    let nested = root.join("Sources/App");
    fs::create_dir_all(&nested).expect("nested dir should exist");
    let config_path = root.join("swiftgen.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"
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
    let fixture_root = repo_root().join("fixtures/multimodule-repo");
    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "locate"])
        .current_dir(&fixture_root)
        .output()
        .expect("numi config locate should run");

    assert!(!output.status.success(), "command unexpectedly succeeded");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf8");
    assert!(stderr.contains("Multiple configuration files found"));
    assert!(stderr.contains("AppUI/swiftgen.toml"));
    assert!(stderr.contains("Core/swiftgen.toml"));
}

#[test]
fn config_locate_prefers_explicit_path_over_ancestor() {
    let root = make_temp_dir("explicit-config");
    let nested = root.join("Sources/App");
    let explicit_dir = root.join("Configs");
    fs::create_dir_all(&nested).expect("nested dir should exist");
    fs::create_dir_all(&explicit_dir).expect("explicit dir should exist");

    let ancestor_config = root.join("swiftgen.toml");
    fs::write(
        &ancestor_config,
        r#"
version = 1

[[jobs]]
name = "ancestor"
output = "Generated/Ancestor.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Ancestor.xcassets"

[jobs.template]
builtin = "swiftui-assets"
"#,
    )
    .expect("ancestor config should be written");

    let explicit_config = explicit_dir.join("custom.toml");
    fs::write(
        &explicit_config,
        r#"
version = 1

[[jobs]]
name = "explicit"
output = "Generated/Explicit.swift"

[[jobs.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.template]
builtin = "l10n"
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

    let config_path = config_dir.join("swiftgen.toml");
    fs::write(
        &config_path,
        r#"
version = 1

[[jobs]]
name = "assets"
output = "Generated/Assets.swift"

[[jobs.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.template]
builtin = "swiftui-assets"
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
        .args(["check", "--config", "swiftgen.toml"])
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
fn init_refuses_to_overwrite_existing_config_without_force() {
    let root = make_temp_dir("init-refuse-overwrite");
    let existing = root.join("swiftgen.toml");
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
fn init_creates_starter_swiftgen_toml() {
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

    let created =
        fs::read_to_string(root.join("swiftgen.toml")).expect("starter config should exist");
    assert_eq!(
        created,
        include_str!("../../../docs/examples/starter-swiftgen.toml")
    );
    assert!(
        created.contains("builtin = \"l10n\""),
        "starter config was: {created}"
    );
    assert!(
        !created.contains("path = \"Templates/l10n.stencil\""),
        "starter config was: {created}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}

#[test]
fn config_print_emits_the_resolved_config_with_effective_defaults() {
    let root = make_temp_dir("config-print-defaults");
    let config_path = root.join("swiftgen.toml");
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

    let output = Command::new(env!("CARGO_BIN_EXE_numi"))
        .args(["config", "print", "--config", "swiftgen.toml"])
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
    assert!(stdout.contains("name = \"l10n\""), "stdout was: {stdout}");
    assert!(
        stdout.contains("builtin = \"l10n\""),
        "stdout was: {stdout}"
    );

    fs::remove_dir_all(root).expect("temp dir should be removed");
}
