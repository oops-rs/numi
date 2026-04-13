use super::*;

#[test]
fn make_temp_dir_ignores_cache_root_override() {
    let temp_dir = make_temp_dir("pipeline-temp-dir-recover");
    let bad_tmp = temp_dir.join("not-a-directory");
    fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
    let recovered =
        with_temp_dir_override(&bad_tmp, || make_temp_dir("pipeline-temp-dir-recovered"));

    assert!(recovered.is_dir());
    assert!(!recovered.starts_with(&bad_tmp));
    if cfg!(unix) {
        assert_eq!(recovered.parent(), Some(Path::new("/tmp")));
    }

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    fs::remove_dir_all(recovered).expect("recovered temp dir should be removed");
}

#[test]
fn generate_rejects_duplicate_strings_table_names_from_directory_inputs() {
    let temp_dir = make_temp_dir("duplicate-strings-table");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization");
    let en_dir = localization_root.join("en.lproj");
    let fr_dir = localization_root.join("fr.lproj");
    fs::create_dir_all(&en_dir).expect("en dir should exist");
    fs::create_dir_all(&fr_dir).expect("fr dir should exist");
    fs::write(
        en_dir.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("en strings should be written");
    fs::write(
        fr_dir.join("Localizable.strings"),
        "\"profile.title\" = \"Profil\";\n",
    )
    .expect("fr strings should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("config should be written");

    let error = generate(&config_path, None).expect_err("duplicate tables should fail");
    let message = error.to_string();

    assert!(message.contains("duplicate localization table `Localizable`"));
    assert!(message.contains("en.lproj/Localizable.strings"));
    assert!(message.contains("fr.lproj/Localizable.strings"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_rejects_duplicate_files_module_names_from_distinct_inputs() {
    let temp_dir = make_temp_dir("duplicate-files-module");
    let config_path = temp_dir.join("numi.toml");
    let first_root = temp_dir.join("Resources/A/Fixtures");
    let second_root = temp_dir.join("Resources/B/Fixtures");
    fs::create_dir_all(&first_root).expect("first files directory should exist");
    fs::create_dir_all(&second_root).expect("second files directory should exist");
    fs::write(first_root.join("faq.pdf"), "faq").expect("first file should be written");
    fs::write(second_root.join("faq.pdf"), "faq").expect("second file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/A/Fixtures"

[[jobs.files.inputs]]
type = "files"
path = "Resources/B/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "swift"
name = "files"
"#,
    )
    .expect("config should be written");

    let error = generate(&config_path, None).expect_err("duplicate modules should fail");
    let message = error.to_string();

    assert!(message.contains("duplicate files module `Fixtures`"));
    assert!(message.contains("Resources/A/Fixtures"));
    assert!(message.contains("Resources/B/Fixtures"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_rejects_duplicate_fonts_module_names_from_distinct_inputs() {
    let temp_dir = make_temp_dir("duplicate-fonts-module");
    let config_path = temp_dir.join("numi.toml");
    let first_root = temp_dir.join("Resources/A/Fonts");
    let second_root = temp_dir.join("Resources/B/Fonts");
    fs::create_dir_all(&first_root).expect("first fonts directory should exist");
    fs::create_dir_all(&second_root).expect("second fonts directory should exist");
    fs::write(
        first_root.join("Baloo2-Bold.ttf"),
        make_test_font_bytes("Baloo 2", "Bold", "Baloo2-Bold"),
    )
    .expect("first font should be written");
    fs::write(
        second_root.join("Baloo2-Regular.ttf"),
        make_test_font_bytes("Baloo 2", "Regular", "Baloo2-Regular"),
    )
    .expect("second font should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.fonts]
output = "Generated/Fonts.swift"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/A/Fonts"

[[jobs.fonts.inputs]]
type = "fonts"
path = "Resources/B/Fonts"

[jobs.fonts.template]
path = "Templates/fonts.jinja"
"#,
    )
    .expect("config should be written");
    fs::create_dir_all(temp_dir.join("Templates")).expect("templates dir should exist");
    fs::write(
        temp_dir.join("Templates/fonts.jinja"),
        "{{ modules | length }}\n",
    )
    .expect("template should be written");

    let error = generate(&config_path, None).expect_err("duplicate modules should fail");
    let message = error.to_string();

    assert!(message.contains("duplicate fonts module `Fonts`"));
    assert!(message.contains("Resources/A/Fonts"));
    assert!(message.contains("Resources/B/Fonts"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_accepts_strings_with_escaped_apostrophes_via_langcodec() {
    let temp_dir = make_temp_dir("pipeline-strings-apostrophe");
    let config_path = temp_dir.join("swiftgen.toml");
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

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
[jobs.l10n.template.builtin]
language = "swift"
name = "l10n"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let generated_path = temp_dir.join("Generated/L10n.swift");
    let generated = fs::read_to_string(&generated_path).expect("generated output should exist");

    assert!(report.warnings.is_empty());
    assert_eq!(
        generated,
        r#"import Foundation

internal enum L10n {
    internal enum Localizable {
        internal static let inviteAccept = tr("Localizable", "invite.accept")
    }
}

private func tr(_ table: String, _ key: String) -> String {
    NSLocalizedString(key, tableName: table, bundle: .main, value: "", comment: "")
}
"#
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_renders_custom_template_includes_from_config_root() {
    let temp_dir = make_temp_dir("custom-template-shared-include");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization");
    let templates_dir = temp_dir.join("Templates");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(localization_root.join("en.lproj"))
        .expect("localization dir should exist");
    fs::create_dir_all(&templates_dir).expect("templates dir should exist");
    fs::create_dir_all(temp_dir.join("partials")).expect("shared partial dir should exist");

    fs::write(
        localization_root.join("en.lproj/Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        templates_dir.join("main.jinja"),
        "{% include \"partials/header.jinja\" %}|{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    fs::write(temp_dir.join("partials/header.jinja"), "SHARED")
        .expect("shared include should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.l10n]
output = "Generated/L10n.swift"

[[jobs.l10n.inputs]]
type = "strings"
path = "Resources/Localization"

[jobs.l10n.template]
path = "Templates/main.jinja"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "SHARED|L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_resolves_extensionless_template_path_to_jinja_file() {
    let temp_dir = make_temp_dir("pipeline-extensionless-template-path");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let template_path = temp_dir.join("Templates/l10n.jinja");
    let generated_path = temp_dir.join("Generated/L10n.swift");

    fs::create_dir_all(&localization_root).expect("localization dir should exist");
    fs::create_dir_all(
        template_path
            .parent()
            .expect("template path should have parent"),
    )
    .expect("template dir should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::write(
        &template_path,
        "{{ job.swiftIdentifier }}|{{ modules[0].name }}\n",
    )
    .expect("template should be written");
    write_extensionless_l10n_job_config(&config_path);

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(rendered, "L10n|Localizable\n");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_writes_builtin_files_accessors() {
    let temp_dir = make_temp_dir("pipeline-files-generate");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let generated_path = temp_dir.join("Generated/Files.swift");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::write(
        &config_path,
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

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(
        rendered,
        r#"import Foundation

internal enum Files {
    internal enum Onboarding {
        internal static let welcomeVideoMp4 = file("Onboarding/welcome-video.mp4")
    }
    internal static let faqPdf = file("faq.pdf")
}

private func resourceBundle() -> Bundle {
    Bundle.module
}

private func file(_ path: String) -> URL {
    guard let url = resourceBundle().url(forResource: path, withExtension: nil) else {
        fatalError("Missing file resource: \(path)")
    }
    return url
}
"#
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_writes_objc_builtin_files_accessors() {
    let temp_dir = make_temp_dir("pipeline-objc-files-generate");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let generated_path = temp_dir.join("Generated/Files.h");

    fs::create_dir_all(files_root.join("Onboarding")).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(files_root.join("Onboarding/welcome-video.mp4"), "video")
        .expect("video file should be written");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.h"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
    )
    .expect("config should be written");

    let report = generate(&config_path, None).expect("generation should succeed");
    let rendered = fs::read_to_string(&generated_path).expect("output should be written");

    assert_eq!(report.jobs.len(), 1);
    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(!rendered.contains("@implementation"));
    assert!(
        rendered
            .contains("NS_INLINE NSURL *Files__Fixtures__Onboarding__WelcomeVideoMp4(void)")
    );
    assert!(rendered.contains("SWIFTPM_MODULE_BUNDLE"));
    assert!(!rendered.contains("bundleForClass:"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generation_fingerprint_changes_when_builtin_language_changes() {
    let temp_dir = make_temp_dir("pipeline-fingerprint-builtin-language");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");

    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::write(files_root.join("faq.pdf"), "faq").expect("faq file should be written");
    fs::write(
        &config_path,
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

    let loaded = numi_config::load_from_path(&config_path).expect("config should load");
    let config_dir = config_path.parent().expect("config should have parent");
    let selected_jobs = vec!["files".to_string()];
    let swift_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
        .expect("files job should resolve");
    let swift_job = swift_jobs
        .into_iter()
        .next()
        .expect("files job should exist");
    let swift_fingerprint =
        compute_generation_fingerprint(config_dir, &loaded.config.defaults, swift_job)
            .expect("swift builtin fingerprint should compute");

    fs::write(
        &config_path,
        r#"
version = 1

[jobs.files]
output = "Generated/Files.swift"

[[jobs.files.inputs]]
type = "files"
path = "Resources/Fixtures"

[jobs.files.template]
[jobs.files.template.builtin]
language = "objc"
name = "files"
"#,
    )
    .expect("objc config should be written");

    let loaded = numi_config::load_from_path(&config_path).expect("objc config should load");
    let objc_jobs = numi_config::resolve_selected_jobs(&loaded.config, Some(&selected_jobs))
        .expect("files job should resolve");
    let objc_job = objc_jobs
        .into_iter()
        .next()
        .expect("files job should exist");
    let objc_fingerprint =
        compute_generation_fingerprint(config_dir, &loaded.config.defaults, objc_job)
            .expect("objc builtin fingerprint should compute");

    assert_ne!(swift_fingerprint.fingerprint, objc_fingerprint.fingerprint);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_skips_when_generation_contract_is_unchanged_by_default() {
    let temp_dir = make_temp_dir("pipeline-generate-skip-default");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");

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
    write_custom_files_job_config(&config_path, None);

    let first = generate(&config_path, None).expect("initial generation should succeed");
    assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should exist"),
        "faq.pdf\n"
    );

    let second = generate(&config_path, None).expect("second generation should succeed");
    assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should remain"),
        "faq.pdf\n"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_respects_job_incremental_opt_out_and_rerenders() {
    let temp_dir = make_temp_dir("pipeline-generate-opt-out");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");

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
    write_custom_files_job_config(&config_path, Some(false));

    let first = generate(&config_path, None).expect("initial generation should succeed");
    assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

    let second = generate(&config_path, None).expect("second generation should rerender");
    assert_eq!(second.jobs[0].outcome, WriteOutcome::Unchanged);
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should remain"),
        "faq.pdf\n"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_options_override_job_incremental_setting() {
    let temp_dir = make_temp_dir("pipeline-generate-options-override");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");

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
    write_custom_files_job_config(&config_path, Some(false));

    let first = generate(&config_path, None).expect("initial generation should succeed");
    assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

    let second = generate_with_options(
        &config_path,
        None,
        GenerateOptions {
            incremental: Some(true),
            parse_cache: None,
            force_regenerate: false,
            workspace_manifest_path: None,
        },
    )
    .expect("second generation should honor the explicit override");
    assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should remain"),
        "faq.pdf\n"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_refresh_bypasses_generation_skip() {
    let temp_dir = make_temp_dir("pipeline-generate-refresh");
    let config_path = temp_dir.join("numi.toml");
    let files_root = temp_dir.join("Resources/Fixtures");
    let template_path = temp_dir.join("Templates/files.jinja");
    let generated_path = temp_dir.join("Generated/Files.swift");

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
    write_custom_files_job_config(&config_path, Some(true));

    let first = generate(&config_path, None).expect("initial generation should succeed");
    assert_eq!(first.jobs[0].outcome, WriteOutcome::Created);

    let second = generate_with_options(
        &config_path,
        None,
        GenerateOptions {
            incremental: Some(true),
            parse_cache: None,
            force_regenerate: false,
            workspace_manifest_path: None,
        },
    )
    .expect("second generation should skip");
    assert_eq!(second.jobs[0].outcome, WriteOutcome::Skipped);

    let third = generate_with_options(
        &config_path,
        None,
        GenerateOptions {
            incremental: Some(true),
            parse_cache: Some(true),
            force_regenerate: true,
            workspace_manifest_path: None,
        },
    )
    .expect("force regenerate should rerender");
    assert_eq!(third.jobs[0].outcome, WriteOutcome::Unchanged);
    assert_eq!(
        fs::read_to_string(&generated_path).expect("generated file should remain"),
        "faq.pdf\n"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
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
        format!(
            "post_generate|files|{generated_abs}|{generated_dir_abs}|{config_abs}|created|"
        )
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


#[test]
fn generate_uses_cached_xcassets_parse_payload_on_cache_hit() {
    let temp_dir = make_temp_dir("pipeline-assets-cache-hit");
    let config_path = temp_dir.join("numi.toml");
    let catalog_root = temp_dir.join("Resources/Assets.xcassets");
    let color_root = catalog_root.join("Brand.colorset");

    fs::create_dir_all(&color_root).expect("catalog should exist");
    fs::write(
        catalog_root.join("Contents.json"),
        r#"{"info":{"author":"xcode","version":1}}"#,
    )
    .expect("catalog contents should exist");
    fs::write(
            color_root.join("Contents.json"),
            r#"{"colors":[{"idiom":"universal","color":{"color-space":"srgb","components":{"red":"1.000","green":"0.000","blue":"0.000","alpha":"1.000"}}}],"info":{"author":"xcode","version":1}}"#,
        )
        .expect("color contents should exist");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    fs::write(
        &config_path,
        r#"
version = 1

[jobs.assets]
output = "Generated/Assets.swift"

[[jobs.assets.inputs]]
type = "xcassets"
path = "Resources/Assets.xcassets"

[jobs.assets.template]
[jobs.assets.template.builtin]
language = "swift"
name = "swiftui-assets"
"#,
    )
    .expect("config should be written");

    let cached_source = Utf8PathBuf::from_path_buf(color_root.join("Contents.json"))
        .expect("cached source path should be utf8");
    seed_cached_parse(
        CacheKind::Xcassets,
        &catalog_root,
        CachedParseData::Xcassets(XcassetsReport {
            entries: vec![RawEntry {
                path: "CachedPalette".to_string(),
                source_path: cached_source,
                kind: EntryKind::Color,
                properties: Metadata::from([("assetName".to_string(), json!("CachedPalette"))]),
            }],
            warnings: Vec::new(),
        }),
    )
    .expect("xcassets cache should be seeded");

    let report = generate(&config_path, None).expect("generation should succeed");
    let generated = fs::read_to_string(temp_dir.join("Generated/Assets.swift"))
        .expect("generated assets should exist");

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(generated.contains("ColorAsset(name: \"CachedPalette\")"));
    assert!(!generated.contains("ColorAsset(name: \"Brand\")"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_uses_cached_strings_parse_payload_on_cache_hit() {
    let temp_dir = make_temp_dir("pipeline-strings-cache-hit");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let strings_path = localization_root.join("Localizable.strings");

    fs::create_dir_all(&localization_root).expect("localization directory should exist");
    fs::write(&strings_path, "\"profile.title\" = \"Profile\";\n")
        .expect("strings file should be written");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    write_strings_job_config(&config_path);

    let cached_source = Utf8PathBuf::from_path_buf(strings_path.clone())
        .expect("cached source path should be utf8");
    seed_cached_parse(
        CacheKind::Strings,
        &temp_dir.join("Resources/Localization"),
        CachedParseData::Strings(vec![LocalizationTable {
            table_name: "Localizable".to_string(),
            source_path: cached_source.clone(),
            module_kind: ModuleKind::Strings,
            entries: vec![RawEntry {
                path: "cached.banner".to_string(),
                source_path: cached_source,
                kind: EntryKind::StringKey,
                properties: Metadata::from([
                    ("key".to_string(), json!("cached.banner")),
                    ("translation".to_string(), json!("Cached banner")),
                ]),
            }],
            warnings: Vec::new(),
        }]),
    )
    .expect("strings cache should be seeded");

    let report = generate(&config_path, None).expect("generation should succeed");
    let generated = fs::read_to_string(temp_dir.join("Generated/L10n.swift"))
        .expect("generated l10n should exist");

    assert_eq!(report.jobs[0].outcome, WriteOutcome::Created);
    assert!(generated.contains("cachedBanner = tr(\"Localizable\", \"cached.banner\")"));
    assert!(!generated.contains("profileTitle = tr(\"Localizable\", \"profile.title\")"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn cache_store_skips_entries_when_inputs_change_during_parse() {
    let temp_dir = make_temp_dir("pipeline-cache-skip-unstable-input");
    let files_root = temp_dir.join("Resources/Fixtures");
    let input_file = files_root.join("faq.pdf");

    fs::create_dir_all(&files_root).expect("files directory should exist");
    fs::write(&input_file, "before").expect("fixture file should be written");

    let stale_entries = vec![RawEntry {
        path: "stale.pdf".to_string(),
        source_path: Utf8PathBuf::from_path_buf(input_file.clone())
            .expect("stale source path should be utf8"),
        kind: EntryKind::Data,
        properties: Metadata::from([
            ("relativePath".to_string(), json!("stale.pdf")),
            ("fileName".to_string(), json!("stale.pdf")),
        ]),
    }];
    let fresh_entries = vec![RawEntry {
        path: "fresh.pdf".to_string(),
        source_path: Utf8PathBuf::from_path_buf(input_file.clone())
            .expect("fresh source path should be utf8"),
        kind: EntryKind::Data,
        properties: Metadata::from([
            ("relativePath".to_string(), json!("fresh.pdf")),
            ("fileName".to_string(), json!("fresh.pdf")),
        ]),
    }];

    let first = load_or_parse_cached(
        CacheKind::Files,
        &files_root,
        None,
        None,
        || {
            fs::write(&input_file, "after").expect("fixture file should mutate during parse");
            Ok::<_, GenerateError>(stale_entries.clone())
        },
        CachedParseData::Files,
        |cached| match cached {
            CachedParseData::Files(entries) => Some(entries),
            _ => None,
        },
    )
    .expect("first parse should succeed");
    assert_eq!(first, stale_entries);

    let second = load_or_parse_cached(
        CacheKind::Files,
        &files_root,
        None,
        None,
        || Ok::<_, GenerateError>(fresh_entries.clone()),
        CachedParseData::Files,
        |cached| match cached {
            CachedParseData::Files(entries) => Some(entries),
            _ => None,
        },
    )
    .expect("second parse should succeed");
    assert_eq!(second, fresh_entries);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}


#[test]
fn generate_degrades_when_cache_root_is_unusable() {
    let temp_dir = make_temp_dir("pipeline-cache-degrade-generate");
    let config_path = temp_dir.join("numi.toml");
    let localization_root = temp_dir.join("Resources/Localization/en.lproj");
    let bad_tmp = temp_dir.join("not-a-directory");

    fs::create_dir_all(&localization_root).expect("localization directory should exist");
    fs::write(
        localization_root.join("Localizable.strings"),
        "\"profile.title\" = \"Profile\";\n",
    )
    .expect("strings file should be written");
    fs::create_dir_all(temp_dir.join("Generated")).expect("generated dir should exist");
    fs::write(&bad_tmp, "cache root blocker").expect("bad tmp file should exist");
    write_strings_job_config(&config_path);

    let report = with_temp_dir_override(&bad_tmp, || generate(&config_path, None))
        .expect("generation should succeed without cache access");
    let generated = fs::read_to_string(temp_dir.join("Generated/L10n.swift"))
        .expect("generated output should exist");

    assert_eq!(report.jobs.len(), 1);
    assert!(generated.contains("profileTitle = tr(\"Localizable\", \"profile.title\")"));

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
