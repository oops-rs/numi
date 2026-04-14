use super::super::{check, generate};
use super::{make_temp_dir, seed_cached_parse, with_temp_dir_override};
use crate::parse_cache::{CacheKind, CachedParseData};
use camino::Utf8PathBuf;
use numi_ir::{EntryKind, Metadata, RawEntry};
use serde_json::json;
use std::{fs, path::Path};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn write_files_job_config(config_path: &Path) {
    fs::write(
        config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    )
    .expect("config should be written");
}

fn write_mutating_post_hook(root: &Path, name: &str) -> String {
    let scripts_root = root.join("Scripts");
    fs::create_dir_all(&scripts_root).expect("scripts dir should exist");
    let file_name = if cfg!(windows) {
        format!("{name}.cmd")
    } else {
        format!("{name}.sh")
    };
    let script_path = scripts_root.join(&file_name);
    let script_body = if cfg!(windows) {
        "@echo off\r\npowershell -NoProfile -Command \"$path = $env:NUMI_HOOK_OUTPUT_PATH; $content = Get-Content -Raw -LiteralPath $path; Set-Content -NoNewline -LiteralPath $path -Value ($content + '// formatted\\r\\n')\"\r\n".to_string()
    } else {
        "#!/bin/sh\nprintf '%s' '// formatted\n' >> \"$NUMI_HOOK_OUTPUT_PATH\"\n".to_string()
    };
    fs::write(&script_path, script_body).expect("hook script should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .expect("script permissions should be updated");
    }

    Path::new("Scripts")
        .join(file_name)
        .display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn write_workspace_aware_mutating_post_hook(
    root: &Path,
    name: &str,
    expected_workspace: &Path,
) -> String {
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
            "@echo off\r\nif /I not \"%NUMI_HOOK_WORKSPACE_CONFIG_PATH%\"==\"{}\" exit /b 9\r\npowershell -NoProfile -Command \"$path = $env:NUMI_HOOK_OUTPUT_PATH; $content = Get-Content -Raw -LiteralPath $path; Set-Content -NoNewline -LiteralPath $path -Value ($content + '// formatted\\r\\n')\"\r\n",
            expected_workspace.display()
        )
    } else {
        format!(
            "#!/bin/sh\nif [ \"$NUMI_HOOK_WORKSPACE_CONFIG_PATH\" != \"{}\" ]; then\n  exit 9\nfi\nprintf '%s' '// formatted\n' >> \"$NUMI_HOOK_OUTPUT_PATH\"\n",
            expected_workspace.display()
        )
    };
    fs::write(&script_path, script_body).expect("hook script should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions)
            .expect("script permissions should be updated");
    }

    Path::new("Scripts")
        .join(file_name)
        .display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn write_files_job_config_with_post_hook(config_path: &Path, command: &[String]) {
    let command = command
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(", ");
    fs::write(
        config_path,
        format!(
            r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
path = "Templates/files.jinja"

[jobs.files.hooks.post_generate]
command = [{command}]
"#
        ),
    )
    .expect("config should be written");
}

#[test]
fn check_uses_cached_files_parse_and_still_reports_stale_outputs() {
    let temp_dir = make_temp_dir("pipeline-files-check-cache-hit");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    write_files_job_config(&config_path);

    generate(&config_path, None).expect("initial generation should succeed");
    let generated_path = temp_dir.join("Generated/Files.swift");
    let baseline = fs::read_to_string(&generated_path).expect("generated output should exist");
    assert!(baseline.contains("welcomeVideoMp4"));

    seed_cached_parse(
        CacheKind::Files,
        &files_root,
        CachedParseData::Files(vec![RawEntry {
            path: "cached-guide.pdf".to_string(),
            source_path: Utf8PathBuf::from_path_buf(files_root.join("cached-guide.pdf"))
                .expect("cached source path should be utf8"),
            kind: EntryKind::Data,
            properties: Metadata::from([
                ("relativePath".to_string(), json!("cached-guide.pdf")),
                ("fileName".to_string(), json!("cached-guide.pdf")),
            ]),
        }]),
    )
    .expect("files cache should be seeded");

    let report = check(&config_path, None).expect("check should succeed");

    assert_eq!(
        report.stale_paths,
        vec![Utf8PathBuf::from_path_buf(generated_path).expect("utf8 output path")]
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn check_degrades_when_cache_root_is_unusable() {
    let temp_dir = make_temp_dir("pipeline-cache-degrade-check");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let generated_path = temp_dir.join("Generated/Files.swift");
    let bad_tmp = temp_dir.join("not-a-directory");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
    write_files_job_config(&config_path);

    generate(&config_path, None).expect("initial generation should succeed");
    fs::write(&generated_path, "stale output").expect("generated output should be mutated");

    let report = with_temp_dir_override(&bad_tmp, || check(&config_path, None))
        .expect("check should succeed without cache access");

    assert_eq!(
        report.stale_paths,
        vec![Utf8PathBuf::from_path_buf(generated_path).expect("utf8 output path")]
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn check_accepts_outputs_mutated_by_post_generate_hook() {
    let temp_dir = make_temp_dir("pipeline-check-post-hook-output");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let post_hook = write_mutating_post_hook(&temp_dir, "check-post-hook");

    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(
        &template_path,
        "{{ modules[0].entries[0].properties.fileName }}\n",
    )
    .expect("template should be written");
    write_files_job_config_with_post_hook(&config_path, &[post_hook]);

    generate(&config_path, None).expect("initial generation should succeed");
    let report = check(&config_path, None).expect("check should succeed");

    assert!(
        report.stale_paths.is_empty(),
        "expected no stale paths, got {:?}",
        report.stale_paths
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn check_loaded_config_uses_workspace_manifest_path_for_post_hook() {
    let temp_dir = make_temp_dir("pipeline-check-workspace-post-hook");
    let workspace_root = temp_dir.join("workspace");
    let workspace_manifest_path = workspace_root.join("numi.toml");
    let member_root = workspace_root.join("AppUI");
    let member_config_path = member_root.join("numi.toml");
    let files_root = member_root.join("Resources/Fixtures");
    let template_path = member_root.join("Templates/files.jinja");
    let hook = write_workspace_aware_mutating_post_hook(
        &workspace_root,
        "workspace-check-post-hook",
        &workspace_manifest_path,
    );

    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(
        &template_path,
        "{{ modules[0].entries[0].properties.fileName }}\n",
    )
    .expect("template should be written");
    fs::write(
        &workspace_manifest_path,
        format!(
            r#"
version = 1

[workspace]
members = ["AppUI"]

[workspace.defaults.jobs.files.hooks.post_generate]
command = ["{hook}"]
"#
        ),
    )
    .expect("workspace manifest should be written");
    write_files_job_config(&member_config_path);

    let manifest = numi_config::parse_manifest_str(
        &fs::read_to_string(&workspace_manifest_path).expect("workspace manifest should exist"),
    )
    .expect("workspace manifest should parse");
    let numi_config::Manifest::Workspace(workspace) = manifest else {
        panic!("expected workspace manifest");
    };
    let member_config = numi_config::parse_str(
        &fs::read_to_string(&member_config_path).expect("member config should exist"),
    )
    .expect("member config should parse");
    let resolved = numi_config::resolve_workspace_member_config(
        &workspace_root,
        &workspace,
        "AppUI",
        &member_config,
    )
    .expect("workspace config should resolve");

    crate::generate_loaded_config(
        &member_config_path,
        &resolved,
        None,
        crate::GenerateOptions {
            incremental: Some(false),
            parse_cache: None,
            force_regenerate: false,
            workspace_manifest_path: Some(workspace_manifest_path.clone()),
        },
    )
    .expect("generation should succeed");
    let report = crate::check_loaded_config_with_options(
        &member_config_path,
        &resolved,
        None,
        crate::CheckOptions {
            workspace_manifest_path: Some(workspace_manifest_path.clone()),
        },
    )
    .expect("check should succeed");

    assert!(
        report.stale_paths.is_empty(),
        "expected no stale paths, got {:?}",
        report.stale_paths
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
