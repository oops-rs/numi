use super::super::super::{GenerateOptions, WriteOutcome, generate, generate_loaded_config};
use super::super::{make_temp_dir, write_custom_files_job_config};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn toml_array(values: &[String]) -> String {
    let parts = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>();
    format!("[{}]", parts.join(", "))
}

fn write_hook_probe_script(root: &std::path::Path, name: &str, exit_code: i32) -> String {
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
            "@echo off\r\nsetlocal\r\n>> \"%~1\" echo %NUMI_HOOK_PHASE%^|%NUMI_HOOK_JOB_NAME%^|%NUMI_HOOK_OUTPUT_PATH%^|%NUMI_HOOK_OUTPUT_DIR%^|%NUMI_HOOK_CONFIG_PATH%^|%NUMI_HOOK_WRITE_OUTCOME%^|%NUMI_HOOK_WORKSPACE_CONFIG_PATH%\r\nexit /b {exit_code}\r\n"
        )
    } else {
        format!(
            "#!/bin/sh\nlog_path=\"$1\"\nprintf '%s|%s|%s|%s|%s|%s|%s\\n' \"$NUMI_HOOK_PHASE\" \"$NUMI_HOOK_JOB_NAME\" \"$NUMI_HOOK_OUTPUT_PATH\" \"$NUMI_HOOK_OUTPUT_DIR\" \"$NUMI_HOOK_CONFIG_PATH\" \"${{NUMI_HOOK_WRITE_OUTCOME-}}\" \"${{NUMI_HOOK_WORKSPACE_CONFIG_PATH-}}\" >> \"$log_path\"\nexit {exit_code}\n"
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

    std::path::PathBuf::from("Scripts")
        .join(file_name)
        .display()
        .to_string()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn write_custom_files_job_config_with_hooks(
    config_path: &std::path::Path,
    incremental: Option<bool>,
    pre_generate: Option<&[String]>,
    post_generate: Option<&[String]>,
) {
    let incremental_line = incremental
        .map(|value| format!("incremental = {value}\n"))
        .unwrap_or_default();
    let mut manifest = format!(
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"
{incremental_line}
[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
path = "Templates/files.jinja"
"#
    );

    if let Some(command) = pre_generate {
        manifest.push_str("\n[jobs.files.hooks.pre_generate]\n");
        manifest.push_str(&format!("command = {}\n", toml_array(command)));
    }

    if let Some(command) = post_generate {
        manifest.push_str("\n[jobs.files.hooks.post_generate]\n");
        manifest.push_str(&format!("command = {}\n", toml_array(command)));
    }

    fs::write(config_path, manifest).expect("config should be written");
}

#[test]
fn generate_runs_pre_and_post_hooks_with_target_env() {
    let temp_dir = make_temp_dir("pipeline-hooks-pre-post");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");
    let log_path = temp_dir.join("hook.log");
    let pre_script = write_hook_probe_script(&temp_dir, "pre-hook", 0);
    let post_script = write_hook_probe_script(&temp_dir, "post-hook", 0);

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
    write_custom_files_job_config_with_hooks(
        &config_path,
        Some(false),
        Some(&[pre_script, log_path.display().to_string()]),
        Some(&[post_script, log_path.display().to_string()]),
    );

    let report = generate(&config_path, None).expect("generation should succeed");
    let log = fs::read_to_string(&log_path).expect("hook log should exist");
    let lines = log.lines().collect::<Vec<_>>();
    let generated_abs = generated_path.display().to_string();
    let generated_dir_abs = generated_path
        .parent()
        .expect("generated path should have parent")
        .display()
        .to_string();
    let config_abs = config_path.display().to_string();

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(lines.len(), 2);
    assert_eq!(
        lines[0],
        format!("pre_generate|files|{generated_abs}|{generated_dir_abs}|{config_abs}||")
    );
    assert_eq!(
        lines[1],
        format!("post_generate|files|{generated_abs}|{generated_dir_abs}|{config_abs}|created|")
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_does_not_run_post_hook_when_output_is_unchanged() {
    let temp_dir = make_temp_dir("pipeline-hooks-post-unchanged");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let log_path = temp_dir.join("hook.log");
    let post_script = write_hook_probe_script(&temp_dir, "post-hook", 0);

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
    write_custom_files_job_config_with_hooks(
        &config_path,
        Some(false),
        None,
        Some(&[post_script, log_path.display().to_string()]),
    );

    let first = generate(&config_path, None).expect("first generation should succeed");
    let second = generate(&config_path, None).expect("second generation should succeed");
    let log = fs::read_to_string(&log_path).expect("hook log should exist");
    let lines = log.lines().collect::<Vec<_>>();

    assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(second.jobs[0].outcome, WriteOutcome::Unchanged);
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("|created|"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_fails_when_pre_generate_hook_fails() {
    let temp_dir = make_temp_dir("pipeline-hooks-pre-fail");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");
    let log_path = temp_dir.join("hook.log");
    let pre_script = write_hook_probe_script(&temp_dir, "pre-hook", 7);

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
    write_custom_files_job_config_with_hooks(
        &config_path,
        Some(false),
        Some(&[pre_script, log_path.display().to_string()]),
        None,
    );

    let error = generate(&config_path, None).expect_err("generation should fail");
    let message = error.to_string();

    assert!(message.contains("pre_generate"));
    assert!(message.contains("job `files`"));
    assert!(!generated_path.exists());

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn generate_loaded_config_passes_workspace_manifest_path_to_hooks() {
    let temp_dir = make_temp_dir("pipeline-hooks-workspace-env");
    let workspace_root = temp_dir.join("workspace");
    let workspace_manifest_path = workspace_root.join("numi.toml");
    let member_root = workspace_root.join("AppUI");
    let member_config_path = member_root.join("numi.toml");
    let files_root = member_root.join("Resources/Fixtures");
    let template_path = member_root.join("Templates/files.jinja");
    let log_path = workspace_root.join("hook.log");
    let hook_script = write_hook_probe_script(&workspace_root, "workspace-post-hook", 0);

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
command = {}
"#,
            toml_array(&[hook_script, log_path.display().to_string()])
        ),
    )
    .expect("workspace manifest should be written");
    write_custom_files_job_config(&member_config_path, Some(false));

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

    let report = generate_loaded_config(
        &member_config_path,
        &resolved,
        None,
        GenerateOptions {
            incremental: Some(false),
            parse_cache: None,
            force_regenerate: false,
            workspace_manifest_path: Some(workspace_manifest_path.clone()),
        },
    )
    .expect("generation should succeed");
    let log = fs::read_to_string(&log_path).expect("hook log should exist");
    let line = log.lines().next().expect("hook line should exist");

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(line.ends_with(&workspace_manifest_path.display().to_string()));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
